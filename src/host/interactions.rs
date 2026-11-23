//! Anchored interaction ownership, dismissal and surface visibility.
use super::*;

impl Desktop {
    /// Opens a shell interaction with an explicit origin before injecting its opening input.
    pub(super) fn open_flyout(&mut self, hwnd: HWND, kind: crate::anchored::Kind) {
        if self
            .native_popup
            .close_shown(crate::anchored::Owner(hwnd), kind)
        {
            return;
        }
        let Some(surface) = self
            .surfaces
            .iter()
            .find(|s| s.window.0 == hwnd && s.visible)
        else {
            return;
        };
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == surface.monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let button = if kind == crate::anchored::Kind::Tray {
            Button::Tray
        } else {
            Button::Start
        };
        let (left, width) = dock::span(
            surface.dock_buttons,
            surface.bounds.width,
            surface.dpi,
            Hit::Button(button),
        );
        let anchor = (
            surface.bounds.x + left + width / 2,
            surface.bounds.y - dock::px(8, surface.dpi),
        );
        if !self
            .native_popup
            .open(crate::anchored::Owner(hwnd), kind, anchor, work)
        {
            eprintln!("Shell popup: another interaction is open or Windows refused input");
        }
    }
    /// The same ownership snapshot governs tuck, notch dismissal and visibility.
    pub(super) fn interaction_owners(&self) -> [Option<HWND>; 5] {
        [
            self.menu.as_ref().map(|m| m.dock()),
            self.hover_popup.as_ref().map(|m| m.dock()),
            self.popup.as_ref().and_then(|_| {
                self.surfaces
                    .iter()
                    .find(|s| self.preview.owns(s.window.0))
                    .map(|s| s.window.0)
            }),
            self.native_popup.owner().map(|owner| owner.0),
            self.pending_popup.map(|(owner, _)| owner.0),
        ]
    }
    pub(super) fn refresh_interactions(&mut self) -> u32 {
        let now = Instant::now();
        let native = self.native_popup.refresh(now);
        if self.pending_popup.is_some_and(|(_, due)| now >= due) {
            self.pending_popup = None;
        }
        if self.native_popup.owner().is_some_and(|owner| {
            !self
                .surfaces
                .iter()
                .any(|s| s.window.0 == owner.0 && s.visible)
        }) {
            self.native_popup.cancel();
        }
        native
            .into_iter()
            .chain(self.pending_popup.map(|(_, due)| due))
            .min()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// Dock auto-hide: re-reads which window covers a dock when the catalog or the foreground window's frame
    /// changed, and applies the strips' hover deadlines.
    pub(super) fn refresh_tuck(&mut self, generation: u64) -> Result<()> {
        let now = Instant::now();
        let mut changed = false;
        if self.tuck_generation != Some(generation) {
            self.tuck_generation = Some(generation);
            let frame = dock::foreground_key()
                .filter(|key| dock::valid_target(*key))
                .and_then(|key| {
                    native_window::window_frame(key).map(|(device, frame)| (key, device, frame))
                });
            // No strip on a monitor a full-screen app owns: it would sit over the player's controls.
            let monitors: Vec<_> = self
                .monitors
                .iter()
                .filter(|m| {
                    !self
                        .fullscreen
                        .iter()
                        .any(|(_, device)| device == &m.device)
                })
                .cloned()
                .collect();
            let docks: Vec<_> = self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Dock)
                .map(|s| (s.monitor.clone(), s.bounds))
                .collect();
            self.tuck
                .set_delays(self.settings.reveal_delay, self.settings.hide_delay);
            self.tuck_notch
                .set_delays(self.settings.reveal_delay, self.settings.hide_delay);
            changed |= self.tuck.sync(
                self.settings.dock_hide,
                frame.clone(),
                &docks,
                &monitors,
                native_window::window_frame,
            );
            // The notch's footprint is its closed pill at the top centre, not the whole transparent window.
            let notches: Vec<_> = self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Notch)
                .map(|s| {
                    let width = dock::px(layout::NOTCH.width as i32, s.dpi).min(s.bounds.width);
                    let pill = Bounds {
                        x: s.bounds.x + (s.bounds.width - width) / 2,
                        width,
                        height: dock::px(layout::NOTCH.height as i32, s.dpi),
                        ..s.bounds
                    };
                    (s.monitor.clone(), pill)
                })
                .collect();
            changed |= self.tuck_notch.sync(
                self.settings.notch_hide,
                frame,
                &notches,
                &monitors,
                native_window::window_frame,
            );
        }
        let surfaces = &self.surfaces;
        let owners = self.interaction_owners();
        let popup = |s: &Surface| owners.contains(&Some(s.window.0));
        changed |= self.tuck_notch.tick(now, |device| {
            surfaces.iter().any(|s| {
                s.kind() == SurfaceKind::Notch
                    && s.monitor == device
                    && (s.hovered || s.motion.target() > 0.0 || popup(s))
            })
        });
        changed |= self.tuck.tick(now, |device| {
            surfaces.iter().any(|s| {
                s.kind() == SurfaceKind::Dock && s.monitor == device && (s.hovered || popup(s))
            })
        });
        if changed {
            self.apply_visibility()?;
        }
        Ok(())
    }
    pub(super) fn tuck_wait(&self) -> u32 {
        self.tuck
            .deadline()
            .into_iter()
            .chain(self.tuck_notch.deadline())
            .min()
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// returns the wait until the next of those (`INFINITE` with none, the common case).
    #[inline(never)]
    pub(super) fn refresh_close(&mut self) -> u32 {
        let now = Instant::now();
        let owners = self.interaction_owners();
        if self.peek.is_some_and(|due| now >= due) {
            self.peek = None;
            self.surfaces
                .iter_mut()
                .filter(|s| !owners.contains(&Some(s.window.0)))
                .for_each(Surface::close_alert);
        }
        let mut next = self.peek;
        for surface in &mut self.surfaces {
            let Role::Notch {
                linger, engaged, ..
            } = &mut surface.role
            else {
                continue;
            };
            let held = owners.contains(&Some(surface.window.0));
            match linger.refresh(now, held, !*engaged && surface.motion.target() > 0.0) {
                Some(due) if now >= due => {
                    linger.clear();
                    surface
                        .motion
                        .retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
                    surface.live = true;
                }
                Some(due) => next = Some(next.map_or(due, |soonest: Instant| soonest.min(due))),
                None => {}
            }
        }
        next.map_or(INFINITE, |due| milliseconds_until(due, now))
    }
}

/// A notch's leave grace can be suspended by its anchored interaction without a timer wake.
#[derive(Default)]
pub(super) struct Linger {
    due: Option<Instant>,
    held: bool,
}
impl Linger {
    pub(super) fn clear(&mut self) {
        self.due = None;
    }
    pub(super) fn leave(&mut self, now: Instant) {
        self.due = Some(now + motion::time(motion::LINGER_TIME));
    }
    fn refresh(&mut self, now: Instant, held: bool, unattended: bool) -> Option<Instant> {
        if held {
            self.clear();
        } else if self.held && unattended {
            self.leave(now);
        }
        self.held = held;
        self.due
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notch_menu_suspends_leave_and_resumes_only_after_its_owner_releases() {
        let now = Instant::now();
        let mut linger = Linger::default();
        linger.leave(now);
        assert_eq!(linger.refresh(now, true, true), None);
        let later = now + std::time::Duration::from_secs(60);
        assert_eq!(linger.refresh(later, true, true), None);
        let due = later + motion::time(motion::LINGER_TIME);
        assert_eq!(linger.refresh(later, false, true), Some(due));
        assert_eq!(linger.refresh(due, false, true), Some(due));
        linger.clear();
        assert_eq!(linger.refresh(due, false, true), None);
    }
    #[test]
    fn returning_to_the_notch_does_not_arm_a_leave_on_popup_dismissal() {
        let now = Instant::now();
        let mut linger = Linger::default();
        assert_eq!(linger.refresh(now, true, false), None);
        assert_eq!(linger.refresh(now, false, false), None);
        linger.leave(now);
        linger.clear();
        assert_eq!(linger.refresh(now, false, false), None);
    }
}
