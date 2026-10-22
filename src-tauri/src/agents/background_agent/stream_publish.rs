use std::time::Duration;

/// Limits observer snapshots only; model deltas are always accumulated in full.
#[derive(Default)]
pub(super) struct StreamPublishGate {
    last: Option<Duration>,
}

impl StreamPublishGate {
    pub fn ready(&mut self, elapsed: Duration) -> bool {
        if self.last.is_some_and(|last| elapsed.saturating_sub(last) < Duration::from_millis(50)) {
            return false;
        }
        self.last = Some(elapsed);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publishes_first_delta_and_bounds_burst_frequency() {
        let mut gate = StreamPublishGate::default();
        let published: Vec<_> = (0..=1000).filter(|ms| gate.ready(Duration::from_millis(*ms))).collect();
        assert_eq!(published, (0..=20).map(|n| n * 50).collect::<Vec<_>>());
    }
    #[test]
    fn allows_a_delta_after_an_idle_period() {
        let mut gate = StreamPublishGate::default();
        assert!(gate.ready(Duration::ZERO));
        assert!(!gate.ready(Duration::from_millis(1)));
        assert!(gate.ready(Duration::from_secs(5)));
    }
}
