use serde_json::{json, Value};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::strage::history::HistoryStore;
use crate::ai::client::AiClient;
use crate::search::WebSearchClient;

pub async fn handle_interaction(
    interaction: Value,
    channel_models: Arc<RwLock<HashMap<u64, String>>>,
    channel_sessions: Arc<RwLock<HashMap<u64, String>>>,
    history: Arc<HistoryStore>,
    ai_client: Arc<AiClient>,
    search_client: Arc<WebSearchClient>,
) {
    let interaction_id = interaction["id"].as_str().unwrap_or_default();
    let token = interaction["token"].as_str().unwrap_or_default();
    let app_id = interaction["application_id"].as_str().unwrap_or_default();
    let channel_id_str = interaction["channel_id"].as_str().unwrap_or_default();
    let channel_id: u64 = channel_id_str.parse().unwrap_or(0);

    let command_name = interaction["data"]["name"].as_str().unwrap_or_default();
    let options = interaction["data"]["options"].as_array().cloned().unwrap_or_default();

    let client = reqwest::Client::new();

    // 1. まず「考え中...」を即座に返す(3秒以内)
    let defer_url = format!("https://discord.com/api/v10/interactions/{}/{}/callback", interaction_id, token);
    let defer_body = json!({ "type": 5 });
    let _ = client.post(&defer_url).json(&defer_body).send().await;

    // 2. 時間のかかる処理を実行
    let reply = match command_name {
        "model" => {
            let name = options.iter()
                .find(|o| o["name"] == "name")
                .and_then(|o| o["value"].as_str())
                .unwrap_or_default();
            channel_models.write().await.insert(channel_id, name.to_string());
            format!("モデルを {} に切り替えました", name)
        }
        "session" => {
            let name = options.iter()
                .find(|o| o["name"] == "name")
                .and_then(|o| o["value"].as_str());
            let delete_target = options.iter()
                .find(|o| o["name"] == "delete")
                .and_then(|o| o["value"].as_str());

            if let Some(target) = delete_target {
                match history.delete_session(channel_id_str, target) {
                    Ok(_) => format!("セッション「{}」を削除しました", target),
                    Err(e) => {
                        eprintln!("セッション削除エラー: {:?}", e);
                        "削除に失敗しました".to_string()
                    }
                }
            } else {
                match name {
                    Some(n) => {
                        channel_sessions.write().await.insert(channel_id, n.to_string());
                        format!("セッションを「{}」に切り替えました", n)
                    }
                    None => {
                        let current = channel_sessions.read().await.get(&channel_id).cloned().unwrap_or_else(|| "default".to_string());
                        let sessions = history.list_sessions(channel_id_str).unwrap_or_default();
                        let list = if sessions.is_empty() { "(まだ記録なし)".to_string() } else { sessions.join(", ") };
                        format!("現在のセッション: {}\n既存セッション: {}", current, list)
                    }
                }
            }
        }
        "search" => {
            let query = options.iter()
                .find(|o| o["name"] == "query")
                .and_then(|o| o["value"].as_str())
                .unwrap_or_default();

            let use_ai = options.iter()
                .find(|o| o["name"] == "ai")
                .and_then(|o| o["value"].as_bool())
                .unwrap_or(false);

            let count = options.iter()
                .find(|o| o["name"] == "count")
                .and_then(|o| o["value"].as_u64())
                .map(|v| v as u8)
                .unwrap_or(5);

            if use_ai {
                match ai_client.generate_with_search(query, "gemini-3-flash-preview").await {
                    Ok(text) => text,
                    Err(e) => {
                        eprintln!("AI検索エラー: {:?}", e);
                        "検索に失敗しました".to_string()
                    }
                }
            } else {
                match search_client.search(query, count).await {
                    Ok(results) if !results.is_empty() => {
                        results.iter()
                            .enumerate()
                            .map(|(i, r)| format!("{}. **{}**\n{}\n{}", i + 1, r.title, r.url, r.description))
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    }
                    Ok(_) => "検索結果が見つかりませんでした".to_string(),
                    Err(e) => {
                        eprintln!("検索エラー: {:?}", e);
                        "検索に失敗しました".to_string()
                    }
                }
            }
        }
        _ => "不明なコマンドです".to_string(),
    };

    // 3. 「考え中...」を実際の結果に編集
    let edit_url = format!("https://discord.com/api/v10/webhooks/{}/{}/messages/@original", app_id, token);
    let edit_body = json!({ "content": reply });
    let _ = client.patch(&edit_url).json(&edit_body).send().await;
}