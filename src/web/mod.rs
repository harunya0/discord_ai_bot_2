use axum::{
    extract::{State, Path},
    http::{StatusCode, HeaderMap, Method, HeaderName, HeaderValue},
    middleware::{self, Next},
    response::Response,
    routing::{get, post, delete},
    Json, Router,
};
use axum::extract::Request;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use base64::{engine::general_purpose, Engine as _};

use crate::ai::client::AiClient;
use crate::ai::openai::OpenAiClient;
use crate::ai::embedding::EmbeddingClient;
use crate::strage::history::HistoryStore;
use crate::rag;
use crate::search::WebSearchClient;

const TEXT_EXTENSIONS: [&str; 15] = [
    "txt", "md", "rs", "py", "js", "ts", "json", "toml", "yaml", "yml",
    "csv", "html", "css", "c", "cpp",
];
const MAX_FILE_CHARS: usize = 8000;

#[derive(Clone)]
pub struct AppState {
    pub ai_client: Arc<AiClient>,
    pub openai_client: Arc<OpenAiClient>,
    pub embedding_client: Arc<EmbeddingClient>,
    pub history: Arc<HistoryStore>,
    pub channel_models: Arc<RwLock<HashMap<u64, String>>>,
    pub channel_sessions: Arc<RwLock<HashMap<u64, String>>>,
    pub search_client: Arc<WebSearchClient>,
    pub api_token: String,
    pub start_time: Instant,
    pub target_channel_id: Arc<RwLock<u64>>,
}

#[derive(Deserialize)]
struct WebFile {
    name: String,
    mime: String,
    data: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    files: Option<Vec<WebFile>>,
}

#[derive(Serialize)]
struct ChatResponse {
    reply: String,
}

#[derive(Deserialize)]
struct SwitchSessionRequest {
    name: String,
}

#[derive(Deserialize)]
struct SwitchModelRequest {
    name: String,
}

#[derive(Deserialize)]
struct SwitchChannelRequest {
    channel_id: String,
}

#[derive(Serialize)]
struct StatusResponse {
    current_channel_id: String,
    current_model: String,
    current_session: String,
    session_count: usize,
    uptime_seconds: u64,
}

#[derive(Deserialize)]
struct SearchRequest {
    query: String,
    count: Option<u8>,
    ai: Option<bool>,
}

#[derive(Serialize)]
struct SearchResultItem {
    title: String,
    url: String,
    description: String,
}

#[derive(Serialize)]
struct SearchResponse {
    ai: bool,
    text: Option<String>,
    results: Option<Vec<SearchResultItem>>,
}

#[derive(Serialize)]
struct HistoryItem {
    role: String,
    text: String,
}

async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = headers.get("x-api-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if provided != state.api_token {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}

async fn get_history_handler(State(state): State<AppState>) -> Json<Vec<HistoryItem>> {
    let ch_id = *state.target_channel_id.read().await;
    let session = state.channel_sessions.read().await
        .get(&ch_id).cloned().unwrap_or_else(|| "default".to_string());
    let db_key = format!("{}:{}", ch_id, session);

    let recent = state.history.get_recent_history(&db_key, 50).unwrap_or_default();

    let items = recent.into_iter().map(|(role, text)| {
        let display_role = if role == "model" || role == "bot" { "bot".to_string() } else { "user".to_string() };
        HistoryItem { role: display_role, text }
    }).collect();

    Json(items)
}

async fn search_handler(State(state): State<AppState>, Json(req): Json<SearchRequest>) -> Json<SearchResponse> {
    let use_ai = req.ai.unwrap_or(false);

    if use_ai {
        let text = match state.ai_client.generate_with_search(&req.query, "gemini-3-flash-preview").await {
            Ok(text) => text,
            Err(e) => {
                eprintln!("AI検索エラー: {:?}", e);
                "検索に失敗しました".to_string()
            }
        };
        return Json(SearchResponse { ai: true, text: Some(text), results: None });
    }

    let count = req.count.unwrap_or(5);
    let results = state.search_client.search(&req.query, count).await.unwrap_or_default();
    Json(SearchResponse {
        ai: false,
        text: None,
        results: Some(results.into_iter().map(|r| SearchResultItem {
            title: r.title, url: r.url, description: r.description
        }).collect()),
    })
}

async fn chat_handler(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Json<ChatResponse> {
    let ch_id = *state.target_channel_id.read().await;
    let session = state.channel_sessions.read().await
        .get(&ch_id).cloned().unwrap_or_else(|| "default".to_string());
    let db_key = format!("{}:{}", ch_id, session);

    let model = state.channel_models.read().await
        .get(&ch_id).cloned().unwrap_or_else(|| "gemini-3.1-flash-lite".to_string());

    let mut image_parts: Vec<serde_json::Value> = Vec::new();
    let mut file_texts: Vec<String> = Vec::new();

    if let Some(files) = &req.files {
        for file in files {
            let ext = file.name.rsplit('.').next().unwrap_or("").to_lowercase();
            if file.mime.starts_with("image/") {
                image_parts.push(serde_json::json!({
                    "inlineData": { "mimeType": file.mime, "data": file.data }
                }));
            } else if TEXT_EXTENSIONS.contains(&ext.as_str()) || file.mime.starts_with("text/") {
                if let Ok(bytes) = general_purpose::STANDARD.decode(&file.data) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        let truncated: String = text.chars().take(MAX_FILE_CHARS).collect();
                        let note = if text.chars().count() > MAX_FILE_CHARS { "\n...(以降省略)" } else { "" };
                        file_texts.push(format!("添付ファイル「{}」の内容:\n```\n{}{}\n```", file.name, truncated, note));
                    }
                }
            }
        }
    }

    let embed_text = if req.message.trim().is_empty() && !image_parts.is_empty() {
        "[画像添付]".to_string()
    } else {
        req.message.clone()
    };
    let user_embedding = state.embedding_client.embed(&embed_text, "RETRIEVAL_DOCUMENT").await.unwrap_or_default();
    let _ = state.history.save_message(&db_key, "web_user", "user", &embed_text, &user_embedding);

    let query_embedding = state.embedding_client.embed(&embed_text, "RETRIEVAL_QUERY").await.unwrap_or_default();
    let candidates = state.history.get_candidates_for_search(&db_key, 300).unwrap_or_default();
    let relevant = rag::rag::search_similar_with_decay(&candidates, &query_embedding, 3, 14.0, 0.3);

    let recent = state.history.get_recent_history(&db_key, 10).unwrap_or_default();

    let mut contents: Vec<serde_json::Value> = Vec::new();
    if !relevant.is_empty() {
        let context_text = format!("(過去の関連する会話)\n{}", relevant.join("\n"));
        contents.push(serde_json::json!({ "role": "user", "parts": [{ "text": context_text }] }));
    }
    for (role, text) in &recent {
        contents.push(serde_json::json!({ "role": role, "parts": [{ "text": text }] }));
    }

    let mut combined_text = req.message.clone();
    if !file_texts.is_empty() {
        if !combined_text.trim().is_empty() { combined_text.push_str("\n\n"); }
        combined_text.push_str(&file_texts.join("\n\n"));
    }

    let mut current_parts: Vec<serde_json::Value> = Vec::new();
    if !combined_text.trim().is_empty() {
        current_parts.push(serde_json::json!({ "text": combined_text }));
    }
    current_parts.extend(image_parts);
    contents.push(serde_json::json!({ "role": "user", "parts": current_parts }));

    let reply = if model.starts_with("gpt-") {
        let messages = crate::ai::convert::to_openai_messages(&contents);
        state.openai_client.generate(messages, &model).await
    } else {
        state.ai_client.generate_with_contents(contents, &model).await
    }.unwrap_or_else(|_| "エラーが発生しました".to_string());

    let bot_embedding = state.embedding_client.embed(&reply, "RETRIEVAL_DOCUMENT").await.unwrap_or_default();
    let _ = state.history.save_message(&db_key, "bot", "model", &reply, &bot_embedding);

    Json(ChatResponse { reply })
}

async fn list_sessions_handler(State(state): State<AppState>) -> Json<Vec<String>> {
    let ch_id = *state.target_channel_id.read().await;
    let sessions = state.history.list_sessions(&ch_id.to_string()).unwrap_or_default();
    Json(sessions)
}

async fn switch_session_handler(State(state): State<AppState>, Json(req): Json<SwitchSessionRequest>) -> StatusCode {
    let ch_id = *state.target_channel_id.read().await;
    state.channel_sessions.write().await.insert(ch_id, req.name);
    StatusCode::OK
}

async fn delete_session_handler(State(state): State<AppState>, Path(name): Path<String>) -> StatusCode {
    let ch_id = *state.target_channel_id.read().await;
    match state.history.delete_session(&ch_id.to_string(), &name) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn switch_model_handler(State(state): State<AppState>, Json(req): Json<SwitchModelRequest>) -> StatusCode {
    let ch_id = *state.target_channel_id.read().await;
    state.channel_models.write().await.insert(ch_id, req.name);
    StatusCode::OK
}

async fn switch_channel_handler(State(state): State<AppState>, Json(req): Json<SwitchChannelRequest>) -> StatusCode {
    let id: u64 = req.channel_id.parse().unwrap_or(0);
    *state.target_channel_id.write().await = id;
    StatusCode::OK
}

async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    let ch_id = *state.target_channel_id.read().await;
    let current_model = state.channel_models.read().await
        .get(&ch_id).cloned().unwrap_or_else(|| "gemini-3.1-flash-lite (デフォルト)".to_string());
    let current_session = state.channel_sessions.read().await
        .get(&ch_id).cloned().unwrap_or_else(|| "default".to_string());
    let session_count = state.history.list_sessions(&ch_id.to_string()).unwrap_or_default().len();

    Json(StatusResponse {
        current_channel_id: ch_id.to_string(),
        current_model,
        current_session,
        session_count,
        uptime_seconds: state.start_time.elapsed().as_secs(),
    })
}

/// フロントエンド(別サービス)からのアクセスを許可するCORS設定。
/// WEB_ORIGIN環境変数に許可したいオリジン(例: https://web.example.com)を1つ指定する。
/// 自分専用サービスのため、未設定の場合は起動時に警告を出しつつ全オリジンを許可する(認証はx-api-tokenで担保)。
fn build_cors_layer() -> CorsLayer {
    let allowed_header = HeaderName::from_static("x-api-token");

    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([axum::http::header::CONTENT_TYPE, allowed_header]);

    match std::env::var("WEB_ORIGIN") {
        Ok(origin) => match origin.parse::<HeaderValue>() {
            Ok(value) => layer.allow_origin(value),
            Err(_) => {
                eprintln!("WEB_ORIGINの形式が不正です。全オリジンを許可します: {}", origin);
                layer.allow_origin(tower_http::cors::Any)
            }
        },
        Err(_) => {
            eprintln!("WEB_ORIGINが未設定です。全オリジンを許可します(x-api-tokenでの認証は有効です)");
            layer.allow_origin(tower_http::cors::Any)
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/chat", post(chat_handler))
        .route("/sessions", get(list_sessions_handler))
        .route("/sessions/switch", post(switch_session_handler))
        .route("/sessions/:name", delete(delete_session_handler))
        .route("/model", post(switch_model_handler))
        .route("/channel", post(switch_channel_handler))
        .route("/status", get(status_handler))
        .route("/search", post(search_handler))
        .route("/history", get(get_history_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .nest("/api", api_routes)
        .layer(build_cors_layer())
        .with_state(state)
}
