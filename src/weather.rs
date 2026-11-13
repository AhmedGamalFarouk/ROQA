//! Weather (features W1): the pure half. A tiny JSON scanner for the few Open-Meteo fields, the WMO code
//! to icon map, the request paths, the cached report and place with their store lines, the refresh rules
//! (`Feed`) and the page text (`View`). No window, thread or network: `weather_os` does the I/O and the
//! host feeds in the facts. Coordinates are kept in the store only, never logged.

use std::{
    fmt::Write,
    time::{Duration, Instant},
};

pub const FORECAST_HOST: &str = "api.open-meteo.com";
pub const GEOCODE_HOST: &str = "geocoding-api.open-meteo.com";
/// The hours kept after "now", and the longest city the settings keep.
pub const HOURS: usize = 4;
pub const MAX_CITY: usize = 64;
/// Cached data older than this is refetched when the Weather page opens; the background rhythm is slower.
const OPEN_STALE: u64 = 3600;
const BACKGROUND: u64 = 3 * 3600;
/// A job with no answer after this is given up on (its late result is ignored).
const DEAD: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Units {
    #[default]
    Auto,
    Celsius,
    Fahrenheit,
}
impl Units {
    pub const ALL: [Units; 3] = [Units::Auto, Units::Celsius, Units::Fahrenheit];
    pub fn key(self) -> &'static str {
        ["auto", "c", "f"][self as usize]
    }
    pub fn from_key(key: &str) -> Option<Units> {
        Units::ALL.into_iter().find(|units| units.key() == key)
    }
    pub fn label(self) -> &'static str {
        ["Automatic", "Celsius (°C)", "Fahrenheit (°F)"][self as usize]
    }
    /// °F for the explicit choice, else by the Windows locale's `region` (the few countries that use it).
    pub fn fahrenheit(self, region: &str) -> bool {
        match self {
            Units::Auto => matches!(
                region,
                "US" | "LR" | "MM" | "BS" | "BZ" | "KY" | "PW" | "FM" | "MH"
            ),
            Units::Celsius => false,
            Units::Fahrenheit => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Icon {
    #[default]
    Clear,
    PartlyCloudy,
    Cloud,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Storm,
}
impl Icon {
    /// The WMO weather interpretation code as one of eight pictures.
    pub fn from_wmo(code: u8) -> Icon {
        match code {
            0 | 1 => Icon::Clear,
            2 => Icon::PartlyCloudy,
            45 | 48 => Icon::Fog,
            51..=57 => Icon::Drizzle,
            61..=67 | 80..=82 => Icon::Rain,
            71..=77 | 85 | 86 => Icon::Snow,
            95..=99 => Icon::Storm,
            _ => Icon::Cloud,
        }
    }
    pub fn label(self) -> &'static str {
        [
            "Clear",
            "Partly cloudy",
            "Cloudy",
            "Fog",
            "Drizzle",
            "Rain",
            "Snow",
            "Thunderstorm",
        ][self as usize]
    }
}

/// A place in 1e-4 degrees (about 11 m), and whether the device located it (else it was geocoded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub lat: i32,
    pub lon: i32,
    pub detected: bool,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hour {
    pub hour: u8,
    pub tenths: i16,
    pub code: u8,
}
/// One fetch: when (Unix seconds), the temperatures in tenths of a degree Celsius and the next hours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub at: u64,
    pub tenths: i16,
    pub code: u8,
    pub hi: i16,
    pub lo: i16,
    pub hours: Vec<Hour>,
}

// ---- the scanner --------------------------------------------------------------------------------------

/// What follows `"key":` (spaces skipped), from the first such key in `json`.
fn value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = json;
    while let Some(at) = rest.find(key) {
        let after = &rest[at + key.len()..];
        if rest[..at].ends_with('"')
            && let Some(found) = after
                .strip_prefix('"')
                .map(str::trim_start)
                .and_then(|a| a.strip_prefix(':'))
        {
            return Some(found.trim_start());
        }
        rest = after;
    }
    None
}

/// The `{...}` or `[...]` that starts `text`, brackets balanced and strings skipped.
fn group(text: &str) -> Option<&str> {
    if !matches!(text.as_bytes().first()?, b'{' | b'[') {
        return None;
    }
    let (mut depth, mut quoted, mut escaped) = (0u32, false, false);
    for (at, byte) in text.bytes().enumerate() {
        if quoted {
            (quoted, escaped) = match (escaped, byte) {
                (true, _) => (true, false),
                (false, b'\\') => (true, true),
                (false, b'"') => (false, false),
                _ => (true, false),
            };
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[..=at]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The number that starts `text` (`null`, a string or junk is `None`).
fn number(text: &str) -> Option<f64> {
    let end = text.find([',', ']', '}']).unwrap_or(text.len());
    text[..end]
        .trim()
        .parse()
        .ok()
        .filter(|n: &f64| n.is_finite())
}
fn num(json: &str, key: &str) -> Option<f64> {
    number(value(json, key)?)
}
fn object<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    group(value(json, key)?)
}
/// The items of the array under `key`, as raw tokens (none when it is absent or cut off).
fn list<'a>(json: &'a str, key: &str) -> impl Iterator<Item = &'a str> {
    let inner = object(json, key)
        .and_then(|array| array.strip_prefix('[')?.strip_suffix(']'))
        .filter(|inner| !inner.trim().is_empty());
    inner
        .into_iter()
        .flat_map(|inner| inner.split(',').map(str::trim))
}
/// The string that starts `text`, unescaped (`\uXXXX` outside the surrogate range only).
fn string(text: &str) -> Option<String> {
    let mut chars = text.strip_prefix('"')?.chars();
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(
                        u32::from_str_radix(&hex, 16)
                            .ok()
                            .and_then(char::from_u32)?,
                    );
                }
                'n' | 'r' | 't' => out.push(' '),
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
}

fn tenths(degrees: f64) -> i16 {
    (degrees * 10.0).round().clamp(-1000.0, 1000.0) as i16
}
fn wmo(code: f64) -> u8 {
    code.round().clamp(0.0, 99.0) as u8
}

/// Reads an Open-Meteo forecast. `None` without a current temperature and code; the daily range falls
/// back to the current temperature and a missing or cut hourly list gives no hours.
pub fn parse_forecast(json: &str, at: u64) -> Option<Report> {
    let current = object(json, "current")?;
    let (now, code) = (
        tenths(num(current, "temperature_2m")?),
        wmo(num(current, "weather_code")?),
    );
    let this_hour = value(current, "time")?.strip_prefix('"')?.get(..13)?;
    let daily = object(json, "daily");
    let day = |key: &str| {
        let first = daily.and_then(|daily| list(daily, key).next());
        first.and_then(number).map_or(now, tenths)
    };
    let hours = object(json, "hourly").map_or_else(Vec::new, |hourly| {
        let rows = list(hourly, "time")
            .zip(list(hourly, "temperature_2m"))
            .zip(list(hourly, "weather_code"));
        rows.filter_map(|((time, temp), code)| {
            let time = time.strip_prefix('"')?;
            (time.get(..13)? > this_hour).then_some(())?;
            Some(Hour {
                hour: time.get(11..13)?.parse().ok().filter(|hour| *hour < 24)?,
                tenths: tenths(number(temp)?),
                code: wmo(number(code)?),
            })
        })
        .take(HOURS)
        .collect()
    });
    Some(Report {
        at,
        tenths: now,
        code,
        hi: day("temperature_2m_max"),
        lo: day("temperature_2m_min"),
        hours,
    })
}

/// The first geocoding result; `None` for no match (the reply then has no `results`).
pub fn parse_place(json: &str) -> Option<Place> {
    let first = group(value(json, "results")?.strip_prefix('[')?.trim_start())?;
    let name: String = string(value(first, "name")?)?
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CITY)
        .collect();
    let (lat, lon) = (num(first, "latitude")?, num(first, "longitude")?);
    Place::new(name.trim(), lat, lon, false)
}

impl Place {
    fn new(name: &str, lat: f64, lon: f64, detected: bool) -> Option<Place> {
        let valid = lat.abs() <= 90.0 && lon.abs() <= 180.0 && !name.is_empty();
        valid.then(|| Place {
            lat: (lat * 1e4).round() as i32,
            lon: (lon * 1e4).round() as i32,
            detected,
            name: name.to_owned(),
        })
    }
    /// A place the device reported: no name of its own.
    pub fn located(lat: f64, lon: f64) -> Option<Place> {
        Place::new("Current location", lat, lon, true)
    }
}

/// `-12.3456` for -123456 (tenths of thousandths): the request's number format.
fn fixed(micro: i32) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    format!("{sign}{}.{:04}", micro.abs() / 10_000, micro.abs() % 10_000)
}

pub fn forecast_path(place: &Place) -> String {
    format!(
        "/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code\
         &hourly=temperature_2m,weather_code&daily=temperature_2m_max,temperature_2m_min\
         &forecast_days=1&forecast_hours=6&timezone=auto",
        fixed(place.lat),
        fixed(place.lon)
    )
}

/// The geocoding request for a typed city; `None` for a blank one, so nothing is ever asked for it.
pub fn geocode_path(city: &str) -> Option<String> {
    let city = city.trim();
    if city.is_empty() {
        return None;
    }
    let mut path = String::from("/v1/search?count=1&format=json&name=");
    for byte in city.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                path.push(byte as char)
            }
            _ => {
                let _ = write!(path, "%{byte:02X}");
            }
        }
    }
    Some(path)
}

// ---- the store lines ----------------------------------------------------------------------------------

impl Place {
    /// `lat,lon,detected,name`: the name last, so it may hold commas.
    pub fn to_line(&self) -> String {
        format!(
            "{},{},{},{}",
            self.lat,
            self.lon,
            u8::from(self.detected),
            self.name
        )
    }
    pub fn from_line(line: &str) -> Option<Place> {
        let mut parts = line.splitn(4, ',');
        let (lat, lon) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
        let detected = match parts.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let name = parts.next()?.trim();
        let ok = (-900_000..=900_000).contains(&lat) && (-1_800_000..=1_800_000).contains(&lon);
        (ok && !name.is_empty()
            && name.chars().count() <= MAX_CITY
            && !name.contains(char::is_control))
        .then(|| Place {
            lat,
            lon,
            detected,
            name: name.to_owned(),
        })
    }
}
impl Report {
    /// `at,tenths,code,hi,lo` then `,hour:tenths:code` per hour.
    pub fn to_line(&self) -> String {
        let mut line = format!(
            "{},{},{},{},{}",
            self.at, self.tenths, self.code, self.hi, self.lo
        );
        for h in &self.hours {
            let _ = write!(line, ",{}:{}:{}", h.hour, h.tenths, h.code);
        }
        line
    }
    pub fn from_line(line: &str) -> Option<Report> {
        let mut parts = line.split(',');
        let mut next = || parts.next();
        let report = Report {
            at: next()?.parse().ok()?,
            tenths: next()?.parse().ok()?,
            code: next()?.parse().ok().filter(|c| *c < 100)?,
            hi: next()?.parse().ok()?,
            lo: next()?.parse().ok()?,
            hours: Vec::new(),
        };
        let mut hours = Vec::new();
        for part in parts.take(HOURS) {
            let mut fields = part.split(':');
            hours.push(Hour {
                hour: fields.next()?.parse().ok().filter(|h| *h < 24)?,
                tenths: fields.next()?.parse().ok()?,
                code: fields.next()?.parse().ok().filter(|c| *c < 100)?,
            });
        }
        Some(Report { hours, ..report })
    }
}

// ---- when to fetch ------------------------------------------------------------------------------------

/// A city is set or auto-detect is on: until then nothing is requested at all.
pub fn configured(detect: bool, city: &str) -> bool {
    detect || !city.trim().is_empty()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Detect,
    Known,
    City,
}
/// Where a job looks for the place, in order: the device when auto-detect is on, a place already held,
/// then the typed city. Empty when nothing is configured.
pub fn sources(detect: bool, known: bool, city: &str) -> Vec<Source> {
    [
        (detect, Source::Detect),
        (known, Source::Known),
        (!city.trim().is_empty(), Source::City),
    ]
    .into_iter()
    .filter_map(|(on, source)| on.then_some(source))
    .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    /// No place: detection denied or unavailable and no usable city, or the city was not found.
    Unlocated,
    Network,
    Data,
}

/// What the worker needs; the place goes in only when it should be reused.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: u64,
    pub detect: bool,
    pub known: Option<Place>,
    pub city: String,
}
pub struct Outcome {
    /// A place resolved by this job (kept even if the forecast then failed).
    pub place: Option<Place>,
    pub result: Result<Report, Fail>,
}

/// Does the feed run? The Weather tab alone decides it; the Prayer tab only needs the place, so with Weather
/// off it takes one lookup (none once a place is held) and never refreshes the report.
pub fn feed_wanted(weather_on: bool, prayer_on: bool, has_place: bool) -> bool {
    weather_on || (prayer_on && !has_place)
}

/// Facts the host reads for one decision.
pub struct Ctx {
    pub enabled: bool,
    pub configured: bool,
    /// A notch is shown (not hidden, not suppressed by a full-screen app).
    pub visible: bool,
    /// The Weather page is open on a visible notch.
    pub open: bool,
    pub wall: u64,
}

/// The place and report the host holds, the job in flight and the back-off after failures. Pure: `Instant`s
/// come in, nothing is read or started here.
pub struct Feed {
    pub place: Option<Place>,
    pub report: Option<Report>,
    /// Bumped on every change a page would show.
    pub rev: u32,
    pub last_fail: Option<Fail>,
    /// Look the device up again at the next job (a new session, or the city was the fallback).
    redetect: bool,
    inflight: Option<(u64, Instant)>,
    next_id: u64,
    fails: u32,
    blocked_until: Option<Instant>,
}

/// 15 minutes after the first failure, then an hour: no retry loop.
fn backoff(fails: u32) -> Duration {
    Duration::from_secs(if fails <= 1 { 15 * 60 } else { 3600 })
}

impl Feed {
    pub fn new(place: Option<Place>, report: Option<Report>, detect: bool) -> Self {
        Self {
            place,
            report,
            rev: 0,
            last_fail: None,
            redetect: detect,
            inflight: None,
            next_id: 0,
            fails: 0,
            blocked_until: None,
        }
    }
    /// The city or auto-detect changed: forget the place and what was fetched for it, and allow a fetch now.
    pub fn reset(&mut self, detect: bool) {
        // `next_id` carries on, so a late answer from before the reset never matches a new job.
        *self = Self {
            rev: self.rev.wrapping_add(1),
            next_id: self.next_id,
            ..Self::new(None, None, detect)
        };
    }
    /// Seconds since the report; a report stamped in the future counts as ancient.
    fn age(&self, wall: u64) -> Option<u64> {
        let report = self.report.as_ref()?;
        Some(wall.checked_sub(report.at).unwrap_or(u64::MAX))
    }
    /// Time for a job? Open on the page: data older than an hour. Otherwise, only with a notch showing and
    /// the user around: data older than three hours (or none). Never off, unconfigured, hidden (for the
    /// background), in flight or backing off. `idle` is asked last, only when it could matter.
    pub fn wants(&self, c: &Ctx, now: Instant, idle: impl FnOnce() -> bool) -> bool {
        if !c.enabled || !c.configured || !c.visible {
            return false;
        }
        if self.inflight.is_some_and(|(_, since)| now < since + DEAD)
            || self.blocked_until.is_some_and(|until| now < until)
        {
            return false;
        }
        let limit = if c.open { OPEN_STALE } else { BACKGROUND };
        if self.age(c.wall).is_some_and(|age| age < limit) {
            return false;
        }
        c.open || !idle()
    }
    pub fn start(&mut self, now: Instant, detect: bool, city: &str) -> Job {
        self.next_id += 1;
        self.inflight = Some((self.next_id, now));
        let held = self.place.as_ref();
        let locate = detect && (self.redetect || held.is_none_or(|place| !place.detected));
        Job {
            id: self.next_id,
            detect: locate,
            // A detected place is not a fallback for a lookup that just failed to detect.
            known: held.filter(|place| !(locate && place.detected)).cloned(),
            city: city.trim().to_owned(),
        }
    }
    /// Takes a worker's answer; false for a stale one (a job given up on, or before a reset).
    pub fn finish(&mut self, id: u64, outcome: Outcome, now: Instant) -> bool {
        if self.inflight.is_none_or(|(current, _)| current != id) {
            return false;
        }
        self.inflight = None;
        self.rev = self.rev.wrapping_add(1);
        if outcome.place.is_some() {
            self.place = outcome.place;
        }
        match outcome.result {
            Ok(report) => {
                (self.report, self.last_fail, self.fails) = (Some(report), None, 0);
                (self.blocked_until, self.redetect) = (None, false);
            }
            Err(fail) => {
                self.fails += 1;
                self.last_fail = Some(fail);
                self.blocked_until = Some(now + backoff(self.fails));
                if fail == Fail::Unlocated {
                    (self.place, self.report) = (None, None);
                }
            }
        }
        true
    }
}

// ---- the page -----------------------------------------------------------------------------------------

pub struct Cell {
    pub label: String,
    pub icon: Icon,
    pub temp: String,
}
/// Everything the Weather page draws and says, as ready strings.
#[derive(Default)]
pub struct View {
    pub title: String,
    pub temp: String,
    pub icon: Icon,
    pub range: String,
    pub cond: &'static str,
    pub hours: Vec<Cell>,
    pub footer: String,
    /// Shown instead of the data when there is none.
    pub message: Option<&'static str>,
}

/// Whole degrees from tenths of a degree Celsius.
fn degrees(tenths: i16, fahrenheit: bool) -> i32 {
    let t = i32::from(tenths);
    (if fahrenheit { t * 9 / 5 + 320 } else { t } + 5).div_euclid(10)
}

fn age_text(age: u64) -> Option<String> {
    match age {
        0..3600 => None,
        3600..172_800 => Some(format!("{} h ago", age / 3600)),
        _ => Some(format!("{} d ago", age / 86_400)),
    }
}

/// The page for the feed's state. `label` formats an hour of the day the way the user's locale does.
pub fn view(
    feed: &Feed,
    configured: bool,
    fahrenheit: bool,
    wall: u64,
    label: &dyn Fn(u8) -> String,
) -> View {
    let (Some(report), Some(place)) = (&feed.report, &feed.place) else {
        let message = match feed.last_fail {
            _ if !configured => "Set your city in Settings",
            Some(Fail::Unlocated) => "Location not found. Set your city in Settings",
            Some(_) => "Weather unavailable",
            None => "Loading weather…",
        };
        return View {
            title: "Weather".to_owned(),
            message: Some(message),
            ..View::default()
        };
    };
    let temp = |tenths| format!("{}°", degrees(tenths, fahrenheit));
    let icon = Icon::from_wmo(report.code);
    let mut footer = String::from("Weather by Open-Meteo.com");
    if let Some(age) = feed.age(wall).and_then(age_text) {
        let _ = write!(footer, " · {age}");
    }
    View {
        title: place.name.clone(),
        temp: temp(report.tenths),
        icon,
        range: format!("H {}  L {}", temp(report.hi), temp(report.lo)),
        cond: icon.label(),
        hours: report
            .hours
            .iter()
            .map(|h| Cell {
                label: label(h.hour),
                icon: Icon::from_wmo(h.code),
                temp: temp(h.tenths),
            })
            .collect(),
        footer,
        message: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_follows_the_weather_tab_and_prayer_only_needs_a_place() {
        assert!(feed_wanted(true, false, true)); // Weather alone, Prayer off
        assert!(feed_wanted(true, true, true));
        assert!(feed_wanted(false, true, false)); // Prayer alone looks the place up once
        assert!(!feed_wanted(false, true, true)); // then never refreshes the report
        assert!(!feed_wanted(false, false, false));
    }

    const FORECAST: &str = r#"{"latitude":30.0625,"longitude":31.25,"timezone":"Africa/Cairo",
      "current_units":{"time":"iso8601","temperature_2m":"°C","weather_code":"wmo code"},
      "current":{"time":"2026-11-12T14:15","interval":900,"temperature_2m":24.3,"weather_code":2},
      "hourly_units":{"time":"iso8601"},
      "hourly":{"time":["2026-11-12T14:00","2026-11-12T15:00","2026-11-12T16:00","2026-11-12T17:00","2026-11-12T18:00","2026-11-12T19:00"],
        "temperature_2m":[24.0,25.1,25.6,23.2,21.0,19.4],"weather_code":[2,1,3,61,61,0]},
      "daily_units":{"temperature_2m_max":"°C"},
      "daily":{"time":["2026-11-12"],"temperature_2m_max":[29.1],"temperature_2m_min":[18.0]}}"#;

    #[test]
    fn the_scanner_reads_the_forecast_fixture() {
        let report = parse_forecast(FORECAST, 1000).unwrap();
        assert_eq!((report.at, report.tenths, report.code), (1000, 243, 2));
        assert_eq!((report.hi, report.lo), (291, 180));
        // The current hour is not "next": the four after it, in order.
        let hours: Vec<_> = report
            .hours
            .iter()
            .map(|h| (h.hour, h.tenths, h.code))
            .collect();
        assert_eq!(
            hours,
            [(15, 251, 1), (16, 256, 3), (17, 232, 61), (18, 210, 61)]
        );
        assert!(value(FORECAST, "latitude").unwrap().starts_with("30.0625,"));
        // `temperature_2m` in the units object is a string, not the reading.
        assert_eq!(
            num(object(FORECAST, "current").unwrap(), "temperature_2m"),
            Some(24.3)
        );
    }

    #[test]
    fn malformed_and_missing_fields_degrade_without_panicking() {
        // No current reading: nothing to show.
        for bad in [
            "",
            "{}",
            "[]",
            "garbage",
            r#"{"current":{}}"#,
            r#"{"current":{"time":"2026-11-12T14:15","temperature_2m":null,"weather_code":1}}"#,
            r#"{"current":{"time":"2026-11-12T14:15","temperature_2m":"warm","weather_code":1}}"#,
            r#"{"current":{"temperature_2m":20,"weather_code":1}}"#,
        ] {
            assert_eq!(parse_forecast(bad, 0), None, "{bad:?}");
        }
        // Cut off inside the current block: nothing to show. Cut off later: the reading without its hours.
        let inside = FORECAST.find("weather_code\":2").unwrap();
        assert_eq!(parse_forecast(&FORECAST[..inside], 0), None);
        let later = FORECAST.find("\"time\":[\"2026-11-12T14:00").unwrap() + 20;
        let cut = parse_forecast(&FORECAST[..later], 0).unwrap();
        assert_eq!((cut.tenths, cut.hi, cut.hours.len()), (243, 243, 0));
        // Only the current block: the range falls back to it and there are no hours.
        let bare =
            r#"{"current":{"time":"2026-11-12T14:15","temperature_2m":-3.46,"weather_code":71}}"#;
        let report = parse_forecast(bare, 5).unwrap();
        assert_eq!(
            (report.tenths, report.hi, report.lo, report.hours.len()),
            (-35, -35, -35, 0)
        );
        // Null entries and short lists drop those hours only; out-of-range numbers are clamped.
        let holes = r#"{"current":{"time":"2026-11-12T14:15","temperature_2m":900,"weather_code":500},
          "hourly":{"time":["2026-11-12T15:00","2026-11-12T16:00","2026-11-12T17:00"],"temperature_2m":[null,20.0],"weather_code":[1,2,3]}}"#;
        let report = parse_forecast(holes, 0).unwrap();
        assert_eq!((report.tenths, report.code), (1000, 99));
        assert_eq!(
            report.hours.iter().map(|h| h.hour).collect::<Vec<_>>(),
            [16]
        );
        // A key name inside a value is not a key; strings may hold brackets.
        assert_eq!(
            value(r#"{"a":"temperature_2m","b":1}"#, "temperature_2m"),
            None
        );
        assert_eq!(
            group(r#"{"s":"}]\"}","t":[1]}tail"#),
            Some(r#"{"s":"}]\"}","t":[1]}"#)
        );
        assert_eq!(group("[1,[2]"), None);
        assert_eq!(group("12"), None);
    }

    #[test]
    fn wmo_codes_map_to_eight_pictures() {
        let all = [
            (0, Icon::Clear),
            (1, Icon::Clear),
            (2, Icon::PartlyCloudy),
            (3, Icon::Cloud),
            (45, Icon::Fog),
            (48, Icon::Fog),
            (51, Icon::Drizzle),
            (57, Icon::Drizzle),
            (61, Icon::Rain),
            (67, Icon::Rain),
            (80, Icon::Rain),
            (82, Icon::Rain),
            (71, Icon::Snow),
            (77, Icon::Snow),
            (85, Icon::Snow),
            (86, Icon::Snow),
            (95, Icon::Storm),
            (96, Icon::Storm),
            (99, Icon::Storm),
            (4, Icon::Cloud),
            (58, Icon::Cloud),
        ];
        for (code, icon) in all {
            assert_eq!(Icon::from_wmo(code), icon, "{code}");
        }
        assert_eq!(Icon::from_wmo(2).label(), "Partly cloudy");
    }

    #[test]
    fn geocoding_takes_the_first_result_and_nothing_else() {
        let found = r#"{"results":[{"id":360630,"name":"Cairo","latitude":30.06263,"longitude":31.24967,"country":"Egypt"},
          {"id":2,"name":"Cairo","latitude":37.0,"longitude":-89.1}],"generationtime_ms":0.5}"#;
        let place = parse_place(found).unwrap();
        assert_eq!(
            (place.lat, place.lon, place.detected, place.name.as_str()),
            (300_626, 312_497, false, "Cairo")
        );
        assert_eq!(parse_place(r#"{"generationtime_ms":0.5}"#), None);
        assert_eq!(parse_place(r#"{"results":[]}"#), None);
        assert_eq!(
            parse_place(r#"{"results":[{"name":"X","latitude":95,"longitude":0}]}"#),
            None
        );
        assert_eq!(
            parse_place(r#"{"results":[{"name":"X","latitude":1}]}"#),
            None
        );
        let escaped =
            r#"{"results":[{"name":"São \"Paulo\"\n","latitude":-23.55,"longitude":-46.63}]}"#;
        assert_eq!(parse_place(escaped).unwrap().name, "São \"Paulo\"");
        assert_eq!(parse_place("{\"results\":[{\"name\":\"Oslo"), None);
    }

    #[test]
    fn requests_are_built_for_a_configured_city_only() {
        assert_eq!(geocode_path(""), None);
        assert_eq!(geocode_path("  \t"), None);
        assert_eq!(
            geocode_path(" São Paulo ").as_deref(),
            Some("/v1/search?count=1&format=json&name=S%C3%A3o%20Paulo")
        );
        let place = Place {
            lat: 300_444,
            lon: -12_005,
            detected: true,
            name: "x".into(),
        };
        let path = forecast_path(&place);
        assert!(
            path.contains("latitude=30.0444&longitude=-1.2005&"),
            "{path}"
        );
        assert_eq!(fixed(-5), "-0.0005");
        assert!(!configured(false, " ") && configured(false, "Cairo") && configured(true, ""));
    }

    #[test]
    fn the_place_comes_from_the_device_then_a_held_place_then_the_city() {
        use Source::*;
        assert_eq!(sources(true, true, "Cairo"), [Detect, Known, City]);
        assert_eq!(sources(false, true, "Cairo"), [Known, City]);
        assert_eq!(sources(true, false, ""), [Detect]);
        assert_eq!(sources(false, false, "Cairo"), [City]);
        // Nothing configured: nothing to look up, so nothing to request.
        assert!(sources(false, false, "  ").is_empty());
    }

    fn ctx(wall: u64) -> Ctx {
        Ctx {
            enabled: true,
            configured: true,
            visible: true,
            open: false,
            wall,
        }
    }
    fn report(at: u64) -> Report {
        Report {
            at,
            tenths: 200,
            code: 0,
            hi: 250,
            lo: 150,
            hours: vec![],
        }
    }
    fn place() -> Place {
        Place {
            lat: 300_444,
            lon: 312_357,
            detected: false,
            name: "Cairo".into(),
        }
    }

    #[test]
    fn nothing_is_asked_unless_configured_enabled_and_shown() {
        let t0 = Instant::now();
        let feed = Feed::new(None, None, false);
        assert!(feed.wants(&ctx(0), t0, || false));
        for off in [
            Ctx {
                enabled: false,
                ..ctx(0)
            },
            Ctx {
                configured: false,
                ..ctx(0)
            },
            Ctx {
                visible: false,
                ..ctx(0)
            },
            Ctx {
                visible: false,
                open: true,
                ..ctx(0)
            },
        ] {
            assert!(!feed.wants(&off, t0, || false));
        }
    }

    #[test]
    fn open_refetches_after_an_hour_and_the_background_after_three() {
        let t0 = Instant::now();
        let feed = Feed::new(Some(place()), Some(report(10_000)), false);
        let open = |wall| Ctx {
            open: true,
            ..ctx(wall)
        };
        assert!(!feed.wants(&open(10_000 + 3599), t0, || false));
        assert!(feed.wants(&open(10_000 + 3600), t0, || false));
        assert!(
            feed.wants(&open(10_000 + 3600), t0, || true),
            "open ignores idle"
        );
        let wall = |secs: u64| ctx(10_000 + secs);
        assert!(!feed.wants(&wall(3 * 3600 - 1), t0, || false));
        assert!(feed.wants(&wall(3 * 3600), t0, || false));
        assert!(
            !feed.wants(&wall(3 * 3600), t0, || true),
            "idle: no background fetch"
        );
        // The idle probe is not even made while the data is fresh.
        assert!(!feed.wants(&wall(60), t0, || panic!("asked")));
        // A report from the future (clock set back) is refetched.
        assert!(feed.wants(&ctx(5), t0, || false));
    }

    #[test]
    fn failures_back_off_fifteen_minutes_then_an_hour_and_success_resets() {
        let t0 = Instant::now();
        let mut feed = Feed::new(Some(place()), None, false);
        let fail = |feed: &mut Feed, now| {
            let job = feed.start(now, false, "Cairo");
            feed.finish(
                job.id,
                Outcome {
                    place: None,
                    result: Err(Fail::Network),
                },
                now,
            )
        };
        assert!(fail(&mut feed, t0));
        let wants =
            |feed: &Feed, secs| feed.wants(&ctx(0), t0 + Duration::from_secs(secs), || false);
        assert!(!wants(&feed, 15 * 60 - 1) && wants(&feed, 15 * 60));
        assert!(fail(&mut feed, t0 + Duration::from_secs(900)));
        assert!(!wants(&feed, 900 + 3599) && wants(&feed, 900 + 3600));
        assert!(fail(&mut feed, t0 + Duration::from_secs(5000)));
        assert!(!wants(&feed, 5000 + 3599), "it stays at an hour");
        assert_eq!(feed.last_fail, Some(Fail::Network));
        let job = feed.start(t0 + Duration::from_secs(9000), false, "Cairo");
        let ok = Outcome {
            place: Some(place()),
            result: Ok(report(1)),
        };
        assert!(feed.finish(job.id, ok, t0 + Duration::from_secs(9001)));
        assert!(feed.last_fail.is_none() && feed.report.is_some());
        assert!(feed.wants(&ctx(1 + 3 * 3600), t0 + Duration::from_secs(9002), || false));
    }

    #[test]
    fn one_job_at_a_time_and_a_dead_one_is_replaced() {
        let t0 = Instant::now();
        let mut feed = Feed::new(None, None, false);
        let first = feed.start(t0, false, "Cairo");
        assert!(!feed.wants(&ctx(0), t0 + Duration::from_secs(59), || false));
        assert!(
            feed.wants(&ctx(0), t0 + DEAD, || false),
            "given up after a minute"
        );
        let second = feed.start(t0 + DEAD, false, "Cairo");
        let late = Outcome {
            place: None,
            result: Ok(report(1)),
        };
        assert!(
            !feed.finish(first.id, late, t0 + DEAD),
            "the old answer is ignored"
        );
        assert!(feed.report.is_none());
        // A reset also drops the job in flight and the back-off.
        feed.reset(false);
        let stale = Outcome {
            place: None,
            result: Ok(report(2)),
        };
        assert!(!feed.finish(second.id, stale, t0 + DEAD));
        assert!(feed.wants(&ctx(0), t0 + DEAD, || false));
    }

    #[test]
    fn detection_runs_first_and_never_falls_back_to_a_detected_place() {
        let t0 = Instant::now();
        let detected = Place {
            detected: true,
            ..place()
        };
        // A fresh session re-detects once; the held detected place is not a fallback for that.
        let mut feed = Feed::new(Some(detected.clone()), None, true);
        let job = feed.start(t0, true, "");
        assert!(job.detect && job.known.is_none());
        let ok = Outcome {
            place: Some(detected.clone()),
            result: Ok(report(1)),
        };
        feed.finish(job.id, ok, t0);
        let again = feed.start(t0, true, "");
        assert!(!again.detect, "once located it is not asked again");
        assert_eq!(again.known, Some(detected));
        // A geocoded place is retried for detection and kept as the fallback.
        let mut feed = Feed::new(Some(place()), None, true);
        let job = feed.start(t0, true, "Cairo");
        assert!(job.detect && job.known == Some(place()));
        // Detection off: the held place is just reused.
        let job = Feed::new(Some(place()), None, false).start(t0, false, "Cairo");
        assert!(!job.detect && job.known.is_some());
        // Not located at all: the place and report go, and the page says so.
        let mut feed = Feed::new(Some(place()), Some(report(1)), false);
        let job = feed.start(t0, false, "Cairo");
        feed.finish(
            job.id,
            Outcome {
                place: None,
                result: Err(Fail::Unlocated),
            },
            t0,
        );
        assert!(feed.place.is_none() && feed.report.is_none());
        let label = |h: u8| h.to_string();
        let page = view(&feed, true, false, 0, &label);
        assert_eq!(
            page.message,
            Some("Location not found. Set your city in Settings")
        );
    }

    #[test]
    fn the_page_says_what_is_missing_and_formats_what_is_there() {
        let label = |hour: u8| format!("{hour}h");
        let empty = Feed::new(None, None, false);
        assert_eq!(
            view(&empty, false, false, 0, &label).message,
            Some("Set your city in Settings")
        );
        assert_eq!(
            view(&empty, true, false, 0, &label).message,
            Some("Loading weather…")
        );
        let mut failed = Feed::new(None, None, false);
        let job = failed.start(Instant::now(), false, "x");
        failed.finish(
            job.id,
            Outcome {
                place: None,
                result: Err(Fail::Data),
            },
            Instant::now(),
        );
        assert_eq!(
            view(&failed, true, false, 0, &label).message,
            Some("Weather unavailable")
        );

        let mut data = report(1_000);
        data.hours = vec![Hour {
            hour: 15,
            tenths: -55,
            code: 61,
        }];
        let feed = Feed::new(Some(place()), Some(data), false);
        let page = view(&feed, true, false, 1_000 + 2 * 3600 + 5, &label);
        assert_eq!(
            (page.title.as_str(), page.temp.as_str(), page.range.as_str()),
            ("Cairo", "20°", "H 25°  L 15°")
        );
        assert_eq!(page.footer, "Weather by Open-Meteo.com · 2 h ago");
        assert_eq!(
            (
                page.hours[0].label.as_str(),
                page.hours[0].temp.as_str(),
                page.hours[0].icon
            ),
            ("15h", "-5°", Icon::Rain)
        );
        let fahrenheit = view(&feed, true, true, 1_000, &label);
        assert_eq!(
            (fahrenheit.temp.as_str(), fahrenheit.range.as_str()),
            ("68°", "H 77°  L 59°")
        );
        assert_eq!(
            fahrenheit.footer, "Weather by Open-Meteo.com",
            "fresh data shows no age"
        );
        assert_eq!(
            (
                degrees(-5, false),
                degrees(-6, false),
                degrees(0, true),
                degrees(-400, true)
            ),
            (0, -1, 32, -40)
        );
        assert_eq!(age_text(3599), None);
        assert_eq!(age_text(7200).as_deref(), Some("2 h ago"));
        assert_eq!(age_text(3 * 86_400).as_deref(), Some("3 d ago"));
    }

    #[test]
    fn units_default_by_region_and_round_trip_their_keys() {
        assert!(
            Units::Auto.fahrenheit("US")
                && !Units::Auto.fahrenheit("EG")
                && !Units::Auto.fahrenheit("")
        );
        assert!(!Units::Celsius.fahrenheit("US") && Units::Fahrenheit.fahrenheit("EG"));
        for units in Units::ALL {
            assert_eq!(Units::from_key(units.key()), Some(units));
        }
        assert_eq!(Units::from_key("k"), None);
    }

    #[test]
    fn the_cache_lines_round_trip_and_reject_junk() {
        let place = Place {
            name: "Washington, D.C.".into(),
            detected: true,
            ..place()
        };
        assert_eq!(Place::from_line(&place.to_line()), Some(place));
        for bad in [
            "",
            "1,2",
            "1,2,0,",
            "x,2,0,A",
            "999999,2,0,A",
            "1,2,2,A",
            "1,2,0,a\nb",
        ] {
            assert_eq!(Place::from_line(bad), None, "{bad:?}");
        }
        let mut data = report(1_800_000_000);
        data.hours = vec![
            Hour {
                hour: 0,
                tenths: -3,
                code: 3,
            },
            Hour {
                hour: 23,
                tenths: 301,
                code: 95,
            },
        ];
        assert_eq!(Report::from_line(&data.to_line()), Some(data));
        assert_eq!(
            Report::from_line("1,2,3,4,5"),
            Some(Report {
                at: 1,
                tenths: 2,
                code: 3,
                hi: 4,
                lo: 5,
                hours: vec![]
            })
        );
        for bad in [
            "",
            "1,2,3,4",
            "1,2,100,4,5",
            "1,2,3,4,5,24:1:1",
            "1,2,3,4,5,1:1",
            "a,2,3,4,5",
        ] {
            assert_eq!(Report::from_line(bad), None, "{bad:?}");
        }
    }
}
