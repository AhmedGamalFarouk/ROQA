use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(50);
const REPAIR: Duration = Duration::from_secs(30);

pub struct CatalogTiming {
    settle: Option<Instant>,
    repair: Instant,
    retry: Option<Instant>,
    failures: u32,
}
impl CatalogTiming {
    pub fn new(now: Instant) -> Self {
        Self {
            settle: None,
            repair: now + REPAIR,
            retry: None,
            failures: 0,
        }
    }
    pub fn arm(&mut self, now: Instant) {
        self.settle.get_or_insert(now + SETTLE);
    }
    pub fn repair_due(&mut self, now: Instant) -> bool {
        if now < self.repair {
            return false;
        }
        self.repair = now + REPAIR;
        true
    }
    pub fn completed(&mut self, complete: bool, now: Instant) {
        if complete {
            self.failures = 0;
            self.retry = None;
        } else {
            self.failures = (self.failures + 1).min(6);
            self.retry = Some(now + Duration::from_secs((1 << (self.failures - 1)).min(30)));
            self.arm(now);
        }
    }
    fn refresh_deadline(&self) -> Option<Instant> {
        self.settle
            .map(|settle| self.retry.map_or(settle, |retry| settle.max(retry)))
    }
    pub fn refresh_due(&mut self, now: Instant) -> bool {
        if self
            .refresh_deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            self.settle = None;
            true
        } else {
            false
        }
    }
    pub fn wait_milliseconds(&self, now: Instant) -> u32 {
        let next = self
            .refresh_deadline()
            .map_or(self.repair, |deadline| deadline.min(self.repair));
        // Round up: a sub-millisecond remainder must not turn the idle wait into a busy loop.
        next.saturating_duration_since(now)
            .as_nanos()
            .div_ceil(1_000_000)
            .min(u32::MAX as u128) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_traffic_cannot_postpone_refresh_or_repair() {
        let start = Instant::now();
        let mut timing = CatalogTiming::new(start);
        timing.arm(start);
        for milliseconds in 1..50 {
            timing.arm(start + Duration::from_millis(milliseconds));
            assert!(!timing.refresh_due(start + Duration::from_millis(milliseconds)));
        }
        assert!(timing.refresh_due(start + SETTLE));
        assert!(!timing.refresh_due(start + SETTLE));
        assert!(timing.repair_due(start + REPAIR));
        assert!(!timing.repair_due(start + REPAIR));
        assert_eq!(timing.wait_milliseconds(start + REPAIR), 30_000);
    }
    #[test]
    fn incomplete_retries_back_off_despite_new_events_and_reset_on_success() {
        let start = Instant::now();
        let mut timing = CatalogTiming::new(start);
        let mut now = start;
        for seconds in [1, 2, 4, 8, 16, 30, 30] {
            timing.completed(false, now);
            timing.arm(now + SETTLE);
            assert!(!timing.refresh_due(now + Duration::from_millis(999)));
            assert!(
                !timing.refresh_due(now + Duration::from_secs(seconds) - Duration::from_millis(1))
            );
            now += Duration::from_secs(seconds);
            assert!(timing.refresh_due(now));
        }
        timing.completed(true, now);
        timing.completed(false, now);
        assert!(timing.refresh_due(now + Duration::from_secs(1)));
    }
    #[test]
    fn wait_rounds_up_and_followup_keeps_fifty_millisecond_batching() {
        let start = Instant::now();
        let mut timing = CatalogTiming::new(start);
        timing.arm(start);
        assert_eq!(
            timing.wait_milliseconds(start + Duration::from_micros(49_999)),
            1
        );
        assert!(timing.refresh_due(start + SETTLE));
        timing.arm(start + SETTLE);
        assert!(!timing.refresh_due(start + SETTLE + Duration::from_millis(49)));
        assert!(timing.refresh_due(start + SETTLE + SETTLE));
    }
}
