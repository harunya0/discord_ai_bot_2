use gcp_auth::{CustomServiceAccount, TokenProvider};
use serde_json::json;

pub struct EmbeddingClient {
    service_account: CustomServiceAccount,
    project_id: String,
    http: reqwest::Client,
}

impl EmbeddingClient {
    pub fn new(service_account: CustomServiceAccount, project_id: String) -> Self {
        Self {
            service_account,
            project_id,
            http: reqwest::Client::new(),
        }
    }

    pub async fn embed(&self, text: &str, task_type: &str) -> anyhow::Result<Vec<f32>> {
        let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
        let token = self.service_account.token(scopes).await?;

        let url = format!(
            "https://us-central1-aiplatform.googleapis.com/v1/projects/{}/locations/us-central1/publishers/google/models/gemini-embedding-001:predict",
            self.project_id
        );

        let body = json!({
            "instances": [{
                "content": text,
                "task_type": task_type
            }]
        });

        let res = self.http
            .post(&url)
            .bearer_auth(token.as_str())
            .json(&body)
            .send()
            .await?;

        let json_res: serde_json::Value = res.json().await?;

        let values = json_res["predictions"][0]["embeddings"]["values"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("embedding values not found"))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(values)
    }
}