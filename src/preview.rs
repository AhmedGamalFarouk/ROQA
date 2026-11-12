//! Dock hover thumbnails (features B2): after the pointer rests on a slot, a live DWM thumbnail of the
//! group's most recent window floats above it. The DWM renders it (no capture, no copy); the popup exists
//! only while shown, and the wait is one host deadline.

use crate::{dock, layout::Bounds, native_window::NativeWindow, window_model::WindowKey};
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND, RECT},
        Graphics::Dwm::{
            DWM_THUMBNAIL_PROPERTIES, DWM_TNP_RECTDESTINATION, DWM_TNP_VISIBLE,
            DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail,
            DwmUpdateThumbnailProperties,
        },
        UI::WindowsAndMessaging::{GW_HWNDNEXT, GetTopWindow, GetWindow},
    },
    core::{Error, Result},
};

/// How long the pointer rests on a slot before its preview shows.
pub const DELAY: Duration = Duration::from_millis(400);
/// The largest preview, and the gap between it and the dock, in DIPs.
const MAX_DIPS: (i32, i32) = (240, 150);
const GAP_DIPS: i32 = 8;
/// Windows walked in z-order looking for the group's topmost; the rest fall back to the first.
const MAX_Z: usize = 1024;

/// A dock and one of its slots: what the pointer rests on.
pub type Spot = (usize, usize);

/// Hover-delay state: which spot the pointer is on, when its preview is due, whether one is shown.
#[derive(Default)]
pub struct Intent {
    spot: Option<Spot>,
    due: Option<Instant>,
    shown: bool,
}
impl Intent {
    /// Follows the pointer (`None` is off the dock's slots). A new spot restarts the delay; returns true
    /// when that ends a preview that was shown.
    pub fn point(&mut self, spot: Option<Spot>, now: Instant) -> bool {
        if spot == self.spot {
            return false;
        }
        let ended = self.shown;
        *self = Self {
            spot,
            due: spot.map(|_| now + DELAY),
            shown: false,
        };
        ended
    }
    /// The spot whose preview is due, once. Ask `dismiss` if showing it fails.
    pub fn take_due(&mut self, now: Instant) -> Option<Spot> {
        self.due.filter(|&due| now >= due)?;
        self.due = None;
        self.shown = true;
        self.spot
    }
    /// A click, a vanished window or a failed show: nothing is shown or pending until the pointer
    /// moves to another spot. Returns whether a preview was shown.
    pub fn dismiss(&mut self) -> bool {
        self.due = None;
        std::mem::take(&mut self.shown)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.due
    }
}

/// The window to preview: the group's foreground window, else its topmost in `z_order` (window handles,
/// top first: the most recently used), else its first.
pub fn most_recent(
    group: &[WindowKey],
    foreground: Option<WindowKey>,
    z_order: impl IntoIterator<Item = usize>,
) -> Option<WindowKey> {
    foreground
        .filter(|key| group.contains(key))
        .or_else(|| {
            z_order
                .into_iter()
                .find_map(|handle| group.iter().find(|key| key.handle == handle).copied())
        })
        .or_else(|| group.first().copied())
}

/// Top-level window handles, topmost first, lazily: the walk stops where the caller stops.
fn z_order() -> impl Iterator<Item = usize> {
    // SAFETY: scalar window-order queries on transient foreign HWNDs; a failure ends the walk.
    let top = unsafe { GetTopWindow(None) }.ok();
    std::iter::successors(top, |&window| {
        // SAFETY: as above.
        unsafe { GetWindow(window, GW_HWNDNEXT) }.ok()
    })
    .filter(|window| !window.0.is_null())
    .map(|window| window.0 as usize)
    .take(MAX_Z)
}

/// Where the preview goes (physical pixels): the source scaled down to fit the maximum box, centred
/// above `slot_x`, kept inside the work area, with a gap over the dock. `None` with no room above.
pub fn popup_bounds(
    source: (i32, i32),
    slot_x: i32,
    dock: Bounds,
    work: Bounds,
    dpi: u32,
) -> Option<Bounds> {
    let (w, h) = source;
    if w <= 0 || h <= 0 {
        return None;
    }
    let (max_w, max_h) = (dock::px(MAX_DIPS.0, dpi), dock::px(MAX_DIPS.1, dpi));
    let scale = (f64::from(max_w) / f64::from(w))
        .min(f64::from(max_h) / f64::from(h))
        .min(1.0);
    let width = ((f64::from(w) * scale).round() as i32).clamp(1, work.width.max(1));
    let height = ((f64::from(h) * scale).round() as i32).max(1);
    let y = dock.y - dock::px(GAP_DIPS, dpi) - height;
    let x = (slot_x - width / 2).clamp(work.x, (work.x + work.width - width).max(work.x));
    (y >= work.y).then_some(Bounds {
        x,
        y,
        width,
        height,
    })
}

/// A shown preview: the popup and its DWM thumbnail. Dropping it unregisters the thumbnail and destroys
/// the popup, so ending a preview is `Option::take`.
pub struct Popup {
    thumbnail: isize,
    /// The previewed window; the host ends the preview when the dock no longer holds it.
    pub target: WindowKey,
    window: NativeWindow,
}
impl Popup {
    /// Shows the preview of the most recent window of `windows` above the screen column `slot_x`. The popup is
    /// named "Preview, <app>" for UI Automation, and is never activated.
    pub fn show(
        windows: &[WindowKey],
        app: &str,
        slot_x: i32,
        dock: Bounds,
        work: Bounds,
        dpi: u32,
    ) -> Result<Self> {
        let target = most_recent(windows, dock::foreground_key(), z_order())
            .filter(|&key| dock::valid_target(key))
            .ok_or_else(|| Error::new(E_FAIL, "No window to preview."))?;
        let window = NativeWindow::create_preview(&format!("Preview, {app}"))?;
        // SAFETY: both HWNDs are live (ours, and the revalidated target); the DWM owns the thumbnail
        // until `Drop`.
        let thumbnail = unsafe { DwmRegisterThumbnail(window.0, HWND(target.handle as *mut _)) }?;
        let popup = Self {
            thumbnail,
            target,
            window,
        };
        // SAFETY: the id was just registered and is released only by `Drop`.
        let size = unsafe { DwmQueryThumbnailSourceSize(thumbnail) }?;
        let bounds = popup_bounds((size.cx, size.cy), slot_x, dock, work, dpi)
            .ok_or_else(|| Error::new(E_FAIL, "No room for a preview."))?;
        popup.window.resize(bounds)?;
        let properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE,
            rcDestination: RECT {
                left: 0,
                top: 0,
                right: bounds.width,
                bottom: bounds.height,
            },
            fVisible: true.into(),
            ..Default::default()
        };
        // SAFETY: `properties` outlives the call; the id is ours.
        unsafe { DwmUpdateThumbnailProperties(thumbnail, &properties) }?;
        popup.window.show();
        Ok(popup)
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        // SAFETY: releases the id registered in `show`; the window is destroyed after this, by its field.
        unsafe {
            let _ = DwmUnregisterThumbnail(self.thumbnail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(handle: usize) -> WindowKey {
        WindowKey {
            handle,
            process: handle as u32,
        }
    }

    #[test]
    fn the_preview_waits_400_ms_on_one_slot_and_restarts_on_another() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut intent = Intent::default();
        assert_eq!(
            (intent.deadline(), intent.take_due(at(10_000))),
            (None, None)
        );
        assert!(!intent.point(Some((1, 2)), t0));
        assert_eq!(intent.deadline(), Some(at(400)));
        // Early wakes show nothing; a move within the same slot does not restart the delay.
        assert_eq!(intent.take_due(at(399)), None);
        assert!(!intent.point(Some((1, 2)), at(300)));
        assert_eq!(intent.deadline(), Some(at(400)));
        assert_eq!(intent.take_due(at(400)), Some((1, 2)));
        // Shown once: no deadline, no second show.
        assert_eq!((intent.deadline(), intent.take_due(at(900))), (None, None));
        // Another slot ends the shown preview and starts a new delay from that moment.
        assert!(intent.point(Some((1, 3)), at(1000)));
        assert_eq!(intent.deadline(), Some(at(1400)));
        // Leaving before the delay ends nothing shown and cancels the deadline.
        assert!(!intent.point(None, at(1100)));
        assert_eq!((intent.deadline(), intent.take_due(at(9000))), (None, None));
        // Another dock with the same slot number is a different spot.
        assert!(!intent.point(Some((1, 3)), at(2000)));
        assert!(!intent.point(Some((2, 3)), at(2100)));
        assert_eq!(intent.deadline(), Some(at(2500)));
    }

    #[test]
    fn a_click_or_failure_stays_quiet_until_the_pointer_changes_slot() {
        let t0 = Instant::now();
        let mut intent = Intent::default();
        intent.point(Some((1, 1)), t0);
        assert_eq!(intent.take_due(t0 + DELAY), Some((1, 1)));
        assert!(intent.dismiss(), "a shown preview was dismissed");
        assert!(!intent.dismiss());
        // The pointer keeps moving inside the slot: still nothing.
        assert!(!intent.point(Some((1, 1)), t0 + DELAY * 2));
        assert_eq!(intent.deadline(), None);
        // Leaving and re-entering (or another slot) re-arms it.
        intent.point(None, t0 + DELAY * 3);
        intent.point(Some((1, 1)), t0 + DELAY * 3);
        assert_eq!(intent.deadline(), Some(t0 + DELAY * 4));
        // A dismissal before the show (a pending click) cancels the deadline and reports nothing shown.
        assert!(!intent.dismiss());
        assert_eq!(intent.deadline(), None);
    }

    #[test]
    fn the_most_recent_window_is_the_foreground_one_else_the_topmost() {
        let group = [key(10), key(20), key(30)];
        // The foreground member wins over z-order.
        assert_eq!(most_recent(&group, Some(key(30)), [20, 10]), Some(key(30)));
        // A foreground window of another group is ignored; the topmost member in z-order wins.
        assert_eq!(
            most_recent(&group, Some(key(99)), [5, 30, 10]),
            Some(key(30))
        );
        assert_eq!(most_recent(&group, None, [99, 20, 30]), Some(key(20)));
        // Not in the walk at all: the first, and nothing for an empty group.
        assert_eq!(most_recent(&group, None, [99]), Some(key(10)));
        assert_eq!(most_recent(&[], Some(key(10)), [10]), None);
        // The walk is lazy: it stops once a member is found.
        let mut seen = 0;
        let walk = [7, 20, 8, 9].into_iter().inspect(|_| seen += 1);
        assert_eq!(most_recent(&group, None, walk), Some(key(20)));
        assert_eq!(seen, 2);
    }

    #[test]
    fn the_popup_fits_the_box_stays_on_screen_and_clears_the_dock() {
        let work = Bounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let dock = Bounds {
            x: 700,
            y: 950,
            width: 400,
            height: 64,
        };
        // 96 dpi, 1920x1080 source: scaled to the 240 x 150 box by height (267 x 150 does not fit: width wins).
        let at = |source, slot_x| popup_bounds(source, slot_x, dock, work, 96);
        let wide = at((1920, 1080), 900).unwrap();
        assert_eq!((wide.width, wide.height), (240, 135));
        assert_eq!((wide.x, wide.y), (780, 950 - 8 - 135));
        // Never upscaled; a tall window is limited by height.
        assert_eq!(
            at((100, 80), 900).map(|b| (b.width, b.height)),
            Some((100, 80))
        );
        assert_eq!(
            at((300, 600), 900).map(|b| (b.width, b.height)),
            Some((75, 150))
        );
        // Clamped to the work area at both ends.
        assert_eq!(at((1920, 1080), 5).unwrap().x, 0);
        assert_eq!(at((1920, 1080), 1915).unwrap().x, 1920 - 240);
        // 150%: the box and the gap scale.
        let big = popup_bounds((1920, 1080), 900, dock, work, 144).unwrap();
        assert_eq!((big.width, big.height), (360, 203));
        assert_eq!(big.y, 950 - 12 - 203);
        // No source size, or no room above the dock, means no popup.
        assert_eq!(at((0, 10), 900), None);
        let low = Bounds { y: 100, ..dock };
        assert_eq!(popup_bounds((1920, 1080), 900, low, work, 96), None);
    }
}
