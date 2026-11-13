//! Water reminders (F5-P3): the pure schedule and progress rules. The host keeps one deadline to the next
//! reminder (no polling); times are local seconds since midnight, so a DST change inside the active hours
//! shifts that day's slots by the jump (not handled: the next pass after midnight is right again).

/// Daily goal in glasses, glass size in ml, first and last active hour, minutes between reminders.
pub type Config = [u16; 5];
pub const DEFAULT: Config = [8, 250, 9, 21, 60];
pub const LABELS: [&str; 5] = [
    "Daily &goal (glasses)",
    "Glass &size (ml)",
    "Active from (hour)",
    "Active until (hour)",
    "Remind &every (minutes)",
];

/// Forces every value into its range; the last hour is always after the first.
pub fn clamp(c: &mut Config) {
    c[0] = c[0].clamp(1, 30);
    c[1] = c[1].clamp(50, 1000);
    c[2] = c[2].min(23);
    c[3] = c[3].clamp(c[2] + 1, 24);
    c[4] = c[4].clamp(10, 240);
}

/// Seconds from `now` (since local midnight) to the next reminder: slots start at the first active hour and
/// repeat every interval up to and including the last hour; after the final one, tomorrow's first.
pub fn wait_secs(now: u32, c: &Config) -> u32 {
    let (from, to, gap) = (
        u32::from(c[2]) * 3600,
        u32::from(c[3]) * 3600,
        u32::from(c[4]) * 60,
    );
    if now < from {
        return from - now;
    }
    let slot = from + ((now - from) / gap + 1) * gap;
    if slot <= to {
        slot - now
    } else {
        86_400 - now + from
    }
}

/// Glasses drunk today: the stored count belongs to its day and is zero on any other day (midnight reset).
pub fn today(stored: (i64, u8), day: i64) -> u8 {
    if stored.0 == day { stored.1 } else { 0 }
}

/// "3 of 8 glasses (750 ml)"; with none drunk, no amount.
pub fn text(drank: u8, c: &Config) -> String {
    if drank == 0 {
        return format!("0 of {} glasses", c[0]);
    }
    format!(
        "{drank} of {} glasses ({} ml)",
        c[0],
        u32::from(drank) * u32::from(c[1])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_keep_the_end_after_the_start() {
        let mut c = [0, 5, 30, 0, 1];
        clamp(&mut c);
        assert_eq!(c, [1, 50, 23, 24, 10]);
        let mut c = [99, 9999, 5, 5, 999];
        clamp(&mut c);
        assert_eq!(c, [30, 1000, 5, 6, 240]);
        let mut d = DEFAULT;
        clamp(&mut d);
        assert_eq!(d, DEFAULT);
    }

    #[test]
    fn slots_run_from_the_first_hour_to_the_last_inclusive() {
        let c = [8, 250, 9, 12, 60]; // 09:00 10:00 11:00 12:00
        assert_eq!(wait_secs(0, &c), 9 * 3600);
        assert_eq!(wait_secs(9 * 3600 - 1, &c), 1);
        assert_eq!(
            wait_secs(9 * 3600, &c),
            3600,
            "a slot is never its own next"
        );
        assert_eq!(wait_secs(10 * 3600 + 1799, &c), 1801);
        assert_eq!(
            wait_secs(11 * 3600, &c),
            3600,
            "the last hour still reminds"
        );
    }

    #[test]
    fn after_the_last_slot_it_waits_for_tomorrows_first_across_midnight() {
        let c = [8, 250, 9, 12, 60];
        assert_eq!(wait_secs(12 * 3600, &c), 86_400 - 12 * 3600 + 9 * 3600);
        assert_eq!(wait_secs(86_399, &c), 1 + 9 * 3600);
        // An interval that does not divide the window: 09:00 09:50 10:40 11:30 (12:20 is past the end).
        let c = [8, 250, 9, 12, 50];
        assert_eq!(wait_secs(11 * 3600 + 1800, &c), 86_400 - 41_400 + 32_400);
    }

    #[test]
    fn progress_belongs_to_its_day() {
        assert_eq!(today((100, 3), 100), 3);
        assert_eq!(today((100, 3), 101), 0);
        assert_eq!(text(3, &DEFAULT), "3 of 8 glasses (750 ml)");
        assert_eq!(text(0, &DEFAULT), "0 of 8 glasses");
    }
}
