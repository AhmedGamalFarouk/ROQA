//! Clicks on the notch: its buttons, widgets, month and the pressed controls.

use super::*;
use windows::Win32::{Foundation::POINT, UI::Input::KeyboardAndMouse::GetCapture};

#[derive(Clone, Copy, PartialEq)]
struct PaintedFooter {
    layout: layout::NoticeFooter,
    extra: power::Extra,
    started: Option<Instant>,
}

type NoticePress = (Instant, power::Extra, bool);

impl PaintedFooter {
    fn identity(self) -> Option<NoticePress> {
        Some((self.started?, self.extra, self.layout.compact))
    }
}

#[derive(Default)]
pub(super) struct NoticeInput {
    presentation: Option<bool>,
    footer: Option<PaintedFooter>,
    // A press anywhere in the notice is consumed. Only a press on its action carries an identity.
    press: Option<Option<NoticePress>>,
    stationary: Option<(i32, i32)>,
    hover_dirty: bool,
}

impl NoticeInput {
    pub(super) fn painted(
        &mut self,
        presentation: Option<bool>,
        layout: Option<layout::NoticeFooter>,
        extra: power::Extra,
        started: Option<Instant>,
    ) {
        let footer = layout.map(|layout| PaintedFooter {
            layout,
            extra,
            started,
        });
        if footer != self.footer {
            if let Some(pressed) = &mut self.press
                && *pressed != footer.and_then(PaintedFooter::identity)
            {
                *pressed = None;
            }
            self.hover_dirty |= [self.footer, footer]
                .into_iter()
                .flatten()
                .any(|footer| footer.extra == power::Extra::Bell && footer.started.is_some());
            self.footer = footer;
        }
        if presentation == Some(true) && self.presentation != presentation {
            let mut at = POINT::default();
            // SAFETY: reads the pointer position into a valid output buffer.
            self.stationary = unsafe { GetCursorPos(&mut at) }.ok().map(|()| (at.x, at.y));
        }
        self.presentation = presentation;
    }

    pub(super) fn hide(&mut self) {
        self.painted(None, None, power::Extra::None, None);
        self.stationary = None;
        self.press = None;
    }
}

impl Surface {
    fn notice_limits_glance(&self) -> bool {
        self.motion.target() < 1.0 && self.notice_input.presentation.is_some()
    }

    pub(super) fn notice_hover(&mut self, over: bool, media: bool, message: &MSG) {
        if self.notice_input.stationary == Some((message.pt.x, message.pt.y)) {
            return;
        }
        self.notice_input.stationary = None;
        let over =
            over && (!self.notice_limits_glance() || self.notice_glance(media, message.lParam));
        self.set_notch_hover(over);
    }

    fn notice_glance(&self, media: bool, lparam: LPARAM) -> bool {
        layout::notice_glance_at(self.bounds.width, self.dpi, media, point(lparam))
    }
}

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
        if self.footer_input(notch, message) {
            return true;
        }
        self.shot_input(notch, message);
        self.notice_input(notch, message);
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
    fn footer_input(&mut self, notch: usize, message: &MSG) -> bool {
        let surface = &mut self.surfaces[notch];
        let input = &mut surface.notice_input;
        if matches!(message.message, WM_LBUTTONDOWN | WM_CAPTURECHANGED) {
            input.press = None;
        }
        let footer = input.footer;
        let at = point(message.lParam);
        let row = footer.is_some_and(|footer| footer.layout.hit(at, false));
        let current = footer.filter(|footer| {
            footer.started.is_some()
                && footer.started == self.power.started()
                && footer.extra == self.power.extra()
        });
        if matches!(message.message, WM_MOUSEMOVE | WM_MOUSELEAVE)
            && (footer.is_some() || self.power.extra().footer())
        {
            self.power.hover(
                message.message == WM_MOUSEMOVE
                    && row
                    && current.is_some_and(|footer| {
                        matches!(footer.extra, power::Extra::Bell | power::Extra::Snoozed)
                    }),
                Instant::now(),
            );
            return false;
        }
        let action = current
            .filter(|footer| match footer.extra {
                power::Extra::Bell | power::Extra::Snoozed => row,
                power::Extra::Snooze => footer.layout.hit(at, true),
                _ => false,
            })
            .and_then(PaintedFooter::identity);
        let media = self.pages.track(&self.media).is_some();
        let apron = surface.notice_limits_glance() && !surface.notice_glance(media, message.lParam);
        match message.message {
            WM_LBUTTONDOWN if (row || apron) && !surface.grip.drag => {
                surface.notice_input.press = Some(action);
                // SAFETY: captures to the owned notch during the press, including a release outside it.
                unsafe { SetCapture(surface.window.0) };
                true
            }
            WM_LBUTTONUP if surface.notice_input.press.is_some() => {
                let pressed = surface.notice_input.press.take().flatten();
                // SAFETY: reads the calling thread's capture HWND.
                let captured = unsafe { GetCapture() } == surface.window.0;
                if captured {
                    // SAFETY: this notch still owns the capture it took on press.
                    let _ = unsafe { ReleaseCapture() };
                }
                let now = Instant::now();
                if let Some((_, extra, _)) = action.filter(|_| captured && pressed == action) {
                    if extra == power::Extra::Snooze {
                        self.snooze();
                    } else if self.power.settled(now) && self.power.clear_plain() {
                        if self.power.promote(now) {
                            self.present_notice();
                        } else {
                            if !self.shelf.shots.showing() {
                                self.close_retired_notice(extra);
                            }
                            self.request_notches(Repaint::Full, |_| true);
                        }
                    }
                }
                true
            }
            WM_LBUTTONUP => (row || apron) && !surface.grip.drag,
            _ => false,
        }
    }

    pub(super) fn refresh_footer_hover(&mut self) -> u32 {
        let mut dirty = false;
        for surface in &mut self.surfaces {
            dirty |= std::mem::take(&mut surface.notice_input.hover_dirty);
        }
        if !dirty || self.power.extra() != power::Extra::Bell {
            return INFINITE;
        }
        let mut at = POINT::default();
        let mut on = false;
        // SAFETY: `at` is a valid buffer for the screen point.
        if unsafe { GetCursorPos(&mut at) }.is_ok() {
            // SAFETY: hit-tests the screen point without moving the pointer.
            let hwnd = unsafe { WindowFromPoint(at) };
            for surface in &mut self.surfaces {
                if surface.visible
                    && surface.window.0 == hwnd
                    && let Some(footer) = surface.notice_input.footer
                    && footer.started == self.power.started()
                {
                    let mut bounds = Default::default();
                    // SAFETY: reads the bounds of this owned HWND into a valid output buffer.
                    if unsafe { GetWindowRect(hwnd, &mut bounds) }.is_ok() {
                        on = footer
                            .layout
                            .hit((at.x - bounds.left, at.y - bounds.top), false);
                        if on {
                            let _ = surface.track_pointer();
                        }
                    }
                }
            }
        }
        let now = Instant::now();
        self.power.hover(on, now);
        self.power
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }

    /// A plain notice row: the pointer on it holds it, and a click closes it once it has settled, so a click meant
    /// for something under it is not taken. Never consumes the message.
    fn notice_input(&mut self, notch: usize, message: &MSG) {
        let surface = &self.surfaces[notch];
        if self.power.extra().footer() || surface.notice_input.footer.is_some() {
            return;
        }
        let now = Instant::now();
        let media = self.pages.track(&self.media).is_some();
        let on = self.power.text().is_some()
            && layout::notice_row_at(
                surface.bounds.width,
                surface.dpi,
                surface.motion.value(now),
                media,
                point(message.lParam),
            );
        match message.message {
            WM_MOUSEMOVE => self.power.hover(on, now),
            WM_MOUSELEAVE => self.power.hover(false, now),
            WM_LBUTTONUP if on && self.power.settled(now) && self.power.clear_plain() => {
                self.power.promote(now);
                if self.power.text().is_none() && !self.shelf.shots.showing() {
                    self.surfaces.iter_mut().for_each(Surface::close_alert);
                }
                self.request_notches(Repaint::Full, |_| true);
            }
            _ => {}
        }
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
            && let Role::Notch {
                intent, engaged, ..
            } = &mut closed.role
        {
            *engaged = true;
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
        // The plain Media page's glance line opens the tab it speaks for (hidden under a notice or a media status).
        if page == Tab::Media
            && self.pages.tabs.on(Tab::Media)
            && !live
            && (self.power.text().is_none() || self.power.extra().footer())
            && self.media.view().status().is_none()
            && layout::glance_at(width, dpi, progress, (x, y))
        {
            let glance = crate::tabs::glance(
                &self.pages.tabs,
                view.as_ref(),
                &self.pages.prayer,
                &self.pages.weather,
            );
            if let Some((_, tab)) = glance {
                self.select_tab(tab);
            }
            return true;
        }
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
    /// Presses a shown media control or the mute toggle.
    pub(super) fn press(&mut self, control: usize) {
        if control != audio::MUTE {
            self.media.command(control);
        } else if self.pages.track(&self.media).is_some() {
            self.audio.toggle_mute();
        }
    }
}
