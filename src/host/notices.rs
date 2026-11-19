//! The notice row: prayer, agent, progress, reminder, key and notification pills, and Snooze.
//! One `impl Desktop` block moved out of `host.rs` unchanged; the methods are `pub(super)` so the host reaches them.

use super::*;

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
        self.show_notice(prayer_tab::notice(event), now);
    }
    /// Expands every notch with `text` for `PRAYER_NOTICE`; on a monitor a full-screen app covers it appears
    /// over the app (topmost, never activated) and hides again once retracted.
    pub(super) fn show_notice(&mut self, text: String, now: Instant) {
        self.show_notice_with(text, now, power::Extra::None);
    }
    /// `show_notice`, with a button or bar on its row.
    pub(super) fn show_notice_with(&mut self, text: String, now: Instant, extra: power::Extra) {
        self.power.announce(text, now, PRAYER_NOTICE, extra);
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
    /// A permission question from a hook shows on the notice row with Allow and Deny, a short peek that stays under
    /// a full-screen app (the agent's own prompt is still there). It waits as long as the hook does.
    pub(super) fn show_ask(&mut self, id: u32, text: String, now: Instant) {
        self.power.announce(
            text,
            now,
            Duration::from_secs(agent::ASK_SECS),
            power::Extra::Ask(id),
        );
        self.open_notches();
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
    /// Reminders: the wait is the soonest one's time, none with nothing set. A due one chimes (unless reminder
    /// sounds are off) and shows with a Snooze button; one that came due while ROQA was off shows at the start.
    #[inline(never)]
    pub(super) fn refresh_reminders(&mut self) -> u32 {
        let wall = timer::wall_now();
        if let Some(label) = self.pages.reminders.take_due(wall) {
            let now = Instant::now();
            self.pages
                .reminders
                .write_to(&mut self.store, &mut self.pending, now);
            if self.settings.reminder_plays() {
                chime();
            }
            self.show_notice_with(remind::shown(&label), now, power::Extra::Snooze);
            self.last_reminder = label;
        }
        self.pages
            .reminders
            .wait_secs(wall)
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
                (0, minutes) => format!("I'll remind you in {minutes} min"),
                (hours, 0) => format!("I'll remind you in {hours} h"),
                (hours, minutes) => format!("I'll remind you in {hours} h {minutes} min"),
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
        self.pill(toast::take(), DEVICE_NOTICE);
    }
    pub(super) fn pill(&mut self, text: Option<String>, hold: Duration) {
        let Some(text) = text else {
            return;
        };
        let now = Instant::now();
        self.power.announce(text, now, hold, power::Extra::None);
        self.open_notches();
        self.request_notches(Repaint::Full, |_| true);
    }
    /// The Snooze button on a due reminder: the same words again in ten minutes.
    pub(super) fn snooze(&mut self) {
        let now = Instant::now();
        let label = std::mem::take(&mut self.last_reminder);
        self.pages
            .reminders
            .add(timer::wall_now(), remind::SNOOZE, label);
        self.pages
            .reminders
            .write_to(&mut self.store, &mut self.pending, now);
        self.power.clear();
        self.power.announce(
            "Snoozed for 10 min".into(),
            now,
            DEVICE_NOTICE,
            power::Extra::None,
        );
        self.request_notches(Repaint::Full, |_| true);
    }
}
