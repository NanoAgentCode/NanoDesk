//! Query-time lexical ranking over existing document chunks; no embedding reindex.
use crate::models::RagChunkMatch;
use std::collections::{BTreeMap, HashMap, HashSet};

fn cjk(ch: char) -> bool {
    matches!(ch, '\u{3400}'..='\u{9fff}' | '\u{20000}'..='\u{323af}')
}

fn tokens(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut word = String::new();
    let mut previous = None;
    fn flush(word: &mut String, output: &mut Vec<String>) {
        if word.is_empty() {
            return;
        }
        let token = word.trim_matches(['-', '_', '.']).to_string();
        if !token.is_empty() {
            output.push(token.clone());
            if token.contains(['-', '_', '.']) {
                output.extend(
                    token
                        .split(['-', '_', '.'])
                        .filter(|part| !part.is_empty())
                        .map(str::to_string),
                );
            }
        }
        word.clear();
    }
    for ch in text.chars().flat_map(char::to_lowercase) {
        if cjk(ch) {
            flush(&mut word, &mut output);
            output.push(ch.to_string());
            if let Some(left) = previous {
                output.push(format!("{left}{ch}"));
            }
            previous = Some(ch);
        } else {
            previous = None;
            if ch.is_alphanumeric() || ['-', '_', '.'].contains(&ch) {
                word.push(ch);
            } else {
                flush(&mut word, &mut output);
            }
        }
    }
    flush(&mut word, &mut output);
    output
}

pub(crate) fn lexical_ranks(chunks: &[RagChunkMatch], query: &str) -> Vec<usize> {
    let terms = tokens(query).into_iter().collect::<HashSet<_>>();
    if terms.is_empty() || chunks.is_empty() {
        return Vec::new();
    }
    let mut frequency = HashMap::<String, usize>::new();
    let mut documents = Vec::new();
    let mut total_length = 0;
    for chunk in chunks {
        let tokens = tokens(&format!("{}\n{}", chunk.file_name, chunk.text));
        let length = tokens.len();
        total_length += length;
        let mut counts = BTreeMap::<String, usize>::new();
        for token in tokens.into_iter().filter(|token| terms.contains(token)) {
            *counts.entry(token).or_default() += 1;
        }
        for term in counts.keys() {
            *frequency.entry(term.clone()).or_default() += 1;
        }
        documents.push((length, counts));
    }
    let average = (total_length as f64 / chunks.len() as f64).max(1.0);
    let mut scores = documents
        .iter()
        .enumerate()
        .filter_map(|(index, (length, counts))| {
            let score = counts
                .iter()
                .map(|(term, count)| {
                    let df = frequency[term] as f64;
                    let idf = (1.0 + (chunks.len() as f64 - df + 0.5) / (df + 0.5)).ln();
                    let tf = *count as f64;
                    idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * *length as f64 / average))
                })
                .sum::<f64>();
            (score > 0.0).then_some((index, score))
        })
        .collect::<Vec<_>>();
    scores.sort_by(|(left, a), (right, b)| {
        b.total_cmp(a)
            .then_with(|| chunks[*left].chunk_id.cmp(&chunks[*right].chunk_id))
    });
    scores.into_iter().map(|(index, _)| index).collect()
}

pub(crate) fn fuse(
    vector: Vec<RagChunkMatch>,
    chunks: &[RagChunkMatch],
    lexical: Vec<usize>,
    limit: usize,
) -> Vec<RagChunkMatch> {
    let mut candidates = HashMap::new();
    for (rank, mut hit) in vector.into_iter().enumerate() {
        hit.score = 1.0 / (60.0 + rank as f32 + 1.0);
        candidates.insert(hit.chunk_id.clone(), (hit, usize::MAX));
    }
    for (rank, index) in lexical.into_iter().take(20).enumerate() {
        let hit = chunks[index].clone();
        let entry = candidates.entry(hit.chunk_id.clone()).or_insert_with(|| {
            let mut hit = hit;
            hit.score = 0.0;
            (hit, rank)
        });
        entry.0.score += 1.0 / (60.0 + rank as f32 + 1.0);
        entry.1 = rank;
    }
    let mut results = candidates.into_values().collect::<Vec<_>>();
    results.sort_by(|(left, a), (right, b)| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| a.cmp(b))
            .then_with(|| left.chunk_id.cmp(&right.chunk_id))
    });
    results
        .into_iter()
        .take(limit)
        .map(|(hit, _)| hit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keywords_are_literal_bounded_words_and_cjk_bigrams_not_query_syntax() {
        let terms = tokens("供应链 ZX-8972 \" OR * (测试)");
        for term in ["供应", "应链", "zx-8972", "zx", "8972", "测试"] {
            assert!(terms.contains(&term.to_string()), "{term}");
        }
        assert!(tokens("\"*() :").is_empty());
    }
}
