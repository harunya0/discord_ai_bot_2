use serde_json::json;

pub fn to_openai_messages(contents: &[serde_json::Value]) -> Vec<serde_json::Value> {
    contents.iter().map(|c| {
        let role = c["role"].as_str().unwrap_or("user");
        let openai_role = if role == "model" { "assistant" } else { "user" };

        let parts = c["parts"].as_array().cloned().unwrap_or_default();

        // テキストのみ1個の場合はシンプルな文字列content
        if parts.len() == 1 {
            if let Some(text) = parts[0]["text"].as_str() {
                return json!({ "role": openai_role, "content": text });
            }
        }

        // テキスト+画像混在の場合は配列形式
        let content: Vec<serde_json::Value> = parts.iter().filter_map(|p| {
            if let Some(text) = p["text"].as_str() {
                Some(json!({ "type": "text", "text": text }))
            } else if let Some(inline) = p.get("inlineData") {
                let mime = inline["mimeType"].as_str().unwrap_or("image/png");
                let data = inline["data"].as_str().unwrap_or("");
                Some(json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{};base64,{}", mime, data) }
                }))
            } else {
                None
            }
        }).collect();

        json!({ "role": openai_role, "content": content })
    }).collect()
}