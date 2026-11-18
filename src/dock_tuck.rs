//! Dock and notch auto-hide: the surface slides away while the foreground window covers it, and comes back while the
//! pointer rests on a thin strip along the monitor's bottom edge (or stays on the dock). Event-driven: the
//! catalog's generation feeds `sync` (it hears the foreground window's own move and resize too); the only
//! waits are the strips' hover deadlines. A strip never keeps a click: it hides and hands the click on.
use crate::{
    dock::px, edge::Hover, layout::Bounds, native_window::NativeWindow, topology::Monitor,
    window_model::WindowKey,
};
use std::{
    mem::size_of,
    time::{Duration, Instant},
};
use windows::{
    Win32::UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
            MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SendInput, TME_LEAVE,
            TRACKMOUSEEVENT, TrackMouseEvent,
        },
        WindowsAndMessaging::{
            MSG, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
            WM_RBUTTONDOWN, WM_RBUTTONUP,
        },
    },
    core::w,
};

/// The strip's thickness in DIPs.
const STRIP: i32 = 2;
/// How many choices each delay has (Settings: short, medium, long); the first is today's.
pub const DELAYS: usize = 3;
/// The pointer resting on the strip reveals the dock after this (short: it should feel instant, yet a pointer
/// sweeping along the edge does not flash it).
const REVEAL: [Duration; DELAYS] = [
    Duration::from_millis(90),
    Duration::from_millis(300),
    Duration::from_millis(600),
];
/// After the pointer leaves a revealed dock it slides away this much later.
const LEAVE: [Duration; DELAYS] = [
    Duration::from_millis(400),
    Duration::from_millis(1000),
    Duration::from_millis(2500),
];
/// A strip that handed a click on stays out of the way this long.
const REST: Duration = Duration::from_millis(800);

pub fn overlaps(a: Bounds, b: Bounds) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

struct Strip {
    window: NativeWindow,
    monitor: String,
    at: Bounds,
    hover: Hover,
    tracking: bool,
    /// Hidden after a click until this time.
    rest: Option<Instant>,
}

/// The foreground window, its monitor and its frame; `None` when none counts (ROQA itself, the desktop).
pub type Frame = Option<(WindowKey, String, Bounds)>;

#[derive(Default)]
pub struct Tuck {
    /// The notch's: its strip runs along the top edge instead of the bottom.
    top: bool,
    /// Per monitor, the window that covers its dock.
    covers: Vec<(String, WindowKey)>,
    /// The monitor whose covered dock is shown again.
    peek: Option<String>,
    strips: Vec<Strip>,
    /// The chosen rows of `REVEAL` and `LEAVE`.
    reveal: usize,
    leave: usize,
}
impl Tuck {
    /// Picks the reveal and hide delays (settings rows; out of range means the longest).
    pub fn set_delays(&mut self, reveal: u8, leave: u8) {
        self.reveal = usize::from(reveal).min(DELAYS - 1);
        self.leave = usize::from(leave).min(DELAYS - 1);
    }
    /// The notch's: its strip runs along the top edge.
    pub fn top() -> Self {
        Self {
            top: true,
            ..Self::default()
        }
    }
    /// False while a covered dock is hidden on `device`.
    pub fn shown(&self, device: &str) -> bool {
        !self.tucked(device) || self.peek.as_deref() == Some(device)
    }
    fn tucked(&self, device: &str) -> bool {
        self.covers.iter().any(|(m, _)| m == device)
    }
    /// Returns true when a dock must show or hide. `look` reads a window's monitor and frame now (`None` once
    /// it is minimized, closed or hidden); `monitors` are the ones that may carry a strip.
    pub fn sync(
        &mut self,
        on: bool,
        frame: Frame,
        docks: &[(String, Bounds)],
        monitors: &[Monitor],
        look: impl Fn(WindowKey) -> Option<(String, Bounds)>,
    ) -> bool {
        let before = (self.covers.clone(), self.peek.clone());
        if !on {
            *self = Self {
                top: self.top,
                ..Self::default()
            };
        } else {
            let over = |device: &str, frame: Bounds| {
                docks
                    .iter()
                    .any(|(d, bounds)| d == device && overlaps(frame, *bounds))
            };
            // A window that moved away, closed or minimized no longer covers its dock.
            self.covers.retain(|(device, key)| {
                look(*key).is_some_and(|(at, frame)| &at == device && over(device, frame))
            });
            // The foreground window alone decides its own monitor; the others keep their covers.
            if let Some((key, at, frame)) = frame {
                self.covers.retain(|(device, k)| *k != key && *device != at);
                if over(&at, frame) {
                    self.covers.push((at, key));
                }
            }
            if self.peek.as_deref().is_some_and(|m| !self.tucked(m)) {
                self.peek = None;
            }
            self.strips.retain(|strip| {
                self.covers.iter().any(|(m, _)| *m == strip.monitor)
                    && monitors.iter().any(|m| m.device == strip.monitor)
            });
            for (device, bounds) in docks {
                let Some(monitor) = monitors.iter().find(|m| &m.device == device) else {
                    continue;
                };
                if !self.tucked(device) {
                    continue;
                }
                let at = Strip::place(monitor, *bounds, self.top);
                match self.strips.iter_mut().find(|s| &s.monitor == device) {
                    // A dock, DPI or taskbar change moved the edge: follow it.
                    Some(strip) if strip.at != at => {
                        if strip.window.resize(at).is_ok() {
                            strip.at = at;
                        }
                    }
                    Some(_) => {}
                    None => self.strips.extend(Strip::create(monitor, at)),
                }
            }
        }
        before != (self.covers.clone(), self.peek.clone())
    }
    /// Applies the strips' hover deadlines; `held` says the pointer is on that monitor's dock.
    pub fn tick(&mut self, now: Instant, held: impl Fn(&str) -> bool) -> bool {
        let mut changed = false;
        for strip in &mut self.strips {
            if strip.rest.is_some_and(|due| now >= due) {
                strip.rest = None;
                strip.window.show();
            }
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
                strip.hover.enter_after(now, REVEAL[self.reveal]);
            }
            WM_MOUSELEAVE => {
                strip.tracking = false;
                strip.hover.leave(now);
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN
            | WM_MBUTTONUP => strip.pass(message.message, now),
            // Anything else (a WM_PAINT above all) goes on to the window procedure: a paint taken here is never
            // validated and comes back at once, which kept the loop awake and starved the control events.
            _ => return false,
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
    /// Shows every tucked surface for `delay`, as a show command asks.
    pub fn reveal(&mut self, now: Instant, delay: Duration) {
        let devices: Vec<_> = self.strips.iter().map(|s| s.monitor.clone()).collect();
        for device in devices {
            self.alert(&device, now, delay);
        }
    }
    /// The pointer left the dock on `device`: a revealed dock leaves after `LEAVE`.
    pub fn release(&mut self, device: &str, now: Instant) {
        if self.peek.as_deref() == Some(device)
            && let Some(strip) = self.strips.iter_mut().find(|s| s.monitor == device)
        {
            strip.hover.release(now, LEAVE[self.leave]);
        }
    }
    /// The soonest deadline, if any.
    pub fn deadline(&self) -> Option<Instant> {
        self.strips
            .iter()
            .flat_map(|strip| [strip.hover.deadline(), strip.rest])
            .flatten()
            .min()
    }
}
impl Strip {
    /// Along the work area's bottom: on the screen edge when the taskbar shows (the work area ends above it, so the
    /// strip never takes the taskbar's clicks) and on the screen's last rows when it hides (the work area is the whole
    /// screen): a pointer pushed to the bottom stops on the last row, so a strip above it was never reached.
    fn place(monitor: &Monitor, dock: Bounds, top: bool) -> Bounds {
        let height = px(STRIP, monitor.dpi).max(2);
        Bounds {
            x: dock.x,
            y: if top {
                monitor.work.y
            } else {
                monitor.work.y + monitor.work.height - height
            },
            width: dock.width,
            height,
        }
    }
    fn create(monitor: &Monitor, at: Bounds) -> Option<Self> {
        let window = NativeWindow::create_hidden(at, w!("ROQA Dock Edge")).ok()?;
        window.show();
        Some(Self {
            window,
            monitor: monitor.device.clone(),
            at,
            hover: Hover::default(),
            tracking: false,
            rest: None,
        })
    }
    /// A click landed on the strip, which must not keep it (browser tabs, window edges): hide, then send the
    /// same button event to whatever is underneath. The strip comes back after `REST`.
    fn pass(&mut self, message: u32, now: Instant) {
        let flag: MOUSE_EVENT_FLAGS = match message {
            WM_LBUTTONDOWN => MOUSEEVENTF_LEFTDOWN,
            WM_LBUTTONUP => MOUSEEVENTF_LEFTUP,
            WM_RBUTTONDOWN => MOUSEEVENTF_RIGHTDOWN,
            WM_RBUTTONUP => MOUSEEVENTF_RIGHTUP,
            WM_MBUTTONDOWN => MOUSEEVENTF_MIDDLEDOWN,
            _ => MOUSEEVENTF_MIDDLEUP,
        };
        self.window.hide();
        self.rest = Some(now + REST);
        self.hover.leave(now);
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flag,
                    ..Default::default()
                },
            },
        };
        // SAFETY: one well-formed mouse event at the pointer's current place, sent with the real struct size.
        unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
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
    fn k(handle: usize) -> WindowKey {
        WindowKey { handle, process: 1 }
    }
    fn frame(handle: usize, device: &str, at: Bounds) -> Frame {
        Some((k(handle), device.to_owned(), at))
    }
    #[test]
    fn the_bottom_strip_reaches_the_last_row_when_the_taskbar_hides() {
        let monitor = |work: Bounds| Monitor {
            device: "A".to_owned(),
            work,
            rect: b(0, 0, 1920, 1080),
            dpi: 96,
            taskbar: 0,
        };
        let dock = b(400, 1000, 500, 60);
        // Auto-hidden taskbar: the work area is the whole screen and the strip ends on row 1079.
        let strip = Strip::place(&monitor(b(0, 0, 1920, 1080)), dock, false);
        assert_eq!((strip.y, strip.y + strip.height), (1078, 1080));
        // A shown taskbar ends the work area above it, and the strip sits on that edge.
        let strip = Strip::place(&monitor(b(0, 0, 1920, 1032)), dock, false);
        assert_eq!(strip.y + strip.height, 1032);
    }
    #[test]
    fn delays_start_at_todays_and_stay_in_range() {
        let mut tuck = Tuck::default();
        assert_eq!(
            (REVEAL[tuck.reveal], LEAVE[tuck.leave]),
            (Duration::from_millis(90), Duration::from_millis(400))
        );
        tuck.set_delays(9, 1);
        assert_eq!((tuck.reveal, tuck.leave), (DELAYS - 1, 1));
    }
    #[test]
    fn a_window_over_the_dock_tucks_it_and_leaving_shows_it() {
        let docks = [("A".to_owned(), b(400, 1000, 500, 60))];
        let mut tuck = Tuck::default();
        let gone = |_| None;
        // Not covering: nothing changes.
        let far = frame(1, "A", b(0, 0, 1920, 900));
        assert!(!tuck.sync(true, far, &docks, &[], gone) && tuck.shown("A"));
        let over = frame(1, "A", b(0, 0, 1920, 1080));
        assert!(tuck.sync(true, over, &docks, &[], gone) && !tuck.shown("A"));
        assert!(tuck.shown("B"));
        // ROQA holding the foreground: the window still covers it.
        let still = |_| Some(("A".to_owned(), b(0, 0, 1920, 1080)));
        assert!(!tuck.sync(true, None, &docks, &[], still) && !tuck.shown("A"));
        // The window closes or minimizes.
        assert!(tuck.sync(true, None, &docks, &[], gone) && tuck.shown("A"));
        assert!(tuck.deadline().is_none());
    }
    #[test]
    fn a_cover_on_another_monitor_survives_a_focus_change() {
        let docks = [
            ("A".to_owned(), b(0, 1000, 500, 60)),
            ("B".to_owned(), b(2000, 1000, 500, 60)),
        ];
        let mut tuck = Tuck::default();
        let screens = |key: WindowKey| {
            Some(if key == k(2) {
                ("B".to_owned(), b(1920, 0, 1920, 1080))
            } else {
                ("A".to_owned(), b(0, 0, 800, 600))
            })
        };
        tuck.sync(
            true,
            frame(2, "B", b(1920, 0, 1920, 1080)),
            &docks,
            &[],
            screens,
        );
        // Focus moves to a small window on A: B's dock stays hidden under its maximized window.
        tuck.sync(true, frame(1, "A", b(0, 0, 800, 600)), &docks, &[], screens);
        assert!(tuck.shown("A") && !tuck.shown("B"));
        // B's window is restored away from its dock while another window holds the focus.
        let moved = |_| Some(("B".to_owned(), b(1920, 0, 800, 600)));
        tuck.sync(true, frame(1, "A", b(0, 0, 800, 600)), &docks, &[], moved);
        assert!(tuck.shown("B"));
    }
    #[test]
    fn off_forgets_everything() {
        let docks = [("A".to_owned(), b(0, 0, 10, 10))];
        let mut tuck = Tuck::default();
        let over = frame(1, "A", b(0, 0, 20, 20));
        tuck.sync(true, over, &docks, &[], |_| None);
        assert!(tuck.sync(false, None, &docks, &[], |_| None) && tuck.shown("A"));
    }
}
