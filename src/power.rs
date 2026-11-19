//! Power pill (features B1): a transient notch notice when the power state changes.
//!
//! The window procedure posts a wake on `PBT_APMPOWERSTATUSCHANGE`; the host reads the status once and
//! this state machine decides whether a notice starts. Its end is one host deadline: no thread, no timer,
//! and a machine without a battery never produces a notice.

use std::time::{Duration, Instant};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

/// How long a notice keeps the notch open.
pub const NOTICE: Duration = Duration::from_secs(4);

/// A question ignores clicks for this long after it shows, so a click meant for something under it is not an answer.
const SETTLE: Duration = Duration::from_millis(500);

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
    /// Seconds of charge left on battery, when Windows knows.
    pub left: Option<u32>,
}

/// The Settings "Low battery at" percent; 0 leaves it to Windows' own low flag.
static LOW_AT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub fn set_low_at(percent: u8) {
    LOW_AT.store(percent, std::sync::atomic::Ordering::Relaxed);
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
            left: Some(status.BatteryLifeTime).filter(|secs| *secs != u32::MAX),
        })
    }

    fn under_threshold(self) -> bool {
        let at = LOW_AT.load(std::sync::atomic::Ordering::Relaxed);
        at > 0 && self.percent <= at
    }

    /// On battery and low: Windows' flag or the chosen percent. False without a battery.
    pub fn low(self) -> bool {
        matches!(self.state(), Some((State::Low | State::Critical, _)))
    }

    /// The battery as the power notice words it ("Charging 80%"); `None` without a battery.
    pub fn summary(self) -> Option<String> {
        self.state().map(|(state, percent)| text(state, percent))
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
            (0, flag) if flag & FLAG_LOW != 0 || self.under_threshold() => State::Low,
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

/// What the notice row carries besides its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Extra {
    #[default]
    None,
    /// An Eject button for this drive.
    Eject(char),
    /// A progress bar, in percent.
    Progress(u8),
    /// A Snooze button (a due reminder).
    Snooze,
    /// Deny and Allow buttons for the permission question of this hook process.
    Ask(u32),
}

#[derive(Default)]
pub struct Power {
    last: Option<State>,
    notice: Option<(String, Instant)>,
    extra: Extra,
    /// When the notice on the row started.
    since: Option<Instant>,
}

impl Power {
    /// The state at start is the baseline and announces nothing.
    pub fn new(raw: Option<Raw>) -> Self {
        Self {
            last: raw.and_then(Raw::state).map(|(state, _)| state),
            notice: None,
            extra: Extra::None,
            since: None,
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
        self.announce(text(state, percent), now, NOTICE, Extra::None)
    }

    /// A question, an Eject or a Snooze waits for a click: only a new question may take its row, and a click
    /// clears it first (`clear`).
    fn guarded(&self) -> bool {
        matches!(
            self.extra(),
            Extra::Ask(_) | Extra::Eject(_) | Extra::Snooze
        )
    }

    /// Shows `text` and its `extra` for `hold` in place of any notice (the prayer reminder reuses the row);
    /// false, and nothing changes, when a guarded notice keeps the row.
    pub fn announce(&mut self, text: String, now: Instant, hold: Duration, extra: Extra) -> bool {
        if self.guarded()
            && (matches!(self.extra, Extra::Ask(_)) || !matches!(extra, Extra::Ask(_)))
        {
            return false;
        }
        self.notice = Some((text, now + hold));
        self.extra = extra;
        self.since = Some(now);
        true
    }

    /// Whether a question has shown long enough (`SETTLE`) for a click to mean it.
    pub fn settled(&self, now: Instant) -> bool {
        self.since
            .is_none_or(|since| now.duration_since(since) >= SETTLE)
    }

    /// The extra of the notice that shows now.
    pub fn extra(&self) -> Extra {
        self.notice.as_ref().map_or(Extra::None, |_| self.extra)
    }

    /// The drive to eject while its notice shows.
    pub fn eject(&self) -> Option<char> {
        match self.extra() {
            Extra::Eject(letter) => Some(letter),
            _ => None,
        }
    }

    /// Drops the notice at once (a state change that makes it stale); true if one was showing.
    pub fn clear(&mut self) -> bool {
        self.notice.take().is_some()
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
        Some(Raw {
            ac,
            flag,
            percent,
            left: None,
        })
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
    fn clear_drops_a_stale_notice_once() {
        let now = Instant::now();
        let mut power = Power::new(raw(0, 1, 60));
        assert!(!power.clear(), "nothing to clear");
        power.announce("Pomodoro paused.".to_owned(), now, NOTICE, Extra::None);
        assert!(power.clear());
        assert_eq!((power.text(), power.deadline()), (None, None));
        assert!(!power.clear());
    }

    #[test]
    fn a_waiting_button_notice_is_not_replaced_by_a_plain_one() {
        let (now, hold) = (Instant::now(), NOTICE);
        let mut power = Power::new(raw(0, 1, 60));
        assert!(power.announce("Bash: git push".into(), now, hold, Extra::Ask(7)));
        // A plain notice, a battery change and a second question leave the first question as it was.
        assert!(!power.announce("Done".into(), now, hold, Extra::None));
        assert!(!power.update(raw(1, 9, 59), now));
        assert!(!power.announce("Rm".into(), now, hold, Extra::Ask(8)));
        assert_eq!(
            (power.text(), power.extra()),
            (Some("Bash: git push"), Extra::Ask(7))
        );
        assert!(!power.settled(now) && power.settled(now + SETTLE));
        // After a click, the next notice shows, and a battery notice carries no old buttons.
        assert!(power.clear());
        assert!(power.announce("Drive connected".into(), now, hold, Extra::Eject('E')));
        assert!(!power.update(raw(0, 1, 59), now));
        assert!(
            power.announce("Snooze".into(), now, hold, Extra::Ask(9)),
            "a question may take Eject's row"
        );
        assert!(power.clear());
        assert!(power.announce("Progress".into(), now, hold, Extra::Progress(40)));
        assert!(power.update(raw(1, 9, 59), now));
        assert_eq!(
            (power.text(), power.extra()),
            (Some("Charging 59%"), Extra::None)
        );
    }

    #[test]
    fn a_desktop_never_shows_a_notice() {
        let mut power = Power::new(raw(1, 128, 255));
        assert!(!power.update(raw(1, 128, 255), Instant::now()));
        assert_eq!(power.deadline(), None);
    }
}
