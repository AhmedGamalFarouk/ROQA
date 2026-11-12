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

#[derive(Debug, Default)]
pub struct Pomo {
    pub phase: Phase,
    /// Focus sessions finished (or skipped) so far.
    done: u16,
    /// Seconds left of a paused phase.
    pub left: Option<u64>,
}

impl Pomo {
    /// The current phase's length in seconds.
    pub fn secs(&self, c: &Config) -> u64 {
        u64::from(c[self.phase as usize]) * 60
    }

    /// Moves on: focus leads to a short break, or to a long one after every `c[3]`th session; a break leads
    /// back to focus. Any paused remainder is dropped.
    pub fn advance(&mut self, c: &Config) {
        self.left = None;
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
}
