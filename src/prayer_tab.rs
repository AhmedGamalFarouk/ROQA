//! The Prayer tab (features P1): pure rules around `prayer.rs`. When a reminder is due and the one wait until
//! the next, the page's ready-made text, the countdown wording, the Hijri text and which audio file plays.
//! No clock, window or file is touched here; `prayer_os.rs` supplies the facts.

use crate::prayer::{self, Params, Prayer, Times, Ymd};
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

/// Today's reminders in time order: each of the five prayers, and with `lead` minutes set, a heads-up before it.
pub fn events(times: &Times, lead: u32) -> Vec<Event> {
    let mut all = Vec::with_capacity(10);
    for prayer in FIVE {
        let at = times[prayer as usize];
        all.push(Event {
            at,
            prayer,
            lead: 0,
        });
        if lead > 0 && at > lead as i32 * 60 {
            all.push(Event {
                at: at - lead as i32 * 60,
                prayer,
                lead,
            });
        }
    }
    all.sort_by_key(|e| e.at);
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
    let fire = events
        .iter()
        .rev()
        .find(|e| {
            last.is_some() && e.at <= secs && secs - e.at <= GRACE && Some((ymd, e.at)) > *last
        })
        .copied();
    *last = Some(last.map_or((ymd, secs), |old| old.max((ymd, secs))));
    let next = events
        .iter()
        .find(|e| e.at > secs)
        .map_or(i32::MAX, |e| e.at);
    let wait = next.min(86_400 + 1).saturating_sub(secs).max(1);
    (fire, wait as u32)
}

/// The notice text: "Asr" at the time, "Asr in 10 min" for a heads-up.
pub fn notice(e: Event) -> String {
    if e.lead == 0 {
        format!("Time for {}", e.prayer.name())
    } else {
        format!("{} in {} min", e.prayer.name(), e.lead)
    }
}

/// "in 2 h 10 min" (drawn) or "in 2 hours 10 minutes" (spoken); minutes round up, so never "in 0 min" early.
pub fn countdown(secs_left: u32, spoken: bool) -> String {
    let minutes = secs_left.div_ceil(60);
    let (h, m) = (minutes / 60, minutes % 60);
    let unit = |n: u32, short: &str, long: &str| {
        if !spoken {
            format!("{n} {short}")
        } else if n == 1 {
            format!("1 {long}")
        } else {
            format!("{n} {long}s")
        }
    };
    let parts: Vec<String> = [(h, "h", "hour"), (m, "min", "minute")]
        .into_iter()
        .filter(|&(n, ..)| n > 0)
        .map(|(n, short, long)| unit(n, short, long))
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
    pub next: String,
    pub left: String,
    pub hijri: String,
    pub rows: Vec<Row>,
    /// The UI Automation name, such as "Next prayer Asr in 2 hours 10 minutes".
    pub name: String,
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
        name: message.to_owned(),
        ..View::default()
    };
    let Some(params) = params else {
        return none(message);
    };
    let (Some(times), Some(next)) = (
        prayer::times(params, local.ymd),
        prayer::next_prayer(params, local.ymd, local.secs),
    ) else {
        return none("No prayer times for this place today");
    };
    // The current prayer is the latest started; before Fajr it is still last night's Isha.
    let current = FIVE
        .iter()
        .rposition(|&p| times[p as usize] <= local.secs)
        .unwrap_or(FIVE.len() - 1);
    let (next_name, short) = (next.prayer.name(), countdown(next.secs_left, false));
    View {
        message: None,
        name: format!(
            "Next prayer {next_name} {}",
            countdown(next.secs_left, true)
        ),
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
    const TIMES: Times = [
        4 * 3600 + 900,
        6 * 3600,
        11 * 3600 + 1800,
        14 * 3600 + 1200,
        16 * 3600 + 600,
        17 * 3600 + 900,
    ];

    #[test]
    fn events_are_ordered_and_a_lead_adds_a_heads_up_before_each() {
        let plain = events(&TIMES, 0);
        assert_eq!(plain.len(), 5);
        assert!(
            plain
                .iter()
                .all(|e| e.lead == 0 && e.prayer != Prayer::Sunrise)
        );
        let with = events(&TIMES, 10);
        assert_eq!(with.len(), 10);
        assert!(with.windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(
            (with[0].prayer, with[0].lead, with[0].at),
            (Prayer::Fajr, 10, TIMES[0] - 600)
        );
    }

    #[test]
    fn a_step_fires_on_time_once_and_waits_for_the_next() {
        let ev = events(&TIMES, 0);
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
        let ev = events(&TIMES, 0);
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
        let ev = events(&TIMES, 0);
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
        };
        let winter = prayer::times(&cairo(120), DAY).unwrap();
        let summer = prayer::times(&cairo(180), DAY).unwrap();
        assert_eq!(summer[2] - winter[2], 3600);
        assert_eq!(events(&summer, 0)[1].at - events(&winter, 0)[1].at, 3600);
    }

    #[test]
    fn countdown_reads_short_and_spoken() {
        assert_eq!(countdown(2 * 3600 + 10 * 60 - 5, false), "in 2 h 10 min");
        assert_eq!(countdown(2 * 3600 + 10 * 60, true), "in 2 hours 10 minutes");
        assert_eq!(countdown(3600, true), "in 1 hour");
        assert_eq!(countdown(59, false), "in 1 min");
        assert_eq!(countdown(3 * 3600, false), "in 3 h");
        assert_eq!(countdown(61, true), "in 2 minutes");
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
        assert!(v.name.starts_with("Next prayer Maghrib in "), "{}", v.name);
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
        assert_eq!(
            (none.message, none.name.as_str()),
            (
                Some("Set your city in Settings"),
                "Set your city in Settings"
            )
        );
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
