use twilight_http::Client as HttpClient;
use twilight_model::gateway::payload::incoming::MessageCreate;
use twilight_model::id::Id;
use twilight_model::id::marker::UserMarker;
use std::sync::Arc;
use crate::ai::client::AiClient;
use crate::ai::embedding::EmbeddingClient;
use crate::strage::history::HistoryStore;
use crate::rag;

pub async fn handle_message(
    msg: Box<MessageCreate>,
    http: Arc<HttpClient>,
    ai_client: Arc<AiClient>,
    embedding_client: Arc<EmbeddingClient>,
    history: Arc<HistoryStore>,
    bot_id: Id<UserMarker>,
) {
    if msg.author.bot {
        return;
    }

    let is_mentioned = msg.mentions.iter().any(|m| m.id == bot_id);
    if !is_mentioned {
        return;
    }

    let cleaned = msg.content
        .split_whitespace()
        .filter(|w| !w.starts_with("<@"))
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.trim().is_empty() {
        return;
    }

    let channel_id = msg.channel_id.to_string();
    let author_id = msg.author.id.to_string();

    let _ = http.create_typing_trigger(msg.channel_id).await;

    // 1. ユーザー発言のEmbeddingを生成して保存(RETRIEVAL_DOCUMENT)
    let user_embedding = embedding_client
        .embed(&cleaned, "RETRIEVAL_DOCUMENT")
        .await
        .unwrap_or_default();

    if let Err(e) = history.save_message(&channel_id, &author_id, "user", &cleaned, &user_embedding) {
        eprintln!("履歴保存エラー: {:?}", e);
    }

    // 2. クエリ用Embeddingで直近の関連発言を検索(RETRIEVAL_QUERY)
    let query_embedding = embedding_client
        .embed(&cleaned, "RETRIEVAL_QUERY")
        .await
        .unwrap_or_default();

    let candidates = history
        .get_candidates_for_search(&channel_id, 300)
        .unwrap_or_default();

    let relevant = rag::rag::search_similar(&candidates, &query_embedding, 3);

    // 3. 直近の会話履歴(時系列)も取得
    let recent = history.get_recent_history(&channel_id, 10).unwrap_or_default();

    // 4. 関連発言をプロンプトの先頭に「参考情報」として追加
    let mut contents: Vec<(String, String)> = Vec::new();
    if !relevant.is_empty() {
        let context_text = format!(
            "(過去の関連する会話)\n{}",
            relevant.join("\n")
        );
        contents.push(("user".to_string(), context_text));
    }
    contents.extend(recent);

    match ai_client.generate_with_history(&contents).await {
        Ok(response) => {
            let _ = http.create_message(msg.channel_id).content(&response).await;

            let bot_embedding = embedding_client
                .embed(&response, "RETRIEVAL_DOCUMENT")
                .await
                .unwrap_or_default();

            if let Err(e) = history.save_message(&channel_id, "bot", "model", &response, &bot_embedding) {
                eprintln!("履歴保存エラー: {:?}", e);
            }
        }
        Err(e) => {
            eprintln!("AI応答エラー: {:?}", e);
            let _ = http.create_message(msg.channel_id).content("エラーが発生しました…").await;
        }
    }
}