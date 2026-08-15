pub mod handler;
pub mod interaction;

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use twilight_http::Client as HttpClient;
use twilight_model::id::Id;
use twilight_model::id::marker::UserMarker;

use crate::ai::client::AiClient;
use crate::ai::openai::OpenAiClient;
use crate::storage::HistoryStore;

#[derive(Clone)]
pub struct BotContext {
    pub http: Arc<HttpClient>,
    pub ai_client: Arc<AiClient>,
    pub openai_client: Arc<OpenAiClient>,
    pub history: Arc<HistoryStore>,
    pub channel_models: Arc<RwLock<HashMap<u64, String>>>,
    pub channel_sessions: Arc<RwLock<HashMap<u64, String>>>,
    pub bot_id: Id<UserMarker>,
}
