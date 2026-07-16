mod bot;
mod ai;
mod strage;
mod rag;

use twilight_gateway::{Intents, Shard, ShardId, StreamExt, EventTypeFlags, Event};
use twilight_http::Client as HttpClient;
use twilight_model::id::Id;
use twilight_model::id::marker::UserMarker;
use std::env;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use ai::client::AiClient;
use ai::openai::OpenAiClient;
use ai::embedding::EmbeddingClient;
use strage::history::HistoryStore;
use gcp_auth::CustomServiceAccount;
use std::path::Path;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKENが見つかりません");
    register_commands(&token).await?;
    let credentials_path = env::var("GCP_CREDENTIALS_PATH").expect("GCP_CREDENTIALS_PATHが見つかりません");
    let project_id = env::var("GCP_PROJECT_ID").expect("GCP_PROJECT_IDが見つかりません");
    let location = env::var("GCP_LOCATION").unwrap_or_else(|_| "global".to_string());
    let model = env::var("GCP_MODEL").unwrap_or_else(|_| "gemini-3.1-flash-lite".to_string());
    let openai_api_key = env::var("OPENAI_API_KEY").unwrap_or_default();
    let openai_client = Arc::new(OpenAiClient::new(openai_api_key));
    let channel_sessions: Arc<RwLock<HashMap<u64, String>>> = Arc::new(RwLock::new(HashMap::new()));
    

    let ai_client = Arc::new(
        AiClient::new(&credentials_path, project_id.clone(), location, model).await?
    );

    // Embedding用に別途サービスアカウントを読み込む(AiClient内のものは非公開のため)
    let embed_service_account = CustomServiceAccount::from_file(Path::new(&credentials_path))?;
    let embedding_client = Arc::new(EmbeddingClient::new(embed_service_account, project_id));

    let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;
    let mut shard = Shard::new(ShardId::ONE, token.clone(), intents);
    let http = Arc::new(HttpClient::new(token));

    let bot_id: Arc<RwLock<Option<Id<UserMarker>>>> = Arc::new(RwLock::new(None));
    let history_store = Arc::new(HistoryStore::new("./data/history.db")?);
    let channel_models: Arc<RwLock<HashMap<u64, String>>> = Arc::new(RwLock::new(HashMap::new()));

    let event_types = EventTypeFlags::READY
        | EventTypeFlags::MESSAGE_CREATE
        | EventTypeFlags::GUILD_CREATE
        | EventTypeFlags::INTERACTION_CREATE;

    while let Some(item) = shard.next_event(event_types).await {
        let event = match item {
            Ok(event) => event,
            Err(source) => {
                eprintln!("error receiving event: {source}");
                continue;
            }
        };

        match event {
            Event::Ready(ready) => {
                println!("Bot is ready! (id: {})", ready.user.id);
                let mut id_lock = bot_id.write().await;
                *id_lock = Some(ready.user.id);
            }
            Event::GuildCreate(guild) => {
                println!("Joined guild: {:?}", guild.id());
            }
            Event::MessageCreate(msg) => {
                let http = Arc::clone(&http);
                let ai_client = Arc::clone(&ai_client);
                let embedding_client = Arc::clone(&embedding_client);
                let history_store = Arc::clone(&history_store);
                let bot_id = Arc::clone(&bot_id);
                let channel_models = Arc::clone(&channel_models);
                let openai_client = Arc::clone(&openai_client);
                let channel_sessions = Arc::clone(&channel_sessions);
                tokio::spawn(async move {
                    let id = *bot_id.read().await;
                    if let Some(id) = id {
                        bot::handler::handle_message(msg, http, ai_client, embedding_client, history_store, channel_models, id, openai_client, channel_sessions).await;
                    }
                });
            }
            Event::InteractionCreate(interaction) => {
                let value = serde_json::to_value(&*interaction).unwrap_or_default();
                let channel_models = Arc::clone(&channel_models);
                let channel_sessions = Arc::clone(&channel_sessions);
                let history_store = Arc::clone(&history_store);
                tokio::spawn(async move {
                    bot::interaction::handle_interaction(value, channel_models, channel_sessions, history_store).await;
                });
            }
            _ => {}
        }
    }

    Ok(())
}

async fn register_commands(token: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    let app: serde_json::Value = client
        .get("https://discord.com/api/v10/oauth2/applications/@me")
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await?
        .json()
        .await?;

    let app_id = app["id"].as_str().ok_or_else(|| anyhow::anyhow!("application idが取得できません"))?;

    let commands = serde_json::json!([
        {
            "name": "model",
            "description": "AIモデルを切り替えます",
            "type": 1,
            "options": [{
                "type": 3,
                "name": "name",
                "description": "使用するモデル",
                "required": true,
                "choices": [
                    {"name": "gemini-3.1-flash-lite", "value": "gemini-3.1-flash-lite"},
                    {"name": "gemini-3-flash-preview", "value": "gemini-3-flash-preview"},
                    {"name": "gemini-3.1-pro-preview", "value": "gemini-3.1-pro-preview"},
                    {"name": "gpt-4o-mini", "value": "gpt-4o-mini"},
                    {"name": "gpt-4o", "value": "gpt-4o"}
                ]
            }]
        },
        {
            "name": "session",
            "description": "会話セッションを切り替え/確認/削除します",
            "type": 1,
            "options": [
                {
                    "type": 3,
                    "name": "name",
                    "description": "セッション名(空欄で一覧表示)",
                    "required": false
                },
                {
                    "type": 3,
                    "name": "delete",
                    "description": "削除したいセッション名",
                    "required": false
                }
            ]
        }
    ]);

    let url = format!("https://discord.com/api/v10/applications/{}/commands", app_id);
    let res = client
        .put(&url)
        .header("Authorization", format!("Bot {}", token))
        .json(&commands)
        .send()
        .await?;

    println!("コマンド登録結果: {}", res.status());
    Ok(())
}