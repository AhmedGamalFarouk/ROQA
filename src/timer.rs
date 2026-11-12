//! Countdown timers (M5 S5): a small pure set that the host's existing wait minimum drives.
//!
//! Each timer has a monotonic `deadline` (what the host waits on) and an absolute wall-clock `expiry`
//! (Unix seconds, what the store keeps). A clock change moves the expiry and leaves the deadline, so a
//! countdown keeps its remaining time; a restart rebuilds the deadline from the stored expiry. There is
//! no thread, wait handle or `WM_TIMER`: with no timers `next_wake` is `None` and nothing wakes.

use crate::store::{Pending, Store};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// At most this many timers run at once (UNVERIFIED default from m5-preparation.md section 4).
pub const MAX: usize = 3;
/// The notch's idle Timer page buttons, in minutes.
pub const PRESETS: [u64; 4] = [5, 10, 15, 25];
/// Longest countdown; also rejects a corrupt or clock-skewed stored expiry.
pub const MAX_SECONDS: u64 = 24 * 60 * 60;
/// How long a completion keeps the notch open before it closes by itself.
pub const ALERT: Duration = Duration::from_secs(10);
/// Wall-versus-monotonic drift up to this is rounding or wake jitter, not a clock change.
const CLOCK_SLACK: u64 = 2;

#[derive(Debug)]
struct Timer {
    deadline: Instant,
    expiry: u64,
    /// A Pomodoro phase's name (not stored: after a restart it is a plain timer).
    tag: Option<&'static str>,
}

/// Running timers, soonest first, and the end of the last completion's alert, if one is showing.
#[derive(Debug, Default)]
pub struct Timers {
    list: Vec<Timer>,
    alert: Option<Instant>,
    pomo_done: bool,
}

/// What the notch shows and UI Automation says for one paint.
#[derive(Debug, PartialEq, Eq)]
pub struct View {
    /// Collapsed pill: whole minutes left on the soonest timer, so it changes no faster than the clock.
    pub pill: String,
    /// Expanded panel: the soonest timer as `m:ss`, plus `+N` for the others.
    pub panel: String,
    /// The accessible name part, such as "Timer, 4:32 remaining".
    pub name: String,
    /// A timer just finished and none is left running: the panel says so, in the accent colour.
    pub done: bool,
}

pub fn wall_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn secs_left(deadline: Instant, now: Instant) -> u64 {
    deadline
        .saturating_duration_since(now)
        .as_millis()
        .div_ceil(1000) as u64
}

/// How long until the seconds display changes: the next whole-second boundary of the time left.
fn tick_in(left: Duration) -> Duration {
    Duration::from_millis(match left.subsec_millis() {
        0 => 1000,
        ms => ms.into(),
    })
}

fn clock(secs: u64) -> String {
    let (hours, minutes, seconds) = (secs / 3600, secs % 3600 / 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

impl Timers {
    /// Rebuilds timers from stored expiries after a start. Expired, absurdly distant and surplus ones
    /// are dropped, and nothing announces a timer that ran out while ROQA was not running.
    pub fn restore(expiries: &[u64], wall: u64, now: Instant) -> Self {
        let mut list: Vec<Timer> = expiries
            .iter()
            .filter_map(|&expiry| {
                let left = expiry
                    .checked_sub(wall)
                    .filter(|left| (1..=MAX_SECONDS).contains(left))?;
                let deadline = now + Duration::from_secs(left);
                Some(Timer {
                    deadline,
                    expiry,
                    tag: None,
                })
            })
            .take(MAX)
            .collect();
        crate::sort::by(&mut list, |a, b| a.deadline.cmp(&b.deadline));
        Self {
            list,
            alert: None,
            pomo_done: false,
        }
    }

    /// Starts a countdown; false when it is out of range or `MAX` are already running.
    pub fn start(&mut self, seconds: u64, wall: u64, now: Instant) -> bool {
        self.start_as(seconds, wall, now, None)
    }

    /// `start`, naming the countdown (a Pomodoro phase).
    pub fn start_as(
        &mut self,
        seconds: u64,
        wall: u64,
        now: Instant,
        tag: Option<&'static str>,
    ) -> bool {
        if !(1..=MAX_SECONDS).contains(&seconds) || self.list.len() >= MAX {
            return false;
        }
        self.list.push(Timer {
            deadline: now + Duration::from_secs(seconds),
            expiry: wall.saturating_add(seconds),
            tag,
        });
        crate::sort::by(&mut self.list, |a, b| a.deadline.cmp(&b.deadline));
        true
    }

    /// The wall clock changed (or the machine resumed): keep each countdown's remaining time and move
    /// its stored expiry. Returns whether any expiry moved by more than rounding.
    pub fn resync(&mut self, wall: u64, now: Instant) -> bool {
        let mut moved = false;
        for timer in &mut self.list {
            let expiry = wall.saturating_add(secs_left(timer.deadline, now));
            if expiry.abs_diff(timer.expiry) > CLOCK_SLACK {
                (timer.expiry, moved) = (expiry, true);
            }
        }
        moved
    }

    /// Removes timers whose deadline passed and starts the alert; true exactly once per completion.
    pub fn take_done(&mut self, now: Instant) -> bool {
        let before = self.list.len();
        self.pomo_done |= self
            .list
            .iter()
            .any(|timer| timer.tag.is_some() && timer.deadline <= now);
        self.list.retain(|timer| timer.deadline > now);
        let done = self.list.len() < before;
        if done {
            self.alert = Some(now + ALERT);
        }
        done
    }

    /// True once after a Pomodoro phase ran out (`take_done` saw it).
    pub fn take_pomo_done(&mut self) -> bool {
        std::mem::take(&mut self.pomo_done)
    }

    /// Stops the Pomodoro countdown and returns the seconds it had left.
    pub fn stop_pomo(&mut self, now: Instant) -> Option<u64> {
        let at = self.list.iter().position(|timer| timer.tag.is_some())?;
        Some(secs_left(self.list.remove(at).deadline, now).max(1))
    }

    /// A completion's alert is showing (the mascot is surprised while it does).
    pub fn finished(&self) -> bool {
        self.alert.is_some()
    }

    /// True once when the alert has run its course.
    pub fn alert_over(&mut self, now: Instant) -> bool {
        let over = self.alert.is_some_and(|until| now >= until);
        if over {
            self.alert = None;
        }
        over
    }

    /// The next instant the host must wake for timers: the soonest completion, the alert's end and, only
    /// while a notch is visible and expanded, the next change of the seconds display. `None` means no
    /// timer work at all, so the host adds no deadline.
    pub fn next_wake(&self, expanded: bool, now: Instant) -> Option<Instant> {
        let first = self.list.first().map(|timer| timer.deadline);
        let tick = first
            .filter(|_| expanded)
            .map(|due| now + tick_in(due.saturating_duration_since(now)));
        [first, self.alert, tick].into_iter().flatten().min()
    }

    /// Mirrors the timers into the store's section and marks it dirty only when it changed.
    pub fn write_to(&self, store: &mut Store, pending: &mut Pending, now: Instant) {
        let expiries: Vec<u64> = self.list.iter().map(|timer| timer.expiry).collect();
        if store.timers != expiries {
            store.timers = expiries;
            pending.mark(now);
        }
    }

    /// The text for this paint; `None` with no timer running and no alert showing.
    pub fn view(&self, now: Instant) -> Option<View> {
        let left: Vec<u64> = self
            .list
            .iter()
            .map(|timer| secs_left(timer.deadline, now))
            .collect();
        let finished = self.alert.is_some();
        if left.is_empty() && !finished {
            return None;
        }
        let (soonest, done) = (left.first().copied().unwrap_or(0), left.is_empty());
        let others = left.len().saturating_sub(1);
        let mut names: Vec<String> = finished
            .then(|| "Timer finished".to_owned())
            .into_iter()
            .collect();
        names.extend(left.iter().enumerate().map(|(index, secs)| {
            let which = if let Some(tag) = self.list[index].tag {
                tag.to_owned()
            } else if left.len() > 1 {
                format!("Timer {}", index + 1)
            } else {
                "Timer".to_owned()
            };
            format!("{which}, {} remaining", clock(*secs))
        }));
        Some(View {
            pill: format!("{}m", soonest.div_ceil(60)),
            panel: match others {
                _ if done => "Timer finished".to_owned(),
                0 => clock(soonest),
                n => format!("{} +{n}", clock(soonest)),
            },
            name: names.join("; "),
            done,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALL: u64 = 1_800_000_000;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn deadline_math_follows_the_stored_expiry_and_drops_bad_entries() {
        let now = Instant::now();
        let timers = Timers::restore(
            &[
                WALL + 300,
                WALL + 60,
                WALL,                   // already expired
                WALL - 5,               // expired earlier
                WALL + MAX_SECONDS + 1, // clock set back or corrupt
                u64::MAX,               // must not overflow
            ],
            WALL,
            now,
        );
        assert_eq!(
            timers.list.iter().map(|t| t.deadline).collect::<Vec<_>>(),
            [now + secs(60), now + secs(300)] // soonest first
        );
        assert_eq!(secs_left(now + secs(60), now), 60);
        assert_eq!(secs_left(now + Duration::from_millis(60_001), now), 61); // never shows 0 early
        assert_eq!(secs_left(now, now + secs(1)), 0);
        assert!(
            timers.alert.is_none(),
            "nothing is announced after a restart"
        );
    }

    #[test]
    fn a_tagged_phase_is_named_paused_with_its_remainder_and_reported_once_when_done() {
        let now = Instant::now();
        let mut timers = Timers::default();
        assert!(timers.start_as(600, WALL, now, Some("Focus")));
        assert!(timers.start(900, WALL, now));
        assert_eq!(
            timers.view(now).unwrap().name,
            "Focus, 10:00 remaining; Timer 2, 15:00 remaining"
        );
        // Pausing removes only the tagged one and keeps what was left.
        assert_eq!(timers.stop_pomo(now + secs(100)), Some(500));
        assert_eq!(timers.stop_pomo(now), None);
        assert_eq!(timers.list.len(), 1);
        assert!(timers.start_as(5, WALL, now, Some("Short break")));
        assert!(!timers.take_pomo_done());
        assert!(timers.take_done(now + secs(6)));
        assert!(timers.take_pomo_done() && !timers.take_pomo_done());
        // An ordinary timer finishing is not a phase.
        assert!(timers.take_done(now + secs(901)) && !timers.take_pomo_done());
    }

    #[test]
    fn the_cap_is_three_and_ranges_are_enforced() {
        let now = Instant::now();
        let mut timers = Timers::default();
        assert!(!timers.start(0, WALL, now));
        assert!(!timers.start(MAX_SECONDS + 1, WALL, now));
        for n in 0..MAX as u64 {
            assert!(timers.start(600 - n, WALL, now));
        }
        assert!(!timers.start(5, WALL, now), "a fourth is refused");
        assert_eq!(timers.list.len(), MAX);
        assert!(timers.list[0].deadline <= timers.list[1].deadline);
        // Restoring a file with surplus timers keeps only the first MAX valid ones.
        let many: Vec<u64> = (1..=10).map(|n| WALL + n * 60).collect();
        assert_eq!(Timers::restore(&many, WALL, now).list.len(), MAX);
    }

    #[test]
    fn timers_survive_a_simulated_restart_through_the_store() {
        let now = Instant::now();
        let mut timers = Timers::default();
        assert!(timers.start(900, WALL, now) && timers.start(120, WALL, now));
        let (mut store, mut pending) = (Store::default(), Pending::default());
        timers.write_to(&mut store, &mut pending, now);
        assert_eq!(store.timers, [WALL + 120, WALL + 900]);
        assert_ne!(pending.decision(now), crate::store::Flush::Idle);
        let path =
            std::env::temp_dir().join(format!("roqa-timer-test-{}.roqa", std::process::id()));
        store.save(&path).unwrap();
        let (loaded, state) = crate::store::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(state, crate::store::Load::Loaded);
        // ROQA was closed for 100 s: a new process, a new monotonic origin, a later wall clock.
        let later = Instant::now() + secs(7);
        let restored = Timers::restore(&loaded.timers, WALL + 100, later);
        let left: Vec<u64> = restored
            .list
            .iter()
            .map(|t| secs_left(t.deadline, later))
            .collect();
        assert_eq!(left, [20, 800]);
        // One that ran out while closed is dropped, not announced.
        let expired = Timers::restore(&loaded.timers, WALL + 121, later);
        assert_eq!(expired.list.len(), 1);
        assert!(expired.alert.is_none());
    }

    #[test]
    fn a_clock_change_keeps_the_countdown_and_moves_the_expiry() {
        let now = Instant::now();
        let mut timers = Timers::default();
        assert!(timers.start(600, WALL, now));
        // Wake jitter and rounding are not a change: nothing to write.
        assert!(!timers.resync(WALL + 1, now + secs(1)));
        assert!(!timers.resync(WALL + 3, now + Duration::from_millis(1500)));
        // The user sets the clock forward an hour: the 10 minutes stay, the stored expiry follows.
        let t = now + secs(10);
        assert!(timers.resync(WALL + 3600 + 10, t));
        assert_eq!(timers.list[0].expiry, WALL + 3600 + 10 + 590);
        assert_eq!(secs_left(timers.list[0].deadline, t), 590);
        assert!(
            !timers.resync(WALL + 3600 + 10, t),
            "settled: no second change"
        );
        // And back by two hours.
        assert!(timers.resync(WALL - 7200, t));
        assert_eq!(timers.list[0].expiry, WALL - 7200 + 590);
        // The write marks the store dirty only when the expiry actually changed.
        let (mut store, mut pending) = (Store::default(), Pending::default());
        timers.write_to(&mut store, &mut pending, t);
        assert_eq!(store.timers, [WALL - 7200 + 590]);
        pending.written();
        timers.write_to(&mut store, &mut pending, t);
        assert_eq!(pending.decision(t), crate::store::Flush::Idle);
    }

    #[test]
    fn completion_fires_once_then_the_alert_ends_once() {
        let now = Instant::now();
        let mut timers = Timers::default();
        assert!(timers.start(30, WALL, now) && timers.start(90, WALL, now));
        assert!(!timers.take_done(now + secs(29)));
        assert!(timers.take_done(now + secs(30)), "the first completes");
        assert!(!timers.take_done(now + secs(30)), "and never again");
        assert!(!timers.take_done(now + secs(31)));
        assert_eq!(timers.list.len(), 1);
        let done = now + secs(90);
        assert!(timers.take_done(done)); // the second is its own completion
        assert!(timers.list.is_empty());
        assert!(!timers.alert_over(done + ALERT - Duration::from_millis(1)));
        assert!(timers.alert_over(done + ALERT));
        assert!(
            !timers.alert_over(done + ALERT * 2),
            "the end is reported once"
        );
        assert!(timers.view(done + ALERT).is_none());
    }

    #[test]
    fn the_wake_is_gated_by_expanded_and_absent_with_no_timers() {
        let now = Instant::now();
        let mut timers = Timers::default();
        for expanded in [false, true] {
            assert_eq!(timers.next_wake(expanded, now), None, "no timers, no wake");
        }
        assert!(timers.start(300, WALL, now) && timers.start(700, WALL, now));
        // Collapsed, hidden or fullscreen-suppressed (not expanded): only the completion, never a tick.
        assert_eq!(timers.next_wake(false, now), Some(now + secs(300)));
        assert_eq!(
            timers.next_wake(false, now + Duration::from_millis(299_400)),
            Some(now + secs(300))
        );
        // Expanded: the next second boundary of the soonest timer's display, 1 s when on a boundary.
        assert_eq!(timers.next_wake(true, now), Some(now + secs(1)));
        let t = now + Duration::from_millis(10_250); // 289.75 s left: the display changes in 750 ms
        assert_eq!(
            timers.next_wake(true, t),
            Some(t + Duration::from_millis(750))
        );
        // The completion beats a tick that would land after it.
        let near = now + Duration::from_millis(299_900);
        assert_eq!(timers.next_wake(true, near), Some(now + secs(300)));
        // An alert alone wakes once, at its end, expanded or not.
        let mut alone = Timers::default();
        assert!(alone.start(5, WALL, now));
        assert!(alone.take_done(now + secs(5)));
        let end = now + secs(5) + ALERT;
        assert_eq!(alone.next_wake(true, now + secs(6)), Some(end));
        assert_eq!(alone.next_wake(false, now + secs(6)), Some(end));
        assert!(alone.alert_over(end));
        assert_eq!(alone.next_wake(true, end), None);
    }

    #[test]
    fn the_text_and_the_accessible_name_carry_the_state() {
        let now = Instant::now();
        let mut timers = Timers::default();
        assert_eq!(timers.view(now), None);
        assert!(timers.start(272, WALL, now));
        let view = timers.view(now).unwrap();
        assert_eq!(
            (view.pill.as_str(), view.panel.as_str(), view.name.as_str()),
            ("5m", "4:32", "Timer, 4:32 remaining")
        );
        assert!(timers.start(3725, WALL, now) && timers.start(45, WALL, now));
        let view = timers.view(now).unwrap();
        assert_eq!(view.panel, "0:45 +2");
        assert_eq!(view.pill, "1m");
        assert_eq!(
            view.name,
            "Timer 1, 0:45 remaining; Timer 2, 4:32 remaining; Timer 3, 1:02:05 remaining"
        );
        let mut finished = Timers::default();
        assert!(finished.start(1, WALL, now) && finished.take_done(now + secs(1)));
        let view = finished.view(now + secs(1)).unwrap();
        assert_eq!(
            (view.pill.as_str(), view.panel.as_str(), view.name.as_str()),
            ("0m", "Timer finished", "Timer finished")
        );
        assert_eq!(
            (clock(0), clock(59), clock(3600), clock(86_399)),
            (
                "0:00".into(),
                "0:59".into(),
                "1:00:00".into(),
                "23:59:59".into()
            )
        );
    }
}
