//! Settings (M5 S7): a pure model with defaults and clamping. The store keeps it as `key=value` lines;
//! the host applies a change live through `surface_diff`. No launch-at-login: PLAN forbids startup registration.

use crate::{layout::SurfaceKind, shelf, theme::Theme, timer};

const KINDS: [SurfaceKind; 2] = [SurfaceKind::Notch, SurfaceKind::Dock];
/// Monitors remembered as off, and the longest device name or diagnostics path kept.
pub const MAX_OFF: usize = 8;
const MAX_DEVICE: usize = 64;
pub const MAX_PATH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub dock: bool,
    pub notch: bool,
    /// GDI device names of monitors where ROQA stays off; any monitor not listed is on.
    pub off_monitors: Vec<String>,
    /// Play the system sound when a timer completes.
    pub timer_sound: bool,
    /// File-shelf entries kept, `1..=shelf::MAX`.
    pub shelf_cap: usize,
    /// Opt-in report file the catalog writes; `None` is off.
    pub diagnostics: Option<String>,
    /// Colours of both surfaces (`theme.rs`).
    pub theme: Theme,
    /// The dock lists only windows on the current virtual desktop (pins stay).
    pub desktop_only: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dock: true,
            notch: true,
            off_monitors: Vec::new(),
            timer_sound: true,
            shelf_cap: shelf::MAX,
            diagnostics: None,
            theme: Theme::Classic,
            desktop_only: false,
        }
    }
}

fn flag(value: &str) -> Option<bool> {
    match value {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

impl Settings {
    /// Forces every field into range: at least one surface kind stays on, the shelf cap stays within its
    /// ceiling, and names and paths are bounded, single-line and free of the quotes a pasted path carries.
    pub fn clamped(mut self) -> Self {
        if !self.dock && !self.notch {
            self.notch = true;
        }
        self.off_monitors.retain(|d| {
            !d.is_empty() && d.chars().count() <= MAX_DEVICE && !d.contains(char::is_control)
        });
        self.off_monitors.sort();
        self.off_monitors.dedup();
        self.off_monitors.truncate(MAX_OFF);
        self.shelf_cap = self.shelf_cap.clamp(1, shelf::MAX);
        self.diagnostics = self
            .diagnostics
            .map(|p| p.trim().trim_matches('"').trim().to_owned())
            .filter(|p| {
                !p.is_empty() && p.chars().count() <= MAX_PATH && !p.contains(char::is_control)
            });
        self
    }

    /// Switching every connected monitor off would leave ROQA invisible, so the first stays on.
    pub fn keep_one_monitor(mut self, devices: &[String]) -> Self {
        if !devices.is_empty() && devices.iter().all(|d| self.off_monitors.contains(d)) {
            self.off_monitors.retain(|d| d != &devices[0]);
        }
        self
    }

    pub fn kind_on(&self, kind: SurfaceKind) -> bool {
        match kind {
            SurfaceKind::Notch => self.notch,
            SurfaceKind::Dock => self.dock,
        }
    }

    pub fn wants(&self, kind: SurfaceKind, device: &str) -> bool {
        self.kind_on(kind) && !self.off_monitors.iter().any(|d| d == device)
    }

    /// Store line: true when `key` is a settings key with a usable value (anything else is left to the caller).
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match key {
            "dock" => flag(value).map(|v| self.dock = v).is_some(),
            "notch" => flag(value).map(|v| self.notch = v).is_some(),
            "sound" => flag(value).map(|v| self.timer_sound = v).is_some(),
            "desktop" => flag(value).map(|v| self.desktop_only = v).is_some(),
            "shelfcap" => value.parse().map(|v| self.shelf_cap = v).is_ok(),
            "theme" => Theme::from_key(value).map(|v| self.theme = v).is_some(),
            "diag" if !value.is_empty() => {
                self.diagnostics = Some(value.to_owned());
                true
            }
            "off" if !value.is_empty() => {
                self.off_monitors.push(value.to_owned());
                true
            }
            _ => false,
        }
    }

    /// Appends the lines `set` reads. Values that cannot be one line are dropped (`clamped` removes them too).
    pub fn write_lines(&self, out: &mut String) {
        let one_line = |v: &&String| !v.contains(['\n', '\r']);
        out.push_str(&format!(
            "dock={}\nnotch={}\nsound={}\nshelfcap={}\ntheme={}\ndesktop={}\n",
            u8::from(self.dock),
            u8::from(self.notch),
            u8::from(self.timer_sound),
            self.shelf_cap,
            self.theme.key(),
            u8::from(self.desktop_only)
        ));
        for device in self.off_monitors.iter().filter(one_line) {
            out.push_str(&format!("off={device}\n"));
        }
        for path in self.diagnostics.iter().filter(one_line) {
            out.push_str(&format!("diag={path}\n"));
        }
    }
}

pub type SurfaceId = (SurfaceKind, String);

/// The surfaces a change must create and destroy on the connected `devices`: those whose wanted state
/// differs between the settings. Surfaces the change does not touch (even a runtime-hidden one) stay as they are.
pub fn surface_diff(
    old: &Settings,
    new: &Settings,
    devices: &[String],
) -> (Vec<SurfaceId>, Vec<SurfaceId>) {
    let (mut create, mut destroy) = (Vec::new(), Vec::new());
    for kind in KINDS {
        for device in devices {
            match (old.wants(kind, device), new.wants(kind, device)) {
                (false, true) => create.push((kind, device.clone())),
                (true, false) => destroy.push((kind, device.clone())),
                _ => {}
            }
        }
    }
    (create, destroy)
}

/// Timer length as typed: `m`, `m:ss` or `h:mm:ss`. `None` when malformed or outside `1..=timer::MAX_SECONDS`.
pub fn parse_timer(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let number = |part: &str, below: u64| {
        let ok = !part.is_empty() && part.len() <= 6 && part.bytes().all(|b| b.is_ascii_digit());
        ok.then(|| part.parse::<u64>().ok())
            .flatten()
            .filter(|&n| n < below)
    };
    let seconds = match parts[..] {
        [m] => number(m, u64::MAX)? * 60,
        [m, s] => number(m, u64::MAX)? * 60 + number(s, 60)?,
        [h, m, s] => number(h, u64::MAX)? * 3600 + number(m, 60)? * 60 + number(s, 60)?,
        _ => return None,
    };
    (1..=timer::MAX_SECONDS)
        .contains(&seconds)
        .then_some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn defaults_are_the_pre_settings_behaviour_and_already_clamped() {
        let defaults = Settings::default();
        assert!(defaults.dock && defaults.notch && defaults.timer_sound && !defaults.desktop_only);
        assert_eq!(defaults.shelf_cap, shelf::MAX);
        assert_eq!(defaults.diagnostics, None);
        assert_eq!(defaults.clone().clamped(), defaults);
    }

    #[test]
    fn clamping_bounds_every_field() {
        let wild = Settings {
            dock: false,
            notch: false,
            off_monitors: names(&["b", "", "a", "b", "x\ny", &"d".repeat(MAX_DEVICE + 1)]),
            timer_sound: false,
            shelf_cap: 999,
            diagnostics: Some("  \"C:\\logs\\roqa.json\"  ".into()),
            theme: Theme::Glass,
            desktop_only: true,
        }
        .clamped();
        assert!(!wild.dock && wild.notch, "one surface kind always stays on");
        assert_eq!(wild.off_monitors, ["a", "b"]);
        assert_eq!(wild.shelf_cap, shelf::MAX);
        assert_eq!(wild.theme, Theme::Glass);
        assert!(wild.desktop_only);
        assert_eq!(wild.diagnostics.as_deref(), Some(r"C:\logs\roqa.json"));
        let low = Settings {
            shelf_cap: 0,
            ..Settings::default()
        }
        .clamped();
        assert_eq!(low.shelf_cap, 1);
        for bad in ["", "  ", "\"\"", "a\nb", &"p".repeat(MAX_PATH + 1)] {
            let s = Settings {
                diagnostics: Some(bad.into()),
                ..Settings::default()
            };
            assert_eq!(s.clamped().diagnostics, None, "{bad:?}");
        }
        let many: Vec<String> = (0..20).map(|n| format!("m{n:02}")).collect();
        let s = Settings {
            off_monitors: many,
            ..Settings::default()
        }
        .clamped();
        assert_eq!(s.off_monitors.len(), MAX_OFF);
    }

    #[test]
    fn the_last_monitor_cannot_be_switched_off() {
        let devices = names(&["\\\\.\\DISPLAY1", "\\\\.\\DISPLAY2"]);
        let one = Settings {
            off_monitors: names(&["\\\\.\\DISPLAY2"]),
            ..Settings::default()
        };
        assert_eq!(one.clone().keep_one_monitor(&devices), one);
        let all = Settings {
            off_monitors: devices.clone(),
            ..Settings::default()
        };
        assert_eq!(
            all.keep_one_monitor(&devices).off_monitors,
            [devices[1].clone()]
        );
        // A name that is not connected does not count, and no monitors means nothing to keep.
        let gone = Settings {
            off_monitors: names(&["\\\\.\\DISPLAY9"]),
            ..Settings::default()
        };
        assert_eq!(gone.clone().keep_one_monitor(&devices), gone);
        assert_eq!(gone.clone().keep_one_monitor(&[]), gone);
    }

    #[test]
    fn a_change_maps_to_the_surfaces_to_create_and_destroy() {
        let devices = names(&["D1", "D2"]);
        let base = Settings::default();
        let id = |kind, d: &str| (kind, d.to_owned());
        assert_eq!(surface_diff(&base, &base, &devices), (vec![], vec![]));
        let no_dock = Settings {
            dock: false,
            ..base.clone()
        };
        assert_eq!(
            surface_diff(&base, &no_dock, &devices),
            (
                vec![],
                vec![id(SurfaceKind::Dock, "D1"), id(SurfaceKind::Dock, "D2")]
            )
        );
        assert_eq!(
            surface_diff(&no_dock, &base, &devices),
            (
                vec![id(SurfaceKind::Dock, "D1"), id(SurfaceKind::Dock, "D2")],
                vec![]
            )
        );
        let off2 = Settings {
            off_monitors: names(&["D2"]),
            ..base.clone()
        };
        assert_eq!(
            surface_diff(&base, &off2, &devices),
            (
                vec![],
                vec![id(SurfaceKind::Notch, "D2"), id(SurfaceKind::Dock, "D2")]
            )
        );
        // A kind and a monitor change together; a disabled monitor never gets a created surface.
        let both = Settings {
            notch: false,
            dock: true,
            off_monitors: names(&["D1"]),
            ..base.clone()
        };
        assert_eq!(
            surface_diff(&no_dock, &both, &devices),
            (
                vec![id(SurfaceKind::Dock, "D2")],
                vec![id(SurfaceKind::Notch, "D1"), id(SurfaceKind::Notch, "D2")]
            )
        );
        // Sound, shelf and theme changes create or destroy no surface (a theme rebuilds them in the host).
        let quiet = Settings {
            timer_sound: false,
            shelf_cap: 3,
            theme: Theme::Light,
            ..base.clone()
        };
        assert_eq!(surface_diff(&base, &quiet, &devices), (vec![], vec![]));
        // A disconnected monitor is not in `devices`, so it is never touched.
        assert_eq!(surface_diff(&base, &no_dock, &[]), (vec![], vec![]));
    }

    #[test]
    fn reset_returns_the_defaults_and_the_surfaces_they_imply() {
        let devices = names(&["D1", "D2"]);
        let custom = Settings {
            dock: false,
            off_monitors: names(&["D2"]),
            timer_sound: false,
            shelf_cap: 4,
            diagnostics: Some("C:\\r.json".into()),
            theme: Theme::Glass,
            ..Settings::default()
        };
        let reset = Settings::default();
        assert_ne!(custom, reset);
        assert_eq!(reset.clone().clamped(), reset);
        let (create, destroy) = surface_diff(&custom, &reset, &devices);
        assert_eq!(destroy, vec![]);
        assert_eq!(
            reset.theme,
            Theme::Classic,
            "reset brings the black theme back"
        );
        // Dock on both monitors and the notch on the re-enabled one come back.
        assert_eq!(create.len(), 3);
    }

    #[test]
    fn store_lines_round_trip_and_ignore_what_is_not_a_setting() {
        let custom = Settings {
            dock: false,
            off_monitors: names(&["\\\\.\\DISPLAY2"]),
            timer_sound: false,
            shelf_cap: 5,
            diagnostics: Some("C:\\a=b\\r.json".into()),
            theme: Theme::Midnight,
            desktop_only: true,
            ..Settings::default()
        };
        let mut text = String::new();
        custom.write_lines(&mut text);
        let mut read = Settings::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=').unwrap();
            assert!(read.set(key, value), "{line}");
        }
        assert_eq!(read.clamped(), custom);
        let mut s = Settings::default();
        for (key, value) in [
            ("dock", "yes"),
            ("shelfcap", "x"),
            ("theme", "dark"),
            ("theme", ""),
            ("diag", ""),
            ("off", ""),
            ("pin", "a"),
            ("future", "1"),
        ] {
            assert!(!s.set(key, value), "{key}={value}");
        }
        assert_eq!(s, Settings::default());
        // An out-of-range stored value is accepted by `set` and brought into range by `clamped`.
        assert!(s.set("shelfcap", "9999"));
        assert_eq!(s.clamped().shelf_cap, shelf::MAX);
    }

    #[test]
    fn timer_lengths_parse_as_minutes_or_clock_times() {
        for (text, seconds) in [
            ("5", 300),
            (" 25 ", 1500),
            ("1:30", 90),
            ("0:01", 1),
            ("90:00", 5400),
            ("1:02:03", 3723),
            ("24:00:00", 86_400),
            ("1440", 86_400),
        ] {
            assert_eq!(parse_timer(text), Some(seconds), "{text:?}");
        }
        for bad in [
            "", " ", "0", "0:00", "abc", "-1", "1.5", "1:60", "1:2:60", "1:60:00", "1:2:3:4",
            ":30", "5:", "1441", "24:00:01", "99999999", "٣", "+5",
        ] {
            assert_eq!(parse_timer(bad), None, "{bad:?}");
        }
    }
}
