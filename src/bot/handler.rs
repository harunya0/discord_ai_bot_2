use base64::{engine::general_purpose, Engine as _};
use serde_json::json;
use twilight_http::Client as HttpClient;
use twilight_model::gateway::payload::incoming::MessageCreate;

use super::BotContext;
use crate::storage::default_rag_search_options;
use crate::web::{MAX_FILE_CHARS, TEXT_EXTENSIONS};

pub async fn handle_message(msg: Box<MessageCreate>, ctx: BotContext) {
    if msg.author.bot {
        return;
    }

    let is_mentioned = msg.mentions.iter().any(|m| m.id == ctx.bot_id);
    if !is_mentioned {
        return;
    }

    let cleaned = msg
        .content
        .split_whitespace()
        .filter(|w| !w.starts_with("<@"))
        .collect::<Vec<_>>()
        .join(" ");

    let replied_context: Option<String> = if let Some(referenced) = &msg.referenced_message {
        Some(format!(
            "(返信先メッセージ: {}: {})",
            referenced.author.name, referenced.content
        ))
    } else if let Some(reference) = &msg.reference {
        if let Some(ref_msg_id) = reference.message_id {
            match ctx.http.message(msg.channel_id, ref_msg_id).await {
                Ok(res) => match res.model().await {
                    Ok(fetched) => Some(format!(
                        "(返信先メッセージ: {}: {})",
                        fetched.author.name, fetched.content
                    )),
                    Err(_) => None,
                },
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    };

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
    let session = ctx
        .channel_sessions
        .read()
        .await
        .get(&msg.channel_id.get())
        .cloned()
        .unwrap_or_else(|| "default".to_string());

    let channel_id = format!("{}:{}", channel_id_raw, session);
    let author_id = msg.author.id.to_string();

    let _ = ctx.http.create_typing_trigger(msg.channel_id).await;

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

    // 2. メモリストア(rugst)にメッセージ保存 (ローカルFastEmbed埋め込みを自動生成・保存)
    let embed_text = if cleaned.trim().is_empty() {
        "[画像添付]".to_string()
    } else {
        cleaned.clone()
    };

    if let Err(e) = ctx.history.remember(&channel_id, &author_id, "user", &embed_text) {
        eprintln!("履歴保存エラー: {:?}", e);
    }

    // 3. RAG検索 (rugst によるセマンティック + FTS5ハイブリッド検索 + 時間減衰)
    let search_opts = default_rag_search_options();
    let relevant_results = ctx
        .history
        .search(&channel_id, "user", &embed_text, &search_opts)
        .unwrap_or_default();
    let relevant: Vec<String> = relevant_results.into_iter().map(|r| r.text).collect();

    // 4. 直近の会話履歴 (古い順)
    let recent = ctx.history.get_recent_history(&channel_id, 10).unwrap_or_default();

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

    if let Some(ctx_text) = &replied_context {
        if !combined_text.trim().is_empty() {
            combined_text.push_str("\n\n");
        }
        combined_text.push_str(ctx_text);
    }

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

    let model = ctx
        .channel_models
        .read()
        .await
        .get(&msg.channel_id.get())
        .cloned()
        .unwrap_or_else(|| "gemini-3.1-flash-lite".to_string());

    let response = if model.starts_with("gpt-") {
        let messages = crate::ai::convert::to_openai_messages(&contents);
        ctx.openai_client.generate(messages, &model).await
    } else {
        ctx.ai_client.generate_with_contents(contents, &model).await
    };

    match response {
        Ok(response) => {
            send_response(&ctx.http, msg.channel_id, &response).await;

            // 6. ボット応答をメモリストア(rugst)に保存
            if let Err(e) = ctx.history.remember(&channel_id, "bot", "model", &response) {
                eprintln!("履歴保存エラー: {:?}", e);
            }
        }
        Err(e) => {
            eprintln!("AI応答エラー: {:?}", e);
            let _ = ctx.http.create_message(msg.channel_id).content("エラーが発生しました…").await;
        }
    }
}

fn split_message(text: &str, limit: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.split('\n') {
        if current.chars().count() + line.chars().count() + 1 > limit && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

async fn send_response(
    http: &HttpClient,
    channel_id: twilight_model::id::Id<twilight_model::id::marker::ChannelMarker>,
    text: &str,
) {
    let has_code_block = text.contains("```");
    let over_limit = text.chars().count() > 2000;

    if has_code_block && over_limit {
        // コードが含まれる長文はファイルとして送信(途中で途切れさせない)
        let bytes = text.as_bytes().to_vec();
        let attachment = twilight_model::http::attachment::Attachment::from_bytes(
            "response.md".to_string(),
            bytes,
            1,
        );
        let result = http
            .create_message(channel_id)
            .content("長いコードのためファイルで送ります:")
            .attachments(&[attachment])
            .await;
        if let Err(e) = result {
            eprintln!("ファイル送信エラー: {:?}", e);
        }
        return;
    }

    if !over_limit {
        let _ = http.create_message(channel_id).content(text).await;
        return;
    }

    // コードを含まない長文は分割送信
    for chunk in split_message(text, 1900) {
        let _ = http.create_message(channel_id).content(&chunk).await;
    }
}