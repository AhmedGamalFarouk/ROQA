//! The Prayer tab (features P1): pure rules around `prayer.rs`. When a reminder is due and the one wait until
//! the next, the page's ready-made text, the countdown wording, the Hijri text and which audio file plays.
//! No clock, window or file is touched here; `prayer_os.rs` supplies the facts.

use crate::prayer::{self, Params, Prayer, Ymd};
use std::path::{Path, PathBuf};

/// The sound setting that means "no sound"; empty means "the default" (the first `takbeer*` file, if any).
pub const SILENT: &str = "-";
/// A reminder is announced up to this late (seconds); after a long sleep it is stale and skipped.
pub const GRACE: i32 = 120;
/// Minutes before the prayer for the optional heads-up; 0 is off.
pub const LEADS: [u32; 4] = [0, 5, 10, 15];
/// The five prayers the page lists (sunrise is only a countdown target).
pub const FIVE: [Prayer; 5] = [
    Prayer::Fajr,
    Prayer::Dhuhr,
    Prayer::Asr,
    Prayer::Maghrib,
    Prayer::Isha,
];

/// Local date, seconds since local midnight and the UTC offset now (DST included), in minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub ymd: Ymd,
    pub secs: i32,
    pub offset_min: i32,
}

/// Days since 1970-01-01 of a civil date (for the UTC offset: local minus UTC).
pub fn day_number((y, m, d): Ymd) -> i64 {
    let y = i64::from(if m <= 2 { y - 1 } else { y });
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (i64::from(m) + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(d) - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Seconds since local midnight.
    pub at: i32,
    pub prayer: Prayer,
    /// Minutes before the prayer; 0 is the prayer time itself.
    pub lead: u32,
}

/// Today's reminders in time order from `prayer::timeline` (yesterday, today and tomorrow, so a shifted prayer
/// that crossed midnight is on the day it really falls): each of the five prayers, and with `lead` minutes
/// set, a heads-up before it. Kept: those from `-GRACE` (a reminder just before midnight that a late wake may
/// still announce) up to the end of today.
pub fn events(timeline: &[(i32, Prayer)], lead: u32) -> Vec<Event> {
    let mut all = Vec::with_capacity(20);
    for &(time, prayer) in timeline.iter().filter(|e| FIVE.contains(&e.1)) {
        let mut keep = |at: i32, lead: u32| {
            if (-GRACE..86_400).contains(&at) {
                all.push(Event { at, prayer, lead });
            }
        };
        keep(time, 0);
        if lead > 0 {
            keep(time - lead as i32 * 60, lead);
        }
    }
    crate::sort::by(&mut all, |a, b| a.at.cmp(&b.at));
    all
}

/// One pass of the schedule: the reminder to announce now (the newest due one, within `GRACE`, not before the
/// last pass) and the seconds until the next pass is needed: the next event or a second after midnight, when
/// the date, the UTC offset and so the times are read again. `last` is the previous pass's (date, second); the
/// first pass (none yet) announces nothing, so starting the app just after a prayer is silent.
pub fn step(
    events: &[Event],
    now: (Ymd, i32),
    last: &mut Option<(Ymd, i32)>,
) -> (Option<Event>, u32) {
    let (ymd, secs) = now;
    // Absolute seconds, so an event at -10 (just before today's midnight) that fired yesterday does not fire twice.
    let abs = |ymd: Ymd, secs: i32| day_number(ymd) * 86_400 + i64::from(secs);
    let seen = last.map(|(y, s)| abs(y, s));
    let fire = events
        .iter()
        .rev()
        .find(|e| {
            seen.is_some_and(|seen| abs(ymd, e.at) > seen) && e.at <= secs && secs - e.at <= GRACE
        })
        .copied();
    if seen.is_none_or(|seen| abs(ymd, secs) > seen) {
        *last = Some((ymd, secs));
    }
    let next = events
        .iter()
        .find(|e| e.at > secs)
        .map_or(i32::MAX, |e| e.at);
    let wait = next.min(86_400 + 1).saturating_sub(secs).max(1);
    (fire, wait as u32)
}

/// Whether a due reminder pauses the media: the switch is on, it is the prayer itself (not the heads-up) and
/// something is playing.
pub fn pauses_media(on: bool, e: Event, playing: bool) -> bool {
    on && e.lead == 0 && playing
}

/// The notice text: "Asr" at the time, "Asr in 10 min" for a heads-up.
pub fn notice(e: Event) -> String {
    if e.lead == 0 {
        format!("Time for {}", e.prayer.name())
    } else {
        format!("{} in {} min", e.prayer.name(), e.lead)
    }
}

/// "in 2 h 10 min"; minutes round up, so never "in 0 min" early.
pub fn countdown(secs_left: u32) -> String {
    let minutes = secs_left.div_ceil(60);
    let (h, m) = (minutes / 60, minutes % 60);
    let parts: Vec<String> = [(h, "h"), (m, "min")]
        .into_iter()
        .filter(|&(n, _)| n > 0)
        .map(|(n, unit)| format!("{n} {unit}"))
        .collect();
    format!("in {}", parts.join(" "))
}

pub struct Row {
    pub name: &'static str,
    pub time: String,
    pub current: bool,
}

/// Everything the Prayer page draws and says, as ready strings.
#[derive(Default)]
pub struct View {
    /// Shown instead of the data when there is none.
    pub message: Option<&'static str>,
    pub set_city: bool,
    pub next: String,
    pub left: String,
    pub hijri: String,
    pub rows: Vec<Row>,
}

/// The page for `params` (or `message` without a place) at `local`; `label` writes a time of day.
pub fn view(
    params: Option<&Params>,
    message: &'static str,
    local: Local,
    hijri: String,
    label: &dyn Fn(i32) -> String,
) -> View {
    let none = |message: &'static str| View {
        message: Some(message),
        ..View::default()
    };
    let Some(params) = params else {
        return none(message);
    };
    let (Some(times), Some(line)) = (
        prayer::times(params, local.ymd),
        prayer::timeline(params, local.ymd),
    ) else {
        return none("No prayer times for this place today");
    };
    // The current prayer is the latest started; before Fajr it is still last night's Isha.
    // (A shifted Isha that passed midnight is still last night's current prayer, so the neighbour days count.)
    let current = line
        .iter()
        .rev()
        .find(|e| e.0 <= local.secs && FIVE.contains(&e.1))
        .and_then(|e| FIVE.iter().position(|&p| p == e.1))
        .unwrap_or(FIVE.len() - 1);
    let Some(&(at, ahead)) = line.iter().find(|e| e.0 > local.secs) else {
        return none("No prayer times for this place today");
    };
    let left = (at - local.secs) as u32;
    let (next_name, short) = (ahead.name(), countdown(left));
    View {
        message: None,
        set_city: false,
        next: next_name.to_owned(),
        left: short,
        hijri,
        rows: FIVE
            .iter()
            .enumerate()
            .map(|(i, &p)| Row {
                name: p.name(),
                time: label(times[p as usize]),
                current: i == current,
            })
            .collect(),
    }
}

/// "29 Jumada II 1448 AH" from the `d|M|yyyy` text of the Umm al-Qura calendar, in Latin or Arabic-Indic digits.
pub fn hijri_text(raw: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Muharram",
        "Safar",
        "Rabi I",
        "Rabi II",
        "Jumada I",
        "Jumada II",
        "Rajab",
        "Sha'ban",
        "Ramadan",
        "Shawwal",
        "Dhu al-Qi'dah",
        "Dhu al-Hijjah",
    ];
    let digits = |part: &str| -> Option<u32> {
        part.chars().try_fold(0u32, |n, c| {
            let d = match c {
                '0'..='9' => c as u32 - '0' as u32,
                '\u{660}'..='\u{669}' => c as u32 - 0x660,
                _ => return None,
            };
            n.checked_mul(10)?.checked_add(d)
        })
    };
    let mut parts = raw.trim().split('|').map(digits);
    let (day, month, year) = (parts.next()??, parts.next()??, parts.next()??);
    // A Gregorian answer (a locale without the calendar) would be a wrong date: show none.
    if !(1300..1700).contains(&year) || !(1..=30).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }
    Some(format!("{day} {} {year} AH", MONTHS[month as usize - 1]))
}

// ---- the sound ------------------------------------------------------------------------------------------

pub fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["mp3", "wav", "m4a", "wma"]
            .iter()
            .any(|a| e.eq_ignore_ascii_case(a))
    })
}

/// The `assets` folders searched: next to the exe and beside up to three parents (a dev build runs from
/// `target/release`).
pub fn asset_dirs(exe: &Path) -> Vec<PathBuf> {
    exe.ancestors()
        .skip(1)
        .take(4)
        .map(|dir| dir.join("assets"))
        .collect()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// The list text of a file: its name.
pub fn label(path: &str) -> &str {
    file_name(path)
}

/// The first file named `takbeer*`, in list order.
pub fn default_sound(found: &[String]) -> Option<&str> {
    found
        .iter()
        .map(String::as_str)
        .find(|p| file_name(p).to_ascii_lowercase().starts_with("takbeer"))
}

/// The file to play for a stored `setting`: none when silent, the default when empty, else that path.
pub fn resolve<'a>(setting: &'a str, found: &'a [String]) -> Option<&'a str> {
    match setting {
        SILENT => None,
        "" => default_sound(found),
        path => Some(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prayer::Method;

    const DAY: Ymd = (2026, 11, 12);
    /// A day's times as the timeline `events` reads (today only).
    fn line(times: &prayer::Times) -> Vec<(i32, Prayer)> {
        Prayer::ALL
            .into_iter()
            .map(|p| (times[p as usize], p))
            .collect()
    }
    const TIMES: prayer::Times = [
        4 * 3600 + 900,
        6 * 3600,
        11 * 3600 + 1800,
        14 * 3600 + 1200,
        16 * 3600 + 600,
        17 * 3600 + 900,
    ];

    #[test]
    fn events_are_ordered_and_a_lead_adds_a_heads_up_before_each() {
        let plain = events(&line(&TIMES), 0);
        assert_eq!(plain.len(), 5);
        assert!(
            plain
                .iter()
                .all(|e| e.lead == 0 && e.prayer != Prayer::Sunrise)
        );
        let with = events(&line(&TIMES), 10);
        assert_eq!(with.len(), 10);
        assert!(with.windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(
            (with[0].prayer, with[0].lead, with[0].at),
            (Prayer::Fajr, 10, TIMES[0] - 600)
        );
    }

    #[test]
    fn a_step_fires_on_time_once_and_waits_for_the_next() {
        let ev = events(&line(&TIMES), 0);
        let mut last = None;
        // The first pass only sets the baseline.
        // Before Fajr: nothing due, wait exactly until it.
        let (fire, wait) = step(&ev, (DAY, TIMES[0] - 100), &mut last);
        assert_eq!((fire, wait), (None, 100));
        // One second late, e.g. a coarse wake: fires.
        let (fire, wait) = step(&ev, (DAY, TIMES[0] + 1), &mut last);
        assert_eq!(fire.map(|e| e.prayer), Some(Prayer::Fajr));
        assert_eq!(wait, (TIMES[2] - TIMES[0] - 1) as u32); // Dhuhr is next: sunrise is not a reminder
        // The same time again (a second pass): not twice.
        assert_eq!(step(&ev, (DAY, TIMES[0] + 2), &mut last).0, None);
    }

    #[test]
    fn a_wake_long_after_an_event_is_stale_and_the_first_pass_announces_nothing() {
        let ev = events(&line(&TIMES), 0);
        let mut last = None;
        // First pass after start, minutes after Dhuhr: no catch-up.
        assert_eq!(step(&ev, (DAY, TIMES[2] + 60), &mut last).0, None);
        // A resume three hours later: Asr passed a long time ago.
        assert_eq!(step(&ev, (DAY, TIMES[3] + 3600), &mut last).0, None);
        // Within the grace after a resume it still counts.
        let mut last = Some((DAY, TIMES[3] - 3600));
        assert_eq!(
            step(&ev, (DAY, TIMES[3] + 90), &mut last)
                .0
                .map(|e| e.prayer),
            Some(Prayer::Asr)
        );
    }

    #[test]
    fn the_wait_ends_at_midnight_and_the_next_day_starts_fresh() {
        let ev = events(&line(&TIMES), 0);
        let mut last = None;
        let (_, wait) = step(&ev, (DAY, TIMES[4] + 3 * 3600), &mut last);
        assert_eq!(wait, (86_401 - TIMES[4] - 3 * 3600) as u32);
        // After midnight the date differs: Fajr of the new day is not blocked by yesterday's pass.
        let tomorrow = (2026, 11, 13);
        assert_eq!(
            step(&ev, (tomorrow, TIMES[0]), &mut last)
                .0
                .map(|e| e.prayer),
            Some(Prayer::Fajr)
        );
    }

    #[test]
    fn a_dst_shift_moves_the_times_and_the_schedule_with_it() {
        let cairo = |offset_min| Params {
            lat: 30.0444,
            lon: 31.2357,
            utc_offset_min: offset_min,
            method: Method::Egyptian,
            hanafi: false,
            shift: [0; 6],
        };
        let winter = prayer::times(&cairo(120), DAY).unwrap();
        let summer = prayer::times(&cairo(180), DAY).unwrap();
        assert_eq!(summer[2] - winter[2], 3600);
        assert_eq!(
            events(&line(&summer), 0)[1].at - events(&line(&winter), 0)[1].at,
            3600
        );
    }

    #[test]
    fn midnight_crossing_reminders_land_on_the_right_day_without_firing_twice() {
        let fajr = Prayer::Fajr;
        let isha = Prayer::Isha;
        // Yesterday's Isha shifted to 00:20 today, tomorrow's Fajr shifted to 23:40 today, an Asr that is
        // tomorrow's proper (beyond today) and a prayer a minute before today's midnight.
        let tl = [
            (1200, isha),
            (17_000, fajr),
            (86_400 - 60, isha),
            (85_200, fajr),
            (86_400 + 20_000, Prayer::Asr),
        ];
        let ev = events(&tl, 10);
        let at: Vec<(i32, u32)> = ev.iter().map(|e| (e.at, e.lead)).collect();
        // Isha 00:20 (heads-up 00:10), Fajr 04:43, tomorrow's early Fajr 23:40, 23:59; nothing from tomorrow.
        assert_eq!(
            at,
            [
                (600, 10),
                (1200, 0),
                (16_400, 10),
                (17_000, 0),
                (84_600, 10),
                (85_200, 0),
                (85_740, 10),
                (86_340, 0)
            ]
        );
        // The heads-up of tomorrow's early prayer still falls today.
        let ev = events(&[(86_400 + 100, fajr)], 5);
        assert_eq!(
            ev.iter().map(|e| (e.at, e.lead)).collect::<Vec<_>>(),
            [(86_200, 5)]
        );
        // A reminder at 23:59:50 fires yesterday and is not repeated by today's list holding it at -10.
        let before = [(-10, fajr)];
        let ev = events(&before, 0);
        let mut last = Some(((2026, 11, 11), 86_380));
        let yesterday = events(&[(86_390, fajr)], 0);
        assert_eq!(
            step(&yesterday, ((2026, 11, 11), 86_395), &mut last)
                .0
                .map(|e| e.at),
            Some(86_390)
        );
        assert_eq!(step(&ev, ((2026, 11, 12), 5), &mut last).0, None);
        // Woken after midnight with nothing seen: 15 s late is inside the grace, so it fires once.
        let mut last = Some(((2026, 11, 11), 80_000));
        assert_eq!(
            step(&ev, ((2026, 11, 12), 5), &mut last).0.map(|e| e.at),
            Some(-10)
        );
        assert_eq!(step(&ev, ((2026, 11, 12), 6), &mut last).0, None);
        // More than two minutes late is stale and skipped.
        let mut last = Some(((2026, 11, 11), 80_000));
        assert_eq!(step(&ev, ((2026, 11, 12), 200), &mut last).0, None);
    }

    #[test]
    fn a_shifted_dst_day_moves_with_its_offset_and_the_page_shows_the_wrapped_time() {
        let cairo = |offset_min| Params {
            lat: 30.0444,
            lon: 31.2357,
            utc_offset_min: offset_min,
            method: Method::Egyptian,
            hanafi: false,
            shift: [0, 0, 0, 0, 0, 30],
        };
        let winter = prayer::times(&cairo(120), DAY).unwrap();
        let summer = prayer::times(&cairo(180), DAY).unwrap();
        assert_eq!(
            summer[5] - winter[5],
            3600,
            "the shift rides along with the offset"
        );
        // Isha 00:20 (a shifted 23:50) shows as a time of day, and is still the current prayer at 00:30.
        let night = Params {
            utc_offset_min: 450,
            ..cairo(120)
        };
        let t = prayer::times(&night, DAY).unwrap();
        let local = Local {
            ymd: DAY,
            secs: 0,
            offset_min: 450,
        };
        let v = view(Some(&night), "x", local, String::new(), &|s| {
            format!("{}", s.rem_euclid(86_400))
        });
        assert_eq!(v.rows[4].time, format!("{}", t[5].rem_euclid(86_400)));
    }

    #[test]
    fn countdown_rounds_minutes_up() {
        assert_eq!(countdown(2 * 3600 + 10 * 60 - 5), "in 2 h 10 min");
        assert_eq!(countdown(59), "in 1 min");
        assert_eq!(countdown(3 * 3600), "in 3 h");
        assert_eq!(countdown(61), "in 2 min");
    }

    #[test]
    fn only_the_prayer_itself_pauses_media() {
        let e = Event {
            at: 0,
            prayer: Prayer::Asr,
            lead: 0,
        };
        assert!(pauses_media(true, e, true));
        assert!(!pauses_media(true, Event { lead: 10, ..e }, true));
        assert!(!pauses_media(false, e, true) && !pauses_media(true, e, false));
    }

    #[test]
    fn notice_names_the_prayer() {
        let e = Event {
            at: 0,
            prayer: Prayer::Asr,
            lead: 0,
        };
        assert_eq!(notice(e), "Time for Asr");
        assert_eq!(notice(Event { lead: 10, ..e }), "Asr in 10 min");
    }

    #[test]
    fn the_page_names_the_next_prayer_and_marks_the_current_one() {
        let p = Params {
            lat: 30.0444,
            lon: 31.2357,
            utc_offset_min: 120,
            method: Method::Egyptian,
            hanafi: false,
            shift: [0; 6],
        };
        let times = prayer::times(&p, DAY).unwrap();
        let local = Local {
            ymd: DAY,
            secs: times[3] + 600,
            offset_min: 120,
        };
        let v = view(Some(&p), "x", local, "1 Rajab 1448 AH".into(), &|s| {
            format!("{s}")
        });
        assert_eq!(v.next, "Maghrib");
        assert_eq!(v.rows.len(), 5);
        assert_eq!(
            v.rows.iter().position(|r| r.current),
            Some(2),
            "Asr has started"
        );
        // After Isha the next is tomorrow's Fajr and Isha stays current; before Fajr too.
        let late = Local {
            secs: times[5] + 60,
            ..local
        };
        let v = view(Some(&p), "x", late, String::new(), &|s| format!("{s}"));
        assert_eq!(
            (v.next.as_str(), v.rows.iter().position(|r| r.current)),
            ("Fajr", Some(4))
        );
        let early = Local { secs: 60, ..local };
        assert_eq!(
            view(Some(&p), "x", early, String::new(), &|s| format!("{s}"))
                .rows
                .iter()
                .position(|r| r.current),
            Some(4)
        );
        let none = view(
            None,
            "Set your city in Settings",
            local,
            String::new(),
            &|s| format!("{s}"),
        );
        assert_eq!(none.message, Some("Set your city in Settings"));
    }

    #[test]
    fn hijri_text_reads_both_digit_sets_and_refuses_a_gregorian_answer() {
        assert_eq!(
            hijri_text("29|6|1448").as_deref(),
            Some("29 Jumada II 1448 AH")
        );
        assert_eq!(
            hijri_text("\u{662}\u{669}|\u{666}|\u{661}\u{664}\u{664}\u{668}").as_deref(),
            Some("29 Jumada II 1448 AH")
        );
        assert_eq!(hijri_text("12|11|2026"), None);
        assert_eq!(hijri_text("x|1|1448"), None);
        assert_eq!(hijri_text(""), None);
    }

    #[test]
    fn day_number_gives_the_unix_day() {
        assert_eq!(day_number((1970, 1, 1)), 0);
        assert_eq!(day_number((2026, 11, 13)) - day_number((2026, 11, 12)), 1);
        assert_eq!(day_number((2024, 3, 1)) - day_number((2024, 2, 28)), 2);
    }

    #[test]
    fn sound_files_are_found_by_extension_in_the_exe_folder_and_three_parents() {
        for ok in ["a.mp3", "B.WAV", "c.m4a", "d.Wma"] {
            assert!(is_audio(Path::new(ok)), "{ok}");
        }
        for no in ["a.txt", "mp3", "a.mp3.exe", ""] {
            assert!(!is_audio(Path::new(no)), "{no}");
        }
        let dirs = asset_dirs(Path::new(r"C:\a\b\c\d\e\roqa.exe"));
        let want: Vec<PathBuf> = [r"C:\a\b\c\d\e", r"C:\a\b\c\d", r"C:\a\b\c", r"C:\a\b"]
            .iter()
            .map(|d| Path::new(d).join("assets"))
            .collect();
        assert_eq!(dirs, want);
        assert_eq!(asset_dirs(Path::new("roqa.exe")).len(), 1);
    }

    #[test]
    fn the_default_sound_is_the_first_takbeer_file_and_silent_wins_over_it() {
        let found: Vec<String> = [
            r"C:\x\assets\adhan.mp3",
            r"C:\x\assets\Takbeer-1.mp3",
            r"C:\x\assets\takbeer2.wav",
        ]
        .map(String::from)
        .into();
        assert_eq!(default_sound(&found), Some(r"C:\x\assets\Takbeer-1.mp3"));
        assert_eq!(resolve("", &found), Some(r"C:\x\assets\Takbeer-1.mp3"));
        assert_eq!(resolve(SILENT, &found), None);
        assert_eq!(resolve(r"D:\mine.mp3", &found), Some(r"D:\mine.mp3"));
        assert_eq!(
            resolve("", &found[..1]),
            None,
            "no takbeer: a silent expansion"
        );
        assert_eq!(label(&found[0]), "adhan.mp3");
    }
}
