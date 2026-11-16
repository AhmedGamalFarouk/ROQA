//! A stopwatch with laps. It is only a start instant and the time banked before it: no thread, no wake of its
//! own. The Timer page shows it through `timer::View` when no countdown does, so the collapsed pill and the
//! page's controls work as they do for a timer. Not stored: a restart forgets it, like a paused timer.

use crate::timer::{View, clock};
use std::time::{Duration, Instant};

/// Laps kept (oldest drop off) and laps the page shows.
const MAX_LAPS: usize = 20;
const SHOWN: usize = 3;

#[derive(Debug, Default)]
pub struct Stopwatch {
    /// Started and not reset (it may be paused).
    on: bool,
    /// When the running stretch began; `None` while paused.
    since: Option<Instant>,
    /// Time from the stretches before `since`.
    banked: Duration,
    /// Each lap's own length in seconds, oldest first, and the elapsed seconds at the last lap.
    laps: Vec<u64>,
    last: u64,
    /// Laps closed since the reset, so a lap keeps its number after older ones drop off.
    taken: usize,
}

impl Stopwatch {
    /// Starts it, or resumes it after a pause.
    pub fn start(&mut self, now: Instant) {
        self.on = true;
        self.since.get_or_insert(now);
    }
    pub fn pause(&mut self, now: Instant) {
        if let Some(since) = self.since.take() {
            self.banked += now.saturating_duration_since(since);
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Closes a lap; only while it runs.
    pub fn lap(&mut self, now: Instant) {
        if self.since.is_none() {
            return;
        }
        let at = self.secs(now);
        self.laps.push(at.saturating_sub(self.last));
        self.last = at;
        self.taken += 1;
        if self.laps.len() > MAX_LAPS {
            self.laps.remove(0);
        }
    }
    pub fn running(&self) -> bool {
        self.since.is_some()
    }
    fn secs(&self, now: Instant) -> u64 {
        (self.banked
            + self
                .since
                .map_or(Duration::ZERO, |s| now.saturating_duration_since(s)))
        .as_secs()
    }
    /// When the seconds display next changes, while it runs.
    pub fn next_tick(&self, now: Instant) -> Option<Instant> {
        self.since.map(|since| {
            let ms = (self.banked + now.saturating_duration_since(since)).subsec_millis();
            now + Duration::from_millis(u64::from(1000 - ms))
        })
    }
    /// The Timer page's content; `None` until it is started.
    pub fn view(&self, now: Instant) -> Option<View> {
        if !self.on {
            return None;
        }
        let secs = self.secs(now);
        let first = self.taken - self.laps.len();
        let laps = self
            .laps
            .iter()
            .enumerate()
            .rev()
            .take(SHOWN)
            .map(|(index, lap)| format!("Lap {} {}", first + index + 1, clock(*lap)))
            .collect::<Vec<_>>()
            .join("   ");
        Some(View {
            pill: format!("{}m", secs / 60),
            panel: clock(secs),
            name: format!("Stopwatch, {} elapsed", clock(secs)),
            done: false,
            tagged: false,
            paused: self.since.is_none(),
            label: "Stopwatch".to_owned(),
            left: secs,
            total: 0,
            cycle: None,
            watch: true,
            laps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{Ctl, controls};

    #[test]
    fn it_counts_pauses_laps_and_resets() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut watch = Stopwatch::default();
        assert!(watch.view(t0).is_none());
        watch.lap(t0); // not running: ignored
        watch.start(t0);
        assert_eq!(watch.view(at(65)).unwrap().panel, "1:05");
        assert_eq!(watch.view(at(65)).unwrap().pill, "1m");
        let running = watch.view(at(65));
        assert_eq!(
            controls(running.as_ref()),
            [Ctl::Lap, Ctl::Pause, Ctl::Reset]
        );
        watch.lap(at(40));
        watch.lap(at(70));
        watch.pause(at(100));
        // Paused time does not count, and the page offers Resume.
        let paused = watch.view(at(500)).unwrap();
        assert_eq!((paused.panel.as_str(), paused.paused), ("1:40", true));
        assert_eq!(paused.laps, "Lap 2 0:30   Lap 1 0:40");
        assert_eq!(controls(Some(&paused)), [Ctl::Resume, Ctl::Reset]);
        assert_eq!(watch.next_tick(at(500)), None);
        watch.start(at(500));
        assert_eq!(watch.view(at(510)).unwrap().panel, "1:50");
        assert!(watch.next_tick(at(510)).unwrap() > at(510));
        watch.reset();
        assert!(watch.view(at(600)).is_none() && !watch.running());
    }

    #[test]
    fn only_the_newest_laps_are_kept_and_shown() {
        let t0 = Instant::now();
        let mut watch = Stopwatch::default();
        watch.start(t0);
        for n in 1..=(MAX_LAPS as u64 + 5) {
            watch.lap(t0 + Duration::from_secs(n * 10));
        }
        assert_eq!(watch.laps.len(), MAX_LAPS);
        let view = watch.view(t0 + Duration::from_secs(400)).unwrap();
        assert!(
            view.laps.starts_with(&format!("Lap {} 0:10", MAX_LAPS + 5)),
            "{}",
            view.laps
        );
        assert_eq!(view.laps.matches("Lap").count(), SHOWN);
    }
}
