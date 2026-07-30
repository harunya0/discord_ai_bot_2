use anyhow::{Context, Result};
use gcp_auth::{CustomServiceAccount, TokenProvider};
use serde_json::{json, Value};

pub struct ClaudeClient {
    service_account: CustomServiceAccount,
    project_id: String,
    location: String,
    http: reqwest::Client,
}

impl ClaudeClient {
    pub fn new(
        service_account: CustomServiceAccount,
        project_id: String,
        location: String,
        http: reqwest::Client,
    ) -> Self {
        let location = if location.is_empty() {
            "us-east5".to_string()
        } else {
            location
        };

        Self {
            service_account,
            project_id,
            location,
            http,
        }
    }

    pub async fn generate(&self, messages: Vec<Value>, model: &str) -> Result<String> {
        let is_debug = std::env::var("DEBUG").is_ok();

        let token = self
            .service_account
            .token(&["https://www.googleapis.com/auth/cloud-platform"])
            .await
            .context("Failed to get GCP token")?;

        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:rawPredict",
            self.location, self.project_id, self.location, model
        );

        let body = json!({
            "anthropic_version": "vertex-2023-10-16",
            "max_tokens": 4096,
            "messages": messages,
        });

        if is_debug {
            eprintln!("[DEBUG] Claude request URL: {}", url);
            eprintln!("[DEBUG] Claude request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());
        }

        let response = self
            .http
            .post(&url)
            .bearer_auth(token.as_str())
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Vertex AI Claude endpoint")?;

        let status = response.status();
        let text = response
            .text()
            .await
            .context("Failed to read response text")?;

        if is_debug {
            eprintln!("[DEBUG] Claude response status: {}", status);
            eprintln!("[DEBUG] Claude response body: {}", text);
        }

        if !status.is_success() {
            anyhow::bail!("Vertex AI request failed with status {}: {}", status, text);
        }

        let parsed: Value = serde_json::from_str(&text).context("Failed to parse response JSON")?;
        
        let content_text = parsed["content"][0]["text"]
            .as_str()
            .context("Failed to extract text from response")?
            .to_string();

        Ok(content_text)
    }
}
