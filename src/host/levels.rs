//! The dock's volume and brightness popups (design `cook/design/dock-sliders.md`): slider rows over the button, the
//! master level, the apps playing sound and the output devices, or one row per monitor. One `impl Desktop` block;
//! the methods are `pub(super)` so the host reaches them. Nothing runs while no popup shows.

use super::*;
use crate::{
    brightness::{Brightness, Target},
    edge::{READ_LIMIT, Throttle},
};
use windows::Win32::{System::Threading::GetCurrentThreadId, UI::Shell::ShellExecuteW};

/// What a monitor's brightness is known to be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Know {
    Pending,
    Unsupported,
    Level(u8),
}

struct Screen {
    target: Target,
    name: String,
    know: Know,
    throttle: Throttle,
}

/// The open popup's state; it outlives the popup only while a brightness write waits for its gap.
pub(super) struct Levels {
    button: Button,
    apps: Vec<audio::App>,
    apps_read: Option<u32>,
    devices: Vec<audio::Device>,
    screens: Vec<Screen>,
    /// When a reading nobody answered counts as "not supported".
    read_due: Option<Instant>,
}

type Rows = (Vec<Item>, Vec<String>, Vec<Option<Arc<IconPixels>>>);

impl Levels {
    /// The rows now: the master and the apps with their icons, then the devices; or a row per monitor.
    fn rows(&self, audio: &Audio) -> Rows {
        let (mut items, mut titles, mut icons) = (Vec::new(), Vec::new(), Vec::new());
        let mut add = |item, title: &str, icon| {
            items.push(item);
            titles.push(title.to_owned());
            icons.push(icon);
        };
        if self.button == Button::Brightness {
            for screen in &self.screens {
                match screen.know {
                    Know::Level(percent) => add(Item::Level(percent, false), &screen.name, None),
                    Know::Pending => add(Item::Label, &format!("{}: reading", screen.name), None),
                    Know::Unsupported => {
                        add(
                            Item::Label,
                            &format!("{}: not supported", screen.name),
                            None,
                        );
                    }
                }
            }
            return (items, titles, icons);
        }
        let name = self.devices.iter().find(|device| device.current);
        match audio.level() {
            Some(level) => add(
                Item::Level(level.percent, level.muted),
                name.map_or("Volume", |device| device.name.as_str()),
                None,
            ),
            None => add(Item::Label, "No sound device", None),
        }
        for app in &self.apps {
            add(
                Item::Level(app.percent, app.muted),
                &app.name,
                app.icon.clone(),
            );
        }
        if self.apps.is_empty() {
            add(Item::Label, "Nothing is playing", None);
        }
        if self.devices.len() > 1 {
            for (index, device) in self.devices.iter().enumerate() {
                add(Item::Device(index, device.current), &device.name, None);
            }
        }
        (items, titles, icons)
    }
}

impl Desktop {
    /// The popup over a Volume or Brightness button: opens at once and reads what it shows.
    pub(super) fn show_levels_popup(&mut self, hwnd: HWND, button: Button) {
        let Some((buttons, bounds, dpi, work, home)) = self.dock_at(hwnd).and_then(|dock| {
            self.monitors
                .iter()
                .find(|monitor| monitor.device == dock.monitor)
                .map(|monitor| {
                    (
                        dock.dock_buttons,
                        dock.bounds,
                        dock.dpi,
                        monitor.work,
                        monitor.device.clone(),
                    )
                })
        }) else {
            return;
        };
        let mut levels = Levels {
            button,
            apps: Vec::new(),
            apps_read: None,
            devices: Vec::new(),
            screens: Vec::new(),
            read_due: None,
        };
        if button == Button::Volume {
            self.audio.start();
            self.audio.refresh();
            levels.devices = self.audio.devices();
            self.audio.watch_apps(true);
            levels.apps = self.audio.apps();
            levels.apps_read = Some(self.audio.apps_epoch());
        } else {
            let mut monitors: Vec<&Monitor> = self.monitors.iter().collect();
            monitors.sort_by_key(|monitor| (monitor.work.x, monitor.work.y));
            levels.screens = monitors
                .iter()
                .enumerate()
                .map(|(index, monitor)| Screen {
                    target: (
                        monitor.work.x + monitor.work.width / 2,
                        monitor.work.y + monitor.work.height / 2,
                    ),
                    name: if monitor.device == home {
                        "This screen".into()
                    } else {
                        format!("Screen {}", index + 1)
                    },
                    know: Know::Pending,
                    throttle: Throttle::default(),
                })
                .collect();
            if let Some(worker) = self.brightness_worker() {
                for screen in &levels.screens {
                    worker.read(screen.target);
                }
            }
            levels.read_due = Some(Instant::now() + READ_LIMIT);
        }
        self.fetch_icons(&mut levels);
        let (items, titles, icons) = levels.rows(&self.audio);
        let hit = Hit::Button(button);
        let (left, width) = dock::span(buttons, bounds.width, dpi, hit);
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            identity: Identity::Unavailable(0),
            windows: Vec::new(),
            items,
            dock_bounds: bounds,
            work,
            dpi,
            app: if button == Button::Volume {
                "Volume"
            } else {
                "Brightness"
            }
            .into(),
            titles,
            icons,
            x: bounds.x + left + width / 2,
            below: None,
            return_to: None,
        };
        match dock_menu::Menu::open_hover(&self.graphics, self.palette, spec) {
            Ok(mut popup) => {
                popup.hover_hit = Some(hit);
                self.hover_popup = Some(popup);
                self.levels = Some(levels);
            }
            Err(error) => eprintln!("Dock levels: {error}"),
        }
    }
    /// The brightness worker, started on first need.
    fn brightness_worker(&mut self) -> Option<&Brightness> {
        if self.brightness.is_none() {
            // SAFETY: reads this thread's ID; no pointers.
            self.brightness = Brightness::start(unsafe { GetCurrentThreadId() });
        }
        self.brightness.as_ref()
    }
    /// An icon the service has not extracted yet is asked for again on each pass; true when one arrived.
    fn fetch_icons(&mut self, levels: &mut Levels) -> bool {
        let size = self.icon_size();
        let icons = &mut self.icons;
        let mut arrived = false;
        for app in levels.apps.iter_mut().filter(|app| app.icon.is_none()) {
            let identity = Identity::Executable(app.path.clone());
            app.icon = size.and_then(|size| {
                icons.icon(&identity, crate::icons::sources(&identity, None), size)
            });
            arrived |= app.icon.is_some();
        }
        arrived
    }
    /// One host pass: readings and writes for brightness, the apps and levels for volume, the popup's rows kept
    /// current. Returns the wait until its next deadline; nothing is kept once the popup is gone and no write waits.
    pub(super) fn refresh_levels(&mut self) -> u32 {
        let now = Instant::now();
        let mut readings = Vec::new();
        while let Some(reading) = self.brightness.as_ref().and_then(Brightness::poll) {
            readings.push(reading);
        }
        let Some(mut levels) = self.levels.take() else {
            return INFINITE;
        };
        let mine = self
            .hover_popup
            .as_ref()
            .is_some_and(|popup| popup.hover_hit == Some(Hit::Button(levels.button)));
        for reading in readings {
            for screen in levels
                .screens
                .iter_mut()
                .filter(|screen| screen.target == reading.target)
            {
                screen.know = reading.percent.map_or(Know::Unsupported, Know::Level);
            }
        }
        if levels.read_due.is_some_and(|due| now >= due) {
            levels.read_due = None;
            for screen in levels
                .screens
                .iter_mut()
                .filter(|screen| screen.know == Know::Pending)
            {
                screen.know = Know::Unsupported;
            }
        }
        let writes: Vec<(Target, u8)> = levels
            .screens
            .iter_mut()
            .filter_map(|screen| Some((screen.target, screen.throttle.take(now)?)))
            .collect();
        for (target, percent) in writes {
            let sent = self
                .brightness_worker()
                .is_some_and(|worker| worker.write(target, percent));
            if !sent && let Some(screen) = levels.screens.iter_mut().find(|s| s.target == target) {
                // A slow monitor's full queue: the value waits for the next gap.
                screen.throttle.restore(percent);
            }
        }
        let mut icons_changed = false;
        if mine && levels.button == Button::Volume {
            let epoch = self.audio.apps_epoch();
            if levels.apps_read != Some(epoch) {
                levels.apps_read = Some(epoch);
                levels.apps = self.audio.apps();
                icons_changed = true;
            }
            icons_changed |= self.fetch_icons(&mut levels);
        }
        if mine && let Some(popup) = &mut self.hover_popup {
            let (items, titles, icons) = levels.rows(&self.audio);
            if !popup.held() && (popup.items() != items || icons_changed) {
                let _ = popup.relist(items, titles, icons);
            }
        }
        self.audio
            .watch_apps(mine && levels.button == Button::Volume);
        let pending = levels
            .screens
            .iter()
            .any(|screen| screen.know == Know::Pending);
        let due = levels
            .screens
            .iter()
            .filter_map(|screen| screen.throttle.deadline(now))
            .chain(levels.read_due.filter(|_| pending))
            .min();
        if mine || due.is_some() {
            self.levels = Some(levels);
        }
        due.map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// A slider row was dragged or wheeled to `percent`.
    pub(super) fn set_level(&mut self, row: usize, percent: u8) {
        let Some(levels) = &mut self.levels else {
            return;
        };
        if levels.button == Button::Brightness {
            if let Some(screen) = levels.screens.get_mut(row) {
                screen.know = Know::Level(percent);
                screen.throttle.push(percent);
            }
        } else if row == 0 {
            self.audio.set(f32::from(percent) / 100.0);
        } else if let Some(app) = levels.apps.get_mut(row - 1) {
            app.set(percent);
        }
    }
    /// A press on an app row's icon.
    pub(super) fn mute_row(&mut self, row: usize) {
        if let Some(app) = self
            .levels
            .as_mut()
            .and_then(|levels| levels.apps.get_mut(row.checked_sub(1)?))
        {
            app.toggle_mute();
        }
    }
    /// A device row was chosen: it becomes the default output. Where Windows will not allow it, Sound settings opens.
    pub(super) fn choose_device(&mut self, index: usize) {
        let Some(device) = self
            .levels
            .as_ref()
            .and_then(|levels| levels.devices.get(index))
        else {
            return;
        };
        if let Err(error) = crate::policy_config::set_default(&device.id) {
            eprintln!("Dock volume: switch device: {error}");
            // SAFETY: a shell open of a fixed URI; the shell shows any failure itself.
            unsafe {
                ShellExecuteW(
                    None,
                    w!("open"),
                    w!("ms-settings:sound"),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(name: &str, know: Know) -> Screen {
        Screen {
            target: (0, 0),
            name: name.into(),
            know,
            throttle: Throttle::default(),
        }
    }

    #[test]
    fn brightness_rows_say_reading_not_supported_or_hold_a_slider_each() {
        let levels = Levels {
            button: Button::Brightness,
            apps: Vec::new(),
            apps_read: None,
            devices: Vec::new(),
            screens: vec![
                screen("This screen", Know::Level(70)),
                screen("Screen 2", Know::Pending),
                screen("Screen 3", Know::Unsupported),
            ],
            read_due: None,
        };
        let (items, titles, icons) = levels.rows(&Audio::create());
        assert_eq!(items, [Item::Level(70, false), Item::Label, Item::Label]);
        assert_eq!(
            titles,
            [
                "This screen",
                "Screen 2: reading",
                "Screen 3: not supported"
            ]
        );
        assert_eq!(icons.len(), 3);
    }

    #[test]
    fn volume_without_a_device_says_so_and_nothing_playing_keeps_a_row() {
        let levels = Levels {
            button: Button::Volume,
            apps: Vec::new(),
            apps_read: None,
            devices: Vec::new(),
            screens: Vec::new(),
            read_due: None,
        };
        let (items, titles, _) = levels.rows(&Audio::create());
        assert_eq!(items, [Item::Label, Item::Label]);
        assert_eq!(titles, ["No sound device", "Nothing is playing"]);
    }
}
