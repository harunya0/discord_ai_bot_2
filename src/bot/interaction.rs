use serde_json::{json, Value};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::strage::history::HistoryStore;

pub async fn handle_interaction(
    interaction: Value,
    channel_models: Arc<RwLock<HashMap<u64, String>>>,
    channel_sessions: Arc<RwLock<HashMap<u64, String>>>,
    history: Arc<HistoryStore>,
) {
    let interaction_id = interaction["id"].as_str().unwrap_or_default();
    let token = interaction["token"].as_str().unwrap_or_default();
    let channel_id_str = interaction["channel_id"].as_str().unwrap_or_default();
    let channel_id: u64 = channel_id_str.parse().unwrap_or(0);

    let command_name = interaction["data"]["name"].as_str().unwrap_or_default();
    let options = interaction["data"]["options"].as_array().cloned().unwrap_or_default();

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
        _ => "不明なコマンドです".to_string(),
    };

    let client = reqwest::Client::new();
    let url = format!("https://discord.com/api/v10/interactions/{}/{}/callback", interaction_id, token);
    let body = json!({
        "type": 4,
        "data": { "content": reply }
    });
    let _ = client.post(&url).json(&body).send().await;
}