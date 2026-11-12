use crate::{
    audio::{self, Audio},
    catalog_timing::milliseconds_until,
    clock,
    dock::{self, DockItem},
    dock_uia::{self, Accessibility},
    graphics::{COVER_DIPS, Canvas, DockScene, DockSlot, Graphics, NotchScene, ShelfSlot},
    icons::{self, IconPixels, IconService},
    ipc::{Command, Controls},
    layout::{self, Bounds, SurfaceKind, placement},
    media::{self, Media, Progress},
    motion::{self, Tween},
    native_window::{self, NativeWindow, WindowClass},
    notch_uia,
    picker::Picker,
    pins::Pins,
    power::{self, Power},
    preview,
    settings::{self, Settings},
    settings_window::SettingsWindow,
    shelf::{self, Check, Shelf},
    shelf_os::{self, DropEvent, Drops},
    store::{self, Flush, Pending, Store},
    text_settings::TextSettings,
    theme::Palette,
    timer::{self, Timers},
    topology::{self, Monitor},
    window_catalog::Catalog,
    window_model::{Identity, WindowKey, WindowModel},
};
use std::{
    cell::Cell,
    mem::size_of,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{E_FAIL, HANDLE, HWND, LPARAM, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WPARAM},
    System::Diagnostics::Debug::MessageBeep,
    System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    System::Ole::{OleInitialize, OleUninitialize},
    System::Threading::INFINITE,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{
            GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_O, VK_RETURN, VK_RIGHT,
            VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
        },
        WindowsAndMessaging::*,
    },
};
use windows::core::{Error, Result, s, w};

#[derive(PartialEq, Eq)]
pub enum Probe {
    None,
    #[cfg(debug_assertions)]
    Initialization,
    #[cfg(debug_assertions)]
    Callback,
    /// Debug builds only: one synthetic device-removed failure right after startup.
    #[cfg(debug_assertions)]
    DeviceLoss,
}

struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: main thread owns all COM objects and drops them before Apartment. OLE (an STA, as
        // COM was) is what `RegisterDragDrop` for the shelf needs.
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances successful initialization on this same thread.
        unsafe {
            OleUninitialize();
        }
    }
}

struct NotchShadow {
    painted: Cell<Option<(f32, bool)>>,
    canvas: Canvas,
    window: NativeWindow,
}
impl NotchShadow {
    fn create(graphics: &Graphics, bounds: Bounds, palette: Palette) -> Result<Self> {
        let window = NativeWindow::create_shadow(bounds)?;
        let canvas = Canvas::create(graphics, window.0, bounds, palette)?;
        Ok(Self {
            canvas,
            window,
            painted: Cell::new(None),
        })
    }
    fn paint(&self, scene: &NotchScene, notch: HWND) -> Result<()> {
        let frame = if motion::smooth(scene.progress) == 0.0 {
            (0.0, false)
        } else {
            (scene.progress, scene.track.is_some())
        };
        if self.painted.get() == Some(frame) {
            return Ok(());
        }
        // Follow the body even when a test moves its HWND; keep the shadow immediately behind it.
        // SAFETY: both HWNDs are owned on this UI thread; no activation or focus change.
        unsafe {
            let mut bounds = Default::default();
            GetWindowRect(notch, &mut bounds)?;
            SetWindowPos(
                self.window.0,
                Some(notch),
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                SWP_NOACTIVATE,
            )?;
        }
        self.canvas.paint_notch_shadow(scene)?;
        self.painted.set(Some(frame));
        Ok(())
    }
}

/// What one pass must repaint on a notch. Requests only grow within a pass, so a notch is painted
/// once, by the strongest: a full paint covers a bars-only one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Repaint {
    #[default]
    Nothing,
    /// Only the visualizer slot, for a settled open panel whose bar heights changed.
    Meter,
    Full,
}

struct Surface {
    // Declared first: UIA must release the dock before its HWND is destroyed.
    role: Role,
    canvas: Canvas,
    window: NativeWindow,
    /// Owning monitor's device name; surfaces are keyed by it plus their kind.
    monitor: String,
    /// Monitor DPI; the notch's also includes the Windows text-size factor.
    dpi: u32,
    bounds: Bounds,
    hovered: bool,
    /// False while a full-screen app covers this monitor; hidden surfaces keep their state.
    visible: bool,
    /// Frames are painted only while set; `animate` clears it when motion settles.
    live: bool,
    /// Notch: 0 collapsed to 1 expanded. Dock: pointer-inside progress of the bar fill.
    motion: Tween,
    /// Notch: the progress row last drawn, if any.
    played: Option<Progress>,
    /// Notch: the visualizer bar heights last drawn.
    bars: [i32; audio::BARS],
    /// Notch: what this pass has asked to repaint (`Desktop::paint_requested`).
    repaint: Repaint,
}
impl Drop for Surface {
    fn drop(&mut self) {
        self.release_focus();
        if let Role::Notch {
            accessible, uia, ..
        } = &mut self.role
        {
            shelf_os::revoke(self.window.0);
            accessible.disconnect();
            notch_uia::invalidate(uia);
        }
    }
}
/// State only one surface kind needs.
enum Role {
    /// UIA root and shared state, the media control with keyboard focus, pointer inside the hot
    /// band (not its shadow), the instant hover intent opens the notch, and the window region last
    /// applied (it follows the drawn shape frame by frame).
    Notch {
        accessible: Accessibility,
        uia: notch_uia::Shared,
        focus: Option<Focus>,
        shadow: NotchShadow,
        engaged: bool,
        intent: Option<Instant>,
        area: Vec<Bounds>,
    },
    /// Hovered slot, per-slot hover progress and this dock's UIA geometry/names.
    Dock {
        _accessible: Accessibility,
        hover: Option<usize>,
        slots: Vec<Tween>,
        uia: dock_uia::Shared,
    },
}
impl Surface {
    fn create(
        graphics: &Graphics,
        kind: SurfaceKind,
        monitor: &Monitor,
        dpi: u32,
        slots: usize,
        drops: &Drops,
        palette: Palette,
    ) -> Result<Self> {
        let bounds = placement(monitor.work, dpi, kind, slots);
        let shadow = if kind == SurfaceKind::Notch {
            Some(NotchShadow::create(graphics, bounds, palette)?)
        } else {
            None
        };
        let window = NativeWindow::create(bounds, kind)?;
        if kind == SurfaceKind::Notch
            && let Err(error) = shelf_os::register(window.0, drops)
        {
            eprintln!("Shelf: file drops unavailable: {error}");
        }
        let role = match shadow {
            None => {
                let uia = dock_uia::Shared::default();
                Role::Dock {
                    _accessible: Accessibility::dock(window.0, &uia)?,
                    hover: None,
                    slots: vec![Tween::rest(0.0); slots],
                    uia,
                }
            }
            // The first paint, which always precedes showing the window, applies the input region
            // of the plain or media pill and publishes the UIA state.
            Some(shadow) => {
                let uia = notch_uia::Shared::default();
                Role::Notch {
                    accessible: notch_uia::accessibility(window.0, &uia)?,
                    uia,
                    focus: None,
                    shadow,
                    engaged: false,
                    intent: None,
                    area: Vec::new(),
                }
            }
        };
        let canvas = Canvas::create(graphics, window.0, bounds, palette)?;
        // Created hidden; the owner shows it after the first paint.
        Ok(Self {
            role,
            canvas,
            window,
            monitor: monitor.device.clone(),
            dpi,
            bounds,
            hovered: false,
            visible: false,
            live: false,
            motion: Tween::rest(0.0),
            played: None,
            bars: [0; audio::BARS],
            repaint: Repaint::Nothing,
        })
    }
    fn request(&mut self, repaint: Repaint) {
        self.repaint = self.repaint.max(repaint);
    }
    /// A visible notch that is open or opening; only these read and draw playback progress.
    fn expanded(&self) -> bool {
        self.visible && self.kind() == SurfaceKind::Notch && self.motion.target() >= 1.0
    }
    fn kind(&self) -> SurfaceKind {
        match self.role {
            Role::Notch { .. } => SurfaceKind::Notch,
            Role::Dock { .. } => SurfaceKind::Dock,
        }
    }
    /// Hides or restores the surface (and the notch's shadow) without releasing anything. Hidden
    /// surfaces are not painted, so callers paint before restoring one.
    fn set_visible(&mut self, visible: bool) {
        if visible == self.visible {
            return;
        }
        if !visible {
            self.release_focus();
        }
        self.visible = visible;
        let shadow = match &self.role {
            Role::Notch { shadow, .. } => Some(&shadow.window),
            Role::Dock { .. } => None,
        };
        for window in shadow.into_iter().chain([&self.window]) {
            if visible {
                window.show()
            } else {
                window.hide()
            }
        }
        if !visible {
            // A hidden window gets no leave notification and needs no frames: settle at rest now.
            (self.hovered, self.live, self.motion) = (false, false, Tween::rest(0.0));
            match &mut self.role {
                Role::Notch {
                    engaged,
                    intent,
                    focus,
                    uia,
                    ..
                } => {
                    // A full-screen app took the foreground, and with it the keyboard focus.
                    (*engaged, *intent, *focus) = (false, None, None);
                    if let Ok(mut state) = uia.lock() {
                        (state.focus, state.open) = (None, false);
                    }
                }
                Role::Dock { hover, slots, .. } => {
                    *hover = None;
                    slots.fill(Tween::rest(0.0));
                }
            }
        }
    }
    /// Ends a timer completion's expansion: closes a notch nobody is pointing at or typing in. A pointer
    /// or keyboard focus keeps it open, and the usual leave or blur closes it.
    fn close_alert(&mut self) {
        if self.expanded()
            && matches!(
                self.role,
                Role::Notch {
                    engaged: false,
                    focus: None,
                    ..
                }
            )
        {
            self.motion.retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
            self.live = true;
        }
    }
    fn leave(&mut self) {
        if self.hovered {
            self.hovered = false;
            if self.kind() == SurfaceKind::Dock {
                self.hover_dock(None, false);
            } else {
                self.set_notch_hover(false);
            }
        }
    }
    /// Starts leave tracking on first entry; returns true only for that transition.
    fn track_pointer(&mut self) -> Result<bool> {
        if self.hovered {
            return Ok(false);
        }
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.window.0,
            ..Default::default()
        };
        // SAFETY: tracking only requests leave notification for this owned live HWND.
        unsafe {
            TrackMouseEvent(&mut tracking)?;
        }
        self.hovered = true;
        Ok(true)
    }
    /// Shares what the notch shows with UI Automation: the current session's controls, the focused
    /// one, and whether the panel is open or opening, so clients see the target state before the
    /// motion ends.
    fn publish_notch(
        &self,
        media: &Media,
        audio: &Audio,
        timer: Option<&timer::View>,
        notice: Option<&str>,
        shelf: &ShelfView,
    ) {
        let mut announce = false;
        if let Role::Notch { uia, focus, .. } = &self.role
            && let Ok(mut state) = uia.lock()
        {
            state.dpi = self.dpi;
            let timer_name = timer.map_or("", |view| view.name.as_str());
            // A notice newly shown is announced once, after the lock is released.
            let notice = notice.unwrap_or("");
            if state.notice != notice {
                announce = !notice.is_empty();
                state.notice.clear();
                state.notice.push_str(notice);
            }
            if state.timer != timer_name {
                state.timer.clear();
                state.timer.push_str(timer_name);
            }
            state.version = uia_version(media, &shelf.shelf);
            if state.published_shelf != Some(shelf.shelf.revision()) {
                state.published_shelf = Some(shelf.shelf.revision());
                state.shelf = (0..shelf.shelf.slot_count())
                    .filter_map(|index| shelf.shelf.slot_label(index))
                    .collect();
            }
            let media_stamp = (media.uia_revision(), audio.level());
            if state.published_media != Some(media_stamp) {
                state.published_media = Some(media_stamp);
                state.controls = media
                    .view()
                    .shown()
                    .map(|track| {
                        let mut controls = notch_uia::controls(track);
                        controls.push(notch_uia::mute(audio.level()));
                        controls
                    })
                    .unwrap_or_default();
                state.status = media.view().status();
            }
            state.focus = focus.as_ref().map(|focus| focus.control);
            state.open = self.motion.target() >= 1.0;
            if state.published_position != self.played {
                state.published_position = self.played;
                state.position = self
                    .played
                    .map(|played| played.times().join(" of "))
                    .unwrap_or_default();
            }
        }
        if announce && let Role::Notch { accessible, .. } = &self.role {
            notch_uia::announce(accessible);
        }
    }
    /// Draws one frame, publishing it to UI Automation first.
    fn paint_notch(
        &mut self,
        (time, date): (&str, &str),
        media: &Media,
        audio: &Audio,
        timers: &Timers,
        power: &Power,
        shelf: &ShelfView,
    ) -> Result<()> {
        let track = media.view().shown();
        let progress = self.motion.value(Instant::now());
        let timer = timers.view(Instant::now());
        let notice = power.text();
        // Only a revealed panel row reads the time; a collapsed notch draws no progress.
        self.played = track
            .filter(|_| motion::reveal(progress) > 0.0)
            .and_then(|track| {
                track.progress(media::now_ticks(), layout::media_progress(self.dpi).width)
            });
        // Paused bars lie flat at once; the meter stops on the next pass.
        let peaks = match track {
            Some(track) if track.playing => audio.peaks(),
            _ => [0.0; audio::BARS],
        };
        self.bars = layout::meter_heights(peaks, self.dpi);
        self.publish_notch(media, audio, timer.as_ref(), notice, shelf);
        let Role::Notch {
            shadow,
            area,
            focus,
            ..
        } = &mut self.role
        else {
            return Ok(());
        };
        let focus = focus.as_ref().map(|focus| focus.control);
        // Input and drawing use the same frame progress and shape, without a speculative growth lead.
        let rows = layout::notch_rows(self.bounds, self.dpi, progress, track.is_some());
        if *area != rows {
            self.window.set_notch_region(&rows)?;
            *area = rows;
        }
        let mut slots: Vec<ShelfSlot> = shelf
            .shelf
            .visible()
            .iter()
            .enumerate()
            .map(|(index, entry)| ShelfSlot {
                icon: shelf.icons.get(index).and_then(|icon| icon.as_deref()),
                missing: entry.check == Check::Missing,
                more: false,
                add: false,
            })
            .collect();
        if shelf.shelf.overflow() || shelf.shelf.is_empty() {
            slots.push(ShelfSlot {
                icon: None,
                missing: false,
                more: shelf.shelf.overflow(),
                add: shelf.shelf.is_empty(),
            });
        }
        let scene = NotchScene {
            time,
            date,
            track,
            art: media.art(),
            progress,
            dpi: self.dpi,
            focus,
            played: self.played,
            volume: audio.level(),
            bars: self.bars,
            // A notice owns the panel text slots while it shows.
            timer: timer.as_ref().filter(|_| notice.is_none()),
            notice,
            shelf: &slots,
        };
        shadow.paint(&scene, self.window.0)?;
        self.canvas.paint_notch(&scene, media.view().status())
    }
    /// Repaints only the visualizer slot with the new bar heights; no UIA state changes with them.
    fn paint_meter(&mut self, peaks: [f32; audio::BARS]) -> Result<()> {
        self.bars = layout::meter_heights(peaks, self.dpi);
        self.canvas.paint_meter(self.dpi, self.bars)
    }
    /// Pointer entered or left the notch. Entering swells the pill at once and opens it only after
    /// `INTENT_TIME` (a pointer passing along the screen edge never pops the panel open); leaving
    /// closes from wherever the motion is. The HWND itself never moves.
    fn set_notch_hover(&mut self, over: bool) {
        let Role::Notch {
            engaged,
            intent,
            focus,
            ..
        } = &mut self.role
        else {
            return;
        };
        if over == *engaged {
            return;
        }
        *engaged = over;
        if !over {
            *intent = None;
            // Keyboard focus holds the panel open; `blur` closes it.
            if focus.is_none() {
                self.motion.retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
            }
        } else if self.motion.value(Instant::now()) > NOTCH_SWELL {
            // Re-entered while still closing: reopen directly.
            open_notch(intent, &mut self.motion);
        } else {
            *intent = Some(Instant::now() + motion::INTENT_TIME);
            self.motion
                .retarget(NOTCH_SWELL, motion::INTENT_TIME, motion::EASE_OUT);
        }
        self.live = true;
    }
    /// Moves keyboard focus to media `control`: the panel opens, always animated, and stays open
    /// until `blur`; the next frame draws the focus ring. `return_to` is kept from the first entry.
    fn focus_control(&mut self, control: usize, return_to: Option<WindowKey>) {
        let Role::Notch {
            focus, intent, uia, ..
        } = &mut self.role
        else {
            return;
        };
        let return_to = focus.as_ref().map_or(return_to, |focus| focus.return_to);
        *focus = Some(Focus { control, return_to });
        open_notch(intent, &mut self.motion);
        self.live = true;
        // Published now, not at the next frame, so the focus event reads the new state.
        if let Ok(mut state) = uia.lock() {
            (state.focus, state.open) = (Some(control), true);
        }
    }
    /// Ends keyboard focus, closing the panel unless the pointer still holds it.
    fn blur(&mut self) -> Option<Focus> {
        let Role::Notch {
            focus,
            engaged,
            uia,
            ..
        } = &mut self.role
        else {
            return None;
        };
        let ended = focus.take()?;
        if !*engaged {
            self.motion.retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
        }
        self.live = true;
        if let Ok(mut state) = uia.lock() {
            (state.focus, state.open) = (None, self.motion.target() >= 1.0);
        }
        Some(ended)
    }
    fn focused(&self) -> Option<&Focus> {
        match &self.role {
            Role::Notch { focus, .. } => focus.as_ref(),
            Role::Dock { .. } => None,
        }
    }
    /// Restore only while this HWND still owns foreground; another app's focus is never stolen.
    fn release_focus(&mut self) {
        if let Some(ended) = self.blur() {
            dock::restore_foreground(self.window.0, ended.return_to);
        }
    }
    fn dismiss_focus(&mut self) {
        if let Role::Notch {
            engaged, intent, ..
        } = &mut self.role
        {
            (*engaged, *intent) = (false, None);
        }
        self.release_focus();
    }
    /// Tells UIA clients where keyboard focus now is.
    fn announce_focus(&self) {
        if let Role::Notch {
            accessible,
            uia,
            focus: Some(focus),
            ..
        } = &self.role
        {
            notch_uia::focus_changed(accessible, uia, focus.control);
        }
    }
    /// Highlights the hovered slot (or none) and the bar fill while the pointer is inside.
    fn hover_dock(&mut self, slot: Option<usize>, inside: bool) {
        let Role::Dock { hover, slots, .. } = &mut self.role else {
            return;
        };
        let ease = |tween: &mut Tween, on: bool| {
            let (target, duration) = if on {
                (1.0, motion::HOVER_IN_TIME)
            } else {
                (0.0, motion::HOVER_OUT_TIME)
            };
            tween.retarget(target, duration, motion::EASE_OUT);
        };
        ease(&mut self.motion, inside);
        for (index, tween) in slots.iter_mut().enumerate() {
            ease(tween, slot == Some(index));
        }
        *hover = slot;
        self.live = true;
    }
    /// Shares slot names/geometry with this dock's UIA providers; a layout change retires every older element.
    fn publish_accessibility(&self, names: &[String], layout_changed: bool) {
        if let Role::Dock { uia, .. } = &self.role
            && let Ok(mut state) = uia.lock()
        {
            state.dpi = self.dpi;
            state.names = names.to_vec();
            if layout_changed {
                state.version = state.version.wrapping_add(1);
            }
        }
    }
    fn paint_dock(&self, items: &[DockItem], icons: &[Option<Arc<IconPixels>>]) -> Result<()> {
        let Role::Dock { slots: tweens, .. } = &self.role else {
            return Ok(());
        };
        let now = Instant::now();
        let hover = |slot: usize| tweens.get(slot).map_or(0.0, |t| t.value(now));
        let mut slots = vec![DockSlot {
            icon: None,
            active: false,
            picker: true,
            hover: hover(0),
        }];
        slots.extend(
            items
                .iter()
                .zip(icons)
                .enumerate()
                .map(|(index, (item, icon))| DockSlot {
                    icon: icon.as_deref(),
                    active: item.active,
                    picker: false,
                    hover: hover(index + 1),
                }),
        );
        self.canvas.paint_dock(&DockScene {
            slots,
            inside: self.motion.value(now),
            dpi: self.dpi,
        })
    }
    fn dock_slot(&self, lparam: LPARAM, slots: usize) -> Option<usize> {
        let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
        dock::slot_at(x, self.bounds.width, self.dpi, slots)
    }
}

/// Hover-intent swell: the pill grows ~11 x 3 DIP (the media pill ~7 x 6) to acknowledge the
/// pointer before opening.
const NOTCH_SWELL: f32 = 0.07;
/// Opens the notch fully, ending any pending hover intent.
fn open_notch(intent: &mut Option<Instant>, tween: &mut Tween) {
    *intent = None;
    tween.retarget(1.0, motion::OPEN_TIME, motion::OPEN);
}
/// Keyboard focus inside a notch: the focused media control, and the window that had the
/// foreground before the notch took it, which gets it back on Escape.
struct Focus {
    control: usize,
    return_to: Option<WindowKey>,
}
/// Client coordinates of a mouse message.
fn point(lparam: LPARAM) -> (i32, i32) {
    let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
    let y = i32::from((lparam.0 >> 16) as u16 as i16);
    (x, y)
}
/// Brings our own notch to the foreground for keyboard focus, which only an explicit UIA focus
/// request asks for; Windows may refuse, as for any app not in front.
fn take_foreground(notch: HWND) -> bool {
    // SAFETY: scalar foreground request for our own live notch HWND; never forced.
    unsafe { SetForegroundWindow(notch).as_bool() || GetForegroundWindow() == notch }
}
/// The next enabled control after `from`, wrapping, or the first (last) enabled one without it.
fn step<const N: usize>(enabled: [bool; N], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = enabled.len();
    let start = from.unwrap_or(if forward { count - 1 } else { 0 });
    (1..=count)
        .map(|offset| {
            if forward {
                (start + offset) % count
            } else {
                (start + count - offset) % count
            }
        })
        .find(|&control| enabled[control])
}
/// What `reconcile_focus` does for a notch whose keyboard focus is on `control`.
#[derive(Debug, PartialEq, Eq)]
enum Refocus {
    Keep,
    Announce,
    Move(usize),
    Release,
}
fn refocus<const N: usize>(enabled: [bool; N], control: usize, replaced: bool) -> Refocus {
    if enabled.get(control) == Some(&true) {
        return if replaced {
            Refocus::Announce
        } else {
            Refocus::Keep
        };
    }
    step(enabled, Some(control), true).map_or(Refocus::Release, Refocus::Move)
}
/// Transient topology reads are retried this often, at most `TOPOLOGY_RETRIES` times per change.
const TOPOLOGY_RETRY: Duration = Duration::from_millis(250);
const TOPOLOGY_RETRIES: u32 = 8;
/// Windows sends no notification when a full-screen app starts or stops. The foreground geometry is
/// re-read on every catalog generation (foreground, show/hide, title, and the 30 s repair scan) and
/// topology change; the shell state is polled at this interval only while the foreground window
/// covers its whole monitor, so an ordinary desktop adds no wakes.
// shortcut: an in-place resize to full screen with no other window event is seen at the next catalog generation (30 s worst case); a foreground-scoped location hook was rejected for its caret/cursor event volume.
const FULLSCREEN_POLL: Duration = Duration::from_secs(2);
/// A lost graphics device is rebuilt after a doubling delay (250 ms up to 8 s), since a driver reset
/// or update takes a moment. Any rebuild failure retries, as one during a reset need not read as a
/// loss; a failure ends the host once `GRAPHICS_ATTEMPTS` rebuilds have run with no settled minute
/// between losses. Repeated successful rebuilds (a flapping device) stay one per 8 s at most.
const GRAPHICS_RETRY: Duration = Duration::from_millis(250);
const GRAPHICS_ATTEMPTS: u32 = 8;
/// A loss this long after the last rebuild starts a fresh budget; sooner ones keep backing off.
const GRAPHICS_SETTLED: Duration = Duration::from_secs(60);
/// Visualizer samples are at least this far apart: at most 15 a second, even after a late wake.
const METER_PERIOD: Duration = Duration::from_millis(67);

/// When the visualizer samples next: never unless a visible notch is open or opening (`open`), the
/// track plays and the endpoint has a meter; otherwise `METER_PERIOD` after the last sample, or at
/// once. The last sample outlives a stop, so stopping and restarting never samples faster.
fn meter_due(
    open: bool,
    playing: bool,
    metered: bool,
    sampled: Option<Instant>,
    now: Instant,
) -> Option<Instant> {
    (open && playing && metered).then(|| sampled.map_or(now, |at| at + METER_PERIOD))
}

fn rebuild_delay(attempts: u32) -> Duration {
    GRAPHICS_RETRY * (1 << attempts.min(5))
}
/// When the next device rebuild is due, and how many have run since the budget was refreshed.
struct Rebuild {
    due: Option<Instant>,
    attempts: u32,
    started: Instant,
}
impl Rebuild {
    fn new(now: Instant) -> Self {
        Self {
            due: None,
            attempts: 0,
            started: now,
        }
    }
    /// Schedules a rebuild for a detected loss; detections while one is pending keep it.
    fn lose(&mut self, now: Instant) {
        if self.due.is_some() {
            return;
        }
        if now.duration_since(self.started) >= GRAPHICS_SETTLED {
            self.attempts = 0;
        }
        self.due = Some(now + rebuild_delay(self.attempts));
    }
    /// Whether a rebuild is due now; if so it counts against the budget.
    fn start(&mut self, now: Instant) -> bool {
        if self.due.is_none_or(|due| due > now) {
            return false;
        }
        self.attempts += 1;
        self.started = now;
        true
    }
    /// Records a rebuild's outcome; false once a failure has spent the budget.
    fn finish(&mut self, rebuilt: bool, now: Instant) -> bool {
        if rebuilt {
            self.due = None;
        } else if self.attempts >= GRAPHICS_ATTEMPTS {
            return false;
        } else {
            self.due = Some(now + rebuild_delay(self.attempts));
        }
        true
    }
    fn wait(&self, now: Instant) -> u32 {
        self.due
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
}

/// `WM_MOUSEMOVE` wParam bit: the left button is still down.
const MK_LBUTTON: usize = 0x0001;

/// A press on a pinned dock slot: which pin, where the pointer went down, and whether it has moved far
/// enough to be a reorder (`dock::drag_started`) rather than a click.
struct Drag {
    from: usize,
    x: i32,
    active: bool,
}

/// Applies one pin edit: only a real change (`edit` returns true) rewrites the store's pin section and marks
/// it dirty, once. Returns whether the dock must be rebuilt.
fn edit_pins(
    pins: &mut Pins,
    store: &mut Store,
    pending: &mut Pending,
    now: Instant,
    edit: impl FnOnce(&mut Pins) -> bool,
) -> bool {
    let changed = edit(pins);
    if changed {
        pins.write_to(store);
        pending.mark(now);
    }
    changed
}

/// The shelf and what drawing it needs beyond the list: the system icons delivered so far for its visible
/// entries, and whether they must be asked for again (the list changed, or the panel was never open).
struct ShelfView {
    shelf: Shelf,
    icons: Vec<Option<Arc<IconPixels>>>,
    stale: bool,
}

/// What UI Automation elements are valid for: the media session and the shelf's list.
fn uia_version(media: &Media, shelf: &Shelf) -> u32 {
    (media.generation() as u32).wrapping_add(shelf.revision())
}

/// As `edit_pins`, for the shelf: only a real change marks the store dirty (once) and re-asks for icons.
fn edit_shelf(
    view: &mut ShelfView,
    store: &mut Store,
    pending: &mut Pending,
    now: Instant,
    edit: impl FnOnce(&mut Shelf) -> bool,
) -> bool {
    let changed = edit(&mut view.shelf);
    if changed {
        view.stale = true;
        if view.shelf.write_to(store) {
            pending.mark(now);
        }
    }
    changed
}

struct Desktop {
    surfaces: Vec<Surface>,
    /// Last settled topology; each monitor in it carries every shown kind.
    monitors: Vec<Monitor>,
    /// Kinds shown on every monitor; Show/Hide commands and closing a surface apply to all monitors.
    shown: Vec<SurfaceKind>,
    /// When the next topology read is due, and how many transient reads this change has retried.
    topology_due: Option<Instant>,
    topology_retries: u32,
    /// At most one validated full-screen window per settled monitor.
    fullscreen: Vec<(WindowKey, String)>,
    /// Catalog generation seen by the last full-screen check, and the next shell poll, armed only
    /// while a current or retained window covers its monitor.
    fullscreen_generation: Option<u64>,
    fullscreen_due: Option<Instant>,
    graphics: Graphics,
    rebuild: Rebuild,
    picker: Option<Picker>,
    /// Open only while the user has it up; created on demand, destroyed on close, so it costs nothing closed.
    palette: Palette,
    palette_dirty: bool,
    settings_window: Option<SettingsWindow>,
    file_picker: shelf_os::FilePicker,
    /// What the user chose: loaded once, persisted through `store` on change, applied live by `apply_settings`.
    settings: Settings,
    dock_items: Vec<DockItem>,
    dock_names: Vec<String>,
    dock_generation: Option<u64>,
    /// Pinned apps, their persisted form, and the unwritten-change times. Read once at startup; written on
    /// a host-loop deadline (`store_wait`) after a change and once at quit, never by a timer or thread.
    pins: Pins,
    /// File references, drawn in the expanded notch; drops arrive through `drops` and are applied by
    /// `refresh_shelf`, so the drop target touches no host state.
    shelf: ShelfView,
    drops: Drops,
    /// An icon finished this pass (`refresh_dock` collected it).
    icons_arrived: bool,
    store: Store,
    store_path: Option<std::path::PathBuf>,
    pending: Pending,
    drag: Option<Drag>,
    /// Media session state, drawn by the notches. Declared before `text` so its WinRT objects and
    /// event tokens are released before that field's `RoUninitialize`, and before `icons`, whose
    /// worker stops only once media's handle to its queue is gone.
    media: Media,
    icons: IconService,
    /// The default render endpoint's volume, bound once a media notch is first shown.
    audio: Audio,
    /// Windows text size (Accessibility > Text size) scales the whole notch, whose only content is text.
    text: TextSettings,
    text_factor: f64,
    /// Last drawn time text; the notches repaint only when it changes.
    clock: String,
    date: String,
    /// Just after the next minute boundary; until then a wake reuses the drawn text unformatted.
    clock_due: Instant,
    /// The next progress tick, and the scheduled time of the last one that came due: the 1 s
    /// floor counts from that, not from its late wake, so wake latency never accumulates.
    progress_due: Option<Instant>,
    progress_tick: Instant,
    /// When the visualizer last read the meter.
    sampled: Option<Instant>,
    /// Countdown timers: restored once from the store and driven only by `refresh_timers`' deadline.
    timers: Timers,
    /// The soonest timer's last requested panel text, so an expanded notch repaints only when it changes.
    timer_shown: String,
    /// The power pill: set by `message` on a status change, ended by `refresh_power`'s deadline.
    power: Power,
    /// Dock hover thumbnails: the pointer's rest on a slot, and the popup while one shows.
    preview: preview::Intent,
    popup: Option<preview::Popup>,
}
impl Desktop {
    fn create(_probe: &Probe) -> Result<Self> {
        let monitors = topology::settle(native_window::monitors()?)
            .ok_or_else(|| Error::new(E_FAIL, "No usable display monitor."))?;
        let text = TextSettings::create()?;
        let text_factor = text.factor()?;
        let store_path = store::default_path();
        let mut store = store_path
            .as_deref()
            .map_or_else(Store::default, |path| store::load(path).0);
        let now = Instant::now();
        let devices: Vec<String> = monitors.iter().map(|m| m.device.clone()).collect();
        let settings = store.settings.clone().keep_one_monitor(&devices);
        let mut shelf = Shelf::from_store(&store);
        if shelf.set_cap(settings.shelf_cap) {
            shelf.write_to(&mut store); // the cut list leaves the file with its next write
        }
        let timers = Timers::restore(&store.timers, timer::wall_now(), now);
        // The section now mirrors what is running; stale entries leave the file with its next write.
        timers.write_to(&mut store, &mut Pending::default(), now);
        let mut desktop = Self {
            surfaces: Vec::new(),
            monitors,
            shown: Vec::new(),
            topology_due: None,
            topology_retries: 0,
            fullscreen: native_window::fullscreen_candidate()
                .filter(|_| native_window::shell_fullscreen())
                .into_iter()
                .collect(),
            fullscreen_generation: None,
            fullscreen_due: None,
            graphics: Graphics::create()?,
            rebuild: Rebuild::new(Instant::now()),
            picker: None,
            palette: crate::theme::system_palette(settings.theme, text.accent()),
            palette_dirty: false,
            settings_window: None,
            file_picker: Default::default(),
            settings,
            icons: IconService::create()?,
            dock_items: Vec::new(),
            dock_names: dock::names(&[], &WindowModel::default()),
            dock_generation: None,
            pins: Pins::from_store(&store),
            shelf: ShelfView {
                shelf,
                icons: Vec::new(),
                stale: true,
            },
            drops: Drops::default(),
            icons_arrived: false,
            store,
            store_path,
            pending: Pending::default(),
            drag: None,
            media: Media::create(),
            audio: Audio::create(),
            text,
            text_factor,
            clock: String::new(),
            date: String::new(),
            clock_due: Instant::now(),
            progress_due: None,
            progress_tick: Instant::now(),
            sampled: None,
            timers,
            timer_shown: String::new(),
            power: Power::new(power::Raw::read()),
            preview: preview::Intent::default(),
            popup: None,
        };
        if desktop.settings.notch {
            desktop.show(SurfaceKind::Notch)?;
        }
        #[cfg(debug_assertions)]
        if *_probe == Probe::Initialization {
            return Err(Error::new(
                E_FAIL,
                "Injected failure after first composed window.",
            ));
        }
        if desktop.settings.dock {
            desktop.show(SurfaceKind::Dock)?;
        }
        // After the first paints, so loading the media component never delays them. WinRT is
        // already initialized on this thread by `TextSettings`.
        desktop.media.start(desktop.icons.jobs());
        Ok(desktop)
    }
    fn dock_slots(&self) -> usize {
        self.dock_items.len() + 1
    }
    fn show(&mut self, kind: SurfaceKind) -> Result<()> {
        if !self.shown.contains(&kind) {
            self.shown.push(kind);
        }
        for monitor in self.monitors.clone() {
            self.place(&monitor, kind)?;
        }
        Ok(())
    }
    /// Creates and paints `kind` on `monitor` unless it already has one.
    fn place(&mut self, monitor: &Monitor, kind: SurfaceKind) -> Result<()> {
        if self.settings.off_monitors.contains(&monitor.device) {
            return Ok(()); // the user switched ROQA off on this monitor
        }
        if self
            .surfaces
            .iter()
            .any(|surface| surface.kind() == kind && surface.monitor == monitor.device)
        {
            return Ok(());
        }
        let dpi = match kind {
            // The notch's effective DPI: monitor DPI times the Windows text-size factor.
            SurfaceKind::Notch => (f64::from(monitor.dpi) * self.text_factor).round() as u32,
            SurfaceKind::Dock => monitor.dpi,
        };
        let mut surface = Surface::create(
            &self.graphics,
            kind,
            monitor,
            dpi,
            self.dock_slots(),
            &self.drops,
            self.palette,
        )?;
        if kind == SurfaceKind::Notch {
            // Advances every existing notch first, so all monitors show the same minute.
            self.refresh_clock()?;
            surface.paint_notch(
                (&self.clock, &self.date),
                &self.media,
                &self.audio,
                &self.timers,
                &self.power,
                &self.shelf,
            )?;
            self.surfaces.push(surface);
        } else {
            surface.publish_accessibility(&self.dock_names, false);
            self.surfaces.push(surface);
            // Sized after the push: icons are extracted once, for the densest dock.
            let icons = self.dock_icons();
            self.surfaces[self.surfaces.len() - 1].paint_dock(&self.dock_items, &icons)?;
        }
        // Shown after its first paint, and never on the monitor a full-screen app covers.
        let visible = !self
            .fullscreen
            .iter()
            .any(|(_, device)| device == &monitor.device);
        let last = self.surfaces.len() - 1;
        self.surfaces[last].set_visible(visible);
        Ok(())
    }
    /// Formats the time only once the minute may have changed, and repaints the notches only when the
    /// text changed; returns how long until the next minute.
    // Windows `Instant` uses QPC, including suspend; an elapsed deadline is detected on the next loop pass.
    fn refresh_clock(&mut self) -> Result<u32> {
        let now = Instant::now();
        if now >= self.clock_due {
            let clock = clock::now();
            self.clock_due = now + Duration::from_millis(clock.next_minute_ms.into());
            if clock.time != self.clock || clock.date != self.date {
                (self.clock, self.date) = (clock.time, clock.date);
                self.request_notches(Repaint::Full, |_| true);
            }
        }
        Ok(milliseconds_until(self.clock_due, now))
    }
    /// Asks for `repaint` on every visible notch `wanted` accepts. Nothing is drawn until the pass
    /// ends (`paint_requested`); hidden notches are painted before they are restored.
    fn request_notches(&mut self, repaint: Repaint, wanted: impl Fn(&Surface) -> bool) {
        for notch in &mut self.surfaces {
            if notch.kind() == SurfaceKind::Notch && notch.visible && wanted(notch) {
                notch.request(repaint);
            }
        }
    }
    /// Paints each visible notch once for whatever this pass asked of it.
    fn paint_requested(&mut self) -> Result<()> {
        let peaks = self.audio.peaks();
        for notch in &mut self.surfaces {
            match std::mem::take(&mut notch.repaint) {
                _ if !notch.visible => {}
                Repaint::Nothing => {}
                Repaint::Meter => notch.paint_meter(peaks)?,
                Repaint::Full => notch.paint_notch(
                    (&self.clock, &self.date),
                    &self.media,
                    &self.audio,
                    &self.timers,
                    &self.power,
                    &self.shelf,
                )?,
            }
        }
        Ok(())
    }
    /// Repaints each open notch whose progress row changed, such as after a seek, and returns how
    /// long until a playing track's row should tick next (`Track::next_tick`, with its 1 s floor).
    /// Collapsed and hidden notches cost nothing here.
    // Out of line: inlined into `update`, it cost about 12 KB more release binary (measured).
    #[inline(never)]
    fn refresh_progress(&mut self) -> Result<u32> {
        let mut next_due = None::<Instant>;
        for notch in self.surfaces.iter_mut().filter(|s| s.expanded()) {
            let Some(track) = self.media.view().shown() else {
                break;
            };
            let (now, ticks) = (Instant::now(), media::now_ticks());
            if let Some(due) = self.progress_due.filter(|&due| due <= now) {
                self.progress_tick = due;
            }
            let width = layout::media_progress(notch.dpi).width;
            // A running animation paints every frame already.
            if !notch.live && notch.played != track.progress(ticks, width) {
                notch.request(Repaint::Full);
            }
            let floor =
                (self.progress_tick + Duration::from_secs(1)).saturating_duration_since(now);
            if let Some(wait) = track.next_tick(ticks, width, floor) {
                let due = now + wait;
                next_due = Some(next_due.map_or(due, |next| next.min(due)));
            }
        }
        self.progress_due = next_due;
        Ok(next_due.map_or(INFINITE, |due| milliseconds_until(due, Instant::now())))
    }
    /// Reads the endpoint meter once per due sample (`meter_due`) and asks to repaint the meter slot
    /// of each settled open notch whose bar heights changed; returns how long until the next sample. Stopped, it makes
    /// no meter call, flattens the bars and has no deadline, and with no open notch paints nothing.
    #[inline(never)]
    fn refresh_meter(&mut self) -> Result<u32> {
        let now = Instant::now();
        let playing = self.media.view().shown().is_some_and(|track| track.playing);
        let open = self.surfaces.iter().any(Surface::expanded);
        let due = meter_due(open, playing, self.audio.metered(), self.sampled, now);
        match due {
            None => self.audio.stop(),
            Some(due) if due > now => return Ok(milliseconds_until(due, now)),
            Some(_) => {
                self.audio.sample();
                self.sampled = Some(now);
            }
        }
        let peaks = self.audio.peaks();
        let shown = self.media.view().shown().is_some();
        for notch in self.surfaces.iter_mut().filter(|s| s.expanded()) {
            // A running animation paints every frame already.
            if shown && !notch.live && notch.bars != layout::meter_heights(peaks, notch.dpi) {
                notch.request(Repaint::Meter);
            }
        }
        Ok(due.map_or(INFINITE, |_| milliseconds_until(now + METER_PERIOD, now)))
    }
    /// Completes due timers, ends a finished alert, and asks an expanded notch to repaint when the
    /// soonest timer's seconds changed; returns the wait until the timers' next deadline. With no timers
    /// and no alert it does nothing and returns `INFINITE`: no wake, and collapsed notches get no tick.
    #[inline(never)]
    fn refresh_timers(&mut self) -> u32 {
        let now = Instant::now();
        if self.timers.take_done(now) {
            self.timers
                .write_to(&mut self.store, &mut self.pending, now);
            self.complete_timer();
            self.request_notches(Repaint::Full, |_| true);
        }
        if self.timers.alert_over(now) {
            self.surfaces.iter_mut().for_each(Surface::close_alert);
            self.request_notches(Repaint::Full, |_| true);
        }
        let expanded = self.surfaces.iter().any(Surface::expanded);
        if expanded {
            let panel = self.timers.view(now).map(|view| view.panel);
            if panel.as_deref().unwrap_or("") != self.timer_shown {
                self.timer_shown = panel.unwrap_or_default();
                // A running animation paints every frame already.
                self.request_notches(Repaint::Full, |s| s.expanded() && !s.live);
            }
        }
        self.timers
            .next_wake(expanded, now)
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// Opens every visible notch; a notch hidden by a full-screen app stays hidden.
    fn open_notches(&mut self) {
        for surface in &mut self.surfaces {
            if surface.visible
                && let Role::Notch { intent, .. } = &mut surface.role
            {
                open_notch(intent, &mut surface.motion);
                surface.live = true;
            }
        }
    }
    /// Ends a finished power notice and returns the wait until its deadline; with none showing it does
    /// nothing and returns `INFINITE`.
    #[inline(never)]
    fn refresh_power(&mut self) -> u32 {
        let now = Instant::now();
        if self.power.over(now) {
            self.surfaces.iter_mut().for_each(Surface::close_alert);
            self.request_notches(Repaint::Full, |_| true);
        }
        self.power
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// Follows the pointer on the dock: after `preview::DELAY` on one slot with windows, shows a DWM
    /// thumbnail of the group's most recent window above it. It ends on leave, a slot change, a click, a
    /// hidden dock (full screen) or the window closing. The only wait is the delay's deadline, returned
    /// here (`INFINITE` otherwise); no thread or timer, and with nothing hovered the pass is a short scan.
    #[inline(never)]
    fn refresh_preview(&mut self) -> u32 {
        let now = Instant::now();
        let hovered = self
            .surfaces
            .iter()
            .enumerate()
            .find_map(|(index, surface)| match surface.role {
                // Slot 0 is the picker; a pinned app that is not running has nothing to show.
                Role::Dock {
                    hover: Some(slot @ 1..),
                    ..
                } if surface.visible => self
                    .dock_items
                    .get(slot - 1)
                    .filter(|item| !item.windows.is_empty())
                    .map(|item| (index, slot, item)),
                _ => None,
            });
        let spot = hovered.map(|(index, slot, _)| (self.surfaces[index].window.0.0 as usize, slot));
        let ended = self.preview.point(spot, now);
        let stale = self.popup.as_ref().is_some_and(|popup| {
            !hovered.is_some_and(|(_, _, item)| item.windows.contains(&popup.target))
        });
        if ended || stale {
            self.popup = None;
            if !ended {
                self.preview.dismiss(); // the window went away under a resting pointer
            }
        }
        if self.preview.take_due(now).is_some() {
            let shown = hovered.and_then(|(index, slot, item)| {
                let dock = &self.surfaces[index];
                let work = self
                    .monitors
                    .iter()
                    .find(|m| m.device == dock.monitor)?
                    .work;
                let app = crate::pins::display_name(&item.identity);
                preview::Popup::show(&item.windows, &app, slot, dock.bounds, work, dock.dpi).ok()
            });
            match shown {
                Some(popup) => self.popup = Some(popup),
                None => {
                    self.preview.dismiss();
                }
            }
        }
        self.preview
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// A click on the dock ends the preview; it stays away until the pointer moves to another slot.
    fn dismiss_preview(&mut self) {
        self.popup = None;
        self.preview.dismiss();
    }
    /// A timer finished: every visible notch opens (once, however many finished together) and the
    /// system sound plays if enabled. No toast: notification packaging is M6.
    fn complete_timer(&mut self) {
        self.open_notches();
        if self.settings.timer_sound {
            // SAFETY: plays the user's own default sound asynchronously; no pointers.
            unsafe {
                let _ = MessageBeep(MB_ICONASTERISK);
            }
        }
    }
    /// Paints one frame of each running animation; returns true while any still runs.
    /// The final frame always lands exactly on the target before a surface goes idle.
    fn animate(&mut self) -> Result<bool> {
        let now = Instant::now();
        let docks_live = self
            .surfaces
            .iter()
            .any(|s| s.live && s.kind() == SurfaceKind::Dock);
        let icons = if docks_live {
            self.dock_icons()
        } else {
            Vec::new()
        };
        let mut running = false;
        for surface in &mut self.surfaces {
            if let Role::Notch { intent, .. } = &mut surface.role
                && intent.is_some_and(|at| now >= at)
            {
                open_notch(intent, &mut surface.motion);
            }
            if surface.live {
                match surface.kind() {
                    SurfaceKind::Notch => surface.paint_notch(
                        (&self.clock, &self.date),
                        &self.media,
                        &self.audio,
                        &self.timers,
                        &self.power,
                        &self.shelf,
                    )?,
                    SurfaceKind::Dock => surface.paint_dock(&self.dock_items, &icons)?,
                }
                surface.live = surface.motion.running(now)
                    || match &surface.role {
                        Role::Notch { intent, .. } => intent.is_some(),
                        Role::Dock { slots, .. } => slots.iter().any(|tween| tween.running(now)),
                    };
            }
            running |= surface.live;
        }
        Ok(running)
    }
    fn close(&mut self, kind: SurfaceKind) {
        self.shown.retain(|shown| *shown != kind);
        self.surfaces.retain(|surface| surface.kind() != kind);
    }
    fn dock_at(&self, hwnd: HWND) -> Option<&Surface> {
        self.surfaces
            .iter()
            .find(|s| s.kind() == SurfaceKind::Dock && s.window.0 == hwnd)
    }
    /// Opens the settings window (or replaces an open one) on the connected monitors.
    fn open_settings(&mut self) -> Result<()> {
        self.settings_window.take();
        let devices: Vec<String> = self.monitors.iter().map(|m| m.device.clone()).collect();
        self.settings_window = Some(SettingsWindow::create(
            &self.settings,
            &devices,
            self.text_factor,
        )?);
        self.sync_settings_pins(0);
        Ok(())
    }
    fn sync_settings_pins(&self, selected: usize) {
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
        }
    }
    /// Takes what the settings window produced in its last message: a change to apply live and a timer to
    /// start. The window is told how the timer went.
    fn settings_changed(&mut self, catalog: &mut Catalog) -> Result<()> {
        let Some(window) = &mut self.settings_window else {
            return Ok(());
        };
        let (change, start, movement) = (
            window.take_change(),
            window.take_start(),
            window.take_move(),
        );
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
        if let Some(next) = change {
            self.apply_settings(next, catalog)?;
        }
        if let Some(seconds) = start {
            let status = self.start_timer(seconds);
            if let Some(window) = &self.settings_window {
                let _ = window.say(&status);
            }
        }
        Ok(())
    }
    /// Applies new settings live: creates and destroys exactly the surfaces `settings::surface_diff` names,
    /// trims the shelf to its cap, repoints the diagnostics report and marks the store dirty once.
    fn apply_settings(&mut self, next: Settings, catalog: &mut Catalog) -> Result<()> {
        let devices: Vec<String> = self.monitors.iter().map(|m| m.device.clone()).collect();
        let next = next.clamped().keep_one_monitor(&devices);
        if next == self.settings {
            return Ok(());
        }
        let old = std::mem::replace(&mut self.settings, next);
        let restyle = old.theme != self.settings.theme;
        if old.desktop_only != self.settings.desktop_only {
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
            self.palette = crate::theme::system_palette(self.settings.theme, self.text.accent());
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
    /// Starts a countdown the user asked for in the settings window; the text is the window's status line.
    fn start_timer(&mut self, seconds: u64) -> String {
        let now = Instant::now();
        if !self.timers.start(seconds, timer::wall_now(), now) {
            return format!("{} timers are already running.", timer::MAX);
        }
        self.timers
            .write_to(&mut self.store, &mut self.pending, now);
        self.request_notches(Repaint::Full, |_| true);
        "Timer started.".to_owned()
    }
    fn open_picker(&mut self, catalog: &Catalog) -> Result<()> {
        self.picker.take();
        self.picker = Some(Picker::create(catalog.model())?);
        Ok(())
    }
    /// Rebuilds dock items only when the catalog generation or an icon changed.
    fn refresh_dock(&mut self, model: &WindowModel) -> Result<()> {
        let icons_arrived = self.icons.collect();
        self.icons_arrived = icons_arrived;
        if self.dock_generation == Some(model.generation) && !icons_arrived {
            return Ok(());
        }
        if self.dock_generation != Some(model.generation) {
            self.dock_generation = Some(model.generation);
            let items = dock::items_with_pins(model, &self.pins, self.settings.desktop_only);
            let changed = items != self.dock_items;
            self.dock_names = dock::names(&items, model);
            for dock in self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Dock)
            {
                dock.publish_accessibility(&self.dock_names, changed);
            }
            // Title-only changes publish new generations; they rename UIA items but leave pixels untouched.
            if !changed && !icons_arrived {
                return Ok(());
            }
            if changed {
                let shelf = self.shelf.shelf.entries();
                self.icons.retain(|identity| {
                    items.iter().any(|item| &item.identity == identity)
                        || matches!(identity, Identity::Executable(path) if shelf.iter().any(|e| &e.path == path))
                });
            }
            let resized = items.len() != self.dock_items.len();
            self.dock_items = items;
            if resized {
                let slots = self.dock_slots();
                for dock in &mut self.surfaces {
                    let Role::Dock {
                        hover,
                        slots: tweens,
                        ..
                    } = &mut dock.role
                    else {
                        continue;
                    };
                    let Some(monitor) = self.monitors.iter().find(|m| m.device == dock.monitor)
                    else {
                        continue;
                    };
                    (*hover, *tweens) = (None, vec![Tween::rest(0.0); slots]);
                    dock.bounds = placement(monitor.work, dock.dpi, SurfaceKind::Dock, slots);
                    dock.window.resize(dock.bounds)?;
                    dock.canvas.resize(dock.bounds)?;
                }
            }
        }
        let icons = self.dock_icons();
        for dock in self
            .surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Dock && s.visible)
        {
            dock.paint_dock(&self.dock_items, &icons)?;
        }
        Ok(())
    }
    /// Album art side to decode, in physical pixels: the panel cover on the densest notch (also while
    /// it is hidden), or 0 with no notch surface, which skips the decode until one exists.
    // shortcut: art keeps its decode size after a DPI or text-size change until the track changes; re-decode if it looks soft.
    fn cover_pixels(&self) -> u32 {
        self.surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Notch)
            .map(|s| (COVER_DIPS * s.dpi as f32 / 96.0).round() as u32)
            .max()
            .unwrap_or(0)
    }
    /// The one icon size the worker's cache holds: the densest dock's. The shelf shares it (a different size
    /// would empty the cache on every switch) and draws it smaller; none without a dock.
    fn icon_size(&self) -> Option<u32> {
        let densest = self
            .surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Dock)
            .map(|s| s.dpi)
            .max();
        densest.map(|dpi| dock::px(dock::ICON_DIPS, dpi) as u32)
    }
    /// One icon set for every dock, extracted for the densest dock; lower-DPI docks scale it down.
    // shortcut: mixed-DPI docks resample icons; keep one cache size per DPI if that looks soft on hardware.
    fn dock_icons(&mut self) -> Vec<Option<Arc<IconPixels>>> {
        let Some(size) = self.icon_size() else {
            return Vec::new();
        };
        self.dock_items
            .iter()
            .map(|item| {
                let sources = icons::sources(&item.identity, item.executable.as_deref());
                self.icons.icon(&item.identity, sources, size)
            })
            .collect()
    }
    /// A UIA Invoke from an element of the current layout acts exactly like a click on that slot.
    fn invoke_dock(
        &mut self,
        hwnd: HWND,
        slot: usize,
        version: u32,
        catalog: &Catalog,
    ) -> Result<()> {
        let current = self.dock_at(hwnd).and_then(|dock| match &dock.role {
            Role::Dock { uia, .. } => uia.lock().map(|state| state.version).ok(),
            Role::Notch { .. } => None,
        });
        if current == Some(version) {
            self.activate_slot(Some(slot), catalog)?;
        }
        Ok(())
    }
    /// Slot 0 opens the accessible picker; other slots act on their real window group.
    fn activate_slot(&mut self, slot: Option<usize>, catalog: &Catalog) -> Result<()> {
        match slot {
            Some(0) => self.open_picker(catalog),
            Some(slot) => {
                if let Some(action) = self
                    .dock_items
                    .get(slot - 1)
                    .and_then(|item| dock::action(item, dock::foreground_key()))
                {
                    // A refused or stale target is left alone; the catalog repaints the dock on the next event.
                    match action {
                        dock::Action::Launch(target) => {
                            dock::launch(&target);
                        }
                        action => {
                            dock::perform(action);
                        }
                    }
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
    /// Right-click on a dock slot toggles its pin. The dock is rebuilt on the next pass.
    fn toggle_pin(&mut self, slot: Option<usize>) {
        let Some(item) = slot
            .and_then(|slot| slot.checked_sub(1))
            .and_then(|index| self.dock_items.get(index))
        else {
            return;
        };
        let (identity, pinned) = (item.identity.clone(), dock::pinned_as(item, &self.pins));
        self.change_pins(|pins| match &pinned {
            Some(pin) => pins.remove(pin),
            None => pins.add(identity).is_ok(),
        });
    }
    fn change_pins(&mut self, edit: impl FnOnce(&mut Pins) -> bool) {
        let now = Instant::now();
        if edit_pins(
            &mut self.pins,
            &mut self.store,
            &mut self.pending,
            now,
            edit,
        ) {
            self.dock_generation = None;
            // Every pin change (dock click, drag, settings move) refreshes the list so a later Move is not stale.
            let selected = self
                .settings_window
                .as_ref()
                .map_or(0, SettingsWindow::pin_selection);
            self.sync_settings_pins(selected);
        }
    }
    /// Left button released on a dock. Returns true when it ended a reorder drag (so it is not a click):
    /// the pin moves to the slot under the pointer, as one store change.
    fn end_drag(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        // SAFETY: releases this thread's own pointer capture.
        unsafe {
            let _ = ReleaseCapture();
        }
        if !drag.active {
            return false;
        }
        let (slots, pins) = (self.dock_slots(), self.pins.items().len());
        let target = self.dock_at(hwnd).and_then(|dock| {
            let (x, _) = point(lparam);
            dock::drop_index(x, dock.bounds.width, dock.dpi, slots, pins)
        });
        if let Some(to) = target {
            self.change_pins(|pins| pins.move_to(drag.from, to));
        }
        true
    }
    /// Applies what the drop target queued, then asks the worker for the visible entries' icons, but only
    /// while a panel is open and only when the list changed or an icon arrived: an empty shelf, a collapsed
    /// notch and an idle pass do nothing here.
    #[inline(never)]
    fn refresh_shelf(&mut self) {
        let events = std::mem::take(&mut *self.drops.borrow_mut());
        for event in events {
            match event {
                DropEvent::Hover(hwnd, over) => {
                    if let Some(notch) = self.surfaces.iter_mut().find(|s| s.window.0 == hwnd) {
                        notch.set_notch_hover(over);
                    }
                }
                DropEvent::Files(paths) => self.change_shelf(|shelf| shelf.add_all(&paths) > 0),
            }
        }
        let wanted = self.shelf.stale || self.icons_arrived;
        if self.shelf.shelf.is_empty() || !wanted || !self.surfaces.iter().any(Surface::expanded) {
            return;
        }
        let Some(size) = self.icon_size() else {
            return;
        };
        let icons: Vec<_> = self
            .shelf
            .shelf
            .visible()
            .iter()
            .map(|entry| {
                let identity = Identity::Executable(entry.path.clone());
                self.icons.icon(&identity, vec![entry.path.clone()], size)
            })
            .collect();
        self.shelf.stale = false;
        let same =
            |(new, old): (&Option<Arc<IconPixels>>, &Option<Arc<IconPixels>>)| match (new, old) {
                (Some(new), Some(old)) => Arc::ptr_eq(new, old),
                (None, None) => true,
                _ => false,
            };
        let changed =
            icons.len() != self.shelf.icons.len() || !icons.iter().zip(&self.shelf.icons).all(same);
        self.shelf.icons = icons;
        if changed {
            // A running animation paints every frame already.
            self.request_notches(Repaint::Full, |s| s.expanded() && !s.live);
        }
    }
    /// Opens the picker; a finished one whose completion wake was lost is collected instead of stranding it.
    fn start_file_picker(&mut self) {
        if let Some(paths) = self.file_picker.start() {
            self.change_shelf(|shelf| shelf.add_all(&paths) > 0);
        }
    }
    fn change_shelf(&mut self, edit: impl FnOnce(&mut Shelf) -> bool) {
        let now = Instant::now();
        if edit_shelf(
            &mut self.shelf,
            &mut self.store,
            &mut self.pending,
            now,
            edit,
        ) {
            self.request_notches(Repaint::Full, |_| true);
            self.reconcile_focus(|control| control >= notch_uia::SHELF_BASE);
        }
    }
    /// The visible shelf entry under a window point of notch `notch`, if its slot is drawn and filled.
    fn shelf_slot_at(&self, notch: usize, lparam: LPARAM) -> Option<usize> {
        let surface = &self.surfaces[notch];
        let (x, y) = point(lparam);
        let progress = surface.motion.value(Instant::now());
        let media = self.media.view().shown().is_some();
        layout::shelf_at(surface.bounds.width, surface.dpi, progress, media, x, y)
            .filter(|&index| index < self.shelf.shelf.slot_count())
    }
    /// A user activation of shelf entry `index`: open it, or with `reveal` select it in Explorer. The only
    /// existence check in the shelf's life happens here, never for a UNC path (`shelf_os::open` runs that
    /// call off the host thread); a target that is gone is marked, not opened, and not removed.
    fn open_shelf(&mut self, index: usize, reveal: bool) {
        if index >= self.shelf.shelf.slot_count() {
            return;
        }
        if self.shelf.shelf.is_empty() && index == 0 {
            self.start_file_picker();
            return;
        }
        let Some(index) = self.shelf.shelf.absolute(index) else {
            self.change_shelf(Shelf::next_page);
            return;
        };
        let Some(entry) = self.shelf.shelf.entries().get(index) else {
            return;
        };
        let path = entry.path.clone();
        // A UNC path or a mapped drive could stall the host on a dead share, so neither is stat'ed here.
        let network = shelf::is_network(entry.check, shelf_os::drive_type(&path));
        let found = network || std::path::Path::new(&path).exists();
        if self.shelf.shelf.set_missing(index, !found) {
            self.request_notches(Repaint::Full, |_| true);
        }
        if found {
            shelf_os::open(&path, reveal, network);
        }
    }
    /// Right-click on a drawn shelf entry removes it. Returns whether it was consumed.
    fn remove_shelf_at(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        let notch = self
            .surfaces
            .iter()
            .position(|s| s.kind() == SurfaceKind::Notch && s.window.0 == hwnd);
        let Some(index) = notch.and_then(|notch| self.shelf_slot_at(notch, lparam)) else {
            return false;
        };
        if let Some(index) = self.shelf.shelf.absolute(index) {
            self.change_shelf(|shelf| shelf.remove(index));
        }
        true
    }
    /// Milliseconds until the debounced store write is due; `INFINITE` with no unwritten change.
    fn store_wait(&self) -> u32 {
        let now = Instant::now();
        match self.pending.decision(now) {
            Flush::Idle => INFINITE,
            Flush::Wait(due) => milliseconds_until(due, now),
            Flush::Now => 0,
        }
    }
    /// Writes the store if its deadline passed, or at quit if anything is unwritten. A failure stays
    /// pending and retries after `store::RETRY`.
    fn flush_store(&mut self, quit: bool) {
        let now = Instant::now();
        if !self.pending.should_write(now, quit) {
            return;
        }
        match self
            .store_path
            .as_deref()
            .map(|path| self.store.flush(path))
        {
            Some(Err(error)) => {
                eprintln!("Store: write failed: {error}");
                self.pending.retry(now);
            }
            _ => self.pending.written(),
        }
    }
    fn command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::ShowPicker | Command::ShowSettings => {}
            Command::ShowNotch => self.show(SurfaceKind::Notch)?,
            Command::ShowDock => self.show(SurfaceKind::Dock)?,
            Command::HideNotch => self.close(SurfaceKind::Notch),
            Command::HideDock => self.close(SurfaceKind::Dock),
        }
        if matches!(command, Command::ShowNotch | Command::ShowDock) {
            // Also recovers monitors whose earlier placement ran out of retries.
            self.request_topology();
        }
        Ok(!self.shown.is_empty())
    }
    /// Media control input on a notch: a click on a control, a UIA Invoke or focus request, a key
    /// while it has keyboard focus, or losing the foreground. Returns whether `message` is consumed.
    fn notch_input(&mut self, message: &MSG) -> bool {
        let Some(notch) = self
            .surfaces
            .iter()
            .position(|s| s.kind() == SurfaceKind::Notch && s.window.0 == message.hwnd)
        else {
            return false;
        };
        // UIA posts the session generation its element was created for; older ones are ignored.
        let current = message.lParam.0 as u32 == uia_version(&self.media, &self.shelf.shelf);
        match message.message {
            WM_LBUTTONUP => return self.click_control(notch, message.lParam),
            WM_KEYDOWN => return self.notch_key(notch, message),
            dock_uia::INVOKE if current => self.press(message.wParam.0),
            notch_uia::FOCUS if current => self.enter_focus(notch, message.wParam.0),
            dock_uia::INVOKE | notch_uia::FOCUS => {}
            native_window::DEACTIVATED => {
                // The user went elsewhere: focus is theirs to place, so nothing is restored.
                self.surfaces[notch].blur();
            }
            _ => return false,
        }
        true
    }
    /// A click on a drawn media control sends its command, one on the progress bar seeks, and one
    /// on the volume bar sets the volume; the notch stays nonactivating.
    fn click_control(&mut self, notch: usize, lparam: LPARAM) -> bool {
        let surface = &self.surfaces[notch];
        let (x, y) = point(lparam);
        let progress = surface.motion.value(Instant::now());
        let (width, dpi) = (surface.bounds.width, surface.dpi);
        if let Some(index) = self.shelf_slot_at(notch, lparam) {
            // SAFETY: reads this thread's keyboard state as of the message being handled.
            let reveal = unsafe { GetKeyState(VK_CONTROL.0.into()) } < 0;
            self.open_shelf(index, reveal);
            return true;
        }
        if self.media.view().shown().is_none() {
            return false;
        }
        if let Some(control) = layout::media_control_at(width, dpi, progress, x, y) {
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
    fn press(&mut self, control: usize) {
        if control >= notch_uia::SHELF_BASE {
            self.open_shelf(control - notch_uia::SHELF_BASE, false);
        } else if control != audio::MUTE {
            self.media.command(control);
        } else if self.media.view().shown().is_some() {
            self.audio.toggle_mute();
        }
    }
    /// The notch controls that take focus and presses: the player's three, then mute while a
    /// default render endpoint is bound.
    fn enabled(&self) -> [bool; 7] {
        let track = self.media.view().shown();
        let [previous, play_pause, next] = track.map_or([false; 3], |track| track.controls);
        let mute = track.is_some() && self.audio.level().is_some();
        let mut enabled = [previous, play_pause, next, mute, false, false, false];
        for slot in 0..self.shelf.shelf.slot_count() {
            enabled[notch_uia::SHELF_BASE + slot] = true;
        }
        enabled
    }
    /// A UIA client focused a media control, or the notch itself (`notch_uia::FIRST`): the notch
    /// takes the foreground if Windows allows it, remembers the window it came from, and opens.
    fn enter_focus(&mut self, notch: usize, requested: usize) {
        let enabled = self.enabled();
        let surface = &mut self.surfaces[notch];
        let control = match requested {
            notch_uia::FIRST => step(enabled, None, true),
            control => Some(control).filter(|&c| enabled.get(c) == Some(&true)),
        };
        let Some(control) = control.filter(|_| surface.visible) else {
            return;
        };
        let return_to = if surface.focused().is_some() {
            None
        } else {
            let previous = dock::foreground_key();
            if !take_foreground(surface.window.0) {
                eprintln!("Notch: Windows kept another app in front; no keyboard focus");
                return;
            }
            previous
        };
        surface.focus_control(control, return_to);
        surface.announce_focus();
    }
    /// Keys while a notch has keyboard focus: Tab, Shift+Tab and the arrows move among enabled
    /// controls, Enter and Space press the focused one, and Escape closes and returns the foreground.
    fn notch_key(&mut self, notch: usize, message: &MSG) -> bool {
        let enabled = self.enabled();
        let surface = &mut self.surfaces[notch];
        let Some(control) = surface.focused().map(|focus| focus.control) else {
            return false;
        };
        // SAFETY: reads this thread's keyboard state as of the message being handled.
        let (shift, ctrl) = unsafe {
            (
                GetKeyState(VK_SHIFT.0.into()) < 0,
                GetKeyState(VK_CONTROL.0.into()) < 0,
            )
        };
        let key = VIRTUAL_KEY(message.wParam.0 as u16);
        if key == VK_O && ctrl {
            // One on-demand STA owns the dialog; the host continues servicing its deadlines.
            if message.lParam.0 & (1 << 30) == 0 {
                self.start_file_picker();
            }
            return true;
        }
        let next = match key {
            VK_TAB => step(enabled, Some(control), !shift),
            VK_RIGHT | VK_DOWN => step(enabled, Some(control), true),
            VK_LEFT | VK_UP => step(enabled, Some(control), false),
            VK_RETURN | VK_SPACE => {
                // Held keys repeat; one press is one command.
                if message.lParam.0 & (1 << 30) == 0 {
                    if ctrl && control >= notch_uia::SHELF_BASE {
                        self.open_shelf(control - notch_uia::SHELF_BASE, true);
                    } else {
                        self.press(control);
                    }
                }
                return true;
            }
            windows::Win32::UI::Input::KeyboardAndMouse::VK_DELETE
                if control >= notch_uia::SHELF_BASE =>
            {
                if let Some(index) = self.shelf.shelf.absolute(control - notch_uia::SHELF_BASE) {
                    self.change_shelf(|shelf| shelf.remove(index));
                }
                return true;
            }
            VK_ESCAPE => {
                surface.dismiss_focus();
                return true;
            }
            _ => return false,
        };
        if let Some(next) = next.filter(|&next| next != control) {
            surface.focus_control(next, None);
            surface.announce_focus();
        }
        true
    }
    /// A player can disable the focused action or replace its entire session between input events.
    /// `replaced(control)`: whether the focused `control` was replaced (re-announced) by the change.
    fn reconcile_focus(&mut self, replaced: impl Fn(usize) -> bool) {
        let enabled = self.enabled();
        let timer = self.timers.view(Instant::now());
        for notch in &mut self.surfaces {
            notch.publish_notch(
                &self.media,
                &self.audio,
                timer.as_ref(),
                self.power.text(),
                &self.shelf,
            );
            let Some(control) = notch.focused().map(|focus| focus.control) else {
                continue;
            };
            match refocus(enabled, control, replaced(control)) {
                Refocus::Keep => {}
                // The retained element belonged to the old session: tell clients about the new one.
                Refocus::Announce => notch.announce_focus(),
                Refocus::Move(next) => {
                    notch.focus_control(next, None);
                    notch.announce_focus();
                }
                Refocus::Release => notch.release_focus(),
            }
        }
    }
    fn message(&mut self, message: &MSG) -> Result<()> {
        if message.message == shelf_os::PICKED && message.hwnd.is_invalid() {
            if let Some(paths) = self.file_picker.take() {
                self.change_shelf(|shelf| shelf.add_all(&paths) > 0);
            }
            return Ok(());
        }
        if message.message == native_window::POWER_STATUS {
            // Every surface posts one; the second finds the state unchanged and starts nothing.
            if self.power.update(power::Raw::read(), Instant::now()) {
                self.open_notches();
                self.request_notches(Repaint::Full, |_| true);
            }
            return Ok(());
        }
        if message.message == native_window::REPOSITION {
            if matches!(
                message.lParam.0 as u32,
                WM_SETTINGCHANGE | WM_SYSCOLORCHANGE
            ) {
                self.palette_dirty = true;
            }
            // Every surface forwards display/DPI/settings/time changes, resume and taskbar recreation;
            // one coalesced read serves them all.
            self.request_topology();
            // A clock, time-zone or locale change, or a resume, can alter the text before the deadline.
            self.clock_due = Instant::now();
            // Countdowns keep their remaining time; only their stored wall-clock expiry moves.
            let now = Instant::now();
            if self.timers.resync(timer::wall_now(), now) {
                self.timers
                    .write_to(&mut self.store, &mut self.pending, now);
            }
            if message.wParam.0 != 0 {
                // Resume or a new taskbar: re-read what events may have missed. The bits coalesce, so
                // the post from every surface costs one re-read.
                self.media.reconnect();
                self.audio.reconnect();
            }
            return Ok(());
        }
        let slots = self.dock_slots();
        let Some(surface) = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.window.0 == message.hwnd)
        else {
            return Ok(());
        };
        match message.message {
            native_window::CLOSE_SURFACE => {
                let kind = surface.kind();
                self.close(kind);
            }
            WM_LBUTTONDOWN if matches!(surface.role, Role::Dock { .. }) => {
                // Only a pinned slot can be dragged; pins are always the first items.
                let from = surface
                    .dock_slot(message.lParam, slots)
                    .and_then(|slot| slot.checked_sub(1))
                    .filter(|&index| index < self.pins.items().len());
                self.drag = from.map(|from| Drag {
                    from,
                    x: point(message.lParam).0,
                    active: false,
                });
                if self.drag.is_some() {
                    // SAFETY: captures the pointer for this thread's own live dock window.
                    unsafe {
                        SetCapture(surface.window.0);
                    }
                }
            }
            WM_MOUSEMOVE => {
                let entered = surface.track_pointer()?;
                if let Some(drag) = &mut self.drag {
                    if message.wParam.0 & MK_LBUTTON == 0 {
                        // The release was lost (capture stolen): no drag, no click.
                        self.drag = None;
                    } else if !drag.active {
                        drag.active =
                            dock::drag_started(drag.x, point(message.lParam).0, surface.dpi);
                    }
                }
                if let Role::Dock { hover, .. } = surface.role {
                    let slot = surface.dock_slot(message.lParam, slots);
                    if entered || slot != hover {
                        surface.hover_dock(slot, true);
                    }
                } else {
                    let (x, y) = point(message.lParam);
                    let progress = surface.motion.value(Instant::now());
                    let width = surface.bounds.width;
                    let media = self.media.view().shown().is_some();
                    let over = layout::notch_hit(width, surface.dpi, progress, media, x, y);
                    surface.set_notch_hover(over);
                }
            }
            WM_MOUSELEAVE => surface.leave(),
            WM_PAINT if self.graphics.removed() => self.lose_graphics(),
            _ => {}
        }
        Ok(())
    }
    /// Re-checks the full-screen monitor when the catalog changed or the shell poll is due, then
    /// hides that monitor's surfaces and restores the others, painted current before they appear.
    fn refresh_fullscreen(&mut self, generation: u64) -> Result<()> {
        let now = Instant::now();
        let polled = self.fullscreen_due.is_some_and(|due| now >= due);
        if self.fullscreen_generation == Some(generation) && !polled {
            return Ok(());
        }
        self.fullscreen_generation = Some(generation);
        let before = self.fullscreen.clone();
        self.fullscreen.retain(|(key, device)| {
            native_window::fullscreen_monitor(*key).as_ref() == Some(device)
        });
        let foreground = dock::foreground_key();
        let candidate = foreground
            .and_then(|key| native_window::fullscreen_monitor(key).map(|device| (key, device)));
        let admitted = candidate
            .clone()
            .filter(|_| native_window::shell_fullscreen());
        let normal_monitor = admitted
            .is_none()
            .then(|| foreground.and_then(native_window::foreground_monitor))
            .flatten();
        remember_fullscreen(
            &mut self.fullscreen,
            admitted,
            normal_monitor.as_deref(),
            &self.monitors,
        );
        self.fullscreen_due =
            (candidate.is_some() || !self.fullscreen.is_empty()).then_some(now + FULLSCREEN_POLL);
        if before == self.fullscreen {
            return Ok(());
        }
        let icons = self.dock_icons();
        for surface in &mut self.surfaces {
            let visible = !self
                .fullscreen
                .iter()
                .any(|(_, device)| device == &surface.monitor);
            if visible && !surface.visible {
                match surface.kind() {
                    SurfaceKind::Notch => surface.paint_notch(
                        (&self.clock, &self.date),
                        &self.media,
                        &self.audio,
                        &self.timers,
                        &self.power,
                        &self.shelf,
                    )?,
                    SurfaceKind::Dock => surface.paint_dock(&self.dock_items, &icons)?,
                }
            }
            surface.set_visible(visible);
        }
        Ok(())
    }
    fn fullscreen_wait(&self) -> u32 {
        self.fullscreen_due
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// Milliseconds until the pending topology read, or `INFINITE` without one.
    fn topology_wait(&self) -> u32 {
        self.topology_due
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// Applies a due topology read: releases surfaces of removed or changed monitors and places the
    /// shown kinds on added or changed ones. Unchanged monitors keep their surfaces and motion state.
    /// Transient reads and failed placements retry on a bounded schedule; existing surfaces stay.
    fn refresh_topology(&mut self) -> Result<()> {
        if self.topology_due.is_none_or(|due| due > Instant::now()) {
            return Ok(());
        }
        self.topology_due = None;
        let observed = native_window::monitors().and_then(|monitors| {
            topology::settle(monitors)
                .ok_or_else(|| Error::new(E_FAIL, "Display topology is changing."))
        });
        let next = match observed {
            Ok(next) => next,
            Err(error) => return self.retry_topology(error),
        };
        let (release, place) = topology::diff(&self.monitors, &next);
        if !release.is_empty() || !place.is_empty() {
            // A monitor change can move or end a full-screen app; re-check on the next pass.
            self.fullscreen_generation = None;
        }
        self.surfaces
            .retain(|surface| !release.contains(&surface.monitor));
        self.monitors = next;
        let mut failure = None;
        for monitor in place {
            for kind in self.shown.clone() {
                if let Err(error) = self.place(&monitor, kind) {
                    // Forget the monitor so a later read places all of its surfaces again.
                    self.surfaces.retain(|s| s.monitor != monitor.device);
                    self.monitors.retain(|m| m.device != monitor.device);
                    failure = Some(error);
                    break;
                }
            }
        }
        // After placement, and contained: a settings failure closes that window and never ends the host.
        let devices: Vec<String> = self.monitors.iter().map(|m| m.device.clone()).collect();
        let broken = self.settings_window.as_mut().is_some_and(|window| {
            window
                .refresh_monitors(&devices)
                .map_err(|error| eprintln!("Settings monitors: {error}"))
                .is_err()
        });
        if broken {
            self.settings_window.take();
        }
        failure.map_or(Ok(()), |error| self.retry_topology(error))
    }
    /// Schedules a bounded re-read. Once retries run out with no surface left, `error` ends the host
    /// through the normal failure path instead of leaving it running invisibly.
    fn retry_topology(&mut self, error: Error) -> Result<()> {
        if self.topology_retries < TOPOLOGY_RETRIES {
            self.topology_retries += 1;
            self.topology_due = Some(Instant::now() + TOPOLOGY_RETRY);
        } else if self.surfaces.is_empty() {
            return Err(error);
        } else {
            // Remaining surfaces still forward the next display change, and Show commands re-read.
            eprintln!("Display topology: {error}");
        }
        Ok(())
    }
    /// Schedules a fresh topology read with a full retry budget.
    fn request_topology(&mut self) {
        self.topology_due = Some(Instant::now());
        self.topology_retries = 0;
    }
    /// One pass of state refreshes and frames; returns the clock, progress and visualizer wait and
    /// whether animation runs.
    fn update(&mut self, model: &WindowModel) -> Result<(u32, bool)> {
        self.refresh_topology()?;
        // The accent changed (the event only flagged it): rebuild like any palette change.
        let accent = self.text.take_colors_changed();
        if std::mem::take(&mut self.palette_dirty) || accent {
            let palette = crate::theme::system_palette(self.settings.theme, self.text.accent());
            if palette != self.palette {
                self.palette = palette;
                self.surfaces.clear();
                for kind in self.shown.clone() {
                    self.show(kind)?;
                }
            }
        }
        if self.text.take_changed() {
            self.rescale_text()?;
        }
        self.media
            .set_expanded(self.surfaces.iter().any(Surface::expanded));
        // One paint per visible notch when the drawn track or status changes or its pause grace
        // expires, whatever else this pass asks (`paint_requested`).
        let generation = self.media.generation();
        let status = self.media.view().status();
        let redraw =
            self.media.refresh(self.cover_pixels()) || status != self.media.view().status();
        let shown = self.media.view().shown().is_some();
        if shown {
            // Core Audio loads only once a media panel can show the volume row.
            self.audio.start();
        }
        let volume = self.audio.refresh();
        let replaced = generation != self.media.generation();
        if redraw || volume || replaced {
            self.reconcile_focus(|_| replaced);
        }
        if redraw {
            self.request_notches(Repaint::Full, |_| true);
        } else if volume && shown {
            // Only an open or opening panel draws the volume row; a collapsed pill has nothing new.
            let now = Instant::now();
            self.request_notches(Repaint::Full, |s| motion::reveal(s.motion.value(now)) > 0.0);
        }
        self.refresh_dock(model)?;
        self.refresh_shelf();
        self.refresh_fullscreen(model.generation)?;
        let wait = self
            .refresh_clock()?
            .min(self.refresh_progress()?)
            .min(self.refresh_meter()?)
            .min(self.refresh_timers())
            .min(self.refresh_power())
            .min(self.refresh_preview());
        self.paint_requested()?;
        if let Some(picker) = &mut self.picker {
            picker.synchronize(model)?;
            if picker.closed() {
                self.picker.take();
            }
        }
        if self
            .settings_window
            .as_ref()
            .is_some_and(SettingsWindow::closed)
        {
            self.settings_window.take();
        }
        Ok((wait, self.animate()?))
    }
    /// A device loss schedules a rebuild and yields `fallback`; any other failure still ends the host.
    fn survive<T>(&mut self, result: Result<T>, fallback: T) -> Result<T> {
        match result {
            Err(error) if self.graphics.lost(&error) => {
                eprintln!("Graphics device lost: {error}");
                self.lose_graphics();
                Ok(fallback)
            }
            other => other,
        }
    }
    /// Releases every old-device surface now and schedules the rebuild.
    fn lose_graphics(&mut self) {
        self.surfaces.clear();
        self.rebuild.lose(Instant::now());
    }
    /// Applies a due rebuild: a new device, then every shown kind placed and painted on it. Failures
    /// back off; once the budget is spent the error ends the host instead of leaving it invisible.
    fn refresh_graphics(&mut self) -> Result<()> {
        if !self.rebuild.start(Instant::now()) {
            return Ok(());
        }
        // Includes any surface a topology change placed on the lost device meanwhile.
        self.surfaces.clear();
        // The old device is released by this assignment, after its surfaces and before any new one.
        let rebuilt = Graphics::create().and_then(|graphics| {
            self.graphics = graphics;
            self.shown
                .clone()
                .into_iter()
                .try_for_each(|kind| self.show(kind))
        });
        if let Err(error) = &rebuilt {
            eprintln!("Graphics rebuild: {error}");
            self.surfaces.clear();
        } else {
            // Windowless meanwhile, ROQA missed display, resume and time broadcasts, and a failed
            // topology placement may have dropped a monitor: re-read both.
            self.request_topology();
            self.clock_due = Instant::now();
            // The old window set may have missed media and audio events too, as at a resume.
            self.media.reconnect();
            self.audio.reconnect();
        }
        if self.rebuild.finish(rebuilt.is_ok(), Instant::now()) {
            Ok(())
        } else {
            rebuilt
        }
    }
    /// Re-reads the Windows text size after its change event and rebuilds the notches at the new scale.
    fn rescale_text(&mut self) -> Result<()> {
        let factor = self.text.factor()?;
        if factor == self.text_factor {
            return Ok(());
        }
        self.text_factor = factor;
        if let Some(window) = &mut self.settings_window {
            window.set_factor(factor)?;
        }
        self.surfaces.retain(|s| s.kind() != SurfaceKind::Notch);
        if self.shown.contains(&SurfaceKind::Notch) {
            self.show(SurfaceKind::Notch)?;
        }
        Ok(())
    }
}

pub fn run(probe: Probe, diagnostics: Option<std::path::PathBuf>) -> Result<()> {
    // Admission first: a second launch must not open (truncate) the running instance's log.
    let controls = Controls::create();
    if controls.as_ref().err().map(|e| e.code().0) != Some(crate::error_log::ALREADY_RUNNING) {
        crate::error_log::initialize();
    }
    let controls = controls?;
    let _apartment = Apartment::enter()?;
    let _class = WindowClass::register()?;
    let mut desktop = Desktop::create(&probe)?;
    #[cfg(debug_assertions)]
    if probe == Probe::DeviceLoss {
        let removed = windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED;
        desktop.survive(Err(Error::from(removed)), ())?;
    }
    #[cfg(debug_assertions)]
    if probe == Probe::Callback {
        // SAFETY: synchronous probe targets a live window on this thread; parameters contain no pointers.
        unsafe {
            SendMessageW(
                desktop.surfaces[0].window.0,
                native_window::PANIC_PROBE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
    }
    let report = diagnostics.or_else(|| {
        desktop
            .settings
            .diagnostics
            .as_ref()
            .map(std::path::PathBuf::from)
    });
    let mut catalog = Catalog::create(report)?;
    crate::error_log::started();
    let result = event_loop(&controls, &mut desktop, &mut catalog);
    desktop.flush_store(true);
    result.and(catalog.shutdown())
}

fn event_loop(controls: &Controls, desktop: &mut Desktop, catalog: &mut Catalog) -> Result<()> {
    let events = controls.handles();
    loop {
        catalog.service()?;
        desktop.flush_store(false);
        desktop.refresh_graphics()?;
        let pass = desktop.update(catalog.model());
        let (clock_wait, animating) = desktop.survive(pass, (INFINITE, false))?;
        let timeout = catalog
            .wait_milliseconds()
            .min(clock_wait)
            .min(desktop.topology_wait())
            .min(desktop.fullscreen_wait())
            .min(desktop.store_wait())
            .min(
                desktop
                    .media
                    .deadline()
                    .map_or(INFINITE, |due| milliseconds_until(due, Instant::now())),
            )
            .min(desktop.rebuild.wait(Instant::now()));
        // Bits set after this pass's refresh whose wake was lost or not posted: run again now.
        let timeout = if desktop.media.pending() || desktop.audio.pending() {
            0
        } else {
            timeout
        };
        let wake = if animating {
            next_frame(&events, timeout)
        } else {
            // SAFETY: handles stay alive throughout the wait; the UI queue is pumped after every wake.
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    Some(&events),
                    timeout,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            }
        };
        if wake == WAIT_FAILED {
            return Err(Error::from_thread());
        }
        let index = wake.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
        if index < events.len() {
            match Command::ALL[index] {
                Command::ShowPicker => desktop.open_picker(catalog)?,
                Command::ShowSettings => desktop.open_settings()?,
                command => {
                    let shown = desktop.command(command);
                    if !desktop.survive(shown, true)? {
                        return Ok(());
                    }
                }
            }
        }
        if !drain_messages(desktop, catalog)? || desktop.shown.is_empty() {
            return Ok(());
        }
    }
}

type CompositorClockWait = unsafe extern "system" fn(u32, *const HANDLE, u32) -> u32;

/// `DCompositionWaitForCompositorClock` exists only on Windows 11. It is resolved at run time so ROQA
/// still loads elsewhere and falls back to a short message wait there.
fn compositor_clock() -> Option<CompositorClockWait> {
    static CLOCK: OnceLock<Option<CompositorClockWait>> = OnceLock::new();
    *CLOCK.get_or_init(|| {
        // SAFETY: dcomp.dll is already loaded for DCompositionCreateDevice2; the export, when present,
        // has exactly this documented signature (count, handles, timeout) -> wait result.
        unsafe {
            let module = GetModuleHandleW(w!("dcomp.dll")).ok()?;
            let export = GetProcAddress(module, s!("DCompositionWaitForCompositorClock"))?;
            Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                CompositorClockWait,
            >(export))
        }
    })
}

// https://learn.microsoft.com/windows/win32/directcomp/compositor-clock/compositor-clock
// The function reference also documents immediate occlusion status returns.
fn compositor_wake_supported(wake: u32, event_count: usize) -> bool {
    wake.wrapping_sub(WAIT_OBJECT_0.0) <= event_count as u32
        || wake == windows::Win32::Foundation::WAIT_TIMEOUT.0
}

/// While animating, sleeps until the next compositor frame or an IPC event; queued input is drained
/// right after, so it waits at most one frame. Idle returns to the plain blocking wait in `event_loop`.
fn next_frame(events: &[HANDLE], timeout: u32) -> WAIT_EVENT {
    // SAFETY: handles stay alive throughout either wait; neither wait dispatches callbacks.
    unsafe {
        if let Some(wait) = compositor_clock() {
            let wake = wait(events.len() as u32, events.as_ptr(), timeout.min(33));
            // Microsoft's compositor-clock example assigns the extra index to clock ticks.
            // Occlusion and any unrecognized NTSTATUS must take the bounded fallback below.
            if compositor_wake_supported(wake, events.len()) {
                return WAIT_EVENT(wake);
            }
        }
        MsgWaitForMultipleObjectsEx(
            Some(events),
            timeout.min(16),
            QS_ALLINPUT,
            MWMO_INPUTAVAILABLE,
        )
    }
}

fn drain_messages(desktop: &mut Desktop, catalog: &mut Catalog) -> Result<bool> {
    let mut message = MSG::default();
    let started = std::time::Instant::now();
    let mut dispatched = 0;
    // SAFETY: MSG output is valid; dispatch and all surface ownership stay on this UI thread.
    unsafe {
        while dispatched < 128
            && started.elapsed() < std::time::Duration::from_millis(4)
            && PeekMessageW(&mut message, None::<HWND>, 0, 0, PM_REMOVE).as_bool()
        {
            dispatched += 1;
            if message.message == WM_QUIT {
                if message.wParam.0 != 0 {
                    return Err(Error::new(
                        E_FAIL,
                        "Window callback failed; orderly shutdown completed.",
                    ));
                }
                return Ok(false);
            }
            if catalog.message(&message)? {
                continue;
            }
            if matches!(
                message.message,
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | dock_uia::INVOKE
            ) && desktop.dock_at(message.hwnd).is_some()
            {
                desktop.dismiss_preview();
            }
            if message.message == WM_LBUTTONUP && desktop.dock_at(message.hwnd).is_some() {
                if !desktop.end_drag(message.hwnd, message.lParam) {
                    let slot = desktop
                        .dock_at(message.hwnd)
                        .and_then(|dock| dock.dock_slot(message.lParam, desktop.dock_slots()));
                    desktop.activate_slot(slot, catalog)?;
                }
                continue;
            }
            if message.message == WM_RBUTTONUP
                && let Some(dock) = desktop.dock_at(message.hwnd)
            {
                let slot = dock.dock_slot(message.lParam, desktop.dock_slots());
                desktop.toggle_pin(slot);
                continue;
            }
            if message.message == WM_RBUTTONUP
                && desktop.remove_shelf_at(message.hwnd, message.lParam)
            {
                continue;
            }
            if message.message == dock_uia::INVOKE && desktop.dock_at(message.hwnd).is_some() {
                let (slot, version) = (message.wParam.0, message.lParam.0 as u32);
                desktop.invoke_dock(message.hwnd, slot, version, catalog)?;
                continue;
            }
            if desktop.notch_input(&message) {
                continue;
            }
            let settings = match &mut desktop.settings_window {
                Some(window) => window.message(&message)?,
                None => false,
            };
            if settings {
                let applied = desktop.settings_changed(catalog);
                desktop.survive(applied, ())?;
                continue;
            }
            if desktop
                .settings_window
                .as_ref()
                .is_some_and(|window| window.keyboard(&message))
            {
                continue;
            }
            if let Some(picker) = &mut desktop.picker {
                if picker.message(&message)? {
                    continue;
                }
                if picker.keyboard(&message) {
                    continue;
                }
            }
            desktop.message(&message)?;
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(true)
}

/// Focus changes preserve other monitors; removed monitors and replacements cannot grow the set.
fn remember_fullscreen(
    current: &mut Vec<(WindowKey, String)>,
    candidate: Option<(WindowKey, String)>,
    normal_monitor: Option<&str>,
    monitors: &[Monitor],
) {
    // A normal app covering fullscreen on this monitor restores ROQA, as the native taskbar does.
    current.retain(|(_, device)| {
        normal_monitor != Some(device.as_str()) && monitors.iter().any(|m| &m.device == device)
    });
    if let Some((key, device)) = candidate
        && monitors.iter().any(|m| m.device == device)
    {
        current.retain(|(_, old)| old != &device);
        current.push((key, device));
    }
    current.sort_by(|a, b| a.1.cmp(&b.1));
}

#[cfg(test)]
mod clock_wait_tests {
    use super::*;
    use crate::window_model::Identity;

    #[test]
    fn a_pin_edit_marks_the_store_dirty_once_and_only_when_it_changed() {
        let (mut pins, mut store, mut pending) =
            (Pins::default(), Store::default(), Pending::default());
        let now = Instant::now();
        let app = |id: &str| Identity::Application(id.into());
        for id in ["a", "b", "c"] {
            assert!(edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
                .add(app(id))
                .is_ok()));
        }
        assert_eq!(store.pins, ["app:a", "app:b", "app:c"]);
        // Nothing unwritten means no deadline; no-op edits change neither the store nor the deadline.
        let mut pending = Pending::default();
        assert_eq!(pending.decision(now), Flush::Idle);
        let before = store.clone();
        for (from, to) in [(1, 1), (0, 3), (7, 0)] {
            assert!(!edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
                .move_to(from, to)));
        }
        assert!(!edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
            .add(app("a"))
            .is_ok()));
        assert!(!edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
            .remove(&app("missing"))));
        assert_eq!(
            (store.clone(), pending.decision(now)),
            (before, Flush::Idle)
        );
        // One real reorder: one rewrite, one pending change due a coalesce delay later.
        assert!(edit_pins(&mut pins, &mut store, &mut pending, now, |p| p
            .move_to(0, 2)));
        assert_eq!(store.pins, ["app:b", "app:c", "app:a"]);
        assert_eq!(pending.decision(now), Flush::Wait(now + store::COALESCE));
    }
    #[test]
    fn a_shelf_edit_marks_the_store_dirty_once_and_only_when_it_changed() {
        let mut view = ShelfView {
            shelf: Shelf::default(),
            icons: Vec::new(),
            stale: false,
        };
        let (mut store, mut pending, now) = (Store::default(), Pending::default(), Instant::now());
        let mut add = |view: &mut ShelfView, list: &[&str]| {
            edit_shelf(view, &mut store, &mut pending, now, |s| s.add_all(list) > 0)
        };
        // Nothing valid in a drop: no change, no dirty mark, no icon request.
        assert!(!add(&mut view, &["relative.txt", r"\\.\PhysicalDrive0"]));
        assert!(!view.stale);
        assert!(add(&mut view, &[r"C:\a.txt", r"C:\b.txt"]));
        assert!(view.stale);
        // The same files again are duplicates: nothing to write.
        assert!(!add(&mut view, &[r"C:\A.TXT"]));
        assert_eq!(store.shelf, [r"C:\a.txt", r"C:\b.txt"]);
        assert_eq!(pending.decision(now), Flush::Wait(now + store::COALESCE));
        // A removal is one more change; marking a target missing is not persisted, so not an edit here.
        let mut pending = Pending::default();
        assert!(edit_shelf(&mut view, &mut store, &mut pending, now, |s| s.remove(0)));
        assert_eq!(store.shelf, [r"C:\b.txt"]);
        assert_eq!(pending.decision(now), Flush::Wait(now + store::COALESCE));
    }

    #[test]
    fn the_meter_runs_only_open_playing_and_metered_and_never_above_15_hz() {
        let now = Instant::now();
        // Collapsed, hidden and full-screen-suppressed notches are not open (`Surface::expanded`).
        for (open, playing, metered) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            assert_eq!(meter_due(open, playing, metered, None, now), None);
            assert_eq!(meter_due(open, playing, metered, Some(now), now), None);
        }
        assert_eq!(meter_due(true, true, true, None, now), Some(now));
        // Wakes early, on time, late and after a stop and restart: samples stay a period apart.
        let mut sampled = None;
        let mut samples = Vec::new();
        for (wake, open) in [
            (0, true),
            (10, true),
            (67, true),
            (300, true),
            (310, false),
            (320, true),
            (366, true),
            (367, true),
            (1000, true),
            // Resumed an hour later: one sample, not a catch-up burst, then the period again.
            (3_601_000, true),
            (3_601_001, true),
            (3_601_066, true),
            (3_601_067, true),
        ] {
            let at = now + Duration::from_millis(wake);
            if meter_due(open, true, true, sampled, at).is_some_and(|due| due <= at) {
                sampled = Some(at);
                samples.push(wake);
            }
        }
        assert_eq!(samples, [0, 67, 300, 367, 1000, 3_601_000, 3_601_067]);
        assert!(samples.windows(2).all(|pair| pair[1] - pair[0] >= 67));
    }

    #[test]
    fn a_pass_paints_each_notch_once_by_its_strongest_request() {
        // A tick, a meter sample and a minute in one pass: the full paint covers the bars.
        let mut asked = Repaint::default();
        assert_eq!(asked, Repaint::Nothing);
        for request in [Repaint::Meter, Repaint::Full, Repaint::Meter] {
            asked = asked.max(request);
        }
        assert_eq!(asked, Repaint::Full);
        // A bars-only pass repaints only the meter slot, and taking the request resets it.
        let mut asked = Repaint::Nothing.max(Repaint::Meter);
        assert_eq!(std::mem::take(&mut asked), Repaint::Meter);
        assert_eq!(asked, Repaint::Nothing);
    }

    #[test]
    fn focus_is_reannounced_after_a_player_swap_and_moves_only_off_a_disabled_control() {
        let all = [true; 4];
        assert_eq!(refocus(all, 1, false), Refocus::Keep);
        // The session was replaced and the focused index still exists: clients must hear of it.
        assert_eq!(refocus(all, 1, true), Refocus::Announce);
        let no_next = [true, true, false, true];
        assert_eq!(refocus(no_next, 2, false), Refocus::Move(3));
        assert_eq!(refocus(no_next, 2, true), Refocus::Move(3));
        assert_eq!(refocus([false; 4], 1, true), Refocus::Release);
    }

    #[test]
    fn shelf_focus_enters_without_media_and_recovers_when_the_last_page_shrinks() {
        let shelf_only = [false, false, false, false, true, true, true];
        assert_eq!(step(shelf_only, None, true), Some(4));
        assert_eq!(step(shelf_only, Some(6), true), Some(4));
        assert_eq!(step(shelf_only, Some(4), false), Some(6));
        let add_only = [false, false, false, false, true, false, false];
        assert_eq!(refocus(add_only, 6, true), Refocus::Move(4));
    }

    #[test]
    fn keyboard_focus_steps_over_disabled_controls_and_wraps() {
        let all = [true; 3];
        assert_eq!(step(all, None, true), Some(0));
        assert_eq!(step(all, None, false), Some(2));
        assert_eq!(step(all, Some(2), true), Some(0));
        assert_eq!(step(all, Some(0), false), Some(2));
        // No previous track: focus enters on play/pause, and moving skips the disabled control.
        let first = [false, true, true];
        assert_eq!(step(first, None, true), Some(1));
        assert_eq!(step(first, Some(2), true), Some(1));
        assert_eq!(step(first, Some(1), false), Some(2));
        // A lone enabled control keeps focus; with none, there is nothing to focus.
        assert_eq!(step([false, true, false], Some(1), true), Some(1));
        assert_eq!(step([false; 3], None, true), None);
        assert_eq!(step([false; 3], Some(1), false), None);
    }

    #[test]
    fn fullscreen_focus_changes_preserve_monitors_and_state_is_bounded() {
        let monitors: Vec<_> = ["A", "B"]
            .into_iter()
            .map(|device| Monitor {
                device: device.into(),
                work: Bounds {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                dpi: 96,
            })
            .collect();
        let candidate = |handle, device: &str| {
            (
                WindowKey {
                    handle,
                    process: 10,
                },
                device.to_owned(),
            )
        };
        let mut current = vec![];
        remember_fullscreen(&mut current, Some(candidate(1, "A")), None, &monitors);
        remember_fullscreen(&mut current, None, None, &monitors);
        assert_eq!(current, [candidate(1, "A")]);
        remember_fullscreen(&mut current, Some(candidate(2, "B")), None, &monitors);
        assert_eq!(current, [candidate(1, "A"), candidate(2, "B")]);
        remember_fullscreen(&mut current, Some(candidate(3, "A")), None, &monitors);
        assert_eq!(current, [candidate(3, "A"), candidate(2, "B")]);
        remember_fullscreen(&mut current, Some(candidate(4, "C")), None, &monitors[1..]);
        assert_eq!(current, [candidate(2, "B")]);
        // ROQA foreground has no foreign monitor and retains the entry.
        remember_fullscreen(&mut current, None, None, &monitors);
        assert_eq!(current, [candidate(2, "B")]);
        // Normal foreground on the other monitor retains fullscreen B.
        remember_fullscreen(&mut current, None, Some("A"), &monitors);
        assert_eq!(current, [candidate(2, "B")]);
        // Alt-Tab to a normal app on B releases that monitor's suppression.
        remember_fullscreen(&mut current, None, Some("B"), &monitors);
        assert!(current.is_empty());
        remember_fullscreen(&mut current, Some(candidate(2, "B")), None, &monitors);
        remember_fullscreen(&mut current, None, None, &[]);
        assert!(current.is_empty());
    }

    #[test]
    fn graphics_rebuilds_back_off_to_a_cap() {
        let delays: Vec<u128> = (0..8).map(|n| rebuild_delay(n).as_millis()).collect();
        assert_eq!(delays, [250, 500, 1000, 2000, 4000, 8000, 8000, 8000]);
        assert_eq!(rebuild_delay(u32::MAX).as_millis(), 8000);
    }
    #[test]
    fn rebuilds_keep_one_pending_spend_a_budget_and_refresh_once_settled() {
        let start = Instant::now();
        let mut rebuild = Rebuild::new(start);
        assert!(!rebuild.start(start));
        rebuild.lose(start);
        rebuild.lose(start + Duration::from_millis(200));
        assert_eq!(rebuild.due, Some(start + GRAPHICS_RETRY));
        assert!(!rebuild.start(start + Duration::from_millis(249)));
        // Every failure backs off further until the budget is spent.
        let mut now = start + GRAPHICS_RETRY;
        for attempt in 1..GRAPHICS_ATTEMPTS {
            assert!(rebuild.start(now));
            assert!(rebuild.finish(false, now));
            assert_eq!(rebuild.due, Some(now + rebuild_delay(attempt)));
            now += rebuild_delay(attempt);
        }
        assert!(rebuild.start(now));
        assert!(!rebuild.finish(false, now));
        // Success clears the schedule; a loss soon after keeps backing off, a settled one starts over.
        let mut rebuild = Rebuild::new(start);
        rebuild.lose(start);
        assert!(rebuild.start(start + GRAPHICS_RETRY));
        assert!(rebuild.finish(true, start + GRAPHICS_RETRY));
        assert_eq!(rebuild.wait(start), INFINITE);
        let soon = start + Duration::from_secs(1);
        rebuild.lose(soon);
        assert_eq!(rebuild.due, Some(soon + rebuild_delay(1)));
        let mut rebuild = Rebuild::new(start);
        rebuild.lose(start);
        assert!(rebuild.start(start + GRAPHICS_RETRY));
        assert!(rebuild.finish(true, start + GRAPHICS_RETRY));
        let later = start + GRAPHICS_RETRY + GRAPHICS_SETTLED;
        rebuild.lose(later);
        assert_eq!(rebuild.due, Some(later + GRAPHICS_RETRY));
    }
    #[test]
    fn accepts_events_clock_and_timeout_but_falls_back_on_occlusion_or_failure() {
        for index in 0..=4 {
            assert!(compositor_wake_supported(WAIT_OBJECT_0.0 + index, 4));
        }
        assert!(compositor_wake_supported(
            windows::Win32::Foundation::WAIT_TIMEOUT.0,
            4
        ));
        for status in [WAIT_FAILED.0, 0xC01E0006, 0x80, 5] {
            assert!(!compositor_wake_supported(status, 4));
        }
    }
}
