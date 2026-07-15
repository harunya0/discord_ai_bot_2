pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}

pub fn search_similar<'a>(
    candidates: &'a [(String, Vec<f32>)],
    query_embedding: &[f32],
    top_k: usize,
) -> Vec<&'a str> {
    let mut scored: Vec<(&str, f32)> = candidates
        .iter()
        .map(|(text, emb)| (text.as_str(), cosine_similarity(emb, query_embedding)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.into_iter().take(top_k).map(|(text, _)| text).collect()
}