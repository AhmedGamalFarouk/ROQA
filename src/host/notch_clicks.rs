//! Clicks on the notch: its buttons, widgets, month and the pressed controls.
//! One `impl Desktop` block moved out of `host.rs` unchanged; the methods are `pub(super)` so the host reaches them.

use super::*;

impl Desktop {
    /// Media control input on a notch: a click on a control. Returns whether `message` is consumed.
    pub(super) fn notch_input(&mut self, message: &MSG) -> bool {
        let Some(notch) = self
            .surfaces
            .iter()
            .position(|s| s.kind() == SurfaceKind::Notch && s.window.0 == message.hwnd)
        else {
            return false;
        };
        self.shot_input(notch, message);
        if message.message == WM_LBUTTONUP {
            self.pages.touch();
            let surface = &mut self.surfaces[notch];
            if surface.rail_fx.press.target() > 0.0 {
                RailFx::go(
                    &mut surface.rail_fx.press,
                    0.0,
                    motion::POP_TIME,
                    motion::POP,
                );
                surface.live = true;
            }
        }
        if message.message == WM_LBUTTONUP && self.surfaces[notch].grip.drag {
            self.surfaces[notch].end_grip();
            return true;
        }
        message.message == WM_LBUTTONUP && self.click_control(notch, message.lParam)
    }
    /// A click on a drawn media control sends its command, one on the progress bar seeks, and one
    /// on the volume bar sets the volume; the notch stays nonactivating.
    pub(super) fn click_control(&mut self, notch: usize, lparam: LPARAM) -> bool {
        // The mic dot, while it shows (in use, or muted), mutes or unmutes the mic, open or closed.
        let dots = self.pages.dots.alpha();
        if dots[0].max(dots[2]) > 0.0 {
            let surface = &self.surfaces[notch];
            let media = self.pages.track(&self.media).is_some();
            let progress = surface.motion.value(Instant::now());
            let dot = privacy::mic_dot(surface.bounds.width, surface.dpi, progress, media);
            if privacy::on_mic_dot(dot, surface.dpi, point(lparam)) {
                self.toggle_mic();
                return true;
            }
        }
        // Click mode: a click on the closed pill opens it.
        let closed = &mut self.surfaces[notch];
        if motion::click_to_open()
            && closed.motion.target() < 1.0
            && let Role::Notch { intent, .. } = &mut closed.role
        {
            open_notch(intent, &mut closed.motion);
            closed.live = true;
            return true;
        }
        let surface = &self.surfaces[notch];
        let (x, y) = point(lparam);
        let now = Instant::now();
        let progress = surface.motion.value(now);
        let (width, dpi) = (surface.bounds.width, surface.dpi);
        let live = self.pages.track(&self.media).is_some();
        // The tab handle first: it hangs below the panel.
        if let Some(rail) = surface.tab_rail(&self.pages, live, now)
            && let Some(position) = layout::rail_tab_at(width, dpi, progress, live, rail, (x, y))
        {
            if let Some(&tab) = self.pages.tabs.visible().as_slice().get(position) {
                self.select_tab(tab);
            }
            return true;
        }
        if let Some(letter) = self.power.eject()
            && layout::notice_button_at(width, dpi, progress, live, 0, (x, y))
        {
            if crate::system_os::eject_in_background(letter) {
                self.power.clear();
                self.power.announce(
                    format!("Ejecting {letter}:"),
                    Instant::now(),
                    DRIVE_NOTICE,
                    power::Extra::None,
                );
                self.request_notches(Repaint::Full, |_| true);
            } else {
                // The row keeps its Eject button so the click can be tried again; the notice waits behind it.
                self.show_notice("Still ejecting another drive".into(), Instant::now());
            }
            return true;
        }
        if let power::Extra::Ask(hook) = self.power.extra()
            && let Some(allow) =
                [false, true]
                    .into_iter()
                    .enumerate()
                    .find_map(|(from_right, allow)| {
                        layout::notice_button_at(width, dpi, progress, live, from_right, (x, y))
                            .then_some(allow)
                    })
        {
            if !self.power.settled(Instant::now()) {
                return true;
            }
            let text = match (crate::ipc::answer(hook, allow), allow) {
                (false, _) => "Too late. Answer in the terminal.",
                (true, true) => "Allowed",
                (true, false) => "Denied",
            };
            self.power.clear();
            self.power.announce(
                text.into(),
                Instant::now(),
                DEVICE_NOTICE,
                power::Extra::None,
            );
            self.request_notches(Repaint::Full, |_| true);
            return true;
        }
        if self.power.extra() == power::Extra::Snooze
            && layout::notice_button_at(width, dpi, progress, live, 0, (x, y))
        {
            self.snooze();
            return true;
        }
        if layout::gear_at(width, dpi, progress, live, x, y) {
            if let Err(error) = self.open_settings() {
                eprintln!("Settings: {error}");
            }
            return true;
        }
        let page = self.page();
        // Timer: the presets and Focus, or the controls of the countdown it shows. Weather and Prayer: the
        // "Set city" button opens Settings.
        let view = self.pages.timer_view(&self.timers, now);
        let controls = timer::controls(view.as_ref());
        let beside = view.as_ref().is_some_and(|view| !view.done);
        let buttons = match page {
            Tab::Timer => (controls.len(), beside),
            Tab::Weather if self.pages.weather.set_city => (1, false),
            Tab::Prayer if self.pages.prayer.set_city => (1, false),
            _ => (0, false),
        };
        let idle = view.as_ref().is_none_or(|view| view.done);
        let waiting = self.pages.reminders.waiting().len();
        if page == Tab::Timer
            && idle
            && let Some(index) =
                layout::remind_cancel_at(width, dpi, progress, live, waiting, (x, y))
        {
            self.pages.reminders.cancel(index);
            self.pages
                .reminders
                .write_to(&mut self.store, &mut self.pending, now);
            self.request_notches(Repaint::Full, |_| true);
            return true;
        }
        if page == Tab::Timer
            && idle
            && layout::remind_button_at(width, dpi, progress, live, (x, y))
        {
            self.remind_box.take();
            match self.dialog_work_area().and_then(RemindBox::create) {
                Ok(window) => self.remind_box = Some(window),
                Err(error) => eprintln!("Reminder box: {error}"),
            }
            return true;
        }
        if page == Tab::Timer && idle && layout::watch_button_at(width, dpi, progress, live, (x, y))
        {
            self.timer_press(timer::Ctl::Watch, now);
            return true;
        }
        if let Some(button) = layout::page_button_at(width, dpi, progress, live, buttons, (x, y)) {
            if page != Tab::Timer {
                let opened = self.open_settings().and_then(|()| {
                    if let Some(window) = &mut self.settings_window {
                        window.focus_city()?;
                    }
                    Ok(())
                });
                if let Err(error) = opened {
                    eprintln!("Settings: {error}");
                }
            } else {
                self.timer_press(controls[button], now);
            }
            return true;
        }
        if page == Tab::Shelf {
            if let Some(index) = layout::shelf_switch_at(width, dpi, progress, live, (x, y)) {
                self.set_copied(index == 1);
                return true;
            }
            if self.shelf.copied {
                return self.click_clips(notch, (x, y));
            }
            if self.shelf.removed.is_some()
                && layout::shelf_action_at(width, dpi, progress, live, (x, y), true)
            {
                self.undo_shelf_removal();
                return true;
            }
            if surface.hovered && layout::shelf_action_at(width, dpi, progress, live, (x, y), false)
            {
                self.start_file_picker();
                return true;
            }
            if let Some(index) = layout::shelf_remove_at(width, dpi, progress, false, x, y)
                && let Some(entry) = self
                    .shelf
                    .shelf
                    .absolute(index)
                    .and_then(|index| self.shelf.shelf.entries().get(index))
            {
                let path = entry.path.clone();
                self.remove_shelf_entry(&path);
                return true;
            }
            if surface.hovered
                && let Some(index) = layout::shelf_remove_at(width, dpi, progress, true, x, y)
                && let Some(entry) = self
                    .shelf
                    .shelf
                    .absolute(index)
                    .and_then(|index| self.shelf.shelf.entries().get(index))
                    .filter(|entry| {
                        entry.check != crate::shelf::Check::Missing && entry.shot.is_none()
                    })
            {
                if let Err(error) = shelf_os::share(surface.window.0, &entry.path) {
                    eprintln!("Share: {error}");
                    self.show_open_failed();
                }
                return true;
            }
            if let Some(index) = self.shelf_slot_at(notch, lparam) {
                // SAFETY: reads this thread's keyboard state as of the message being handled.
                let reveal = unsafe { GetKeyState(VK_CONTROL.0.into()) } < 0;
                self.open_shelf(index, reveal);
                return true;
            }
            return false;
        }
        if page == Tab::Widgets {
            return self.click_widgets(notch, (x, y));
        }
        if page != Tab::Media || !live {
            return false;
        }
        let players = self.pages.track(&self.media).map_or(0, |t| t.players.len());
        if let Some(player) = layout::player_at(width, dpi, progress, players, x, y) {
            self.media.choose(player);
        } else if let Some(mode) = layout::mode_at(width, dpi, progress, players, x, y) {
            self.media.mode(mode);
        } else if layout::media_cover_at(width, dpi, progress, x, y) {
            // Only a packaged player can be opened from its id; any other click on the cover does nothing.
            let target = self
                .media
                .packaged_player()
                .and_then(|id| crate::pins::launch_target(&Identity::Application(id)));
            let Some(target) = target else {
                return false;
            };
            self.run_dock_action(dock::Action::Launch(target));
        } else if let Some(control) = layout::media_control_at(width, dpi, progress, x, y) {
            self.press(control);
        } else if let Some(fraction) = layout::media_seek_at(width, dpi, progress, x, y) {
            self.media.seek(fraction);
        } else if let Some(volume) = layout::volume_at(width, dpi, progress, x, y) {
            self.audio.set(volume);
        } else {
            return false;
        }
        true
    }
    /// A click on the Widgets page: the calendar tile opens its month grid; there, an arrow steps the month and
    /// anywhere else goes back to the tiles.
    pub(super) fn click_widgets(&mut self, notch: usize, (x, y): (i32, i32)) -> bool {
        let surface = &self.surfaces[notch];
        let (width, dpi) = (surface.bounds.width, surface.dpi);
        let progress = surface.motion.value(Instant::now());
        let Some((year, month)) = self.pages.month else {
            let cell = layout::widget_at(width, dpi, progress, self.pages.widgets.tall(), x, y);
            if cell.and_then(|cell| self.pages.widgets.cells()[cell])
                != Some(crate::widgets::WidgetKind::WeekCalendar)
            {
                return false;
            }
            let (year, month, _) = prayer_os::local_now().ymd;
            self.show_month(notch, Some((year as u16, month as u16)), 1.0);
            return true;
        };
        match layout::month_arrow_at(width, dpi, progress, x, y) {
            None => self.show_month(notch, None, -1.0),
            Some(arrow) => {
                let index =
                    i32::from(year) * 12 + i32::from(month) - 1 + if arrow == 0 { -1 } else { 1 };
                // Years 1601 to 9999: before that the calendar helpers have nothing to say.
                if (1601 * 12..10_000 * 12).contains(&index) {
                    let next = ((index / 12) as u16, (index % 12) as u16 + 1);
                    self.show_month(notch, Some(next), if arrow == 0 { -1.0 } else { 1.0 });
                }
            }
        }
        true
    }
    /// Shows a month grid or the tiles again; the page eases in on the pop curve from the side `dir` points to.
    pub(super) fn show_month(&mut self, notch: usize, month: Option<(u16, u16)>, dir: f32) {
        self.pages.month = month;
        let surface = &mut self.surfaces[notch];
        surface.page_dir = dir;
        surface.page_fx = Tween::rest(0.0);
        surface.page_fx.retarget(1.0, motion::POP_TIME, motion::POP);
        surface.live = true;
        self.request_notches(Repaint::Full, |_| true);
    }
    /// Presses a shown media control or the mute toggle.
    pub(super) fn press(&mut self, control: usize) {
        if control != audio::MUTE {
            self.media.command(control);
        } else if self.pages.track(&self.media).is_some() {
            self.audio.toggle_mute();
        }
    }
}
