use crate::models::TokenStatistics;
use std::{collections::VecDeque, time::Duration};

/// Same lightweight text estimate used by the usage page: ceil(ASCII / 4) + Unicode scalars.
#[derive(Default)]
struct TextCounter {
    ascii: u64,
    other: u64,
}
impl TextCounter {
    fn push(&mut self, text: &str) {
        for ch in text.chars() {
            if ch.is_ascii() {
                self.ascii += 1;
            } else {
                self.other += 1;
            }
        }
    }
    fn tokens(&self) -> u64 {
        (self.ascii + 3) / 4 + self.other
    }
}

pub(super) struct TokenTracker {
    stats: TokenStatistics,
    content: TextCounter,
    reasoning: TextCounter,
    first: Option<Duration>,
    last: Option<Duration>,
    previous_ms: u64,
    samples: VecDeque<(Duration, u64)>,
}

impl TokenTracker {
    pub fn new(model: String, model_config_id: String) -> Self {
        Self {
            stats: TokenStatistics {
                model,
                model_config_id,
                status: "streaming".into(),
                ..Default::default()
            },
            content: TextCounter::default(),
            reasoning: TextCounter::default(),
            first: None,
            last: None,
            previous_ms: 0,
            samples: VecDeque::new(),
        }
    }
    pub fn start_segment(&mut self) {
        self.stats.input_tokens = None;
        self.stats.output_tokens = None;
        self.previous_ms = self.generation_ms();
        self.first = None;
        self.last = None;
        self.samples.clear();
    }
    pub fn delta(&mut self, text: &str, reasoning: bool, now: Duration) {
        if text.is_empty() {
            return;
        }
        self.first.get_or_insert(now);
        self.last = Some(now);
        let before = self.stats.estimated_output_tokens;
        if reasoning {
            self.reasoning.push(text);
        } else {
            self.content.push(text);
        }
        self.stats.estimated_output_tokens = self.content.tokens() + self.reasoning.tokens();
        self.samples
            .push_back((now, self.stats.estimated_output_tokens - before));
    }
    pub fn usage(&mut self, input: Option<u64>, output: Option<u64>) {
        self.stats.input_tokens = input;
        self.stats.output_tokens = output;
    }
    fn generation_ms(&self) -> u64 {
        self.previous_ms
            + self
                .first
                .zip(self.last)
                .map(|(first, last)| last.saturating_sub(first).as_millis() as u64)
                .unwrap_or(0)
    }
    pub fn snapshot(&mut self, now: Duration, status: &str) -> TokenStatistics {
        let cutoff = now.saturating_sub(Duration::from_secs(2));
        while now >= Duration::from_secs(2)
            && self
                .samples
                .front()
                .is_some_and(|(time, _)| *time <= cutoff)
        {
            self.samples.pop_front();
        }
        let elapsed = self
            .first
            .map(|first| now.saturating_sub(first).as_secs_f64().min(2.0));
        self.stats.tokens_per_second = elapsed
            .filter(|s| *s > 0.0)
            .map(|s| self.samples.iter().map(|(_, tokens)| *tokens).sum::<u64>() as f64 / s);
        self.stats.generation_ms = self.generation_ms();
        self.stats.average_tokens_per_second = (self.stats.generation_ms > 0).then(|| {
            self.stats.estimated_output_tokens as f64 * 1000.0 / self.stats.generation_ms as f64
        });
        self.stats.status = status.into();
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tracker() -> TokenTracker {
        TokenTracker::new("model".into(), "config".into())
    }
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }
    #[test]
    fn estimates_are_independent_of_chunk_boundaries_and_include_reasoning() {
        let mut whole = tracker();
        whole.delta("你好abcdefgh😀", false, ms(100));
        whole.delta("思考abcd", true, ms(200));
        let mut chunks = tracker();
        for ch in "你好abcdefgh😀".chars() {
            chunks.delta(&ch.to_string(), false, ms(100));
        }
        chunks.delta("思考abcd", true, ms(200));
        assert_eq!(
            whole.snapshot(ms(200), "completed").estimated_output_tokens,
            8
        );
        assert_eq!(
            chunks
                .snapshot(ms(200), "completed")
                .estimated_output_tokens,
            8
        );
    }
    #[test]
    fn rolling_speed_decays_while_idle_and_average_excludes_wait_and_usage_tail() {
        let mut t = tracker();
        assert!(t
            .snapshot(ms(5000), "streaming")
            .tokens_per_second
            .is_none());
        t.delta("你好", false, ms(10000));
        assert!(t
            .snapshot(ms(10000), "streaming")
            .average_tokens_per_second
            .is_none());
        t.delta("世界", false, ms(11000));
        assert_eq!(
            t.snapshot(ms(11000), "streaming").tokens_per_second,
            Some(4.0)
        );
        let stats = t.snapshot(ms(14000), "completed");
        assert_eq!(stats.tokens_per_second, Some(0.0));
        assert_eq!(stats.average_tokens_per_second, Some(4.0));
        assert_eq!(stats.generation_ms, 1000);
    }
    #[test]
    fn continuations_exclude_network_gaps_and_usage_is_not_added_twice() {
        let mut t = tracker();
        t.delta("你好", false, ms(0));
        t.delta("世界", false, ms(1000));
        t.start_segment();
        t.delta("你好", false, ms(10000));
        t.delta("世界", false, ms(11000));
        t.usage(Some(100), Some(50));
        t.usage(Some(100), Some(50));
        let stats = t.snapshot(ms(12000), "interrupted");
        assert_eq!(stats.input_tokens, Some(100));
        assert_eq!(stats.output_tokens, Some(50));
        assert_eq!(stats.generation_ms, 2000);
        assert_eq!(stats.average_tokens_per_second, Some(4.0));
        assert_eq!(stats.status, "interrupted");
    }
}
