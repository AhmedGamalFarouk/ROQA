//! Power pill (features B1): a transient notch notice when the power state changes.
//!
//! The window procedure posts a wake on `PBT_APMPOWERSTATUSCHANGE`; the host reads the status once and
//! this state machine decides whether a notice starts. Its end is one host deadline: no thread, no timer,
//! and a machine without a battery never produces a notice.

use std::time::{Duration, Instant};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

/// How long a notice keeps the notch open.
pub const NOTICE: Duration = Duration::from_secs(4);

/// The notice-worthy power states; a change between them (never a percentage tick) starts a notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Charging,
    OnBattery,
    Low,
    Critical,
}

/// `GetSystemPowerStatus` fields: `ACLineStatus`, `BatteryFlag` and `BatteryLifePercent`.
#[derive(Clone, Copy, Debug)]
pub struct Raw {
    pub ac: u8,
    pub flag: u8,
    pub percent: u8,
}

const FLAG_LOW: u8 = 2;
const FLAG_CRITICAL: u8 = 4;
/// "No system battery"; also set in the 255 "unknown" flag.
const FLAG_NO_BATTERY: u8 = 128;

impl Raw {
    pub fn read() -> Option<Self> {
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: a writable, correctly typed status struct.
        unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
        Some(Self {
            ac: status.ACLineStatus,
            flag: status.BatteryFlag,
            percent: status.BatteryLifePercent,
        })
    }

    /// The state and charge to announce; `None` without a battery or with an unknown reading.
    // shortcut: plugged in reads "Charging" even when full or held back; refine with the charging flag
    // if a device reports it reliably at the moment of the event.
    fn state(self) -> Option<(State, u8)> {
        if self.flag & FLAG_NO_BATTERY != 0 || self.percent > 100 {
            return None;
        }
        let state = match (self.ac, self.flag) {
            (1, _) => State::Charging,
            (0, flag) if flag & FLAG_CRITICAL != 0 => State::Critical,
            (0, flag) if flag & FLAG_LOW != 0 => State::Low,
            (0, _) => State::OnBattery,
            _ => return None,
        };
        Some((state, self.percent))
    }
}

fn text(state: State, percent: u8) -> String {
    match state {
        State::Charging => format!("Charging {percent}%"),
        State::OnBattery => format!("On battery {percent}%"),
        State::Low | State::Critical => format!("Battery low {percent}%"),
    }
}

#[derive(Default)]
pub struct Power {
    last: Option<State>,
    notice: Option<(String, Instant)>,
}

impl Power {
    /// The state at start is the baseline and announces nothing.
    pub fn new(raw: Option<Raw>) -> Self {
        Self {
            last: raw.and_then(Raw::state).map(|(state, _)| state),
            notice: None,
        }
    }

    /// True when `raw` is a new state and its notice starts; the same state or an unknown reading is not.
    pub fn update(&mut self, raw: Option<Raw>, now: Instant) -> bool {
        let Some((state, percent)) = raw.and_then(Raw::state) else {
            return false;
        };
        if self.last == Some(state) {
            return false;
        }
        self.last = Some(state);
        self.notice = Some((text(state, percent), now + NOTICE));
        true
    }

    pub fn text(&self) -> Option<&str> {
        self.notice.as_ref().map(|(text, _)| text.as_str())
    }

    /// True once when the notice has run its course.
    pub fn over(&mut self, now: Instant) -> bool {
        let over = self.notice.as_ref().is_some_and(|(_, until)| now >= *until);
        if over {
            self.notice = None;
        }
        over
    }

    /// The one instant the host must wake for: the notice's end. `None` adds no deadline.
    pub fn deadline(&self) -> Option<Instant> {
        self.notice.as_ref().map(|(_, until)| *until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn raw(ac: u8, flag: u8, percent: u8) -> Option<Raw> {
        Some(Raw { ac, flag, percent })
    }

    #[test]
    fn readings_map_to_states_and_a_machine_without_a_battery_has_none() {
        let state = |reading: Option<Raw>| reading.and_then(Raw::state).map(|(state, _)| state);
        assert_eq!(state(raw(1, 9, 54)), Some(State::Charging));
        assert_eq!(state(raw(0, 1, 54)), Some(State::OnBattery));
        assert_eq!(state(raw(0, 2, 20)), Some(State::Low));
        assert_eq!(state(raw(0, 4, 4)), Some(State::Critical));
        // Critical wins when both flags are set; plugging in clears a low flag.
        assert_eq!(state(raw(0, 6, 4)), Some(State::Critical));
        assert_eq!(state(raw(1, 2, 20)), Some(State::Charging));
        for none in [raw(1, 128, 255), raw(255, 128, 0), raw(0, 255, 255)] {
            assert_eq!(state(none), None, "{none:?}");
        }
        // Unknown line status or charge announces nothing.
        assert_eq!(state(raw(255, 1, 50)), None);
        assert_eq!(state(raw(0, 1, 255)), None);
        assert_eq!(state(None), None);
    }

    #[test]
    fn the_text_names_the_state_and_the_charge() {
        assert_eq!(text(State::Charging, 54), "Charging 54%");
        assert_eq!(text(State::OnBattery, 54), "On battery 54%");
        assert_eq!(text(State::Low, 10), "Battery low 10%");
        assert_eq!(text(State::Critical, 4), "Battery low 4%");
    }

    #[test]
    fn only_a_state_change_starts_one_notice_that_ends_at_one_deadline() {
        let now = Instant::now();
        let mut power = Power::new(raw(0, 1, 60));
        assert_eq!((power.text(), power.deadline()), (None, None));
        // A percentage tick in the same state, and an unreadable status, start nothing.
        assert!(!power.update(raw(0, 1, 59), now));
        assert!(!power.update(None, now));
        assert!(!power.update(raw(1, 128, 255), now));
        assert_eq!(power.deadline(), None);
        // Plugging in starts "Charging", ending after `NOTICE`.
        assert!(power.update(raw(1, 9, 59), now));
        assert_eq!(power.text(), Some("Charging 59%"));
        assert_eq!(power.deadline(), Some(now + NOTICE));
        assert!(!power.update(raw(1, 9, 60), now), "same state again");
        assert!(!power.over(now + NOTICE - Duration::from_millis(1)));
        assert!(power.over(now + NOTICE));
        assert!(!power.over(now + NOTICE), "ends once");
        assert_eq!((power.text(), power.deadline()), (None, None));
        // Unplugged, then crossing the low and the critical flags, each announces once.
        for (reading, expected) in [
            (raw(0, 1, 30), "On battery 30%"),
            (raw(0, 2, 30), "Battery low 30%"),
            (raw(0, 4, 4), "Battery low 4%"),
        ] {
            assert!(power.update(reading, now), "{expected}");
            assert_eq!(power.text(), Some(expected));
        }
    }

    #[test]
    fn a_desktop_never_shows_a_notice() {
        let mut power = Power::new(raw(1, 128, 255));
        assert!(!power.update(raw(1, 128, 255), Instant::now()));
        assert_eq!(power.deadline(), None);
    }
}
