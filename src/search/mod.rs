use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
}

pub struct WebSearchClient {
    api_key: String,
    http: reqwest::Client,
}

impl WebSearchClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
        }
    }

    pub async fn search(&self, query: &str, count: u8) -> anyhow::Result<Vec<SearchResult>> {
        let url = "https://api.search.brave.com/res/v1/web/search";

        let res = self.http
            .get(url)
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await?;

        let json_res: serde_json::Value = res.json().await?;

        let results: Vec<SearchResult> = json_res["web"]["results"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|item| SearchResult {
                title: item["title"].as_str().unwrap_or("").to_string(),
                url: item["url"].as_str().unwrap_or("").to_string(),
                description: item["description"].as_str().unwrap_or("").to_string(),
            })
            .collect();

        Ok(results)
    }
}
