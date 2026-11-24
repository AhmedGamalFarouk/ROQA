//! The notice row: prayer, water, agent, progress, reminder, key and notification pills, and Snooze, with the
//! `refresh_*` passes that produce them. One `impl Desktop` block; the methods are `pub(super)` so the host reaches them.

use super::*;

/// A notification over this many characters stays `NOTIFICATION_READ`, long enough to read.
const NOTIFICATION_LONG: usize = 40;
const NOTIFICATION_READ: Duration = Duration::from_secs(7);

impl Desktop {
    /// A reminder is due: the sound plays unless Reminder sounds is off. The notch expands with the prayer's name; on a monitor a
    /// full-screen app covers it appears over the app (topmost, never activated) and hides again once retracted.
    pub(super) fn fire_prayer(&mut self, event: prayer_tab::Event, now: Instant) {
        if self.settings.reminder_plays() {
            self.play_prayer_sound(now);
        }
        // Pausing is one request at the prayer time (not the heads-up), never undone; nothing happens when
        // nothing plays or the Media tab is off (no session is read then).
        let playing = self
            .pages
            .track(&self.media)
            .is_some_and(|track| track.playing);
        if prayer_tab::pauses_media(self.settings.prayer_pause, event, playing) {
            self.media.command(crate::media::PLAY_PAUSE);
        }
        self.prayer.adhan_until = Some(now + PRAYER_NOTICE);
        // A prayer takes the row from a plain notice or a screenshot pill, which return after it.
        let admitted = self
            .power
            .announce_prayer(prayer_tab::notice(event), now, PRAYER_NOTICE);
        self.reveal_notice(admitted, power::Extra::None);
    }
    /// Expands every notch with `text` for `PRAYER_NOTICE`; on a monitor a full-screen app covers it appears
    /// over the app (topmost, never activated) and hides again once retracted.
    pub(super) fn show_notice(&mut self, text: String, now: Instant) {
        self.show_notice_with(text, now, power::Extra::None);
    }
    /// `show_notice`, with a button or bar on its row.
    pub(super) fn show_notice_with(&mut self, text: String, now: Instant, extra: power::Extra) {
        let admitted = self.power.announce(text, now, PRAYER_NOTICE, extra);
        self.reveal_notice(admitted, extra);
    }
    /// Shows the notch for a notice that took the row; one that waits shows later (`refresh_power`).
    fn reveal_notice(&mut self, admitted: bool, extra: power::Extra) {
        if !admitted {
            return;
        }
        if extra.footer() || self.power.extra().footer() {
            self.present_notice();
            return;
        }
        for surface in &mut self.surfaces {
            if surface.kind() == SurfaceKind::Notch && !surface.visible {
                // Painted before it shows, as when a full-screen app ends.
                let shown = surface.paint_notch(
                    (&self.clock, &self.date),
                    &self.media,
                    &self.audio,
                    &self.timers,
                    &self.power,
                    &self.shelf,
                    &self.pages,
                );
                if shown.is_ok() {
                    surface.set_visible(true);
                    surface.overlay = true;
                }
            }
        }
        self.open_notches();
        self.request_notches(Repaint::Full, |_| true);
    }
    /// Said once at start when the settings could not be read: this run keeps no change.
    pub(super) fn startup_notice(&mut self) {
        if self.store_path.is_none() {
            self.show_notice(
                "Couldn't read settings. Changes won't save.".into(),
                Instant::now(),
            );
        }
    }
    /// A command's failure that is not a lost graphics device (that one schedules a rebuild) is logged and the host goes on.
    pub(super) fn recover(&mut self, result: Result<()>, what: &str) {
        if let Err(error) = self.survive(result, ()) {
            eprintln!("{what}: {error}");
        }
    }
    /// Device arrivals, the identities their drives were read with, and drive removals since the last pass.
    pub(super) fn device_wakes(&mut self) {
        let bits = native_window::device_read();
        if bits != 0 {
            self.show_device(bits);
        }
        for volume in crate::system_os::arrived() {
            self.drive_identified(volume);
        }
        // A removed drive's Eject is stale: its letter may soon name another drive.
        let gone = native_window::removed_read();
        if let Some(volume) = self
            .arrival
            .take_if(|volume| drive_gone(volume.letter, gone))
            && self.power.eject() == Some(volume.letter)
            && self.power.clear()
        {
            if self.power.promote(Instant::now()) {
                self.present_notice();
            } else {
                if !self.shelf.shots.showing() {
                    self.close_retired_notice(power::Extra::Eject(volume.letter));
                }
                self.request_notches(Repaint::Full, |_| true);
            }
        }
    }
    /// A drive, camera or speaker connected (`native_window::device_read` bits): a short peek, unlike a prayer alert it
    /// does not show over a full-screen app. A drive's notice waits for its identity, read on a worker.
    fn show_device(&mut self, bits: u32) {
        let drive = bits & native_window::DRIVE_BIT != 0;
        if drive
            && native_window::drive_read().is_some_and(crate::system_os::identify_in_background)
        {
            return;
        }
        let text = if drive {
            "Drive connected."
        } else if bits & native_window::CAMERA_BIT != 0 {
            "Camera connected."
        } else {
            "Speaker connected."
        };
        if self.power.announce(
            text.into(),
            Instant::now(),
            DEVICE_NOTICE,
            power::Extra::None,
        ) {
            self.open_notches();
            self.request_notches(Repaint::Full, |_| true);
        }
    }
    /// An arrived drive's identity came back (`system_os::identify_in_background`): it carries Eject only when it
    /// is one USB volume alone on its device, as in Places, and the click ejects by that identity, never the letter.
    fn drive_identified(&mut self, volume: Option<crate::system_os::Volume>) {
        let (hold, extra) = volume
            .as_ref()
            .map_or((DEVICE_NOTICE, power::Extra::None), |volume| {
                (DRIVE_NOTICE, power::Extra::Eject(volume.letter))
            });
        if self
            .power
            .announce("Drive connected.".into(), Instant::now(), hold, extra)
        {
            self.arrival = volume;
            self.open_notches();
            self.request_notches(Repaint::Full, |_| true);
        }
    }
    /// Reads what coding agents reported since the last time: the latest shows on the pill for the notice's
    /// length. With the switch off they are read and dropped.
    pub(super) fn agent_reports(&mut self) {
        let now = Instant::now();
        for (name, report) in agent::take() {
            match report {
                agent::Report::Percent(percent) => self.show_progress(&name, percent, now),
                agent::Report::Ask(id, text) => self.show_ask(id, text, now),
                agent::Report::State(_) if !self.settings.agent_status => {}
                agent::Report::State(state) => {
                    if let Some(text) = self.agents.set(&name, state, now) {
                        self.show_notice(text, now);
                    }
                }
            }
        }
    }
    /// A launch failed or was refused (a busy network worker): said the same way wherever something is opened.
    pub(super) fn show_open_failed(&mut self) {
        self.show_notice("Windows couldn't open that.".into(), Instant::now());
    }
    /// A permission question from a hook shows on the notice row with Allow and Deny, a short peek that stays under
    /// a full-screen app (the agent's own prompt is still there). It waits as long as the hook does. When the row is
    /// busy or no notch can show, the hook is told at once (`ipc::pass`) so its agent asks in the terminal.
    pub(super) fn show_ask(&mut self, id: u32, text: String, now: Instant) {
        // The hook measured at 96 DPI; a notch at another scale may cut the question, and Allow must not show then.
        let whole = self
            .surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Notch)
            .all(|s| crate::graphics::ask_fits_at(&text, s.dpi, 0.0));
        if !whole
            || !self.power.announce(
                text,
                now,
                Duration::from_secs(agent::ASK_SECS),
                power::Extra::Ask(id),
            )
        {
            crate::ipc::pass(id);
            return;
        }
        self.open_notches();
        if !self
            .surfaces
            .iter()
            .any(|s| s.kind() == SurfaceKind::Notch && s.visible)
        {
            self.power.clear();
            crate::ipc::pass(id);
        }
        self.request_notches(Repaint::Full, |_| true);
    }
    /// A script's progress shows on the notice row with an accent bar; like a device alert it is a short peek
    /// that stays under a full-screen app. Each report renews the hold; "done" shows briefly and hides.
    pub(super) fn show_progress(&mut self, name: &str, percent: u8, now: Instant) {
        let hold = if percent >= 100 {
            PROGRESS_DONE
        } else {
            PROGRESS_HOLD
        };
        self.power.announce(
            agent::progress_text(name, percent),
            now,
            hold,
            power::Extra::Progress(percent),
        );
        self.open_notches();
        self.request_notches(Repaint::Full, |_| true);
    }
    /// Reminders: the wait is the soonest one's time, none with nothing set. A due one is taken only when the row can
    /// show it (it chimes unless reminder sounds are off, and shows with a Snooze button); otherwise it stays in the
    /// list until the row frees, and one that came due while ROQA was off shows at the start.
    #[inline(never)]
    pub(super) fn refresh_reminders(&mut self) -> u32 {
        let wall = timer::wall_now();
        let now = Instant::now();
        if self.pages.reminders.wait_secs(wall) == Some(0)
            && self.power.admits(now, PRAYER_NOTICE, power::Extra::Snooze)
            && let Some(label) = self.pages.reminders.take_due(wall)
        {
            self.pages
                .reminders
                .write_to(&mut self.store, &mut self.pending, now);
            if self.settings.reminder_plays() {
                chime();
            }
            let copy = if label.is_empty() {
                String::new()
            } else {
                remind::shown(&label)
            };
            self.show_notice_with(copy, now, power::Extra::Snooze);
            self.last_reminder = label;
        }
        // A zero wait is a reminder the row cannot show yet: the row's own deadline and the click wake the host.
        self.pages
            .reminders
            .wait_secs(wall)
            .filter(|secs| *secs > 0)
            .map_or(INFINITE, |secs| {
                u32::try_from(secs.saturating_mul(1000))
                    .map_or(INFINITE - 1, |ms| ms.min(INFINITE - 1))
            })
    }
    /// What the reminder box made: set it and say when it will come.
    pub(super) fn remind_message(&mut self, message: &MSG) -> Result<bool> {
        let Some(window) = &mut self.remind_box else {
            return Ok(false);
        };
        if !window.message(message)? {
            return Ok(false);
        }
        let made = window.take();
        if window.closed() {
            self.remind_box = None;
        }
        if let Some((delay, label)) = made {
            self.set_reminder(delay, label);
        }
        Ok(true)
    }
    pub(super) fn set_reminder(&mut self, delay: u64, label: String) {
        let now = Instant::now();
        let text = if self.pages.reminders.add(timer::wall_now(), delay, label) {
            self.pages
                .reminders
                .write_to(&mut self.store, &mut self.pending, now);
            let (hours, minutes) = (delay / 3600, delay % 3600 / 60);
            match (hours, minutes) {
                (0, minutes) => format!("I'll remind you in {minutes} min."),
                (hours, 0) => format!("I'll remind you in {hours} h."),
                (hours, minutes) => format!("I'll remind you in {hours} h {minutes} min."),
            }
        } else {
            format!("{} reminders are already waiting.", remind::MAX)
        };
        self.power
            .announce(text, now, DEVICE_NOTICE, power::Extra::None);
        self.open_notches();
        self.request_notches(Repaint::Full, |_| true);
    }
    /// A keyboard pill (the language, or Caps Lock's state) on the notice row for a moment; it never replaces a question
    /// or another notice that carries a button.
    pub(super) fn key_pill(&mut self, text: Option<String>) {
        self.pill(text, KEY_NOTICE);
    }
    /// A Windows notification's line (`toast.rs`) on the notice row, a short peek like a device alert.
    pub(super) fn notification_peek(&mut self) {
        let hold = |text: &str| {
            if text.chars().count() > NOTIFICATION_LONG {
                NOTIFICATION_READ
            } else {
                DEVICE_NOTICE
            }
        };
        // A script's `roqa.exe notify` line shows the same way.
        for text in [toast::take(), native_window::notify_take()]
            .into_iter()
            .flatten()
        {
            self.pill_with(Some(text.clone()), hold(&text), power::Extra::Bell);
        }
    }
    /// On the minute clock: a short pill when the fetched forecast turns wet next hour, once per such hour.
    pub(super) fn rain_soon(&mut self, hour: u8, minute: u8) {
        let next = (hour + 1) % 24;
        if !self.pages.tabs.on(Tab::Weather) || self.rain_told == Some(next) {
            return;
        }
        let text = self
            .weather
            .report
            .as_ref()
            .and_then(|report| weather::rain_soon(report, hour, minute));
        if text.is_some() {
            self.rain_told = Some(next);
            self.pill(text, DEVICE_NOTICE);
        }
    }
    /// Ctrl+Alt+M or a click on the mic dot: mutes or unmutes the recording device and says which.
    pub(super) fn toggle_mic(&mut self) {
        let muted = self.mic.toggle();
        self.key_pill(Some(crate::mic::toggle_text(muted).into()));
    }
    pub(super) fn pill(&mut self, text: Option<String>, hold: Duration) {
        self.pill_with(text, hold, power::Extra::None);
    }
    fn pill_with(&mut self, text: Option<String>, hold: Duration, extra: power::Extra) {
        let Some(text) = text else {
            return;
        };
        let now = Instant::now();
        if self.power.announce(text, now, hold, extra) {
            self.present_notice();
        }
    }
    pub(super) fn present_notice(&mut self) {
        if self.power.extra().footer() {
            self.peek_tucked_notches();
        } else {
            self.open_notches();
        }
        self.request_notches(Repaint::Full, |_| true);
    }
    /// The Snooze button on a due reminder: the same words again in ten minutes, or, with the list full, the
    /// reminder and its button stay and the row says why.
    pub(super) fn snooze(&mut self) {
        let now = Instant::now();
        self.power.clear();
        if self.pages.reminders.waiting().len() >= remind::MAX {
            self.power.announce(
                format!("{} reminders are already waiting.", remind::MAX),
                now,
                DEVICE_NOTICE,
                power::Extra::Snooze,
            );
        } else {
            let label = std::mem::take(&mut self.last_reminder);
            self.pages
                .reminders
                .add(timer::wall_now(), remind::SNOOZE, label);
            self.pages
                .reminders
                .write_to(&mut self.store, &mut self.pending, now);
            self.power.announce(
                "Snoozed for 10 min.".into(),
                now,
                DEVICE_NOTICE,
                power::Extra::Snoozed,
            );
        }
        self.request_notches(Repaint::Full, |_| true);
    }
    /// Prayer (features P1): builds the page while it shows, then runs the reminders: one pass computes the day's
    /// events (when the date, offset, place, method or heads-up changed) and returns the wait until the next
    /// reminder or a second after midnight. A due reminder expands the notch and starts the sound. Also ends a
    /// finished sound. Off, or without a place, it has no deadline.
    #[inline(never)]
    pub(super) fn refresh_prayer(&mut self) -> u32 {
        let now = Instant::now();
        if self.prayer.sound_end.is_some_and(|end| now >= end) {
            self.prayer.sound_end = None;
            self.prayer.player.stop();
        }
        let sound_wait = self
            .prayer
            .sound_end
            .map_or(INFINITE, |end| milliseconds_until(end, now));
        if !self.pages.tabs.on(Tab::Prayer) && !self.visible_dock_widgets()[1] {
            return sound_wait;
        }
        // The plain Media page's glance (`tabs::glance`) reads the page's next prayer too.
        let shown =
            matches!(self.page(), Tab::Prayer | Tab::Media) || self.visible_dock_widgets()[1];
        let remind = self.settings.prayer_remind;
        if !shown && !remind {
            return sound_wait;
        }
        let local = prayer_os::local_now();
        let method = self.settings.prayer_method;
        let shift = self.settings.shift6();
        let params = self
            .weather
            .place
            .as_ref()
            .filter(|_| weather::configured(self.settings.detect, &self.settings.city))
            .map(|place| crate::prayer::Params {
                lat: f64::from(place.lat) / 1e4,
                lon: f64::from(place.lon) / 1e4,
                utc_offset_min: local.offset_min,
                method,
                hanafi: false,
                shift,
            });
        if shown {
            let key = (self.clock.clone(), self.weather.rev, method, shift);
            if self.pages.prayer_key.as_ref() != Some(&key) {
                let configured = weather::configured(self.settings.detect, &self.settings.city);
                let (message, set_city) = match self.weather.last_fail {
                    _ if !configured => ("Set a city for prayer times", true),
                    Some(_) if self.weather.fetching() => ("Retrying location lookup…", false),
                    Some(weather::Fail::Unlocated) => ("Location not found", true),
                    Some(_) => ("Couldn't load your location", false),
                    None => ("Finding your location…", false),
                };
                self.pages.prayer = prayer_tab::view(
                    params.as_ref(),
                    message,
                    local,
                    prayer_os::hijri(local.ymd).unwrap_or_default(),
                    &prayer_os::time_label,
                );
                self.pages.prayer.set_city = params.is_none() && set_city;
                self.pages.prayer_key = Some(key);
                self.request_notches(Repaint::Full, |s| motion::reveal(s.motion.value(now)) > 0.0);
            }
        }
        let Some(params) = params.filter(|_| remind) else {
            self.prayer.key = None;
            return sound_wait;
        };
        let lead = self.settings.prayer_lead;
        let key = (
            prayer_tab::Local { secs: 0, ..local },
            (params.lat * 1e4) as i32,
            (params.lon * 1e4) as i32,
            method,
            lead,
            shift,
        );
        if self.prayer.key != Some(key) {
            self.prayer.key = Some(key);
            self.prayer.events = crate::prayer::timeline(&params, local.ymd)
                .map(|line| prayer_tab::events(&line, lead))
                .unwrap_or_default();
        }
        let (due, wait) = prayer_tab::step(
            &self.prayer.events,
            (local.ymd, local.secs),
            &mut self.prayer.last,
        );
        if let Some(event) = due {
            self.fire_prayer(event, now);
        }
        sound_wait.min(wait.saturating_mul(1000))
    }
    /// Water reminders (F5): one deadline to the next slot, none while the switch is off. A due slot shows
    /// the progress notice (silent once the goal is met) and the next deadline is computed from the clock.
    #[inline(never)]
    pub(super) fn refresh_water(&mut self) -> u32 {
        if !self.settings.water {
            self.water_due = None;
            return INFINITE;
        }
        let now = Instant::now();
        let cfg = self.settings.water_cfg;
        if self.water_due.is_some_and(|due| now >= due) {
            self.water_due = None;
            let drank = self.water_today();
            if drank < cfg[0] as u8 {
                if self.settings.reminder_plays() {
                    chime();
                }
                self.show_notice(
                    format!("Time for a glass of water. {}", water::text(drank, &cfg)),
                    now,
                );
            }
        }
        // One second past the clock, so a wake a hair before the slot cannot fire it twice.
        let due = *self.water_due.get_or_insert_with(|| {
            let secs = prayer_os::local_now().secs as u32 + 1;
            now + Duration::from_secs(water::wait_secs(secs, &cfg).into())
        });
        milliseconds_until(due, now)
    }
    /// Ends a finished power notice and returns the wait until its deadline; with none showing it does
    /// nothing and returns `INFINITE`.
    #[inline(never)]
    pub(super) fn refresh_power(&mut self) -> u32 {
        let now = Instant::now();
        let retiring = self.power.extra();
        let ended = self.power.over(now);
        // The screenshot pill takes a freed row first; the notice that waited for the row shows after it.
        let shot = self.sync_shots(now);
        let promoted = self.power.promote(now);
        if ended || shot || promoted {
            if self.power.text().is_none() && !self.shelf.shots.showing() {
                self.close_retired_notice(retiring);
            } else if shot || promoted {
                self.present_notice();
            }
            self.request_notches(Repaint::Full, |_| true);
        }
        let shot_due = self.shelf.shots.deadline();
        self.power
            .deadline()
            .into_iter()
            .chain(shot_due)
            .min()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    pub(super) fn close_retired_notice(&mut self, extra: power::Extra) {
        let owners = self.interaction_owners();
        for surface in &mut self.surfaces {
            if !extra.footer()
                || (!owners.contains(&Some(surface.window.0))
                    && matches!(&surface.role, Role::Notch { linger, .. } if !linger.waiting()))
            {
                surface.close_alert();
            }
        }
    }
    pub(super) fn water_today(&self) -> u8 {
        water::today(
            self.store.water,
            prayer_tab::day_number(prayer_os::local_now().ymd),
        )
    }
    /// "I drank a glass": today's count goes up and is stored.
    pub(super) fn water_drink(&mut self) {
        let day = prayer_tab::day_number(prayer_os::local_now().ymd);
        self.store.water = (day, self.water_today().saturating_add(1));
        self.pending.mark(Instant::now());
    }
    pub(super) fn sync_water(&self) {
        if let Some(window) = &self.settings_window {
            window.set_water(&water::text(self.water_today(), &self.settings.water_cfg));
        }
    }
    /// Plays the chosen sound (none when silent or no default exists); false when nothing played.
    pub(super) fn play_prayer_sound(&mut self, now: Instant) -> bool {
        let found = prayer_os::discover();
        let Some(mut path) = prayer_tab::resolve(&self.settings.prayer_sound, &found) else {
            return false;
        };
        // A file that is gone or on a share plays the default instead; the stored setting is left as it is.
        if prayer_tab::missing(path) {
            let Some(default) = prayer_tab::default_sound(&found) else {
                return false;
            };
            path = default;
        }
        let length = self.prayer.player.play(path);
        self.prayer.sound_end = length.map(|length| now + length);
        length.is_some()
    }
}

/// Whether drive `letter` is among the removed drives in `mask` (bit 0 is A).
fn drive_gone(letter: char, mask: u32) -> bool {
    (letter as u32)
        .checked_sub('A' as u32)
        .is_some_and(|bit| bit < 26 && mask & 1 << bit != 0)
}

#[cfg(test)]
mod tests {
    use super::drive_gone;

    #[test]
    fn only_the_removed_letter_takes_the_arrival_eject_away() {
        assert!(drive_gone('E', 1 << 4));
        assert!(!drive_gone('E', 1 << 5) && !drive_gone('E', 0));
        assert!(!drive_gone('?', u32::MAX), "not a drive letter");
    }
}
