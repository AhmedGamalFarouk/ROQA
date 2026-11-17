//! Reminders in plain words ("in 20 min stretch", "at 17:30 call mum"): each is one wall-clock expiry that
//! the host's existing wait minimum drives, so there is no thread, timer message or polling. The expiry
//! is compared with the wall clock on every pass, which also makes a reminder come due after sleep.

use crate::store::{Pending, Store};
use std::time::Instant;

/// At most this many reminders wait at once.
pub const MAX: usize = 5;
/// How far ahead a reminder may be set, in seconds.
const LONGEST: u64 = 24 * 60 * 60;
/// The longest label kept, in characters.
const LABEL: usize = 40;
/// What a snooze waits.
pub const SNOOZE: u64 = 10 * 60;

/// "in 20 min stretch", "20m tea", "2 h call", "at 17:30 call mum": the seconds from now and the label.
/// `day_secs` is the local time of day in seconds; a clock time already past means tomorrow's.
pub fn parse(text: &str, day_secs: u32) -> Option<(u64, String)> {
    let mut words = text.split_whitespace().peekable();
    let delay = if words
        .next_if(|word| word.eq_ignore_ascii_case("at"))
        .is_some()
    {
        let (hours, minutes) = words.next()?.split_once(':')?;
        let (hours, minutes): (u32, u32) = (hours.parse().ok()?, minutes.parse().ok()?);
        if hours > 23 || minutes > 59 {
            return None;
        }
        let at = hours * 3600 + minutes * 60;
        match (at + 86_400 - day_secs) % 86_400 {
            0 => 86_400,
            ahead => u64::from(ahead),
        }
    } else {
        words.next_if(|word| word.eq_ignore_ascii_case("in"));
        let word = words.next()?;
        let split = word
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(word.len());
        let amount: u64 = word[..split].parse().ok()?;
        let unit = match &word[split..] {
            "" => words.next()?,
            unit => unit,
        };
        let seconds = match unit.to_ascii_lowercase().as_str() {
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
            _ => return None,
        };
        amount
            .checked_mul(seconds)
            .filter(|delay| (60..=LONGEST).contains(delay))?
    };
    let label: String = words.collect::<Vec<_>>().join(" ");
    Some((delay, label.chars().take(LABEL).collect()))
}

/// The words a due reminder shows: the label with a capital, or "Reminder" when none was given.
pub fn shown(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Reminder".to_owned(),
    }
}

/// The reminders waiting, soonest first, as `(expiry, label)` with the expiry in Unix seconds.
#[derive(Debug, Default)]
pub struct Reminders {
    list: Vec<(u64, String)>,
}

impl Reminders {
    pub fn from_store(store: &Store) -> Self {
        let mut list: Vec<_> = store.reminders.iter().take(MAX).cloned().collect();
        crate::sort::by(&mut list, |a, b| a.0.cmp(&b.0));
        Self { list }
    }

    /// Sets a reminder `delay` seconds from `wall`; false when `MAX` already wait.
    pub fn add(&mut self, wall: u64, delay: u64, label: String) -> bool {
        if self.list.len() >= MAX {
            return false;
        }
        self.list.push((wall.saturating_add(delay), label));
        crate::sort::by(&mut self.list, |a, b| a.0.cmp(&b.0));
        true
    }

    /// The soonest reminder whose time has come, removed. One at a time, so each gets its own notice.
    pub fn take_due(&mut self, wall: u64) -> Option<String> {
        (self.list.first()?.0 <= wall).then(|| self.list.remove(0).1)
    }

    /// Seconds until the soonest one; `None` with nothing waiting, so the host adds no deadline.
    pub fn wait_secs(&self, wall: u64) -> Option<u64> {
        self.list
            .first()
            .map(|(expiry, _)| expiry.saturating_sub(wall))
    }

    /// Mirrors the list into the store, marking it dirty only when it changed.
    pub fn write_to(&self, store: &mut Store, pending: &mut Pending, now: Instant) {
        if store.reminders != self.list {
            store.reminders.clone_from(&self.list);
            pending.mark(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: u32 = 12 * 3600;

    #[test]
    fn plain_words_give_a_delay_and_a_label() {
        for (text, delay, label) in [
            ("in 20 min stretch", 1200, "stretch"),
            ("20m tea", 1200, "tea"),
            ("In 2 h call mum", 7200, "call mum"),
            ("1hr", 3600, ""),
            ("10 minutes  bake  bread", 600, "bake bread"),
            ("at 17:30 call mum", 5 * 3600 + 1800, "call mum"),
            ("at 9:05 walk", 21 * 3600 + 300, "walk"),
            ("at 12:00 lunch", 86_400, "lunch"),
        ] {
            assert_eq!(
                parse(text, NOON),
                Some((delay, label.to_owned())),
                "{text:?}"
            );
        }
    }

    #[test]
    fn nonsense_and_out_of_range_are_refused() {
        for bad in [
            "",
            "in",
            "tea",
            "in tea",
            "0m tea",
            "25h x",
            "1441m x",
            "20 days x",
            "at",
            "at 25:00 x",
            "at 12:60 x",
            "at noon x",
            "-5m x",
            "5.5m x",
        ] {
            assert_eq!(parse(bad, NOON), None, "{bad:?}");
        }
        assert_eq!(
            parse(&format!("5m {}", "x".repeat(99)), NOON)
                .unwrap()
                .1
                .len(),
            LABEL
        );
    }

    #[test]
    fn the_labels_read_with_a_capital() {
        assert_eq!(shown("call mum"), "Call mum");
        assert_eq!(shown(""), "Reminder");
    }

    #[test]
    fn reminders_come_due_one_at_a_time_and_survive_the_store() {
        let mut list = Reminders::default();
        assert_eq!(list.wait_secs(100), None);
        assert!(list.add(100, 600, "tea".into()) && list.add(100, 60, "stretch".into()));
        assert_eq!(list.wait_secs(100), Some(60));
        assert_eq!(list.take_due(159), None);
        assert_eq!(list.take_due(160).as_deref(), Some("stretch"));
        assert_eq!(list.take_due(1000).as_deref(), Some("tea"));
        assert_eq!(list.take_due(1000), None);
        for n in 0..MAX {
            assert!(list.add(0, 60, format!("r{n}")));
        }
        assert!(!list.add(0, 60, "one more".into()));
        let mut store = Store::default();
        list.write_to(&mut store, &mut Pending::default(), Instant::now());
        assert_eq!(Reminders::from_store(&store).wait_secs(0), Some(60));
        assert_eq!(store.reminders.len(), MAX);
    }
}
