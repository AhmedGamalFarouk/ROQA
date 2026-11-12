//! Settings (M5 S7): a pure model with defaults and clamping. The store keeps it as `key=value` lines;
//! the host applies a change live through `surface_diff`. No launch-at-login: PLAN forbids startup registration.

use crate::{
    dock::{Button, Buttons},
    layout::{DockStyle, SurfaceKind},
    prayer::Method,
    prayer_tab::LEADS,
    shelf,
    tabs::{Flags, Tab},
    theme::Theme,
    timer, water,
    weather::{MAX_CITY, Units},
};

const KINDS: [SurfaceKind; 2] = [SurfaceKind::Notch, SurfaceKind::Dock];
/// Monitors remembered as off, and the longest device name or diagnostics path kept.
pub const MAX_OFF: usize = 8;
const MAX_DEVICE: usize = 64;
pub const MAX_PATH: usize = 512;
/// The notch sizes in percent of the designed pill (0 is auto, the designed size; 100 would be the same).
pub const NOTCH_SIZES: [u16; 4] = [0, 115, 130, 150];
/// The fine-tune of each daily prayer, in minutes either way.
pub const MAX_SHIFT: i8 = 30;

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
    /// Floating (default) or attached to the bottom edge.
    pub dock_style: DockStyle,
    /// Which taskbar-style buttons the dock shows (`dock::Buttons`).
    pub dock_buttons: Buttons,
    /// The dock lists only windows on the current virtual desktop (pins stay).
    pub desktop_only: bool,
    /// The hover sliders on each monitor's right (volume) and left (brightness) edge (`edge.rs`).
    pub edge_volume: bool,
    pub edge_brightness: bool,
    /// One switch per notch tab (`Tab::ALL` order). A switched-off tab leaves the bar and stops its work.
    pub tabs: Flags,
    /// The mascot eyes in the pill; off removes their state, deadlines and drawing.
    pub eyes: bool,
    /// The mic and camera dots (features B5); off closes the registry watch.
    pub privacy: bool,
    /// The weather city as typed (empty is none), the opt-in device location (it wins over the city while
    /// it works) and °C or °F. Nothing is requested until a city is set or detection is on.
    pub city: String,
    pub detect: bool,
    pub units: Units,
    /// Prayer reminders (the notch expands and the sound plays), the calculation method, the optional
    /// heads-up in minutes (one of `LEADS`) and the sound: empty is the default, `SILENT`, or a file path.
    pub prayer_remind: bool,
    pub prayer_method: Method,
    pub prayer_lead: u32,
    pub prayer_sound: String,
    /// Limits tab: may ROQA run `codex app-server` on Refresh and `agy` for Antigravity (both off until the
    /// user says so), and which Antigravity group shows (0 Gemini, 1 Claude and GPT).
    pub limits_codex: bool,
    pub limits_agy: bool,
    pub limits_group: u8,
    /// Hide the dock and notch over a full-screen app (the F3 prayer alert still shows).
    pub hide_fullscreen: bool,
    /// Open the notch on a click instead of hovering over it.
    pub open_click: bool,
    /// Closed pill size, one of `NOTCH_SIZES` percent.
    pub notch_size: u16,
    /// Quicker animations.
    pub anim_fast: bool,
    /// Master gate over the timer and prayer sounds; their own settings stay as they are.
    pub reminder_sounds: bool,
    /// Pause a playing media session when a prayer reminder fires (never resumed).
    pub prayer_pause: bool,
    /// Minutes added to Fajr, Dhuhr, Asr, Maghrib and Isha.
    pub prayer_shift: [i8; 5],
    /// Water reminders (off by default) and their goal, glass, hours and interval (`water::Config`).
    pub water: bool,
    pub water_cfg: water::Config,
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
            dock_style: DockStyle::Floating,
            dock_buttons: Buttons::default(),
            desktop_only: false,
            // Off until a native run proves the 2 px strips leave edge scrollbars alone; a stored value wins.
            edge_volume: false,
            edge_brightness: false,
            tabs: Tab::DEFAULT,
            eyes: true,
            privacy: true,
            city: String::new(),
            detect: false,
            units: Units::Auto,
            prayer_remind: true,
            prayer_method: Method::Egyptian,
            prayer_lead: 0,
            prayer_sound: String::new(),
            limits_codex: false,
            limits_agy: false,
            limits_group: 0,
            hide_fullscreen: true,
            open_click: false,
            notch_size: 0,
            anim_fast: false,
            reminder_sounds: true,
            prayer_pause: false,
            prayer_shift: [0; 5],
            water: false,
            water_cfg: water::DEFAULT,
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
        self.city = self
            .city
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_CITY)
            .collect::<String>()
            .trim()
            .to_owned();
        if !LEADS.contains(&self.prayer_lead) {
            self.prayer_lead = 0;
        }
        self.limits_group = self.limits_group.min(1);
        if !NOTCH_SIZES.contains(&self.notch_size) {
            self.notch_size = 0;
        }
        water::clamp(&mut self.water_cfg);
        for minutes in &mut self.prayer_shift {
            *minutes = (*minutes).clamp(-MAX_SHIFT, MAX_SHIFT);
        }
        self.prayer_sound = self.prayer_sound.trim().trim_matches('"').trim().to_owned();
        if self.prayer_sound.chars().count() > MAX_PATH
            || self.prayer_sound.contains(char::is_control)
        {
            self.prayer_sound.clear();
        }
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

    /// The shift as `prayer::Params` wants it: by `Prayer as usize`, Sunrise (index 1) unshifted.
    pub fn shift6(&self) -> [i8; 6] {
        let [fajr, dhuhr, asr, maghrib, isha] = self.prayer_shift;
        [fajr, 0, dhuhr, asr, maghrib, isha]
    }

    /// The real reminders' sound gates. "Reminder sounds" off silences both; the Settings Test button is an
    /// explicit action and does not ask (its caption says so).
    pub fn reminder_plays(&self) -> bool {
        self.reminder_sounds
    }
    pub fn timer_beeps(&self) -> bool {
        self.timer_sound && self.reminder_sounds
    }

    /// Pushes the settings that live outside the host's own state: animation pace, click-to-open, pill size.
    pub fn apply_globals(&self) {
        crate::motion::configure(self.anim_fast, self.open_click);
        crate::layout::set_pill_percent(self.notch_size);
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
            "edgevol" => flag(value).map(|v| self.edge_volume = v).is_some(),
            "edgebri" => flag(value).map(|v| self.edge_brightness = v).is_some(),
            "eyes" => flag(value).map(|v| self.eyes = v).is_some(),
            "privacy" => flag(value).map(|v| self.privacy = v).is_some(),
            "detect" => flag(value).map(|v| self.detect = v).is_some(),
            "premind" => flag(value).map(|v| self.prayer_remind = v).is_some(),
            "pmethod" => Method::from_key(value)
                .map(|v| self.prayer_method = v)
                .is_some(),
            "limcodex" => flag(value).map(|v| self.limits_codex = v).is_some(),
            "limagy" => flag(value).map(|v| self.limits_agy = v).is_some(),
            "fshide" => flag(value).map(|v| self.hide_fullscreen = v).is_some(),
            "rsounds" => flag(value).map(|v| self.reminder_sounds = v).is_some(),
            "ppause" => flag(value).map(|v| self.prayer_pause = v).is_some(),
            "notchopen" => matches!(value, "hover" | "click")
                .then(|| self.open_click = value == "click")
                .is_some(),
            "animspeed" => matches!(value, "normal" | "fast")
                .then(|| self.anim_fast = value == "fast")
                .is_some(),
            "notchsize" => value.parse().map(|v| self.notch_size = v).is_ok(),
            "pshift" => {
                // Every entry must parse: one bad number rejects the whole line.
                let minutes: Result<Vec<i8>, _> = value.split(',').map(str::parse).collect();
                minutes
                    .ok()
                    .and_then(|v| <[i8; 5]>::try_from(v).ok())
                    .map(|v| self.prayer_shift = v)
                    .is_some()
            }
            "water" => flag(value).map(|v| self.water = v).is_some(),
            "wcfg" => {
                let v: Result<Vec<u16>, _> = value.split(',').map(str::parse).collect();
                v.ok()
                    .and_then(|v| water::Config::try_from(v).ok())
                    .map(|v| self.water_cfg = v)
                    .is_some()
            }
            "limgroup" => value.parse().map(|v| self.limits_group = v).is_ok(),
            "plead" => value.parse().map(|v| self.prayer_lead = v).is_ok(),
            "psound" if !value.is_empty() => {
                self.prayer_sound = value.to_owned();
                true
            }
            "units" => Units::from_key(value).map(|v| self.units = v).is_some(),
            "city" if !value.is_empty() => {
                self.city = value.to_owned();
                true
            }
            _ if key.starts_with("tab") => key
                .strip_prefix("tab")
                .and_then(Tab::from_key)
                .zip(flag(value))
                .map(|(tab, on)| self.tabs[tab.index()] = on)
                .is_some(),
            "shelfcap" => value.parse().map(|v| self.shelf_cap = v).is_ok(),
            "theme" => Theme::from_key(value).map(|v| self.theme = v).is_some(),
            _ if Button::ALL.iter().any(|b| b.key() == key) => Button::ALL
                .iter()
                .find(|b| b.key() == key)
                .zip(flag(value))
                .map(|(button, on)| self.dock_buttons.shown[button.index()] = on)
                .is_some(),
            "dockstyle" => DockStyle::from_key(value)
                .map(|v| self.dock_style = v)
                .is_some(),
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
            "dock={}\nnotch={}\nsound={}\nshelfcap={}\ntheme={}\ndesktop={}\nedgevol={}\nedgebri={}\n",
            u8::from(self.dock),
            u8::from(self.notch),
            u8::from(self.timer_sound),
            self.shelf_cap,
            self.theme.key(),
            u8::from(self.desktop_only),
            u8::from(self.edge_volume),
            u8::from(self.edge_brightness)
        ));
        out.push_str(&format!(
            "eyes={}\nprivacy={}\ndetect={}\nunits={}\ndockstyle={}\n",
            u8::from(self.eyes),
            u8::from(self.privacy),
            u8::from(self.detect),
            self.units.key(),
            self.dock_style.key()
        ));
        out.push_str(&format!(
            "premind={}
pmethod={}
plead={}
",
            u8::from(self.prayer_remind),
            self.prayer_method.key(),
            self.prayer_lead
        ));
        out.push_str(&format!(
            "limcodex={}
limagy={}
limgroup={}
",
            u8::from(self.limits_codex),
            u8::from(self.limits_agy),
            self.limits_group
        ));
        let [a, b, c, d, e] = self.prayer_shift;
        out.push_str(&format!(
            "fshide={}
rsounds={}
ppause={}
notchopen={}
animspeed={}
notchsize={}
pshift={a},{b},{c},{d},{e}
water={}
wcfg={},{},{},{},{}
",
            u8::from(self.hide_fullscreen),
            u8::from(self.reminder_sounds),
            u8::from(self.prayer_pause),
            ["hover", "click"][usize::from(self.open_click)],
            ["normal", "fast"][usize::from(self.anim_fast)],
            self.notch_size,
            u8::from(self.water),
            self.water_cfg[0],
            self.water_cfg[1],
            self.water_cfg[2],
            self.water_cfg[3],
            self.water_cfg[4],
        ));
        if !self.prayer_sound.is_empty() && one_line(&&self.prayer_sound) {
            out.push_str(&format!(
                "psound={}
",
                self.prayer_sound
            ));
        }
        if !self.city.is_empty() && one_line(&&self.city) {
            out.push_str(&format!("city={}\n", self.city));
        }
        for button in Button::ALL {
            out.push_str(&format!(
                "{}={}\n",
                button.key(),
                u8::from(self.dock_buttons.on(button))
            ));
        }
        for tab in Tab::ALL {
            out.push_str(&format!(
                "tab{}={}\n",
                tab.key(),
                u8::from(self.tabs[tab.index()])
            ));
        }
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
        assert!(!defaults.edge_volume && !defaults.edge_brightness);
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
            theme: Theme::Frosted,
            dock_style: DockStyle::Attached,
            dock_buttons: Buttons::of([false, true, true, false, true]),
            desktop_only: true,
            edge_volume: false,
            edge_brightness: false,
            tabs: Tab::DEFAULT,
            eyes: true,
            privacy: false,
            city: "  Ca\niro \u{7}".into(),
            detect: true,
            units: Units::Fahrenheit,
            prayer_remind: true,
            prayer_method: Method::Isna,
            prayer_lead: 7,
            prayer_sound: r#"  "C:\a\takbeer.mp3"  "#.into(),
            limits_codex: true,
            limits_agy: true,
            limits_group: 9,
            notch_size: 120,
            prayer_shift: [-99, 0, 99, 5, -5],
            water_cfg: [0, 5, 30, 0, 1],
            ..Settings::default()
        }
        .clamped();
        assert_eq!(
            (wild.notch_size, wild.prayer_shift),
            (0, [-30, 0, 30, 5, -5])
        );
        assert_eq!(wild.water_cfg, [1, 50, 23, 24, 10]);
        assert_eq!(
            (
                wild.prayer_lead,
                wild.prayer_method,
                wild.prayer_sound.as_str()
            ),
            (0, Method::Isna, r"C:\a\takbeer.mp3")
        );
        assert_eq!(wild.limits_group, 1);
        assert!(!wild.dock && wild.notch, "one surface kind always stays on");
        assert_eq!(wild.off_monitors, ["a", "b"]);
        assert_eq!(wild.shelf_cap, shelf::MAX);
        assert_eq!(wild.theme, Theme::Frosted);
        assert!(wild.desktop_only && !wild.edge_volume && !wild.edge_brightness);
        assert_eq!(wild.diagnostics.as_deref(), Some(r"C:\logs\roqa.json"));
        assert_eq!(
            (wild.city.as_str(), wild.detect, wild.units),
            ("Cairo", true, Units::Fahrenheit)
        );
        let long = Settings {
            city: "c".repeat(MAX_CITY + 9),
            ..Settings::default()
        };
        assert_eq!(long.clamped().city.chars().count(), MAX_CITY);
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
    fn the_new_settings_default_to_todays_behaviour_and_round_trip() {
        let d = Settings::default();
        assert!(d.hide_fullscreen && d.reminder_sounds && !d.open_click && !d.anim_fast);
        assert!(!d.prayer_pause && d.notch_size == 0 && d.prayer_shift == [0; 5]);
        assert!(
            !d.water && d.water_cfg == water::DEFAULT,
            "water is off until switched on"
        );
        assert_eq!(d.shift6(), [0; 6]);
        // An old store holds none of the new keys: loading it leaves every default.
        let mut old = Settings::default();
        for (key, value) in [
            ("dock", "1"),
            ("theme", "glass"),
            ("sound", "0"),
            ("pmethod", "mwl"),
        ] {
            assert!(old.set(key, value), "{key}");
        }
        assert_eq!(
            (old.hide_fullscreen, old.notch_size, old.prayer_shift),
            (true, 0, [0; 5])
        );
        let custom = Settings {
            hide_fullscreen: false,
            open_click: true,
            notch_size: 130,
            anim_fast: true,
            reminder_sounds: false,
            prayer_pause: true,
            prayer_shift: [-5, 0, 7, 30, -30],
            water: true,
            water_cfg: [6, 300, 8, 20, 45],
            ..Settings::default()
        };
        let mut text = String::new();
        custom.write_lines(&mut text);
        for line in [
            "fshide=0",
            "notchopen=click",
            "notchsize=130",
            "animspeed=fast",
            "rsounds=0",
            "ppause=1",
            "pshift=-5,0,7,30,-30",
            "water=1",
            "wcfg=6,300,8,20,45",
        ] {
            assert!(text.lines().any(|l| l == line), "{line}");
        }
        let mut read = Settings::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=').unwrap();
            assert!(read.set(key, value), "{line}");
        }
        assert_eq!(read.clone().clamped(), custom);
        assert_eq!(custom.shift6(), [-5, 0, 0, 7, 30, -30]);
        // Bad values are ignored, leaving the stored default.
        for (key, value) in [
            ("notchopen", "x"),
            ("animspeed", ""),
            ("pshift", "1,2,3"),
            ("pshift", "1,x,3,4,5"),
            ("pshift", "1,2,3,4,5,6"),
            ("pshift", "a,b,c,d,e"),
            ("wcfg", "1,2,3"),
            ("wcfg", "1,2,3,4,x"),
            ("water", "yes"),
            ("notchsize", "big"),
            ("fshide", "2"),
        ] {
            assert!(!read.clone().set(key, value), "{key}={value}");
        }
    }

    #[test]
    fn the_master_switch_silences_the_real_prayer_and_timer_sounds_only() {
        let on = Settings::default();
        assert!(on.reminder_plays() && on.timer_beeps());
        let off = Settings {
            reminder_sounds: false,
            ..Settings::default()
        };
        // The sound choices stay as they were; only the gate closes.
        assert!(!off.reminder_plays() && !off.timer_beeps() && off.timer_sound);
        let quiet_timer = Settings {
            timer_sound: false,
            ..Settings::default()
        };
        assert!(quiet_timer.reminder_plays() && !quiet_timer.timer_beeps());
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
            theme: Theme::Frosted,
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
            dock_style: DockStyle::Attached,
            dock_buttons: Buttons::of([false, true, true, false, true]),
            desktop_only: true,
            edge_volume: true,
            tabs: [false, true, false, true, false, true],
            limits_codex: true,
            limits_agy: true,
            limits_group: 1,
            eyes: false,
            city: "Washington, D.C.".into(),
            detect: true,
            units: Units::Celsius,
            prayer_remind: false,
            prayer_method: Method::Mwl,
            prayer_lead: 10,
            prayer_sound: "D:\\a b\\x.mp3".into(),
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
            ("detect", "2"),
            ("units", "k"),
            ("city", ""),
        ] {
            assert!(!s.set(key, value), "{key}={value}");
        }
        assert_eq!(s, Settings::default());
        // An out-of-range stored value is accepted by `set` and brought into range by `clamped`.
        assert!(s.set("shelfcap", "9999"));
        assert_eq!(s.clamped().shelf_cap, shelf::MAX);
    }

    #[test]
    fn weather_defaults_ask_for_nothing_and_an_empty_city_is_not_written() {
        let defaults = Settings::default();
        assert!(defaults.city.is_empty() && !defaults.detect && defaults.units == Units::Auto);
        assert!(!crate::weather::configured(defaults.detect, &defaults.city));
        let mut text = String::new();
        defaults.write_lines(&mut text);
        assert!(text.contains("detect=0\n") && text.contains("units=auto\n"));
        assert!(!text.contains("city="));
    }

    #[test]
    fn edge_sliders_default_off_but_a_stored_choice_is_kept() {
        let mut stored = Settings::default();
        // An older store without the keys gets the new default; explicit values survive either way.
        assert!(!stored.edge_volume && !stored.edge_brightness);
        assert!(stored.set("edgevol", "1") && stored.set("edgebri", "0"));
        assert!(stored.edge_volume && !stored.edge_brightness);
        let mut text = String::new();
        stored.write_lines(&mut text);
        assert!(text.contains("edgevol=1\n") && text.contains("edgebri=0\n"));
    }

    #[test]
    fn tabs_and_eyes_default_on_round_trip_and_ignore_unknown_tabs() {
        let defaults = Settings::default();
        // Limits is the one tab that starts off; permission to run other tools starts off too.
        assert!(
            defaults.tabs[..5].iter().all(|&on| on)
                && !defaults.tabs[Tab::Limits.index()]
                && defaults.eyes
        );
        assert!(!defaults.limits_codex && !defaults.limits_agy);
        // Each switch is its own line, so one tab can be off without touching the others.
        let mut off = Settings::default();
        assert!(off.set("tabtimer", "0") && off.set("tabweather", "0"));
        assert_eq!(off.tabs, [true, false, true, false, true, false]);
        let mut text = String::new();
        off.write_lines(&mut text);
        for line in [
            "tabmedia=1",
            "tabweather=0",
            "tabprayer=1",
            "tabtimer=0",
            "tabshelf=1",
            "tablimits=0",
            "eyes=1",
            "privacy=1",
        ] {
            assert!(text.lines().any(|l| l == line), "{line}");
        }
        let mut read = Settings::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=').unwrap();
            assert!(read.set(key, value), "{line}");
        }
        assert_eq!(read, off);
        for (key, value) in [("tabfuture", "0"), ("tabmedia", "2"), ("eyes", "x")] {
            assert!(!read.set(key, value), "{key}={value}");
        }
        assert_eq!(read, off);
    }

    #[test]
    fn dock_buttons_default_round_trip_and_ignore_bad_values() {
        let defaults = Settings::default();
        assert_eq!(defaults.dock_buttons.shown, [true, true, false, true, true]);
        let mut off = Settings::default();
        assert!(off.set("dbstart", "0") && off.set("dbtask", "1"));
        assert_eq!(off.dock_buttons.shown, [false, true, true, true, true]);
        let mut text = String::new();
        off.write_lines(&mut text);
        let mut read = Settings::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=').unwrap();
            assert!(read.set(key, value), "{line}");
        }
        assert_eq!(read, off);
        assert!(!read.set("dbsearch", "x") && !read.set("dbfuture", "1"));
        assert_eq!(read, off);
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
