//! The Prayer tab's Windows side (features P1): the local date and UTC offset, a time of day in the user's
//! locale, the Umm al-Qura date, the audio files on disk and MCI playback. Every call is on the host thread,
//! on demand: nothing here waits, polls or keeps a handle between plays except the MCI alias while a sound runs.

use crate::prayer_tab::{self, Local};
use std::time::Duration;
use windows::{
    Win32::{
        Foundation::SYSTEMTIME,
        Globalization::{
            ENUM_DATE_FORMATS_FLAGS, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS,
            TIME_NOTIMEMARKER,
        },
        Media::Multimedia::mciSendStringW,
        System::SystemInformation::{GetLocalTime, GetSystemTime},
    },
    core::{HSTRING, PCWSTR, w},
};

/// The local date and time and the offset from UTC (DST included) at this moment.
pub fn local_now() -> Local {
    // SAFETY: plain clock reads into value results.
    let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let ymd = |t: &SYSTEMTIME| (i32::from(t.wYear), u32::from(t.wMonth), u32::from(t.wDay));
    let minutes = |t: &SYSTEMTIME| {
        prayer_tab::day_number(ymd(t)) * 1440 + i64::from(t.wHour) * 60 + i64::from(t.wMinute)
    };
    Local {
        ymd: ymd(&local),
        secs: i32::from(local.wHour) * 3600
            + i32::from(local.wMinute) * 60
            + i32::from(local.wSecond),
        offset_min: (minutes(&local) - minutes(&utc)) as i32,
    }
}

/// A time of day as the user's locale writes it, without seconds or AM/PM ("5:12"): the columns are narrow.
pub fn time_label(secs: i32) -> String {
    let secs = secs.rem_euclid(86_400);
    let time = SYSTEMTIME {
        wHour: (secs / 3600) as u16,
        wMinute: (secs / 60 % 60) as u16,
        ..Default::default()
    };
    let mut text = [0u16; 32];
    // SAFETY: NULL locale = user default; the output is a bounded slice whose length includes the NUL.
    let length = unsafe {
        GetTimeFormatEx(
            PCWSTR::null(),
            TIME_NOSECONDS | TIME_NOTIMEMARKER,
            Some(&time),
            PCWSTR::null(),
            Some(&mut text),
        )
    };
    String::from_utf16_lossy(&text[..(length.max(1) as usize - 1).min(text.len())])
}

/// The Umm al-Qura date of a Gregorian one: Windows formats it with the Saudi locale's own calendar.
pub fn hijri(ymd: (i32, u32, u32)) -> Option<String> {
    let date = SYSTEMTIME {
        wYear: u16::try_from(ymd.0).ok()?,
        wMonth: ymd.1 as u16,
        wDay: ymd.2 as u16,
        ..Default::default()
    };
    let mut text = [0u16; 48];
    // SAFETY: a fixed locale name and format, a bounded output slice including the NUL, and a NULL calendar.
    let length = unsafe {
        GetDateFormatEx(
            w!("ar-SA"),
            ENUM_DATE_FORMATS_FLAGS(0),
            Some(&date),
            w!("d'|'M'|'yyyy"),
            Some(&mut text),
            PCWSTR::null(),
        )
    };
    (length > 1)
        .then(|| String::from_utf16_lossy(&text[..length as usize - 1]))
        .and_then(|raw| prayer_tab::hijri_text(&raw))
}

/// The audio files in the `assets` folders around the exe, sorted, at most `MAX_FILES`.
pub fn discover() -> Vec<String> {
    const MAX_FILES: usize = 64;
    let Ok(exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let mut found: Vec<String> = prayer_tab::asset_dirs(&exe)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| prayer_tab::is_audio(path) && path.is_file())
        .filter_map(|path| path.to_str().map(str::to_owned))
        .collect();
    found.sort();
    found.dedup();
    found.truncate(MAX_FILES);
    found
}

const ALIAS: &str = "roqaprayer";

fn mci(command: &str, reply: Option<&mut [u16]>) -> bool {
    let command = HSTRING::from(command);
    // SAFETY: a NUL-terminated command and an optional bounded reply slice; no callback window.
    unsafe { mciSendStringW(&command, reply, None) == 0 }
}

/// One MCI sound at a time, opened when played and closed by `stop` (the host's deadline, a new play or Drop).
#[derive(Default)]
pub struct Player {
    open: bool,
}

impl Player {
    /// Starts `path`; the time it should run, or `None` if it would not open or play.
    pub fn play(&mut self, path: &str) -> Option<Duration> {
        self.stop();
        if path.contains('"') {
            return None;
        }
        self.open = mci(
            &format!("open \"{path}\" type mpegvideo alias {ALIAS}"),
            None,
        );
        let mut reply = [0u16; 32];
        let length = self.open
            && mci(&format!("status {ALIAS} length"), Some(&mut reply))
            && mci(&format!("play {ALIAS}"), None);
        if !length {
            self.stop();
            return None;
        }
        let end = reply.iter().position(|&c| c == 0).unwrap_or(reply.len());
        let millis: u64 = String::from_utf16_lossy(&reply[..end])
            .trim()
            .parse()
            .unwrap_or(0);
        // The clip's length plus a little; a file that reports none or a huge one is cut at ten minutes.
        Some(Duration::from_millis(millis.clamp(1000, 600_000) + 500))
    }
    pub fn stop(&mut self) {
        if std::mem::take(&mut self.open) {
            mci(&format!("close {ALIAS}"), None);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_is_a_whole_number_of_quarter_hours_and_the_date_plausible() {
        let now = local_now();
        assert_eq!(now.offset_min % 15, 0, "{now:?}");
        assert!((-720..=840).contains(&now.offset_min));
        assert!((0..86_400).contains(&now.secs) && now.ymd.0 >= 2026);
    }

    #[test]
    fn windows_gives_the_umm_al_qura_year_and_a_time_label() {
        // 2026-11-12 is in Hijri 1448 (Jumada I or II).
        let text = hijri((2026, 11, 12)).expect("the Saudi locale's calendar");
        assert!(text.ends_with("1448 AH"), "{text}");
        let label = time_label(17 * 3600 + 5 * 60);
        assert!(label.contains(':') || label.contains('.'), "{label}");
    }

    #[test]
    fn a_missing_file_does_not_play() {
        assert!(Player::default().play(r"C:\no\such\file.mp3").is_none());
    }
}
