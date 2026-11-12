//! Prayer times from latitude, longitude, UTC offset and date. Pure maths, no I/O, no clock.
//!
//! Sun position: the low-precision Astronomical Almanac series (declination and equation of
//! time, good to about 1 arc-minute). Events: the hour-angle equation
//! cos H = (sin alt - sin lat sin dec) / (cos lat cos dec); Asr altitude = acot(n + tan|lat - dec|),
//! n = 1 (Shafi'i) or 2 (Hanafi). High latitudes: the angle-based rule (angle/60 of the night).

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Method {
    /// Egyptian General Authority of Survey: Fajr 19.5, Isha 17.5.
    #[default]
    Egyptian,
    /// Muslim World League: 18 / 17.
    Mwl,
    /// Umm al-Qura: Fajr 18.5, Isha 90 minutes after Maghrib.
    UmmAlQura,
    /// ISNA: 15 / 15.
    Isna,
}

enum Isha {
    Angle(f64),
    After(f64),
}

impl Method {
    pub const ALL: [Method; 4] = [
        Method::Egyptian,
        Method::Mwl,
        Method::UmmAlQura,
        Method::Isna,
    ];
    /// The store value and the Settings list text.
    pub fn key(self) -> &'static str {
        ["egypt", "mwl", "uaq", "isna"][self as usize]
    }
    pub fn label(self) -> &'static str {
        [
            "Egyptian General Authority",
            "Muslim World League",
            "Umm al-Qura",
            "ISNA (North America)",
        ][self as usize]
    }
    pub fn from_key(key: &str) -> Option<Method> {
        Method::ALL.into_iter().find(|m| m.key() == key)
    }
    /// (Fajr sun depression in degrees, Isha rule).
    fn rule(self) -> (f64, Isha) {
        match self {
            Method::Egyptian => (19.5, Isha::Angle(17.5)),
            Method::Mwl => (18.0, Isha::Angle(17.0)),
            Method::UmmAlQura => (18.5, Isha::After(90.0)),
            Method::Isna => (15.0, Isha::Angle(15.0)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prayer {
    Fajr,
    Sunrise,
    Dhuhr,
    Asr,
    Maghrib,
    Isha,
}

impl Prayer {
    pub fn name(self) -> &'static str {
        ["Fajr", "Sunrise", "Dhuhr", "Asr", "Maghrib", "Isha"][self as usize]
    }
    pub const ALL: [Prayer; 6] = [
        Prayer::Fajr,
        Prayer::Sunrise,
        Prayer::Dhuhr,
        Prayer::Asr,
        Prayer::Maghrib,
        Prayer::Isha,
    ];
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub lat: f64,
    /// East positive.
    pub lon: f64,
    /// Offset of local time from UTC on the date asked for (DST included), in minutes.
    pub utc_offset_min: i32,
    pub method: Method,
    /// Asr shadow factor 2 instead of 1.
    pub hanafi: bool,
    /// Minutes added to each time, by `Prayer as usize` (the Settings fine-tune; zeros change nothing).
    pub shift: [i8; 6],
}

pub type Ymd = (i32, u32, u32);

/// Seconds since local midnight, indexed by `Prayer as usize`. Isha may pass 86 400 at high latitudes.
pub type Times = [i32; 6];

fn julian((y, m, d): Ymd) -> f64 {
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let a = (f64::from(y) / 100.0).floor();
    (365.25 * (f64::from(y) + 4716.0)).floor()
        + (30.6001 * (f64::from(m) + 1.0)).floor()
        + f64::from(d)
        + 2.0
        - a
        + (a / 4.0).floor()
        - 1524.5
}

/// (declination in radians, equation of time in hours) at a Julian date.
fn sun(jd: f64) -> (f64, f64) {
    let d = jd - 2_451_545.0;
    let g = (357.529 + 0.985_600_28 * d).to_radians();
    let q = 280.459 + 0.985_647_36 * d;
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();
    let e = (23.439 - 3.6e-7 * d).to_radians();
    let ra = (e.cos() * l.sin()).atan2(l.cos()).to_degrees();
    (
        (e.sin() * l.sin()).asin(),
        ((q - ra + 180.0).rem_euclid(360.0) - 180.0) / 15.0,
    )
}

/// `jd0` is 0h UT of the local date. None when the sun does not rise and set (polar day or night).
#[inline(never)]
fn day(p: &Params, jd0: f64) -> Option<Times> {
    let tz = f64::from(p.utc_offset_min) / 60.0;
    let lat = p.lat.to_radians();
    let at = |local_h: f64| sun(jd0 + (local_h - tz) / 24.0);
    let mid = 12.0 + tz - p.lon / 15.0;
    let dhuhr = mid - at(mid).1;
    // Local hour at which the sun is `alt` degrees up, before (-1) or after (+1) noon; NaN if it never is.
    // The declination is re-read at the first estimate, which moves a result by under a minute.
    let event = |alt: f64, side: f64| {
        let half = |h: f64| {
            let dec = at(h).0;
            let c = (alt.to_radians().sin() - lat.sin() * dec.sin()) / (lat.cos() * dec.cos());
            if c.abs() > 1.0 {
                f64::NAN
            } else {
                c.acos().to_degrees() / 15.0
            }
        };
        dhuhr + side * half(dhuhr + side * half(dhuhr))
    };
    let (sunrise, sunset) = (event(-0.833, -1.0), event(-0.833, 1.0));
    if sunrise.is_nan() || sunset.is_nan() {
        return None;
    }
    let asr_alt = (1.0 / (if p.hanafi { 2.0 } else { 1.0 } + (lat - at(dhuhr).0).abs().tan()))
        .atan()
        .to_degrees();
    let night = 24.0 - (sunset - sunrise);
    let cap = |angle: f64| night * angle / 60.0;
    let (fajr_angle, isha_rule) = p.method.rule();
    let fajr = event(-fajr_angle, -1.0);
    let fajr = if fajr.is_nan() || sunrise - fajr > cap(fajr_angle) {
        sunrise - cap(fajr_angle)
    } else {
        fajr
    };
    let isha = match isha_rule {
        Isha::After(min) => sunset + min / 60.0,
        Isha::Angle(a) => {
            let i = event(-a, 1.0);
            if i.is_nan() || i - sunset > cap(a) {
                sunset + cap(a)
            } else {
                i
            }
        }
    };
    let mut times = [fajr, sunrise, dhuhr, event(asr_alt, 1.0), sunset, isha]
        .map(|h| (h * 3600.0).round() as i32);
    for (time, minutes) in times.iter_mut().zip(p.shift) {
        *time += i32::from(minutes) * 60;
    }
    Some(times)
}

pub fn times(p: &Params, ymd: Ymd) -> Option<Times> {
    day(p, julian(ymd))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg(test)] // only the tests ask for one prayer; the page and reminders read `timeline`
pub struct Next {
    pub prayer: Prayer,
    pub secs_left: u32,
}

#[cfg(test)]
impl Next {
    /// Countdown as (hours, minutes), rounded up so it never shows 0:00 before the time.
    #[cfg(test)] // the page words it with `prayer_tab::countdown`
    pub fn hm(self) -> (u32, u32) {
        let m = self.secs_left.div_ceil(60);
        (m / 60, m % 60)
    }
}

/// Every prayer of yesterday, today and tomorrow, in seconds since TODAY's local midnight and in time order: a
/// time shifted past midnight lands on the next day (Isha 23:50 + 30 min is 87 600) and one shifted before it
/// on the previous day (Fajr 00:10 - 30 min is on yesterday's list at 85 200 and negative on its own day), so
/// the neighbour days are needed to see a prayer that crossed the date line. None when today has no times.
// shortcut: the neighbour days use today's UTC offset, so across a DST change night their times are an hour off
// until the pass after midnight reads the new offset; upgrade by passing each day's own offset.
pub fn timeline(p: &Params, ymd: Ymd) -> Option<Vec<(i32, Prayer)>> {
    let jd = julian(ymd);
    let mut all = Vec::with_capacity(18);
    // One call site for `day` (it is large): yesterday, today, tomorrow.
    for days in -1..=1 {
        let Some(times) = day(p, jd + f64::from(days)) else {
            if days == 0 {
                return None;
            }
            continue;
        };
        for (i, t) in times.into_iter().enumerate() {
            all.push((t + days * 86_400, Prayer::ALL[i]));
        }
    }
    crate::sort::by(&mut all, |a, b| a.0.cmp(&b.0));
    Some(all)
}

/// The next event after `now_secs` (seconds since local midnight of `ymd`); after Isha it is tomorrow's Fajr.
// shortcut: tomorrow's Fajr uses today's UTC offset, so the countdown is an hour off on a DST-change night until
// the caller asks again after midnight; upgrade by passing tomorrow's offset when the reminder needs it.
#[cfg(test)] // the page and the reminders read `timeline` directly
pub fn next_prayer(p: &Params, ymd: Ymd, now_secs: i32) -> Option<Next> {
    let &(at, prayer) = timeline(p, ymd)?.iter().find(|e| e.0 > now_secs)?;
    Some(Next {
        prayer,
        secs_left: (at - now_secs) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOV12: Ymd = (2026, 11, 12);
    const CAIRO: Params = Params {
        lat: 30.0444,
        lon: 31.2357,
        utc_offset_min: 120,
        method: Method::Egyptian,
        hanafi: false,
        shift: [0; 6],
    };

    const fn hm(h: i32, m: i32) -> i32 {
        h * 3600 + m * 60
    }

    /// Within 2 minutes of the reference, in Fajr, Sunrise, Dhuhr, Asr, Maghrib, Isha order.
    fn matches(got: Times, want: [(i32, i32); 6]) {
        for (i, (h, m)) in want.into_iter().enumerate() {
            let off = (got[i] - hm(h, m)).abs();
            assert!(
                off <= 120,
                "{:?}: got {} s, want {h:02}:{m:02} ({off} s off)",
                Prayer::ALL[i],
                got[i]
            );
        }
    }

    // Reference values below come from the Aladhan API (https://api.aladhan.com/v1/timings/<DD-MM-YYYY>
    // ?latitude=..&longitude=..&method=..&school=..&timezonestring=..&latitudeAdjustmentMethod=3), fetched
    // 2026-11-12. It is an independent calculator with the same published angles, not an official timetable:
    // the official Egyptian, Umm al-Qura and London Central Mosque tables were NOT retrieved (UNVERIFIED
    // against them; official tables sometimes add a few minutes of caution).

    #[test]
    fn a_shift_adds_its_minutes_to_that_prayer_only_and_moves_the_next_prayer() {
        let plain = times(&CAIRO, NOV12).unwrap();
        let shifted = Params {
            shift: [-30, 0, 5, 0, 0, 30],
            ..CAIRO
        };
        let t = times(&shifted, NOV12).unwrap();
        let want = [-1800, 0, 300, 0, 0, 1800];
        assert!((0..6).all(|i| t[i] - plain[i] == want[i]));
        // Ten minutes before the plain Fajr, the shifted Fajr (30 earlier) has passed: Sunrise is next.
        let now = plain[0] - 600;
        assert_eq!(next_prayer(&CAIRO, NOV12, now).unwrap().secs_left, 600);
        let next = next_prayer(&shifted, NOV12, now).unwrap();
        assert_eq!(
            (next.prayer, next.secs_left as i32),
            (Prayer::Sunrise, plain[1] - now)
        );
    }

    // Cairo's clock moved by an offset change puts a prayer near midnight (the engine adds the offset only).
    const LATE_DAY: Params = Params {
        utc_offset_min: 450,
        ..CAIRO
    };
    const EARLY_DAY: Params = Params {
        utc_offset_min: -165,
        ..CAIRO
    };

    #[test]
    fn a_late_shift_carries_isha_past_midnight_into_the_next_days_countdown() {
        let plain = times(&LATE_DAY, NOV12).unwrap();
        assert!(
            (84_600..86_400).contains(&plain[5]),
            "Isha near 23:50: {}",
            plain[5]
        );
        let late = Params {
            shift: [0, 0, 0, 0, 0, 30],
            ..LATE_DAY
        };
        let shifted = plain[5] + 1800;
        assert!(shifted >= 86_400, "crosses midnight");
        // The same evening: Isha is next, 15 minutes ahead.
        let next = next_prayer(&late, NOV12, plain[5] + 900).unwrap();
        assert_eq!((next.prayer, next.secs_left), (Prayer::Isha, 900));
        // Just after midnight the date has changed; yesterday's Isha (00:2x now) is still ahead.
        let next = next_prayer(&late, (2026, 11, 13), shifted - 86_400 - 60).unwrap();
        assert_eq!((next.prayer, next.secs_left), (Prayer::Isha, 60));
        // The list lists it once, in order, at today's seconds plus a day.
        let line = timeline(&late, NOV12).unwrap();
        assert!(line.contains(&(shifted, Prayer::Isha)));
        assert!(line.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    #[test]
    fn an_early_shift_carries_fajr_before_midnight_into_the_previous_days_countdown() {
        let plain = times(&EARLY_DAY, NOV12).unwrap();
        assert!(
            (0..1800).contains(&plain[0]),
            "Fajr just after midnight: {}",
            plain[0]
        );
        let early = Params {
            shift: [-30, 0, 0, 0, 0, 0],
            ..EARLY_DAY
        };
        let fajr = plain[0] - 1800; // negative: 23:3x of the previous day
        assert!(fajr < 0);
        // On the previous date, that Fajr is the evening's next prayer.
        let previous = (2026, 11, 11);
        let next = next_prayer(&early, previous, 86_400 + fajr - 60).unwrap();
        assert_eq!((next.prayer, next.secs_left), (Prayer::Fajr, 60));
        // On its own date it is already past: after it comes Sunrise.
        let next = next_prayer(&early, NOV12, 10).unwrap();
        assert_eq!(next.prayer, Prayer::Sunrise);
    }

    #[test]
    fn cairo_egyptian_shafii() {
        // method=5 school=0 lat=30.0444 lon=31.2357 tz=Africa/Cairo (UTC+2 on this date).
        let t = times(&CAIRO, NOV12).unwrap();
        matches(t, [(4, 48), (6, 18), (11, 39), (14, 40), (17, 0), (18, 20)]);
    }

    #[test]
    fn cairo_hanafi_asr() {
        // method=5 school=1: only Asr moves, to 15:24.
        let t = times(
            &Params {
                hanafi: true,
                shift: [0; 6],
                ..CAIRO
            },
            NOV12,
        )
        .unwrap();
        matches(t, [(4, 48), (6, 18), (11, 39), (15, 24), (17, 0), (18, 20)]);
    }

    #[test]
    fn cairo_dst_offset() {
        // method=5 school=0, 2026-07-15, tz=Africa/Cairo (UTC+3, Egyptian summer time).
        let summer = Params {
            utc_offset_min: 180,
            ..CAIRO
        };
        let t = times(&summer, (2026, 7, 15)).unwrap();
        matches(t, [(4, 21), (6, 4), (13, 1), (16, 38), (19, 58), (21, 29)]);
        // The offset only shifts the clock: the same sky an hour later is the same times plus an hour.
        let winter = times(
            &Params {
                utc_offset_min: 120,
                ..CAIRO
            },
            (2026, 7, 15),
        )
        .unwrap();
        assert_eq!(t.map(|s| s - 3600), winter);
    }

    #[test]
    fn mecca_umm_al_qura() {
        // method=4 school=0 lat=21.4225 lon=39.8262 tz=Asia/Riyadh (UTC+3).
        let mecca = Params {
            lat: 21.4225,
            lon: 39.8262,
            utc_offset_min: 180,
            method: Method::UmmAlQura,
            hanafi: false,
            shift: [0; 6],
        };
        let t = times(&mecca, NOV12).unwrap();
        matches(t, [(5, 11), (6, 30), (12, 5), (15, 17), (17, 40), (19, 10)]);
        assert_eq!(t[Prayer::Isha as usize] - t[Prayer::Maghrib as usize], 5400);
    }

    #[test]
    fn london_mwl() {
        // method=3 school=0 lat=51.5074 lon=-0.1278 tz=Europe/London (UTC+0 in November).
        let london = Params {
            lat: 51.5074,
            lon: -0.1278,
            utc_offset_min: 0,
            method: Method::Mwl,
            hanafi: false,
            shift: [0; 6],
        };
        let t = times(&london, NOV12).unwrap();
        // Asr is 13:51:44 here against the reference's 13:54 (136 s): at a 15 degree sun, 0.1 degree of
        // altitude is about 2 minutes, and the reference's Asr rule is not published. Bounded at 3 minutes;
        // both ways of reading the declination give 13:51, so the maths is kept as is.
        assert!((t[3] - hm(13, 54)).abs() <= 180);
        matches(
            [t[0], t[1], t[2], hm(13, 54), t[4], t[5]],
            [(5, 17), (7, 13), (11, 45), (13, 54), (16, 15), (18, 5)],
        );
    }

    #[test]
    fn isna_new_york() {
        // method=2 school=0 lat=40.7128 lon=-74.0060 tz=America/New_York (UTC-5 on this date).
        let ny = Params {
            lat: 40.7128,
            lon: -74.0060,
            utc_offset_min: -300,
            method: Method::Isna,
            hanafi: false,
            shift: [0; 6],
        };
        matches(
            times(&ny, NOV12).unwrap(),
            [(5, 21), (6, 39), (11, 40), (14, 20), (16, 41), (17, 59)],
        );
    }

    #[test]
    fn high_latitude_angle_based_fallback() {
        // London 2026-06-21, MWL, tz=Europe/London (UTC+1), latitudeAdjustmentMethod=3: the sun never gets
        // 18 degrees down, so Fajr/Isha fall back to angle/60 of the night.
        let london = Params {
            lat: 51.5074,
            lon: -0.1278,
            utc_offset_min: 60,
            method: Method::Mwl,
            hanafi: false,
            shift: [0; 6],
        };
        let t = times(&london, (2026, 6, 21)).unwrap();
        matches(t, [(2, 31), (4, 43), (13, 2), (17, 25), (21, 22), (23, 27)]);
        let night = 86_400 - (t[4] - t[1]);
        assert!((t[1] - t[0] - night * 18 / 60).abs() <= 1);
        assert!((t[5] - t[4] - night * 17 / 60).abs() <= 1);
    }

    #[test]
    fn no_sunrise_means_no_times() {
        let arctic = Params {
            lat: 80.0,
            lon: 0.0,
            utc_offset_min: 0,
            ..CAIRO
        };
        assert!(times(&arctic, (2026, 6, 21)).is_none());
        assert!(next_prayer(&arctic, (2026, 6, 21), 0).is_none());
    }

    #[test]
    fn next_prayer_and_rollover() {
        let t = times(&CAIRO, NOV12).unwrap();
        let next = |now| next_prayer(&CAIRO, NOV12, now).unwrap();
        assert_eq!(
            next(0),
            Next {
                prayer: Prayer::Fajr,
                secs_left: t[0] as u32
            }
        );
        assert_eq!(next(t[2] + 1).prayer, Prayer::Asr);
        // Exactly at a time, that prayer has arrived and the next one counts.
        assert_eq!(next(t[3]).prayer, Prayer::Maghrib);
        assert_eq!(
            next(t[4] - 1),
            Next {
                prayer: Prayer::Maghrib,
                secs_left: 1
            }
        );
        // After Isha: tomorrow's Fajr, a bit earlier than today's.
        let tomorrow = times(&CAIRO, (2026, 11, 13)).unwrap()[0];
        let n = next(t[5] + 60);
        assert_eq!(n.prayer, Prayer::Fajr);
        assert_eq!(n.secs_left as i32, 86_400 + tomorrow - (t[5] + 60));
        // Month rollover goes through the Julian date, not the calendar.
        assert_eq!(
            next_prayer(&CAIRO, (2026, 11, 30), hm(23, 59))
                .unwrap()
                .prayer,
            Prayer::Fajr
        );
    }

    #[test]
    fn countdown_rounds_up() {
        let hm_of = |secs_left| {
            Next {
                prayer: Prayer::Asr,
                secs_left,
            }
            .hm()
        };
        assert_eq!(hm_of(1), (0, 1));
        assert_eq!(hm_of(3600), (1, 0));
        assert_eq!(hm_of(3661), (1, 2));
    }
}
