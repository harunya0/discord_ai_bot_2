use gcp_auth::{CustomServiceAccount, TokenProvider};
use serde_json::json;
use std::path::Path;

pub struct AiClient {
    service_account: CustomServiceAccount,
    project_id: String,
    location: String,
    model: String,
    http: reqwest::Client,
}

impl AiClient {
    pub async fn new(
        credentials_path: &str,
        project_id: String,
        location: String,
        model: String,
    ) -> anyhow::Result<Self> {
        let service_account = CustomServiceAccount::from_file(Path::new(credentials_path))?;

        Ok(Self {
            service_account,
            project_id,
            location,
            model,
            http: reqwest::Client::new(),
        })
    }

    pub async fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
        let token = self.service_account.token(scopes).await?;

        let url = format!(
        "https://aiplatform.googleapis.com/v1/projects/{}/locations/global/publishers/google/models/{}:generateContent",
        self.project_id, self.model
        );

        let body = json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": prompt }]
            }]
        });

        let res = self.http
            .post(&url)
            .bearer_auth(token.as_str())
            .json(&body)
            .send()
            .await?;

        let json_res: serde_json::Value = res.json().await?;

        println!("生レスポンス: {}", serde_json::to_string_pretty(&json_res)?); 

        let text = json_res["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("(応答の解析に失敗しました)")
            .to_string();

        Ok(text)
    }
    pub async fn generate_with_history(&self, history: &[(String, String)]) -> anyhow::Result<String> {
    let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
    let token = self.service_account.token(scopes).await?;

    let url = format!(
        "https://aiplatform.googleapis.com/v1/projects/{}/locations/global/publishers/google/models/{}:generateContent",
        self.project_id, self.model
    );

    let contents: Vec<serde_json::Value> = history
        .iter()
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "parts": [{ "text": content }]
            })
        })
        .collect();

    let body = serde_json::json!({ "contents": contents });

    let res = self.http
        .post(&url)
        .bearer_auth(token.as_str())
        .json(&body)
        .send()
        .await?;

    let json_res: serde_json::Value = res.json().await?;

    let text = json_res["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("(応答の解析に失敗しました)")
        .to_string();

    Ok(text)
    }

    pub async fn generate_with_contents(&self, contents: Vec<serde_json::Value>) -> anyhow::Result<String> {
    let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
    let token = self.service_account.token(scopes).await?;

    let url = format!(
        "https://aiplatform.googleapis.com/v1/projects/{}/locations/global/publishers/google/models/{}:generateContent",
        self.project_id, self.model
    );

    let body = json!({
        "contents": contents,
        "tools": [{ "url_context": {} }]
    });

    let res = self.http
        .post(&url)
        .bearer_auth(token.as_str())
        .json(&body)
        .send()
        .await?;

    let json_res: serde_json::Value = res.json().await?;

    println!("生レスポンス: {}", serde_json::to_string_pretty(&json_res)?);

    let text = json_res["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("(応答の解析に失敗しました)")
        .to_string();

    Ok(text)
    }
}