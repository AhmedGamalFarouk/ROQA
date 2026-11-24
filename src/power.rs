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

/// A plain notice that holds for less does not cut a longer one short in its first moments.
const FIRM: Duration = Duration::from_secs(2);

/// After the pointer leaves a notice it held, the notice stays this long, not its whole hold again.
const RELEASE: Duration = Duration::from_secs(2);

/// A plain notice the row refuses waits for it only when it would show at least this long; a pill of a second
/// or two is stale by then.
const QUEUE_MIN: Duration = Duration::from_secs(3);

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
    /// A bell before the text: an app's notification or a script's line. Otherwise a plain notice.
    Bell,
}

impl Extra {
    /// No button or bar: the notice may wait in the queue and may be cut short or dismissed.
    fn plain(self) -> bool {
        matches!(self, Extra::None | Extra::Bell)
    }
}

#[derive(Default)]
pub struct Power {
    last: Option<State>,
    notice: Option<(String, Instant)>,
    extra: Extra,
    /// When the notice on the row started, and how long it was to hold.
    since: Option<Instant>,
    hold: Duration,
    /// One plain notice the row refused, shown when the row frees (`promote`); the first one refused stays.
    next: Option<(String, Duration, Extra)>,
    /// The pointer is on a plain notice: its end is held until it leaves (`hover`).
    held: bool,
    /// The screenshot pill holds the row (`shots`), and whether it is hovered or dragged (held).
    shot: Option<bool>,
}

impl Power {
    /// The state at start is the baseline and announces nothing.
    pub fn new(raw: Option<Raw>) -> Self {
        Self {
            last: raw.and_then(Raw::state).map(|(state, _)| state),
            ..Self::default()
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

    /// A question, an Eject or a Snooze waits for a click: nothing takes its row, and a click clears it first
    /// (`clear`).
    fn guarded(&self) -> bool {
        matches!(
            self.extra(),
            Extra::Ask(_) | Extra::Eject(_) | Extra::Snooze
        )
    }

    /// A hovered plain notice does not end; a question that arrived over it is not held.
    fn holding(&self) -> bool {
        self.held && !self.guarded()
    }

    /// Whether `announce` with these would take the row now: a producer that must not lose its notice (a due
    /// reminder) asks first.
    pub fn admits(&self, now: Instant, hold: Duration, extra: Extra) -> bool {
        if self.guarded() || self.shot == Some(true) {
            return false;
        }
        // A showing screenshot pill gives way only to a button row; a plain notice waits for it.
        if self.shot.is_some() && !matches!(extra, Extra::Ask(_) | Extra::Eject(_) | Extra::Snooze)
        {
            return false;
        }
        let firm = self.notice.as_ref().is_some_and(|(_, until)| now < *until)
            && self
                .since
                .is_some_and(|since| now.saturating_duration_since(since) < FIRM);
        !(firm && extra.plain() && hold < self.hold)
    }

    /// Shows `text` and its `extra` for `hold` in place of any notice (the prayer reminder reuses the row);
    /// false, and nothing changes, when a guarded or longer notice keeps the row. A plain notice of `QUEUE_MIN`
    /// or more that is refused waits in the one queue place.
    pub fn announce(&mut self, text: String, now: Instant, hold: Duration, extra: Extra) -> bool {
        if !self.admits(now, hold, extra) {
            if extra.plain() && hold >= QUEUE_MIN && self.next.is_none() {
                self.next = Some((text, hold, extra));
            }
            return false;
        }
        self.notice = Some((text, now + hold));
        self.extra = extra;
        self.since = Some(now);
        self.hold = hold;
        true
    }

    /// The screenshot pill shows on the free row (`Some`, true while hovered or dragged), or not.
    pub fn set_shot(&mut self, shot: Option<bool>) {
        self.shot = shot;
    }

    /// A prayer takes the row from a showing screenshot pill that is not held; the pill returns after it.
    pub fn yield_shot(&mut self) {
        if self.shot == Some(false) {
            self.shot = None;
        }
    }

    /// Shows the queued notice once the row is free; true when it did.
    pub fn promote(&mut self, now: Instant) -> bool {
        if self.notice.is_some() {
            return false;
        }
        let Some((text, hold, extra)) = self.next.take() else {
            return false;
        };
        self.announce(text, now, hold, extra)
    }

    /// Whether a question has shown long enough (`SETTLE`) for a click to mean it.
    pub fn settled(&self, now: Instant) -> bool {
        self.since
            .is_none_or(|since| now.duration_since(since) >= SETTLE)
    }

    /// The pointer is on a plain notice (`on`) or left it. While on, the notice does not end; on leaving it ends
    /// `RELEASE` later. A question, Eject or Snooze row keeps its own wait and is not held.
    pub fn hover(&mut self, on: bool, now: Instant) {
        let on = on && self.notice.is_some() && !self.guarded();
        if self.held
            && !on
            && let Some((_, until)) = &mut self.notice
        {
            *until = now + RELEASE;
        }
        self.held = on;
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
        self.held = false;
        self.notice.take().is_some()
    }

    /// Like `clear`, but a question, Eject or Snooze row stays: only a click or its end takes it.
    pub fn clear_plain(&mut self) -> bool {
        !self.guarded() && self.clear()
    }

    pub fn text(&self) -> Option<&str> {
        self.notice.as_ref().map(|(text, _)| text.as_str())
    }

    /// True once when the notice has run its course.
    pub fn over(&mut self, now: Instant) -> bool {
        let over = !self.holding() && self.notice.as_ref().is_some_and(|(_, until)| now >= *until);
        if over {
            self.notice = None;
        }
        over
    }

    /// The one instant the host must wake for: the notice's end. `None` adds no deadline.
    pub fn deadline(&self) -> Option<Instant> {
        self.notice
            .as_ref()
            .filter(|_| !self.holding())
            .map(|(_, until)| *until)
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
            !power.announce("Snooze".into(), now, hold, Extra::Ask(9)),
            "a question never takes Eject's row"
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
    fn a_longer_notice_is_not_cut_short_and_a_refused_one_waits_its_turn() {
        let (now, second) = (Instant::now(), Duration::from_secs(1));
        let (long, pill) = (Duration::from_secs(8), Duration::from_secs(2));
        let mut power = Power::new(raw(0, 1, 60));
        assert!(power.announce("Prayer".into(), now, long, Extra::None));
        // A short pill in the first 2 s does not replace it and is not worth keeping.
        assert!(!power.announce("Volume 40%".into(), now + second, pill, Extra::None));
        assert!(!power.over(now + second) && !power.promote(now + second));
        assert_eq!(power.text(), Some("Prayer"));
        // A 4 s notice is kept in the one place, and the first one refused stays.
        assert!(!power.announce("Drive connected".into(), now + second, NOTICE, Extra::None));
        assert!(!power.announce("Camera connected".into(), now + second, NOTICE, Extra::None));
        // After 2 s a short pill may replace it as before.
        assert!(power.announce("Volume 41%".into(), now + 2 * second, pill, Extra::None));
        assert!(power.over(now + 4 * second));
        assert!(power.promote(now + 4 * second));
        assert_eq!(power.text(), Some("Drive connected"));
        assert_eq!(power.deadline(), Some(now + 4 * second + NOTICE));
        assert!(!power.promote(now + 4 * second), "the queue is empty now");
    }

    #[test]
    fn a_question_keeps_a_waiting_notice_for_when_it_is_answered() {
        let (now, hold) = (Instant::now(), Duration::from_secs(8));
        let mut power = Power::new(raw(0, 1, 60));
        assert!(power.announce("Bash: git push".into(), now, hold, Extra::Ask(7)));
        assert!(!power.announce("Asr in 5 min".into(), now, hold, Extra::None));
        assert!(!power.announce("Fajr".into(), now, hold, Extra::None));
        // A second question is not queued: the caller tells its hook to ask in the terminal.
        assert!(!power.announce("Rm".into(), now, hold, Extra::Ask(8)));
        assert!(!power.promote(now), "the question still shows");
        assert!(power.clear());
        assert!(power.promote(now));
        assert_eq!(
            (power.text(), power.extra()),
            (Some("Asr in 5 min"), Extra::None)
        );
        assert!(power.clear() && !power.promote(now), "only one was kept");
    }

    #[test]
    fn a_due_reminder_is_admitted_only_when_its_snooze_can_show() {
        let (now, hold) = (Instant::now(), Duration::from_secs(8));
        let mut power = Power::new(raw(0, 1, 60));
        assert!(power.admits(now, hold, Extra::Snooze));
        // Over a plain notice, even a longer one in its first moments, it shows.
        power.announce("Prayer".into(), now, hold, Extra::None);
        assert!(power.admits(now, hold, Extra::Snooze));
        // Over another Snooze, an Eject or a question it does not: the caller keeps it waiting.
        for held in [Extra::Snooze, Extra::Eject('E'), Extra::Ask(3)] {
            assert!(power.clear());
            power.announce("Held".into(), now, hold, held);
            assert!(!power.admits(now, hold, Extra::Snooze), "{held:?}");
        }
        assert!(power.clear());
        assert!(power.admits(now, hold, Extra::Snooze));
    }
    #[test]
    fn a_question_and_a_timer_click_leave_a_button_row_alone() {
        let (now, hold) = (Instant::now(), Duration::from_secs(8));
        let mut power = Power::new(raw(0, 1, 60));
        // Reminder, then a question: the Snooze row stays and the caller sends the question to the terminal.
        assert!(power.announce("Stretch".into(), now, hold, Extra::Snooze));
        assert!(!power.announce("Bash: ls".into(), now, hold, Extra::Ask(1)));
        assert_eq!(power.extra(), Extra::Snooze);
        // Question, then a timer preset or Pomodoro start: it stays too.
        assert!(power.clear());
        assert!(power.announce("Bash: ls".into(), now, hold, Extra::Ask(1)));
        assert!(!power.clear_plain());
        assert_eq!(power.extra(), Extra::Ask(1));
        // A plain stale notice does clear.
        assert!(power.clear());
        power.announce("Pomodoro paused.".into(), now, NOTICE, Extra::None);
        assert!(power.clear_plain() && power.text().is_none());
    }

    #[test]
    fn a_desktop_never_shows_a_notice() {
        let mut power = Power::new(raw(1, 128, 255));
        assert!(!power.update(raw(1, 128, 255), Instant::now()));
        assert_eq!(power.deadline(), None);
    }

    #[test]
    fn a_hovered_notice_stays_and_ends_two_seconds_after_the_pointer_leaves() {
        let (mut power, now) = (Power::default(), Instant::now());
        let sec = Duration::from_secs;
        assert!(power.announce("Mail: Hi".into(), now, sec(4), Extra::Bell));
        power.hover(true, now + sec(1));
        assert!(
            !power.over(now + sec(60)),
            "held while the pointer is on it"
        );
        assert_eq!(power.deadline(), None);
        power.hover(false, now + sec(60));
        assert!(!power.over(now + sec(61)));
        assert_eq!(power.deadline(), Some(now + sec(62)));
        assert!(power.over(now + sec(62)));
        // A question is not held, and a hover with nothing showing is nothing.
        power.hover(true, now);
        assert!(power.announce("Run it?".into(), now, sec(4), Extra::Ask(1)));
        assert!(power.over(now + sec(4)));
        power.hover(false, now);
    }

    #[test]
    fn a_bell_notice_waits_its_turn_like_a_plain_one_and_keeps_its_bell() {
        let (mut power, now) = (Power::default(), Instant::now());
        let sec = Duration::from_secs;
        assert!(power.announce("Run it?".into(), now, sec(8), Extra::Ask(1)));
        assert!(!power.announce("Mail: Hi".into(), now, sec(4), Extra::Bell));
        power.clear();
        assert!(power.promote(now));
        assert_eq!(power.extra(), Extra::Bell);
    }
}
