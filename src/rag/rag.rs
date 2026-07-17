pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}

pub fn search_similar_with_decay<'a>(
    candidates: &'a [(String, Vec<f32>, i64)],  // (text, embedding, created_at)
    query_embedding: &[f32],
    top_k: usize,
    half_life_days: f32,
) -> Vec<&'a str> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let mut scored: Vec<(&str, f32)> = candidates
        .iter()
        .map(|(text, emb, created_at)| {
            let similarity = cosine_similarity(emb, query_embedding);
            let age_days = ((now - created_at).max(0) as f32) / 86400.0;
            let decay = 0.5_f32.powf(age_days / half_life_days);
            (text.as_str(), similarity * decay)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.into_iter().take(top_k).map(|(text, _)| text).collect()
}