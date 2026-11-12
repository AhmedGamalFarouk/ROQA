//! The Limits tab (features U1): plan-limit percentages for Claude, Codex and Antigravity. Pure rules only:
//! hand-rolled readers for the three sources, the stale and reset rules, the refresh policy and the page's
//! ready-made text. No file, process, clock or window is touched here; `limits_os.rs` supplies the facts.
//! Nothing about paths, projects or tokens is kept: a reading is two percentages and two reset times.

use crate::prayer_tab::day_number;

/// A reading older than this is dimmed and shows its age; the Claude and Codex sources re-read after it.
pub const STALE_SECS: i64 = 15 * 60;
/// The `agy` answer is kept this long (a spawn costs a backend call).
pub const AGY_SECS: i64 = 5 * 60;
/// Between two manual refreshes.
pub const COOLDOWN_SECS: i64 = 30;
pub const NO_CLAUDE: &str = "No data: open Claude Code once";
pub const NO_CODEX: &str = "No limits in the Codex log: press Refresh";
pub const NO_AGY: &str = "Antigravity gave no limits";
pub const OFF_AGY: &str = "Allow ROQA to run agy in Settings";
pub const ASK_AGY: &str = "Press Refresh to read Antigravity";
pub const WAITING: &str = "Reading...";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Claude,
    Codex,
    Agy,
}
impl Tool {
    pub const ALL: [Tool; 3] = [Tool::Claude, Tool::Codex, Tool::Agy];
    pub fn name(self) -> &'static str {
        ["Claude", "Codex", "Antigravity"][self as usize]
    }
}

/// One limit window: percent used and the Unix second it resets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Win {
    pub used: u8,
    pub reset: i64,
}
/// What a source said: the five-hour and weekly windows (either may not exist) and when it was true.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snap {
    pub h5: Option<Win>,
    pub wk: Option<Win>,
    pub as_of: i64,
}
/// A reading, or the short reason there is none.
pub type Reading = Result<Snap, &'static str>;

fn pct(value: f64) -> Option<u8> {
    (value.is_finite() && (0.0..=100.0).contains(&value)).then(|| value.round() as u8)
}

// ---- A tiny JSON-ish scanner: keys are looked up by name, nothing else is kept. ----

/// The text right after `"key":` (first match followed by a colon).
fn value_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let (mut depth, mut start, mut quoted, mut escaped) = (0usize, 0, false, false);
    for (at, byte) in s.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted {
            match byte {
                b'\\' => escaped = true,
                b'"' => {
                    quoted = false;
                    if depth == 1
                        && &s[start..at] == key
                        && let Some(value) = s[at + 1..].trim_start().strip_prefix(':')
                    {
                        return Some(value.trim_start());
                    }
                }
                _ => {}
            }
        } else {
            match byte {
                b'"' => {
                    quoted = true;
                    start = at + 1;
                }
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}
fn number(s: &str, key: &str) -> Option<f64> {
    let value = value_after(s, key)?;
    let end = value
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(value.len());
    let parsed: f64 = value[..end].parse().ok()?;
    ((0.0..=253_402_300_799_000.0).contains(&parsed)
        && value[end..]
            .starts_with(|c: char| c.is_ascii_whitespace() || matches!(c, ',' | '}' | ']')))
    .then_some(parsed)
}
/// A string value without escapes (timestamps).
fn string<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let value = value_after(s, key)?.strip_prefix('"')?;
    let value = &value[..value.find('"')?];
    (!value.contains('\\')).then_some(value)
}
/// Length of the object at the start of `s`, braces inside strings ignored.
fn object_len(s: &str) -> Option<usize> {
    let (mut depth, mut text, mut escaped) = (0usize, false, false);
    for (at, byte) in s.bytes().enumerate() {
        match byte {
            _ if escaped => escaped = false,
            b'\\' if text => escaped = true,
            b'"' => text = !text,
            b'{' if !text => depth += 1,
            b'}' if !text => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
    }
    None
}
fn object<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let value = value_after(s, key)?;
    value
        .starts_with('{')
        .then(|| object_len(value))
        .flatten()
        .map(|len| &value[..len])
}
/// A JSON string value, unescaped. An unterminated one (output cut at a cap) gives what there is.
fn unescaped(s: &str, key: &str) -> Option<String> {
    let mut chars = value_after(s, key)?.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(
                        u32::from_str_radix(&hex, 16)
                            .ok()
                            .and_then(char::from_u32)
                            .unwrap_or('?'),
                    );
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Zoned ISO timestamps, optionally with fractional seconds, converted to Unix seconds.
pub fn parse_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let n = |at: usize, len: usize| s.get(at..at + len)?.parse::<i64>().ok();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[13] != b':'
        || b[16] != b':'
        || !matches!(b[10], b'T' | b' ')
    {
        return None;
    }
    let (y, mo, d) = (n(0, 4)?, n(5, 2)?, n(8, 2)?);
    let (h, mi, se) = (n(11, 2)?, n(14, 2)?, n(17, 2)?);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_in_month = match mo {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return None,
    };
    if y < 1970
        || !(1..=days_in_month).contains(&d)
        || !(0..=23).contains(&h)
        || !(0..=59).contains(&mi)
        || !(0..=59).contains(&se)
    {
        return None;
    }
    let mut at = 19;
    if b.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while b.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == start {
            return None;
        }
    }
    let offset = match b.get(at) {
        Some(b'Z') if at + 1 == b.len() => 0,
        Some(&sign @ (b'+' | b'-')) => {
            if at + 6 != b.len() || b[at + 3] != b':' {
                return None;
            }
            let (oh, om) = (n(at + 1, 2)?, n(at + 4, 2)?);
            if !(0..=23).contains(&oh) || !(0..=59).contains(&om) {
                return None;
            }
            (oh * 3600 + om * 60) * if sign == b'-' { -1 } else { 1 }
        }
        _ => return None,
    };
    let days = day_number((y as i32, mo as u32, d as u32));
    Some(days * 86_400 + h * 3600 + mi * 60 + se - offset)
}

// ---- The sources ----

/// Claude Code's own cache in `~/.claude.json` (`cachedUsageUtilization`): four numbers and two strings.
pub fn claude_cache(text: &str) -> Option<Snap> {
    object_len(text.trim())?;
    let cache = object(text, "cachedUsageUtilization")?;
    let as_of = (number(cache, "fetchedAtMs")? / 1000.0) as i64;
    let usage = object(cache, "utilization")?;
    let window = |key: &str| {
        let o = object(usage, key)?;
        Some(Win {
            used: pct(number(o, "utilization")?)?,
            reset: parse_iso(string(o, "resets_at")?)?,
        })
    };
    let (h5, wk) = (window("five_hour"), window("seven_day"));
    (h5.is_some() || wk.is_some()).then_some(Snap { h5, wk, as_of })
}

/// The optional ROQA statusline file `{"at":..,"h5":..,"h5_reset":..,"wk":..,"wk_reset":..}` (epoch seconds).
pub fn claude_file(text: &str) -> Option<Snap> {
    object_len(text.trim())?;
    let window = |used: &str, reset: &str| {
        Some(Win {
            used: pct(number(text, used)?)?,
            reset: number(text, reset)? as i64,
        })
    };
    let (h5, wk) = (window("h5", "h5_reset"), window("wk", "wk_reset"));
    (h5.is_some() || wk.is_some()).then_some(Snap {
        h5,
        wk,
        as_of: number(text, "at")? as i64,
    })
}

/// The newer of the two Claude readings; the statusline file wins a tie.
pub fn claude_best(cache: Option<Snap>, file: Option<Snap>) -> Reading {
    match (cache, file) {
        (Some(c), Some(f)) if c.as_of > f.as_of => Ok(c),
        (_, Some(f)) => Ok(f),
        (Some(c), None) => Ok(c),
        (None, None) => Err(NO_CLAUDE),
    }
}

/// Files a window into the five-hour or weekly slot by its length in minutes (300 or 10080); with no length,
/// by position.
fn place(snap: &mut Snap, minutes: Option<f64>, second: bool, win: Win) {
    if minutes.is_some_and(|minutes| minutes != 300.0 && minutes != 10080.0) {
        return;
    }
    if minutes.map_or(second, |minutes| minutes == 10080.0) {
        snap.wk = Some(win);
    } else {
        snap.h5 = Some(win);
    }
}

/// The newest `token_count` line of a Codex session log tail decides: its limits, or none (null limits in a
/// newer Codex build are "no data", never an older snapshot).
pub fn codex_tail(tail: &str) -> Reading {
    let Some(line) = tail.lines().rev().find(|line| {
        object(line, "payload")
            .is_some_and(|payload| string(payload, "type") == Some("token_count"))
    }) else {
        return Err(NO_CODEX);
    };
    let Some(limits) = object(line, "payload").and_then(|payload| object(payload, "rate_limits"))
    else {
        return Err(NO_CODEX);
    };
    let mut snap = Snap {
        as_of: string(line, "timestamp").and_then(parse_iso).unwrap_or(0),
        ..Snap::default()
    };
    for (index, key) in ["primary", "secondary"].into_iter().enumerate() {
        let Some(o) = object(limits, key) else {
            continue;
        };
        if let (Some(used), Some(reset)) = (number(o, "used_percent"), number(o, "resets_at")) {
            let win = Win {
                used: pct(used).ok_or(NO_CODEX)?,
                reset: reset as i64,
            };
            place(&mut snap, number(o, "window_minutes"), index == 1, win);
        }
    }
    if snap.h5.is_none() && snap.wk.is_none() {
        return Err(NO_CODEX);
    }
    Ok(snap)
}

/// The line answering request id 1 among the lines `codex app-server` wrote.
pub fn rpc_reply(output: &str, id: u8) -> Option<&str> {
    output
        .lines()
        .find(|l| number(l, "id") == Some(f64::from(id)) && object_len(l).is_some())
}

/// The limits in the reply to `account/rateLimits/read`.
pub fn codex_rpc(output: &str, now: i64) -> Reading {
    let limits = rpc_reply(output, 1)
        .and_then(|line| object(line, "result"))
        .and_then(|l| object(l, "rateLimits"))
        .ok_or(NO_CODEX)?;
    let mut snap = Snap {
        as_of: now,
        ..Snap::default()
    };
    for (index, key) in ["primary", "secondary"].into_iter().enumerate() {
        let Some(o) = object(limits, key) else {
            continue;
        };
        if let (Some(used), Some(reset)) = (number(o, "usedPercent"), number(o, "resetsAt")) {
            let win = Win {
                used: pct(used).ok_or(NO_CODEX)?,
                reset: reset as i64,
            };
            place(&mut snap, number(o, "windowDurationMins"), index == 1, win);
        }
    }
    if snap.h5.is_none() && snap.wk.is_none() {
        return Err(NO_CODEX);
    }
    Ok(snap)
}

/// What `agy -p "/usage" --output-format json` printed: tab-separated lines
/// `<group>\t<Weekly|Five Hour> Limit Remaining\t<N>%\t<UTC ISO>` in the `response` text. `group` 0 is the
/// Gemini models, 1 the Claude and GPT models. Anything else is drift: no reading.
pub fn agy_text(output: &str, group: u8, now: i64) -> Reading {
    let text = unescaped(output, "response").unwrap_or_else(|| output.to_owned());
    let mut snap = Snap {
        as_of: now,
        ..Snap::default()
    };
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [name, kind, left, reset] = fields[..] else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        let selected = if group == 0 {
            name.contains("gemini")
        } else {
            name.contains("claude") && name.contains("gpt")
        };
        if !selected {
            continue;
        }
        let kind = kind.to_ascii_lowercase();
        let (Ok(left), Some(reset)) = (left.trim_end_matches('%').parse::<f64>(), parse_iso(reset))
        else {
            continue;
        };
        let win = Win {
            used: pct(100.0 - left).ok_or(NO_AGY)?,
            reset,
        };
        if kind.contains("five hour") {
            snap.h5 = Some(win);
        } else if kind.contains("weekly") {
            snap.wk = Some(win);
        }
    }
    if snap.h5.is_none() && snap.wk.is_none() {
        return Err(NO_AGY);
    }
    Ok(snap)
}

// ---- State and refresh policy ----

/// What a source looked like when last read (modified time in ms, size): unchanged means nothing to read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub mtime: i64,
    pub size: u64,
}

/// One worker run: which sources to look at.
#[derive(Clone, Copy, Debug)]
pub struct Job {
    pub id: u64,
    pub now: i64,
    pub manual: bool,
    /// The cache aged out, so read even if the file's stamp is unchanged.
    pub due: [bool; 3],
    pub allow_codex: bool,
    pub allow_agy: bool,
    pub group: u8,
    pub prev: [Stamp; 2],
}
/// What a run found: per tool a reading and the source's stamp; `None` means untouched.
#[derive(Debug, Default)]
pub struct Outcome {
    pub id: u64,
    pub group: u8,
    pub got: [Option<(Reading, Stamp)>; 3],
}

#[derive(Default)]
pub struct Limits {
    pub rows: [Option<Reading>; 3],
    fetched: [i64; 3],
    stamps: [Stamp; 2],
    seen: (bool, bool, u8),
    last_manual: i64,
    busy: Option<u64>,
    next: u64,
    /// Bumped when a row changes, so the page is rebuilt.
    pub rev: u32,
}
impl Limits {
    pub fn configure(&mut self, allow: (bool, bool), group: u8) {
        if (allow.0, allow.1, group) == self.seen {
            return;
        }
        self.seen = (allow.0, allow.1, group);
        self.fetched[2] = 0;
        self.rows[2] = Some(Err(if allow.1 { ASK_AGY } else { OFF_AGY }));
        self.rev = self.rev.wrapping_add(1);
    }
    pub fn busy(&self) -> bool {
        self.busy.is_some()
    }
    /// A job for a tab open (`manual` false) or the Refresh button; `None` while one runs or inside the
    /// manual cooldown. Without permission the Codex RPC and `agy` are never asked for.
    pub fn start(&mut self, now: i64, manual: bool, allow: (bool, bool), group: u8) -> Option<Job> {
        if self.busy.is_some() || (manual && now - self.last_manual < COOLDOWN_SECS) {
            return None;
        }
        self.configure(allow, group);
        if manual {
            self.last_manual = now;
        }
        self.next += 1;
        self.busy = Some(self.next);
        let age = |tool: usize| now - self.fetched[tool];
        Some(Job {
            id: self.next,
            now,
            manual,
            due: [
                self.fetched[0] == 0 || age(0) > STALE_SECS,
                self.fetched[1] == 0 || age(1) > STALE_SECS,
                self.fetched[2] == 0 || age(2) > AGY_SECS,
            ],
            allow_codex: allow.0,
            allow_agy: allow.1,
            group,
            prev: self.stamps,
        })
    }
    /// Takes the pending run's answer. Claude retains last-good data; explicit missing Codex/agy data replaces it.
    pub fn finish(&mut self, outcome: Outcome, now: i64) -> bool {
        if self.busy != Some(outcome.id) {
            return false;
        }
        self.busy = None;
        for (index, got) in outcome.got.into_iter().enumerate() {
            let Some((reading, stamp)) = got else {
                continue;
            };
            if index == 2 && (outcome.group != self.seen.2 || (!self.seen.1 && reading.is_ok())) {
                continue;
            }
            if index < 2 {
                self.stamps[index] = stamp;
            }
            self.fetched[index] = now;
            if index != 0 || reading.is_ok() || !matches!(self.rows[index], Some(Ok(_))) {
                self.rows[index] = Some(reading);
            }
        }
        self.rev = self.rev.wrapping_add(1);
        true
    }
}

// ---- The page ----

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// The tool has no such window (no five-hour limit on some Codex plans).
    Hidden,
    Used {
        pct: u8,
        reset: String,
    },
    /// The reset time has passed: the window is fresh, whatever the old number was.
    Reset {
        reset: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct RowView {
    pub name: &'static str,
    pub h5: Line,
    pub wk: Line,
    /// "as of 16:56", or the age once stale ("23 min ago").
    pub as_of: String,
    pub stale: bool,
    /// Why there is no reading.
    pub why: Option<&'static str>,
}
#[derive(Default)]
pub struct View {
    pub rows: Vec<RowView>,
    pub refreshing: bool,
    /// The UI Automation text, one sentence per window.
    pub name: String,
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Weekday (0 is Sunday), hour and minute at Unix second `t` for a UTC offset in minutes.
pub fn clock(offset_min: i32) -> impl Fn(i64) -> (u8, u8, u8) {
    move |t| {
        let local = t + i64::from(offset_min) * 60;
        let secs = local.rem_euclid(86_400);
        let day = (local.div_euclid(86_400) + 4).rem_euclid(7);
        (day as u8, (secs / 3600) as u8, (secs % 3600 / 60) as u8)
    }
}

pub fn view(
    rows: &[Option<Reading>; 3],
    now: i64,
    clock: &dyn Fn(i64) -> (u8, u8, u8),
    refreshing: bool,
) -> View {
    let hm = |t| {
        let (_, h, m) = clock(t);
        format!("{h:02}:{m:02}")
    };
    let day = |t| {
        let (d, h, m) = clock(t);
        format!("{} {h:02}:{m:02}", DAYS[usize::from(d) % 7])
    };
    let line = |win: Option<Win>, weekly: bool| match win {
        None => Line::Hidden,
        Some(w) => {
            let reset = if weekly { day(w.reset) } else { hm(w.reset) };
            if w.reset <= now {
                Line::Reset { reset }
            } else {
                Line::Used { pct: w.used, reset }
            }
        }
    };
    let mut view = View {
        refreshing,
        ..View::default()
    };
    let mut names = Vec::new();
    for (tool, reading) in Tool::ALL.into_iter().zip(rows) {
        let name = tool.name();
        let (snap, why) = match reading {
            Some(Ok(snap)) => (Some(*snap), None),
            Some(Err(why)) => (None, Some(*why)),
            None => (None, Some(WAITING)),
        };
        let age = snap.map_or(0, |s| now - s.as_of);
        let stale = snap.is_some_and(|s| s.as_of > 0 && age > STALE_SECS);
        let as_of = match snap {
            Some(s) if s.as_of == 0 => String::new(),
            Some(_) if stale && age >= 3600 => format!("{} h ago", age / 3600),
            Some(_) if stale => format!("{} min ago", age / 60),
            Some(s) => format!("as of {}", hm(s.as_of)),
            None => String::new(),
        };
        let row = RowView {
            name,
            h5: line(snap.and_then(|s| s.h5), false),
            wk: line(snap.and_then(|s| s.wk), true),
            as_of,
            stale,
            why,
        };
        if let Some(why) = why {
            names.push(format!("{name}, unavailable, {why}"));
        }
        let tail = match (row.as_of.is_empty(), stale) {
            (true, _) => String::new(),
            (false, false) => format!(", {}", row.as_of),
            (false, true) => format!(", stale, {}", row.as_of),
        };
        for (what, line, weekly) in [("five-hour", &row.h5, false), ("weekly", &row.wk, true)] {
            let words = |reset: &str| {
                if weekly {
                    format!("resets {reset}")
                } else {
                    format!("resets at {reset}")
                }
            };
            match line {
                Line::Hidden => {}
                Line::Used { pct, reset } => names.push(format!(
                    "{name}, {what} limit, {pct} percent used, {}{tail}",
                    words(reset)
                )),
                Line::Reset { reset } => names.push(format!(
                    "{name}, {what} limit, window reset (it was due {reset}){tail}"
                )),
            }
        }
        view.rows.push(row);
    }
    view.name = names.join(". ");
    view
}

/// The command Settings shows for Claude Code's `statusLine`: it writes the small file ROQA prefers
/// (`%LOCALAPPDATA%\ROQA\claude-limits.json`). ROQA never edits `settings.json` itself.
pub const SNIPPET: &str = r#"{"statusLine":{"type":"command","command":"powershell -NoProfile -Command \"$r=([Console]::In.ReadToEnd()|ConvertFrom-Json).rate_limits; if($r){$d=Join-Path $env:LOCALAPPDATA 'ROQA'; $null=New-Item -Force -ItemType Directory $d; $t=Join-Path $d 'claude-limits.tmp'; (@{at=[int][DateTimeOffset]::UtcNow.ToUnixTimeSeconds();h5=$r.five_hour.used_percentage;h5_reset=$r.five_hour.resets_at;wk=$r.seven_day.used_percentage;wk_reset=$r.seven_day.resets_at}|ConvertTo-Json -Compress)|Set-Content -Encoding ascii $t; Move-Item -Force $t (Join-Path $d 'claude-limits.json')}\""}}"#;

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_794_000_000; // 2026-11-06T20:00:00Z, only a base for relative times

    fn win(used: u8, reset: i64) -> Option<Win> {
        Some(Win { used, reset })
    }

    #[test]
    fn truncated_files_and_unknown_windows_are_not_readings() {
        assert!(claude_file(r#"{"at":1,"h5":5,"h5_reset":9,"#).is_none());
        assert!(claude_cache(r#"{"cachedUsageUtilization":{"fetchedAtMs":1000,"utilization":{"five_hour":{"utilization":5,"resets_at":"2026-11-12T16:50:00Z"}}},"#).is_none());
        assert!(codex_rpc(r#"{"id":1,"result":{"rateLimits":{"primary":{"usedPercent":5,"windowDurationMins":15,"resetsAt":9}}}}"#, 1).is_err());
    }

    #[test]
    fn automatic_reopen_keeps_agy_cache_until_explicit_refresh() {
        let mut limits = Limits::default();
        let job = limits.start(1000, true, (false, true), 0).unwrap();
        let snap = Snap {
            as_of: 1000,
            ..Snap::default()
        };
        limits.finish(
            Outcome {
                id: job.id,
                got: [None, None, Some((Ok(snap), Stamp::default()))],
                ..Outcome::default()
            },
            1000,
        );
        let job = limits.start(2000, false, (false, true), 0).unwrap();
        limits.finish(
            Outcome {
                id: job.id,
                ..Outcome::default()
            },
            2000,
        );
        assert_eq!(limits.rows[2], Some(Ok(snap)));
    }

    #[test]
    fn invalid_timestamps_and_percentages_are_unavailable() {
        for text in [
            "2026-02-29T01:02:03Z",
            "2026-01-01T01:02:03Zjunk",
            "2026-01-01T01:02:03+25:00",
            "2026-01-01T01:02:03",
            "2026-01-01T01:02:03.Z",
            "2026-01-01T01x02:03Z",
        ] {
            assert_eq!(parse_iso(text), None, "{text}");
        }
        for text in [
            r#"{"at":1,"h5":101,"h5_reset":5}"#,
            r#"{"at":1,"h5":-1,"h5_reset":5}"#,
            r#"{"at":1,"h5":1e999,"h5_reset":5}"#,
        ] {
            assert!(claude_file(text).is_none());
        }
        assert_eq!(rpc_reply(r#"{"id":1.5,"result":{}}"#, 1), None);
        assert_eq!(unescaped(r#"{"response":"cut"#, "response"), None);
    }

    #[test]
    fn nested_or_quoted_keys_do_not_supply_a_reading() {
        assert!(claude_file(r#"{"unrelated":{"at":1,"h5":99,"h5_reset":5}}"#).is_none());
        assert_eq!(
            codex_tail(
                r#"{"payload":{"type":"other","text":"token_count","rate_limits":{"primary":{"used_percent":99,"resets_at":5}}}}"#
            ),
            Err(NO_CODEX)
        );
        assert_eq!(
            agy_text(
                "Unknown\tWeekly Limit Remaining\t50%\t2026-11-16T20:09:00Z",
                1,
                1
            ),
            Err(NO_AGY)
        );
    }

    #[test]
    fn a_group_change_or_revoked_permission_discards_inflight_agy() {
        let mut limits = Limits::default();
        let job = limits.start(1000, true, (false, true), 0).unwrap();
        limits.configure((false, true), 1);
        limits.finish(
            Outcome {
                id: job.id,
                group: 0,
                got: [
                    None,
                    None,
                    Some((
                        Ok(Snap {
                            as_of: 1000,
                            h5: win(50, 9000),
                            wk: None,
                        }),
                        Stamp::default(),
                    )),
                ],
            },
            1001,
        );
        assert_eq!(limits.rows[2], Some(Err(ASK_AGY)));
        limits.configure((false, false), 1);
        assert_eq!(limits.rows[2], Some(Err(OFF_AGY)));
    }

    #[test]
    fn null_codex_limits_replace_a_previous_good_reading() {
        let mut limits = Limits::default();
        let job = limits.start(1000, false, (false, false), 0).unwrap();
        limits.finish(
            Outcome {
                id: job.id,
                group: 0,
                got: [None, Some((Ok(Snap::default()), Stamp::default())), None],
            },
            1000,
        );
        let job = limits.start(2000, false, (false, false), 0).unwrap();
        limits.finish(
            Outcome {
                id: job.id,
                group: 0,
                got: [None, Some((Err(NO_CODEX), Stamp::default())), None],
            },
            2000,
        );
        assert_eq!(limits.rows[1], Some(Err(NO_CODEX)));
    }

    #[test]
    fn iso_times_with_zones_and_fractions() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("1970-01-02T00:00:00+00:00"), Some(86_400));
        assert_eq!(parse_iso("1970-01-01T02:00:00.123+02:00"), Some(0));
        assert_eq!(parse_iso("1970-01-01T00:00:00-01:30"), Some(5400));
        assert_eq!(parse_iso("2026-11-12T16:50:00+00:00"), Some(1_794_502_200));
        assert_eq!(parse_iso("2026-13-12T16:50:00Z"), None);
        assert_eq!(parse_iso("garbage"), None);
    }

    #[test]
    fn the_claude_cache_is_found_among_other_keys() {
        let text = r#"{"a":{"utilization":7},"oauthAccount":"{x}","cachedUsageUtilization":{"fetchedAtMs":1794502000000,"accountUuid":"REDACTED","utilization":{"five_hour":{"utilization":48.4,"resets_at":"2026-11-12T16:50:00+00:00"},"seven_day":{"utilization":48,"resets_at":"2026-11-15T07:00:00+00:00"},"limits":[{"kind":"session","percent":48}]}},"z":1}"#;
        let snap = claude_cache(text).unwrap();
        assert_eq!(snap.as_of, 1_794_502_000);
        assert_eq!(snap.h5, win(48, 1_794_502_200));
        assert_eq!(
            snap.wk,
            win(48, 1_794_502_200 + 2 * 86_400 + 14 * 3600 + 10 * 60)
        );
        // A missing window is hidden; no cache key or a null window is no data.
        let one = r#"{"cachedUsageUtilization":{"fetchedAtMs":5000,"utilization":{"five_hour":null,"seven_day":{"utilization":3,"resets_at":"2026-11-15T07:00:00Z"}}}}"#;
        let snap = claude_cache(one).unwrap();
        assert!(snap.h5.is_none() && snap.wk.is_some());
        assert!(claude_cache(r#"{"other":1}"#).is_none());
        assert!(
            claude_cache(
                r#"{"cachedUsageUtilization":{"fetchedAtMs":1,"utilization":{"five_hour":null}}}"#
            )
            .is_none()
        );
        // A cut file does not parse.
        assert!(claude_cache(&text[..120]).is_none());
    }

    #[test]
    fn the_statusline_file_and_the_newer_reading_win() {
        let file = r#"{"at":2000,"h5":12.5,"h5_reset":9000,"wk":null,"wk_reset":null}"#;
        let from_file = claude_file(file).unwrap();
        assert_eq!(
            (from_file.h5, from_file.wk, from_file.as_of),
            (win(13, 9000), None, 2000)
        );
        let cache = Snap {
            as_of: 1000,
            ..Snap::default()
        };
        assert_eq!(claude_best(Some(cache), Some(from_file)), Ok(from_file));
        let newer = Snap {
            as_of: 3000,
            ..cache
        };
        assert_eq!(claude_best(Some(newer), Some(from_file)), Ok(newer));
        assert_eq!(claude_best(None, None), Err(NO_CLAUDE));
        assert!(claude_file(r#"{"at":1}"#).is_none());
    }

    #[test]
    fn codex_reads_the_newest_token_count_only() {
        let good = r#"{"timestamp":"2026-11-12T10:00:00Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":33.0,"window_minutes":300,"resets_at":1794502200},"secondary":{"used_percent":63.2,"window_minutes":10080,"resets_at":1794700000},"plan_type":"plus"}}}"#;
        let snap = codex_tail(good).unwrap();
        assert_eq!(snap.h5, win(33, 1_794_502_200));
        assert_eq!(snap.wk, win(63, 1_794_700_000));
        assert_eq!(snap.as_of, parse_iso("2026-11-12T10:00:00Z").unwrap());
        // Pro: only a weekly window arrives as the primary.
        let weekly = r#"{"timestamp":"2026-11-12T10:00:00Z","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":5,"window_minutes":10080,"resets_at":99}}}}"#;
        let snap = codex_tail(weekly).unwrap();
        assert!(snap.h5.is_none() && snap.wk == win(5, 99));
        // A newer line with null limits hides the older good one.
        let null = r#"{"timestamp":"2026-11-12T11:00:00Z","payload":{"type":"token_count","rate_limits":{"primary":null,"secondary":null,"plan_type":null}}}"#;
        assert_eq!(codex_tail(&format!("{good}\n{null}\n")), Err(NO_CODEX));
        // Other lines after it do not matter; a log with no token_count has no data.
        assert!(codex_tail(&format!("{null}\n{good}\n{{\"type\":\"response_item\"}}\n")).is_ok());
        assert_eq!(codex_tail("{\"type\":\"x\"}\n"), Err(NO_CODEX));
    }

    #[test]
    fn the_codex_rpc_reply_is_found_by_its_id() {
        let out = "{\"id\":0,\"result\":{\"userAgent\":\"x\"}}\n{\"method\":\"note\"}\n{\"id\":1,\"result\":{\"rateLimits\":{\"primary\":{\"usedPercent\":25,\"windowDurationMins\":300,\"resetsAt\":1730947200},\"secondary\":{\"usedPercent\":60,\"windowDurationMins\":10080,\"resetsAt\":1731500000},\"planType\":\"plus\"},\"rateLimitsByLimitId\":{\"codex\":{\"primary\":{\"usedPercent\":99,\"windowDurationMins\":300,\"resetsAt\":1}}}}}\n";
        let snap = codex_rpc(out, 77).unwrap();
        assert_eq!(
            (snap.h5, snap.wk, snap.as_of),
            (win(25, 1_730_947_200), win(60, 1_731_500_000), 77)
        );
        assert_eq!(
            codex_rpc("{\"id\":1,\"error\":{\"message\":\"x\"}}\n", 1),
            Err(NO_CODEX)
        );
        assert_eq!(
            codex_rpc("{\"id\":10,\"result\":{\"rateLimits\":{}}}\n", 1),
            Err(NO_CODEX)
        );
        assert_eq!(codex_rpc("", 1), Err(NO_CODEX));
    }

    #[test]
    fn agy_lines_become_used_percentages_for_the_chosen_group() {
        let raw = "{\"status\":\"SUCCESS\",\"response\":\"Gemini Models\\tWeekly Limit Remaining\\t94%\\t2026-11-16T20:09:00Z\\nGemini Models\\tFive Hour Limit Remaining\\t100%\\t2026-11-12T20:05:00Z\\nClaude and GPT models\\tWeekly Limit Remaining\\t40%\\t2026-11-19T15:05:00Z\\nnoise line\\n\",\"command\":{\"data\":{\"groups\":[]}}}";
        let gemini = agy_text(raw, 0, 5).unwrap();
        assert_eq!(
            gemini.wk,
            win(6, parse_iso("2026-11-16T20:09:00Z").unwrap())
        );
        assert_eq!(
            gemini.h5,
            win(0, parse_iso("2026-11-12T20:05:00Z").unwrap())
        );
        let other = agy_text(raw, 1, 5).unwrap();
        assert_eq!((other.wk.map(|w| w.used), other.h5), (Some(60), None));
        // Drift (no tabs, no response) is unavailable, never a guess.
        assert_eq!(
            agy_text("{\"response\":\"Usage: 5 of 10\"}", 0, 1),
            Err(NO_AGY)
        );
        assert_eq!(agy_text("", 0, 1), Err(NO_AGY));
        // Plain text (no JSON envelope) works too.
        assert!(
            agy_text(
                "Gemini\tWeekly Limit Remaining\t10%\t2026-11-16T20:09:00Z\n",
                0,
                1
            )
            .is_ok()
        );
    }

    #[test]
    fn a_passed_reset_shows_the_window_reset_and_old_readings_show_their_age() {
        let at = clock(0);
        let rows = [
            Some(Ok(Snap {
                h5: win(80, T0 - 60),
                wk: win(30, T0 + 3 * 86_400),
                as_of: T0 - 600,
            })),
            Some(Ok(Snap {
                h5: None,
                wk: win(5, T0 + 86_400),
                as_of: T0 - 4000,
            })),
            Some(Err(OFF_AGY)),
        ];
        let v = view(&rows, T0, &at, false);
        assert!(
            matches!(v.rows[0].h5, Line::Reset { .. }),
            "never the old 80%"
        );
        assert!(matches!(v.rows[0].wk, Line::Used { pct: 30, .. }));
        assert!(!v.rows[0].stale && v.rows[0].as_of.starts_with("as of "));
        assert_eq!(v.rows[1].h5, Line::Hidden);
        assert!(v.rows[1].stale && v.rows[1].as_of == "1 h ago");
        assert_eq!(v.rows[2].why, Some(OFF_AGY));
        assert!(
            v.name
                .contains("Antigravity, unavailable, Allow ROQA to run agy in Settings")
        );
        assert!(
            !v.name.contains("80 percent")
                && v.name.contains("Codex, weekly limit, 5 percent used")
        );
        assert!(v.name.contains("stale, 1 h ago"));
    }

    #[test]
    fn the_accessible_sentence_names_tool_window_percent_reset_and_time() {
        // 2026-11-12 16:50 UTC is a Thursday; at UTC+2 the five-hour reset reads 18:50.
        let reset = parse_iso("2026-11-12T16:50:00Z").unwrap();
        let now = reset - 3600;
        let weekly = parse_iso("2026-11-15T05:00:00Z").unwrap();
        let rows = [
            Some(Ok(Snap {
                h5: win(48, reset),
                wk: win(10, weekly),
                as_of: now - 120,
            })),
            None,
            None,
        ];
        let v = view(&rows, now, &clock(120), false);
        assert!(
            v.name.starts_with(
                "Claude, five-hour limit, 48 percent used, resets at 18:50, as of 17:48. "
            ),
            "{}",
            v.name
        );
        assert!(
            v.name
                .contains("Claude, weekly limit, 10 percent used, resets Sun 07:00, as of 17:48")
        );
        assert!(v.name.contains("Codex, unavailable, Reading..."));
        assert_eq!(
            v.rows[0].h5,
            Line::Used {
                pct: 48,
                reset: "18:50".into()
            }
        );
    }

    #[test]
    fn refresh_policy_reads_on_change_or_age_and_respects_the_cooldown() {
        let mut l = Limits::default();
        let job = l.start(1000, false, (false, false), 0).unwrap();
        assert!(job.due == [true; 3] && !job.manual);
        assert!(
            l.start(1001, true, (false, false), 0).is_none(),
            "one worker at a time"
        );
        let ok = Snap {
            as_of: 1000,
            ..Snap::default()
        };
        let stamp = Stamp { mtime: 5, size: 9 };
        let outcome = Outcome {
            id: job.id,
            group: 0,
            got: [
                Some((Ok(ok), stamp)),
                Some((Err(NO_CODEX), Stamp::default())),
                None,
            ],
        };
        assert!(l.finish(outcome, 1000));
        assert!(
            !l.finish(
                Outcome {
                    id: job.id,
                    group: 0,
                    got: Default::default()
                },
                1000
            ),
            "stale answer"
        );
        // Soon after: nothing aged out (agy never fetched is due once, so that row asks).
        let job = l.start(1100, false, (false, false), 0).unwrap();
        assert_eq!(job.due, [false, false, true]);
        assert_eq!(job.prev[0], stamp);
        l.finish(
            Outcome {
                id: job.id,
                group: 0,
                got: [None, None, Some((Err(OFF_AGY), Stamp::default()))],
            },
            1100,
        );
        // Fresh cache: agy is not due until five minutes pass or the permission changes.
        assert_eq!(
            l.start(1200, false, (false, false), 0).unwrap().due,
            [false; 3]
        );
        l.finish(
            Outcome {
                id: 3,
                group: 0,
                got: Default::default(),
            },
            1200,
        );
        assert!(
            l.start(1200 + AGY_SECS + 1, false, (false, false), 0)
                .unwrap()
                .due[2]
        );
        l.finish(
            Outcome {
                id: 4,
                group: 0,
                got: Default::default(),
            },
            1500,
        );
        assert!(
            l.start(1501, false, (false, true), 0).unwrap().due[2],
            "permission changed"
        );
        l.finish(
            Outcome {
                id: 5,
                group: 0,
                got: Default::default(),
            },
            1501,
        );
        // Manual refresh waits out its cooldown.
        assert!(l.start(2000, true, (false, true), 0).is_some());
        l.finish(
            Outcome {
                id: 6,
                group: 0,
                got: Default::default(),
            },
            2000,
        );
        assert!(
            l.start(2000 + COOLDOWN_SECS - 1, true, (false, true), 0)
                .is_none()
        );
        assert!(
            l.start(2000 + COOLDOWN_SECS, true, (false, true), 0)
                .is_some()
        );
    }

    #[test]
    fn a_failed_read_keeps_the_last_good_numbers() {
        let mut l = Limits::default();
        let job = l.start(10, false, (false, false), 0).unwrap();
        let ok = Snap {
            as_of: 10,
            h5: win(1, 99),
            wk: None,
        };
        l.finish(
            Outcome {
                id: job.id,
                group: 0,
                got: [Some((Ok(ok), Stamp::default())), None, None],
            },
            10,
        );
        let job = l.start(5000, false, (false, false), 0).unwrap();
        l.finish(
            Outcome {
                id: job.id,
                group: 0,
                got: [Some((Err(NO_CLAUDE), Stamp::default())), None, None],
            },
            5000,
        );
        assert_eq!(l.rows[0], Some(Ok(ok)));
    }

    #[test]
    fn the_snippet_is_one_json_line_without_credentials() {
        assert!(
            !SNIPPET.contains('\n')
                && SNIPPET.contains("rate_limits")
                && SNIPPET.contains("claude-limits.json")
        );
        assert!(
            !SNIPPET.to_ascii_lowercase().contains("token") && !SNIPPET.contains(".credentials")
        );
    }
}
