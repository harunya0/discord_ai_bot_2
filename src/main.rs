mod ai;
mod bot;
mod search;
mod storage;
mod web;

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt};
use twilight_http::Client as HttpClient;
use twilight_model::id::marker::UserMarker;
use twilight_model::id::Id;

use ai::client::AiClient;
use ai::openai::OpenAiClient;
use bot::BotContext;
use search::WebSearchClient;
use storage::HistoryStore;
use web::{build_router, AppState};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKENが見つかりません");
    let guild_id = env::var("DISCORD_GUILD_ID").ok();
    register_commands(&token, guild_id.as_deref()).await?;

    let credentials_path =
        env::var("GCP_CREDENTIALS_PATH").expect("GCP_CREDENTIALS_PATHが見つかりません");
    let project_id = env::var("GCP_PROJECT_ID").expect("GCP_PROJECT_IDが見つかりません");
    let location = env::var("GCP_LOCATION").unwrap_or_else(|_| "global".to_string());

    let openai_api_key = env::var("OPENAI_API_KEY").unwrap_or_default();
    let openai_client = Arc::new(OpenAiClient::new(openai_api_key));

    let brave_api_key = env::var("BRAVE_API_KEY").unwrap_or_default();
    let search_client = Arc::new(WebSearchClient::new(brave_api_key));

    let ai_client = Arc::new(AiClient::new(&credentials_path, project_id, location).await?);

    let channel_sessions: Arc<RwLock<HashMap<u64, String>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let channel_models: Arc<RwLock<HashMap<u64, String>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // rugst を内包するローカルメモリストア初期化
    let history_store = Arc::new(HistoryStore::new("./data/history.db")?);

    let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;
    let mut shard = Shard::new(ShardId::ONE, token.clone(), intents);
    let http = Arc::new(HttpClient::new(token));
    let bot_id: Arc<RwLock<Option<Id<UserMarker>>>> = Arc::new(RwLock::new(None));

    let event_types = EventTypeFlags::READY
        | EventTypeFlags::MESSAGE_CREATE
        | EventTypeFlags::GUILD_CREATE
        | EventTypeFlags::INTERACTION_CREATE;

    let web_state = AppState {
        ai_client: Arc::clone(&ai_client),
        openai_client: Arc::clone(&openai_client),
        history: Arc::clone(&history_store),
        channel_models: Arc::clone(&channel_models),
        channel_sessions: Arc::clone(&channel_sessions),
        search_client: Arc::clone(&search_client),
        api_token: env::var("WEB_API_TOKEN").expect("WEB_API_TOKENが見つかりません"),
        start_time: Instant::now(),
        target_channel_id: Arc::new(RwLock::new(0)),
    };
    let app = build_router(web_state);

    tokio::spawn(async move {
        // 127.0.0.1でリッスンし、外部公開はCaddy等のリバースプロキシ経由を想定
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
            .await
            .expect("Failed to bind to 127.0.0.1:3000");
        println!("Web API listening on 127.0.0.1:3000 (reverse proxy推奨)");
        axum::serve(listener, app).await.unwrap();
    });

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
                let id = *bot_id.read().await;
                if let Some(bot_user_id) = id {
                    let ctx = BotContext {
                        http: Arc::clone(&http),
                        ai_client: Arc::clone(&ai_client),
                        openai_client: Arc::clone(&openai_client),
                        history: Arc::clone(&history_store),
                        channel_models: Arc::clone(&channel_models),
                        channel_sessions: Arc::clone(&channel_sessions),
                        bot_id: bot_user_id,
                    };
                    tokio::spawn(async move {
                        bot::handler::handle_message(msg, ctx).await;
                    });
                }
            }
            Event::InteractionCreate(interaction) => {
                let value = serde_json::to_value(&*interaction).unwrap_or_default();
                let channel_models = Arc::clone(&channel_models);
                let channel_sessions = Arc::clone(&channel_sessions);
                let history_store = Arc::clone(&history_store);
                let ai_client = Arc::clone(&ai_client);
                let search_client = Arc::clone(&search_client);
                tokio::spawn(async move {
                    bot::interaction::handle_interaction(
                        value,
                        channel_models,
                        channel_sessions,
                        history_store,
                        ai_client,
                        search_client,
                    )
                    .await;
                });
            }
            _ => {}
        }
    }

    Ok(())
}

async fn register_commands(token: &str, guild_id: Option<&str>) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    let app: serde_json::Value = client
        .get("https://discord.com/api/v10/oauth2/applications/@me")
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await?
        .json()
        .await?;

    let app_id = app["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("application idが取得できません"))?;

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
        },
        {
            "name": "search",
            "description": "Web検索して回答します",
            "type": 1,
            "options": [
                {
                    "type": 3,
                    "name": "query",
                    "description": "検索したい内容",
                    "required": true
                },
                {
                    "type": 5,
                    "name": "ai",
                    "description": "AIによる要約回答にする(デフォルトは生の検索結果一覧)",
                    "required": false
                },
                {
                    "type": 4,
                    "name": "count",
                    "description": "表示件数(デフォルト5、最大20)",
                    "required": false,
                    "min_value": 1,
                    "max_value": 20
                }
            ]
        }
    ]);

    // 1. グローバル登録(常に実行)
    let global_url = format!("https://discord.com/api/v10/applications/{}/commands", app_id);
    let global_res = client
        .put(&global_url)
        .header("Authorization", format!("Bot {}", token))
        .json(&commands)
        .send()
        .await?;
    println!("グローバルコマンド登録結果: {}", global_res.status());

    // 2. GUILD_IDが指定されていれば、追加でギルド登録(即時反映用)
    if let Some(gid) = guild_id {
        let guild_url = format!(
            "https://discord.com/api/v10/applications/{}/guilds/{}/commands",
            app_id, gid
        );
        let guild_res = client
            .put(&guild_url)
            .header("Authorization", format!("Bot {}", token))
            .json(&commands)
            .send()
            .await?;
        println!("ギルドコマンド登録結果: {}", guild_res.status());
    }

    Ok(())
}