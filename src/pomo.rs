//! Pomodoro (F5-P4): the phase rules only. Each phase runs as an ordinary tagged countdown in `timer.rs`
//! (same deadline, store, notch view and alert); the host starts the next one when a tagged timer ends.

/// Focus, short break and long break minutes, and the focus sessions before a long break.
pub type Config = [u16; 4];
pub const DEFAULT: Config = [25, 5, 15, 4];
pub const LABELS: [&str; 4] = [
    "Focus (minutes)",
    "Short break (minutes)",
    "Long break (minutes)",
    "Focus sessions before a long break",
];

pub fn clamp(c: &mut Config) {
    c[0] = c[0].clamp(1, 120);
    c[1] = c[1].clamp(1, 60);
    c[2] = c[2].clamp(1, 120);
    c[3] = c[3].clamp(1, 12);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Focus,
    Short,
    Long,
}
impl Phase {
    pub fn label(self) -> &'static str {
        ["Focus", "Short break", "Long break"][self as usize]
    }
}

/// Whether `status` is only the "Focus: 25 min." line a phase start reports (`Host::pomo_start`), which the
/// Timer page's title already says.
pub fn is_start_status(status: &str) -> bool {
    [Phase::Focus, Phase::Short, Phase::Long]
        .iter()
        .any(|phase| {
            status
                .strip_prefix(phase.label())
                .and_then(|rest| rest.strip_prefix(": "))
                .is_some_and(|minutes| minutes.ends_with(" min."))
        })
}

#[derive(Debug, Default)]
pub struct Pomo {
    pub phase: Phase,
    /// Focus sessions finished (or skipped) so far.
    done: u16,
    /// Seconds left of a paused phase.
    pub left: Option<u64>,
    /// Focus sessions per round as of the last start or advance (0: not yet seen, so the default).
    per_round: u16,
    /// The running or paused phase's full length in seconds, as of the last start (`track`).
    pub total: u64,
}

impl Pomo {
    /// The current phase's length in seconds.
    pub fn secs(&self, c: &Config) -> u64 {
        u64::from(c[self.phase as usize]) * 60
    }

    /// Remembers the round length for `title`.
    pub fn track(&mut self, c: &Config) {
        self.per_round = c[3];
        self.total = self.secs(c);
    }

    /// The round as segments, focus then break in turn and a long break last: the current segment and the count
    /// (`2 * sessions`), for the Timer page's cycle capsule.
    pub fn cycle(&self) -> (u8, u8) {
        let n = if self.per_round == 0 {
            DEFAULT[3]
        } else {
            self.per_round
        };
        let finished = self.done % n;
        let current = match self.phase {
            Phase::Focus => 2 * finished,
            Phase::Short => 2 * finished.saturating_sub(1) + 1,
            Phase::Long => 2 * n - 1,
        };
        (current as u8, (2 * n) as u8)
    }

    /// The page title: "Focus 2 of 4", "Short break" or "Long break", with "paused" after it while paused.
    pub fn title(&self, paused: bool) -> String {
        let n = if self.per_round == 0 {
            DEFAULT[3]
        } else {
            self.per_round
        };
        let base = match self.phase {
            Phase::Focus => format!("Focus {} of {n}", self.done % n + 1),
            other => other.label().to_owned(),
        };
        if paused {
            format!("{base}, paused")
        } else {
            base
        }
    }

    /// Moves on: focus leads to a short break, or to a long one after every `c[3]`th session; a break leads
    /// back to focus. Any paused remainder is dropped.
    pub fn advance(&mut self, c: &Config) {
        self.left = None;
        self.per_round = c[3];
        self.phase = if self.phase == Phase::Focus {
            self.done += 1;
            if self.done.is_multiple_of(c[3].max(1)) {
                Phase::Long
            } else {
                Phase::Short
            }
        } else {
            Phase::Focus
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_focus_sessions_then_a_long_break_then_round_again() {
        let c = DEFAULT;
        let mut p = Pomo::default();
        let mut seen = Vec::new();
        for _ in 0..9 {
            seen.push((p.phase, p.secs(&c)));
            p.advance(&c);
        }
        let (f, s, l) = (
            (Phase::Focus, 1500),
            (Phase::Short, 300),
            (Phase::Long, 900),
        );
        assert_eq!(seen, [f, s, f, s, f, s, f, l, f]);
    }

    #[test]
    fn skipping_and_pausing_follow_the_same_rules() {
        let c = [1, 1, 2, 1]; // a long break after every session
        let mut p = Pomo {
            left: Some(10),
            ..Pomo::default()
        };
        p.advance(&c);
        assert_eq!((p.phase, p.left, p.secs(&c)), (Phase::Long, None, 120));
        p.advance(&c);
        assert_eq!(p.phase, Phase::Focus);
        let mut bad = [0, 999, 0, 99];
        clamp(&mut bad);
        assert_eq!(bad, [1, 60, 1, 12]);
        let mut d = DEFAULT;
        clamp(&mut d);
        assert_eq!(d, DEFAULT);
    }

    #[test]
    fn the_title_names_the_phase_and_session() {
        let c = [25, 5, 15, 2];
        let mut p = Pomo::default();
        p.track(&c);
        assert_eq!(p.title(false), "Focus 1 of 2");
        p.advance(&c);
        assert_eq!(p.title(true), "Short break, paused");
        p.advance(&c);
        assert_eq!(p.title(false), "Focus 2 of 2");
        p.advance(&c);
        assert_eq!(p.title(false), "Long break");
        assert_eq!(Pomo::default().title(false), "Focus 1 of 4");
    }

    #[test]
    fn only_a_phase_start_line_is_redundant() {
        for line in [
            "Focus: 25 min.",
            "Short break: 5 min.",
            "Long break: 15 min.",
        ] {
            assert!(is_start_status(line), "{line}");
        }
        for line in [
            "Pomodoro paused.",
            "Pomodoro cancelled.",
            "3 timers are already running.",
            "Focus done.",
        ] {
            assert!(!is_start_status(line), "{line}");
        }
    }

    #[test]
    fn the_cycle_is_focus_and_break_segments_ending_in_the_long_break() {
        let c = [25, 5, 15, 2];
        let mut p = Pomo::default();
        p.track(&c);
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(p.cycle());
            p.advance(&c);
        }
        // Focus, short, focus, long, then the round starts over.
        assert_eq!(seen, [(0, 4), (1, 4), (2, 4), (3, 4), (0, 4)]);
        assert_eq!(p.total, 1500);
    }
}
