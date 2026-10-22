//! Bounded, deduplicated retrieval evidence. Core instructions never enter this budget.
use std::collections::HashSet;

pub(super) struct Evidence {
    remaining: usize,
    seen: HashSet<String>,
    pub text: String,
}

impl Evidence {
    pub fn new() -> Self {
        Self { remaining: 12_000, seen: HashSet::new(), text: String::new() }
    }

    pub fn push(&mut self, source: &str, body: &str) {
        let body = body.trim();
        if body.is_empty() || self.seen.contains(body) { return; }
        let header = format!("\n{source}:\n");
        let overhead = header.chars().count() + 20;
        if self.remaining <= overhead { return; }
        self.seen.insert(body.to_string());
        let limit = 2_400.min(self.remaining - overhead);
        let excerpt: String = body.chars().take(limit).collect();
        self.remaining = self.remaining.saturating_sub(overhead + excerpt.chars().count());
        self.text.push_str(&header);
        self.text.push_str(&excerpt);
        if body.chars().count() > limit {
            self.text.push_str("\n[片段截断，按来源读取全文]");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_bounded_unicode_safe_and_deduplicated() {
        let mut evidence = Evidence::new();
        evidence.push("src/a.rs:1-3", "相同内容");
        evidence.push("another index", "相同内容");
        assert_eq!(evidence.text.matches("相同内容").count(), 1);
        for i in 0..30 { evidence.push(&format!("file{i}"), &format!("{i}{}", "中".repeat(5000))); }
        assert!(evidence.text.chars().count() <= 12_000);
        assert!(evidence.text.contains("按来源读取全文"));
        assert!(evidence.text.contains("src/a.rs:1-3"));
    }
}
