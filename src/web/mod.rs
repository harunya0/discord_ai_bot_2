use axum::{
    extract::{State, Path},
    http::{StatusCode, HeaderMap},
    middleware::{self, Next},
    response::{Response, Html},
    routing::{get, post, delete},
    Json, Router,
};
use axum::extract::Request;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use crate::ai::client::AiClient;
use crate::ai::openai::OpenAiClient;
use crate::ai::embedding::EmbeddingClient;
use crate::strage::history::HistoryStore;
use crate::rag;
use crate::search::WebSearchClient;

// Discordの実チャンネルIDは巨大なsnowflakeなので、0は絶対に衝突しない予約ID
pub const WEB_CHANNEL_ID: u64 = 0;

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
}

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
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

#[derive(Serialize)]
struct StatusResponse {
    current_model: String,
    current_session: String,
    session_count: usize,
    uptime_seconds: u64,
}

#[derive(Deserialize)]
struct SearchRequest {
    query: String,
    count: Option<u8>,
}

#[derive(Serialize)]
struct SearchResultItem {
    title: String,
    url: String,
    description: String,
}

async fn search_handler(State(state): State<AppState>, Json(req): Json<SearchRequest>) -> Json<Vec<SearchResultItem>> {
    let count = req.count.unwrap_or(5);
    let results = state.search_client.search(&req.query, count).await.unwrap_or_default();
    Json(results.into_iter().map(|r| SearchResultItem {
        title: r.title, url: r.url, description: r.description
    }).collect())
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

async fn chat_handler(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Json<ChatResponse> {
    let session = state.channel_sessions.read().await
        .get(&WEB_CHANNEL_ID).cloned().unwrap_or_else(|| "default".to_string());
    let channel_id = format!("web:{}", session);

    let model = state.channel_models.read().await
        .get(&WEB_CHANNEL_ID).cloned().unwrap_or_else(|| "gemini-3.1-flash-lite".to_string());

    // Embedding生成・履歴保存(ユーザー発言)
    let user_embedding = state.embedding_client.embed(&req.message, "RETRIEVAL_DOCUMENT").await.unwrap_or_default();
    let _ = state.history.save_message(&channel_id, "web_user", "user", &req.message, &user_embedding);

    // RAG検索
    let query_embedding = state.embedding_client.embed(&req.message, "RETRIEVAL_QUERY").await.unwrap_or_default();
    let candidates = state.history.get_candidates_for_search(&channel_id, 300).unwrap_or_default();
    let relevant = rag::rag::search_similar_with_decay(&candidates, &query_embedding, 3, 14.0, 0.3);

    // 直近履歴
    let recent = state.history.get_recent_history(&channel_id, 10).unwrap_or_default();

    let mut contents: Vec<serde_json::Value> = Vec::new();
    if !relevant.is_empty() {
        let context_text = format!("(過去の関連する会話)\n{}", relevant.join("\n"));
        contents.push(serde_json::json!({ "role": "user", "parts": [{ "text": context_text }] }));
    }
    for (role, text) in &recent {
        contents.push(serde_json::json!({ "role": role, "parts": [{ "text": text }] }));
    }
    contents.push(serde_json::json!({ "role": "user", "parts": [{ "text": req.message }] }));

    let reply = if model.starts_with("gpt-") {
        let messages = crate::ai::convert::to_openai_messages(&contents);
        state.openai_client.generate(messages, &model).await
    } else {
        state.ai_client.generate_with_contents(contents, &model).await
    }.unwrap_or_else(|_| "エラーが発生しました".to_string());

    let bot_embedding = state.embedding_client.embed(&reply, "RETRIEVAL_DOCUMENT").await.unwrap_or_default();
    let _ = state.history.save_message(&channel_id, "bot", "model", &reply, &bot_embedding);

    Json(ChatResponse { reply })
}

async fn list_sessions_handler(State(state): State<AppState>) -> Json<Vec<String>> {
    let sessions = state.history.list_sessions("web").unwrap_or_default();
    Json(sessions)
}

async fn switch_session_handler(State(state): State<AppState>, Json(req): Json<SwitchSessionRequest>) -> StatusCode {
    state.channel_sessions.write().await.insert(WEB_CHANNEL_ID, req.name);
    StatusCode::OK
}

async fn delete_session_handler(State(state): State<AppState>, Path(name): Path<String>) -> StatusCode {
    match state.history.delete_session("web", &name) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn switch_model_handler(State(state): State<AppState>, Json(req): Json<SwitchModelRequest>) -> StatusCode {
    state.channel_models.write().await.insert(WEB_CHANNEL_ID, req.name);
    StatusCode::OK
}

async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    let current_model = state.channel_models.read().await
        .get(&WEB_CHANNEL_ID).cloned().unwrap_or_else(|| "gemini-3.1-flash-lite (デフォルト)".to_string());
    let current_session = state.channel_sessions.read().await
        .get(&WEB_CHANNEL_ID).cloned().unwrap_or_else(|| "default".to_string());
    let session_count = state.history.list_sessions("web").unwrap_or_default().len();

    Json(StatusResponse {
        current_model,
        current_session,
        session_count,
        uptime_seconds: state.start_time.elapsed().as_secs(),
    })
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

pub fn build_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/chat", post(chat_handler))
        .route("/sessions", get(list_sessions_handler))
        .route("/sessions/switch", post(switch_session_handler))
        .route("/sessions/:name", delete(delete_session_handler))
        .route("/model", post(switch_model_handler))
        .route("/status", get(status_handler))
        .route("/search", post(search_handler))
        // API全体にトークン認証ミドルウェアを適用
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/", get(index_handler))
        .nest("/api", api_routes)
        // ルート("/")や "/api" に該当しないリクエスト(app.jsやstyle.css等)は
        // staticフォルダから探して返す
        .fallback_service(ServeDir::new("static"))
        .with_state(state)
}