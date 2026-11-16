//! Dock and notch auto-hide: the surface slides away while the foreground window covers it, and comes back while the
//! pointer rests on a thin strip along the monitor's bottom edge (or stays on the dock). Event-driven: the
//! catalog's generation feeds `sync`; the only waits are the strip's hover deadline and, while a window
//! covers the dock, one slow re-check (a maximize or a resize raises no event the catalog hears; a drag's end
//! does).
use crate::{
    dock::px, edge::Hover, layout::Bounds, native_window::NativeWindow, topology::Monitor,
};
use std::{
    mem::size_of,
    time::{Duration, Instant},
};
use windows::{
    Win32::UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        WindowsAndMessaging::{MSG, WM_MOUSEMOVE},
    },
    core::w,
};

/// Re-check of the foreground window's place while it covers the dock.
const POLL: Duration = Duration::from_secs(2);
/// The strip's thickness in DIPs.
const STRIP: i32 = 2;
/// The pointer resting on the strip reveals the dock after this (short: it should feel instant, yet a pointer
/// sweeping along the edge does not flash it).
const REVEAL: Duration = Duration::from_millis(90);
/// After the pointer leaves a revealed dock it slides away this much later.
const LEAVE: Duration = Duration::from_millis(400);

pub fn overlaps(a: Bounds, b: Bounds) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

struct Strip {
    window: NativeWindow,
    monitor: String,
    hover: Hover,
    tracking: bool,
}

/// What the foreground window says about the docks: `Some(Some(..))` a window's monitor and frame,
/// `Some(None)` none that counts, `None` leave the last answer (ROQA itself has the foreground).
pub type Frame = Option<Option<(String, Bounds)>>;

#[derive(Default)]
pub struct Tuck {
    /// The notch's: its strip runs along the top edge instead of the bottom.
    top: bool,
    /// Monitors whose dock a window covers.
    tucked: Vec<String>,
    /// The monitor whose covered dock is shown again.
    peek: Option<String>,
    strips: Vec<Strip>,
    poll: Option<Instant>,
}
impl Tuck {
    /// The notch's: its strip runs along the top edge.
    pub fn top() -> Self {
        Self {
            top: true,
            ..Self::default()
        }
    }
    /// False while a covered dock is hidden on `device`.
    pub fn shown(&self, device: &str) -> bool {
        !self.tucked.iter().any(|m| m == device) || self.peek.as_deref() == Some(device)
    }
    /// True when the re-check is due.
    pub fn polled(&self, now: Instant) -> bool {
        self.poll.is_some_and(|due| now >= due)
    }
    /// Returns true when a dock must show or hide.
    pub fn sync(
        &mut self,
        on: bool,
        frame: Frame,
        docks: &[(String, Bounds)],
        monitors: &[Monitor],
        now: Instant,
    ) -> bool {
        let before = (self.tucked.clone(), self.peek.clone());
        if !on {
            *self = Self {
                top: self.top,
                ..Self::default()
            };
        } else {
            if let Some(frame) = frame {
                self.tucked = docks
                    .iter()
                    .filter(|(device, bounds)| {
                        frame
                            .as_ref()
                            .is_some_and(|(at, frame)| at == device && overlaps(*frame, *bounds))
                    })
                    .map(|(device, _)| device.clone())
                    .collect();
            }
            self.poll = (!self.tucked.is_empty()).then_some(now + POLL);
            if self.peek.as_ref().is_some_and(|m| !self.tucked.contains(m)) {
                self.peek = None;
            }
            self.strips
                .retain(|strip| self.tucked.contains(&strip.monitor));
            for (device, bounds) in docks {
                if self.tucked.contains(device)
                    && !self.strips.iter().any(|s| &s.monitor == device)
                    && let Some(monitor) = monitors.iter().find(|m| &m.device == device)
                    && let Some(strip) = Strip::create(monitor, *bounds, self.top)
                {
                    self.strips.push(strip);
                }
            }
        }
        before != (self.tucked.clone(), self.peek.clone())
    }
    /// Applies the strips' hover deadlines; `held` says the pointer is on that monitor's dock.
    pub fn tick(&mut self, now: Instant, held: impl Fn(&str) -> bool) -> bool {
        let mut changed = false;
        for strip in &mut self.strips {
            match strip.hover.tick(now, held(&strip.monitor)) {
                Some(true) => {
                    self.peek = Some(strip.monitor.clone());
                    changed = true;
                }
                Some(false) if self.peek.as_deref() == Some(strip.monitor.as_str()) => {
                    self.peek = None;
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }
    /// A mouse message for a strip's window; true when it was one.
    pub fn message(&mut self, message: &MSG, now: Instant) -> bool {
        let Some(strip) = self.strips.iter_mut().find(|s| s.window.0 == message.hwnd) else {
            return false;
        };
        match message.message {
            WM_MOUSEMOVE => {
                strip.track();
                strip.hover.enter_after(now, REVEAL);
            }
            WM_MOUSELEAVE => {
                strip.tracking = false;
                strip.hover.leave(now);
            }
            _ => {}
        }
        true
    }
    /// Shows a tucked surface on `device` for `delay` (a notch alert); true when it was tucked.
    pub fn alert(&mut self, device: &str, now: Instant, delay: Duration) -> bool {
        let Some(strip) = self.strips.iter_mut().find(|s| s.monitor == device) else {
            return false;
        };
        strip.hover.show_until(now, delay);
        self.peek = Some(device.to_owned());
        true
    }
    /// The pointer left the dock on `device`: a revealed dock leaves after `LEAVE`.
    pub fn release(&mut self, device: &str, now: Instant) {
        if self.peek.as_deref() == Some(device)
            && let Some(strip) = self.strips.iter_mut().find(|s| s.monitor == device)
        {
            strip.hover.release(now, LEAVE);
        }
    }
    /// The soonest deadline, if any.
    pub fn deadline(&self) -> Option<Instant> {
        self.strips
            .iter()
            .filter_map(|strip| strip.hover.deadline())
            .chain(self.poll)
            .min()
    }
}
impl Strip {
    fn create(monitor: &Monitor, dock: Bounds, top: bool) -> Option<Self> {
        let height = px(STRIP, monitor.dpi).max(2);
        // Along the work area's bottom: on the screen edge when the taskbar hides, above it when it shows, so
        // the strip never takes the taskbar's edge clicks (Start, icons).
        let at = Bounds {
            x: dock.x,
            y: if top {
                monitor.work.y
            } else {
                monitor.work.y + monitor.work.height - height
            },
            width: dock.width,
            height,
        };
        let window = NativeWindow::create_hidden(at, w!("ROQA Dock Edge")).ok()?;
        window.show();
        Some(Self {
            window,
            monitor: monitor.device.clone(),
            hover: Hover::default(),
            tracking: false,
        })
    }
    fn track(&mut self) {
        if self.tracking {
            return;
        }
        let mut request = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.window.0,
            ..Default::default()
        };
        // SAFETY: requests a leave notification for this owned live HWND.
        self.tracking = unsafe { TrackMouseEvent(&mut request) }.is_ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b(x: i32, y: i32, width: i32, height: i32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }
    #[test]
    fn a_window_over_the_dock_tucks_it_and_leaving_shows_it() {
        let docks = [("A".to_owned(), b(400, 1000, 500, 60))];
        let now = Instant::now();
        let mut tuck = Tuck::default();
        // Not covering: nothing changes.
        let far = Some(Some(("A".to_owned(), b(0, 0, 1920, 900))));
        assert!(!tuck.sync(true, far, &docks, &[], now) && tuck.shown("A"));
        let over = Some(Some(("A".to_owned(), b(0, 0, 1920, 1080))));
        assert!(tuck.sync(true, over, &docks, &[], now) && !tuck.shown("A"));
        assert!(tuck.shown("B"));
        // ROQA holding the foreground keeps the answer.
        assert!(!tuck.sync(true, None, &docks, &[], now) && !tuck.shown("A"));
        assert!(tuck.sync(true, Some(None), &docks, &[], now) && tuck.shown("A"));
        assert!(tuck.deadline().is_none());
    }
    #[test]
    fn off_forgets_everything() {
        let docks = [("A".to_owned(), b(0, 0, 10, 10))];
        let mut tuck = Tuck::default();
        let over = Some(Some(("A".to_owned(), b(0, 0, 20, 20))));
        tuck.sync(true, over, &docks, &[], Instant::now());
        assert!(tuck.sync(false, None, &docks, &[], Instant::now()) && tuck.shown("A"));
    }
}
