use std::collections::HashSet;

/// Tokenize text into lowercase alphanumeric-ish tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            current.push(c.to_lowercase().next().unwrap_or(c));
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub path: String,
    pub score: f64,
    pub snippet: String,
}

/// TF-IDF cosine ranking of documents against a query.
/// `docs` are (path, tokens) pairs.
pub fn tfidf(docs: &[(String, Vec<String>)], query: &[String]) -> Vec<Hit> {
    let n = docs.len().max(1) as f64;
    let mut df: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for (_, tokens) in docs {
        let mut seen = HashSet::new();
        for t in tokens {
            if seen.insert(t.as_str()) {
                *df.entry(t.as_str()).or_default() += 1;
            }
        }
    }

    let idf = |t: &str| ((n + 1.0) / (df.get(t).copied().unwrap_or(0) as f64 + 0.5)).ln();

    let q_vec: std::collections::HashMap<String, f64> = query
        .iter()
        .fold(std::collections::HashMap::<String, f64>::new(), |mut acc, t| {
            *acc.entry(t.clone()).or_default() += 1.0;
            acc
        })
        .into_iter()
        .map(|(t, tf)| (t.clone(), tf * idf(t.as_str())))
        .collect();
    let q_norm = q_vec.values().map(|w| w * w).sum::<f64>().sqrt();

    let mut hits = Vec::new();
    for (path, tokens) in docs {
        if tokens.is_empty() {
            continue;
        }
        let mut tf: std::collections::HashMap<&str, f64> =
            std::collections::HashMap::new();
        for t in tokens {
            *tf.entry(t.as_str()).or_default() += 1.0;
        }
        let d_norm = tf.values().map(|w| w * w).sum::<f64>().sqrt();
        let mut dot = 0.0;
        for (t, w) in &q_vec {
            if let Some(d) = tf.get(t.as_str()) {
                dot += w * d;
            }
        }
        if dot <= 0.0 || q_norm == 0.0 || d_norm == 0.0 {
            continue;
        }
        hits.push(Hit {
            path: path.clone(),
            score: dot / (q_norm * d_norm),
            snippet: String::new(),
        });
    }
    hits
}

/// First line containing any query token, trimmed to `max` chars.
pub fn snippet(content: &str, query: &[String], max: usize) -> String {
    for line in content.lines() {
        let lower = line.to_lowercase();
        if query.iter().any(|t| lower.contains(t.as_str())) {
            let line = line.trim();
            return if line.len() <= max {
                line.to_string()
            } else {
                let mut cut = max;
                while !line.is_char_boundary(cut) {
                    cut -= 1;
                }
                format!("{}…", &line[..cut])
            };
        }
    }
    content.lines().next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes() {
        assert_eq!(
            tokenize("Hello, World! rust_mcp 42"),
            vec!["hello", "world", "rust", "mcp", "42"]
        );
    }

    #[test]
    fn ranks_relevant_doc_first() {
        let docs = vec![
            ("a".into(), tokenize("the cat sat on the mat")),
            ("b".into(), tokenize("rust is a fast systems language for rustaceans")),
            ("c".into(), tokenize("cooking pasta with rust flour")),
        ];
        let hits = tfidf(&docs, &tokenize("rust fast systems"));
        assert_eq!(hits[0].path, "b");
    }

    #[test]
    fn snippets_match_token() {
        let s = snippet("intro line\nabout rust and mcp\nend", &["rust".to_string()], 80);
        assert_eq!(s, "about rust and mcp");
    }
}
