use serde_json::json;

pub struct OpenAiClient {
    api_key: String,
    http: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
        }
    }

    pub async fn generate(&self, messages: Vec<serde_json::Value>, model: &str) -> anyhow::Result<String> {
        let url = "https://api.openai.com/v1/chat/completions";

        let body = json!({
            "model": model,
            "messages": messages
        });

        let res = self.http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let json_res: serde_json::Value = res.json().await?;

        println!("OpenAI生レスポンス: {}", serde_json::to_string_pretty(&json_res)?);

        let text = json_res["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("(応答の解析に失敗しました)")
            .to_string();

        Ok(text)
    }
}