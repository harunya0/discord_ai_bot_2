use twilight_http::Client as HttpClient;
use twilight_model::gateway::payload::incoming::MessageCreate;
use twilight_model::id::Id;
use twilight_model::id::marker::UserMarker;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use base64::{engine::general_purpose, Engine as _};
use serde_json::json;
use crate::ai::client::AiClient;
use crate::ai::embedding::EmbeddingClient;
use crate::ai::openai::OpenAiClient;
use crate::strage::history::HistoryStore;
use crate::rag;

const TEXT_EXTENSIONS: [&str; 15] = [
    "txt", "md", "rs", "py", "js", "ts", "json", "toml", "yaml", "yml",
    "csv", "html", "css", "c", "cpp",
];

const MAX_FILE_CHARS: usize = 8000; 

pub async fn handle_message(
    msg: Box<MessageCreate>,
    http: Arc<HttpClient>,
    ai_client: Arc<AiClient>,
    embedding_client: Arc<EmbeddingClient>,
    history: Arc<HistoryStore>,
    channel_models: Arc<RwLock<HashMap<u64, String>>>,
    bot_id: Id<UserMarker>,
    openai_client: Arc<OpenAiClient>,
    channel_sessions: Arc<RwLock<HashMap<u64, String>>>
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

    // 画像添付が無く、テキストも空ならスキップ
    let has_images = msg.attachments.iter().any(|a| {
        a.content_type.as_deref().unwrap_or("").starts_with("image/")
    });
    let has_files = msg.attachments.iter().any(|a| {
        let ext = a.filename.rsplit('.').next().unwrap_or("").to_lowercase();
        TEXT_EXTENSIONS.contains(&ext.as_str())
    });
    if cleaned.trim().is_empty() && !has_images && !has_files {
        return;
    }

    let channel_id_raw = msg.channel_id.to_string();
    let session = channel_sessions
        .read()
        .await
        .get(&msg.channel_id.get())
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    
    let channel_id = format!("{}:{}", channel_id_raw, session);
    let author_id = msg.author.id.to_string();

    let _ = http.create_typing_trigger(msg.channel_id).await;

    // 1. 画像添付をダウンロードしてBase64化
    let download_client = reqwest::Client::new();
    let mut image_parts: Vec<serde_json::Value> = Vec::new();

    for attachment in &msg.attachments {
        let content_type = attachment.content_type.as_deref().unwrap_or("");
        if !content_type.starts_with("image/") {
            continue;
        }
        match download_client.get(&attachment.url).send().await {
            Ok(res) => match res.bytes().await {
                Ok(bytes) => {
                    let encoded = general_purpose::STANDARD.encode(&bytes);
                    image_parts.push(json!({
                        "inlineData": {
                            "mimeType": content_type,
                            "data": encoded
                        }
                    }));
                }
                Err(e) => eprintln!("画像バイト取得エラー: {:?}", e),
            },
            Err(e) => eprintln!("画像ダウンロードエラー: {:?}", e),
        }
    }

    let mut file_texts: Vec<String> = Vec::new();

for attachment in &msg.attachments {
    let filename = &attachment.filename;
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();

    if !TEXT_EXTENSIONS.contains(&ext.as_str()) {
        continue;
    }

    match download_client.get(&attachment.url).send().await {
        Ok(res) => match res.text().await {
            Ok(text) => {
                let truncated: String = text.chars().take(MAX_FILE_CHARS).collect();
                let note = if text.chars().count() > MAX_FILE_CHARS {
                    "\n...(以降省略)"
                } else {
                    ""
                };
                file_texts.push(format!(
                    "添付ファイル「{}」の内容:\n```\n{}{}\n```",
                    filename, truncated, note
                ));
            }
            Err(e) => eprintln!("テキストファイル読み込みエラー: {:?}", e),
        },
        Err(e) => eprintln!("テキストファイルダウンロードエラー: {:?}", e),
    }
}

    // 2. Embedding生成・履歴保存はテキストのみ対象(画像はEmbeddingしない簡易実装)
    let embed_text = if cleaned.trim().is_empty() { "[画像添付]".to_string() } else { cleaned.clone() };
    let user_embedding = embedding_client
        .embed(&embed_text, "RETRIEVAL_DOCUMENT")
        .await
        .unwrap_or_default();

    if let Err(e) = history.save_message(&channel_id, &author_id, "user", &embed_text, &user_embedding) {
        eprintln!("履歴保存エラー: {:?}", e);
    }

    // 3. RAG検索(テキストベース)
    let query_embedding = embedding_client
        .embed(&embed_text, "RETRIEVAL_QUERY")
        .await
        .unwrap_or_default();
    let candidates = history.get_candidates_for_search(&channel_id, 300).unwrap_or_default();
    let relevant = rag::rag::search_similar(&candidates, &query_embedding, 3);

    // 4. 直近の会話履歴
    let recent = history.get_recent_history(&channel_id, 10).unwrap_or_default();

    // 5. contents配列を組み立て(過去分はテキストのみ、今回分だけ画像を含める)
    let mut contents: Vec<serde_json::Value> = Vec::new();

    if !relevant.is_empty() {
        let context_text = format!("(過去の関連する会話)\n{}", relevant.join("\n"));
        contents.push(json!({ "role": "user", "parts": [{ "text": context_text }] }));
    }

    for (role, text) in &recent {
        contents.push(json!({ "role": role, "parts": [{ "text": text }] }));
    }

    // 今回のメッセージ: テキスト + ファイル内容 + 画像パーツをまとめて1つのcontentに
    let mut current_parts: Vec<serde_json::Value> = Vec::new();

    let mut combined_text = cleaned.clone();
    if !file_texts.is_empty() {
        if !combined_text.trim().is_empty() {
            combined_text.push_str("\n\n");
        }
        combined_text.push_str(&file_texts.join("\n\n"));
    }

    if !combined_text.trim().is_empty() {
        current_parts.push(json!({ "text": combined_text }));
    }
    current_parts.extend(image_parts);
    contents.push(json!({ "role": "user", "parts": current_parts }));

    let model = channel_models
        .read()
        .await
        .get(&msg.channel_id.get())
        .cloned()
        .unwrap_or_else(|| "gemini-3.1-flash-lite".to_string());

    let response = if model.starts_with("gpt-") {
    let messages = crate::ai::convert::to_openai_messages(&contents);
    openai_client.generate(messages, &model).await
    } else {
        ai_client.generate_with_contents(contents, &model).await
    };

    match response {
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