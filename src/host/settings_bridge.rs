//! The Settings window's bridge: where it opens, what it shows, and applying what it changed.
//! One `impl Desktop` block moved out of `host.rs` unchanged; the methods are `pub(super)` so the host reaches them.

use super::*;

impl Desktop {
    /// The work area a dialog opens on: the monitor under the pointer when ROQA is on it, else the first one ROQA
    /// is on, else the main one.
    pub(super) fn dialog_work_area(&self) -> Result<layout::Bounds> {
        let mut at = Default::default();
        // SAFETY: fills a local point.
        let _ = unsafe { GetCursorPos(&mut at) };
        let shown = |m: &&Monitor| self.surfaces.iter().any(|s| s.monitor == m.device);
        let under = |m: &&Monitor| {
            (m.rect.x..m.rect.x + m.rect.width).contains(&at.x)
                && (m.rect.y..m.rect.y + m.rect.height).contains(&at.y)
        };
        Ok(
            match self
                .monitors
                .iter()
                .filter(shown)
                .find(under)
                .or_else(|| self.monitors.iter().find(shown))
            {
                Some(monitor) => monitor.work,
                None => native_window::primary_work_area()?,
            },
        )
    }
    /// Opens the settings window (or replaces an open one) on the connected monitors.
    pub(super) fn open_settings(&mut self) -> Result<()> {
        self.settings_window.take();
        let work = self.dialog_work_area()?;
        self.settings_window = Some(SettingsWindow::create(
            &self.settings,
            &self.monitors,
            self.text_factor,
            work,
        )?);
        self.sync_settings_pins(0);
        self.sync_water();
        self.sync_settings_hotkeys();
        Ok(())
    }
    pub(super) fn sync_settings_hotkeys(&mut self) {
        if let Some(window) = &mut self.settings_window {
            window.set_hotkeys(&self.hotkeys.notes());
        }
    }
    pub(super) fn sync_settings_pins(&self, selected: usize) {
        if let Some(window) = &self.settings_window {
            window.set_pins(
                &self
                    .pins
                    .items()
                    .iter()
                    .map(crate::pins::display_name)
                    .collect::<Vec<_>>(),
                selected,
            );
            window.set_hidden(
                &self
                    .pins
                    .hidden()
                    .iter()
                    .map(crate::pins::display_name)
                    .collect::<Vec<_>>(),
            );
        }
    }
    /// Takes what the settings window produced in its last message: a change to apply live and a timer to
    /// start. The window is told how the timer went.
    pub(super) fn settings_changed(&mut self, catalog: &mut Catalog) -> Result<()> {
        let Some(window) = &mut self.settings_window else {
            return Ok(());
        };
        let (change, start, movement, show, sound, drank, pomo) = (
            window.take_change(),
            window.take_start(),
            window.take_move(),
            window.take_show(),
            window.take_sound_action(),
            window.take_drank(),
            window.take_pomo(),
        );
        match sound {
            Some(SoundAction::Browse) => {
                if let Some(paths) = self.file_picker.start_with(true) {
                    self.choose_sound(paths);
                }
            }
            Some(SoundAction::Test) => {
                let missing = prayer_tab::resolve(&self.settings.prayer_sound, &[])
                    .is_some_and(Self::sound_missing);
                let status = if !self.play_prayer_sound(Instant::now()) {
                    "No sound to play. Pick another file."
                } else if missing {
                    "Sound file missing. Playing the default."
                } else {
                    "Playing the prayer sound."
                };
                if let Some(window) = &self.settings_window {
                    let _ = window.say(status);
                }
            }
            None => {}
        }
        if let Some((from, later)) = movement {
            let to = if later {
                from.saturating_add(1)
            } else {
                from.saturating_sub(1)
            };
            // Select the destination first: `change_pins` refreshes the combo and keeps that row.
            if let Some(window) = &self.settings_window {
                window.select_pin(to);
            }
            self.change_pins(|pins| pins.move_to(from, to));
        }
        if let Some(index) = show {
            self.change_pins(|pins| pins.unhide(index));
        }
        if let Some(next) = change {
            self.apply_settings(next, catalog)?;
        }
        if drank {
            self.water_drink();
        }
        if let Some(skip) = pomo {
            let status = if self.pages.tabs.on(Tab::Timer) {
                self.pomo_act(skip)
            } else {
                "Switch the Timer tab on to use the Pomodoro.".to_owned()
            };
            if let Some(window) = &self.settings_window {
                let _ = window.say(&status);
            }
        }
        self.sync_water();
        if let Some(seconds) = start {
            let status = if self.pages.tabs.on(Tab::Timer) {
                self.start_timer(seconds)
            } else {
                "Switch the Timer tab on to start a timer.".to_owned()
            };
            if let Some(window) = &self.settings_window {
                let _ = window.say(&status);
            }
        }
        Ok(())
    }
    /// The file the user browsed to becomes the prayer sound.
    pub(super) fn choose_sound(&mut self, paths: Vec<String>) {
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        if shelf_os::is_network(&path) {
            if let Some(window) = &self.settings_window {
                let _ = window.say("Pick a file on this PC.");
            }
            return;
        }
        let next = Settings {
            prayer_sound: path,
            ..self.settings.clone()
        }
        .clamped();
        if next.prayer_sound == self.settings.prayer_sound {
            return;
        }
        self.settings = next;
        self.store.settings = self.settings.clone();
        self.pending.mark(Instant::now());
        if let Some(window) = &mut self.settings_window {
            window.set_sound(&self.settings.prayer_sound);
        }
    }
    /// Applies new settings live: creates and destroys exactly the surfaces `settings::surface_diff` names,
    /// trims the shelf to its cap, repoints the diagnostics report and marks the store dirty once.
    pub(super) fn apply_settings(&mut self, next: Settings, catalog: &mut Catalog) -> Result<()> {
        let devices: Vec<String> = self.monitors.iter().map(|m| m.device.clone()).collect();
        let next = next.clamped().keep_one_monitor(&devices);
        if next == self.settings {
            return Ok(());
        }
        let old = std::mem::replace(&mut self.settings, next);
        self.settings.apply_globals();
        self.water_due = None; // the schedule follows the new hours
        if old.clipboard != self.settings.clipboard {
            for notch in self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Notch)
            {
                clipboard_os::watch(notch.window.0, self.settings.clipboard);
            }
        }
        if old.notifications != self.settings.notifications {
            self.toasts = self.settings.notifications.then(toast::Toasts::new);
        }
        // A new theme, pill size or accent mode is fixed when a surface is created, so they rebuild the surfaces.
        let restyle = old.theme != self.settings.theme
            || old.notch_size != self.settings.notch_size
            || old.match_accent != self.settings.match_accent;
        if self.settings.open_click && !old.open_click {
            // A hover intent still pending must not open the notch after the switch to click mode.
            for surface in &mut self.surfaces {
                if let Role::Notch { intent, .. } = &mut surface.role {
                    *intent = None;
                }
            }
        }
        if old.hide_fullscreen != self.settings.hide_fullscreen {
            self.fullscreen_generation = None; // the next pass hides or shows again
        }
        if old.autostart != self.settings.autostart
            && !crate::system_os::set_autostart(self.settings.autostart)
        {
            // The registry refused: keep what is really set, and the switch goes back and says so.
            self.settings.autostart = old.autostart;
            if let Some(window) = &mut self.settings_window {
                window.set_autostart(old.autostart);
                let _ = window.say("Couldn't change start at sign-in.");
            }
        }
        if (
            old.dock_hide,
            old.notch_hide,
            old.reveal_delay,
            old.hide_delay,
        ) != (
            self.settings.dock_hide,
            self.settings.notch_hide,
            self.settings.reveal_delay,
            self.settings.hide_delay,
        ) {
            self.tuck_generation = None;
        }
        self.edges_dirty = true;
        if old.hotkeys != self.settings.hotkeys {
            self.hotkeys.sync(self.settings.hotkeys);
            self.sync_settings_hotkeys();
        }
        let widgets_changed = old.widgets != self.settings.widgets;
        self.pages.widgets = self.settings.widgets;
        self.pages.month = None;
        self.pages.water.1 = self.settings.water_cfg[0] as u8;
        let tabs_changed = old.tabs != self.settings.tabs;
        if tabs_changed {
            self.pages.tabs.flags = self.settings.tabs;
            if !self.settings.tabs[Tab::Media.index()] {
                self.audio.stop();
            }
        }
        if (old.city.as_str(), old.detect) != (self.settings.city.as_str(), self.settings.detect) {
            // A new place: forget the old one and what was fetched for it; the next pass fetches if due.
            self.weather.reset(self.settings.detect);
            (self.store.place, self.store.report) = (None, None);
        } else if old.units != self.settings.units {
            self.weather.rev = self.weather.rev.wrapping_add(1);
        }
        if !self.settings.agent_status {
            self.agents.clear();
        }
        let eyes_changed = old.eyes != self.settings.eyes;
        if eyes_changed {
            self.pages.eyes = self.settings.eyes.then(Eyes::default);
        }
        if old.privacy != self.settings.privacy
            && self
                .pages
                .set_privacy(self.settings.privacy, Instant::now())
        {
            self.request_notches(Repaint::Full, |_| true);
        }
        if old.dock_style != self.settings.dock_style
            || old.dock_buttons != self.settings.dock_buttons
            || old.dock_left != self.settings.dock_left
        {
            self.relayout_docks()?;
        }
        if old.desktop_only != self.settings.desktop_only
            || old.dock_badges != self.settings.dock_badges
        {
            self.dock_generation = None; // the dock is rebuilt on the next pass
        }
        let (create, destroy) = settings::surface_diff(&old, &self.settings, &devices);
        for kind in [SurfaceKind::Notch, SurfaceKind::Dock] {
            match (old.kind_on(kind), self.settings.kind_on(kind)) {
                (false, true) if !self.shown.contains(&kind) => self.shown.push(kind),
                (true, false) => self.shown.retain(|shown| *shown != kind),
                _ => {}
            }
        }
        self.surfaces
            .retain(|s| !destroy.contains(&(s.kind(), s.monitor.clone())));
        if restyle {
            self.palette =
                crate::theme::system_palette(self.settings.theme, self.effective_accent());
            self.menu = None;
            self.edges.release();
            // Colours are fixed when a surface is created, so a new theme rebuilds the survivors (as a text-scale
            // change does); the diff's creations below then find nothing left to skip and place themselves.
            self.surfaces.clear();
            for kind in self.shown.clone() {
                self.show(kind)?;
            }
        }
        for (kind, device) in create {
            if let Some(monitor) = self.monitors.iter().find(|m| m.device == device).cloned() {
                self.place(&monitor, kind)?;
            }
        }
        // A runtime `--hide-*` plus this change must not leave nothing shown: that would end the host.
        if self.shown.is_empty() {
            for kind in [SurfaceKind::Notch, SurfaceKind::Dock] {
                if self.settings.kind_on(kind) {
                    self.show(kind)?;
                }
            }
        }
        if tabs_changed || eyes_changed || widgets_changed {
            self.request_notches(Repaint::Full, |_| true);
        }
        let cap = self.settings.shelf_cap;
        self.change_shelf(|shelf| shelf.set_cap(cap));
        // Only an edited path repoints the report, so a `--diagnostics <path>` run keeps its own.
        if old.diagnostics != self.settings.diagnostics {
            catalog.set_diagnostics(
                self.settings
                    .diagnostics
                    .as_ref()
                    .map(std::path::PathBuf::from),
            );
        }
        self.store.settings = self.settings.clone();
        self.pending.mark(Instant::now());
        Ok(())
    }
}
