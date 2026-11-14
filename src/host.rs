use crate::{
    audio::{self, Audio},
    catalog_timing::milliseconds_until,
    clock,
    dock::{self, Button, Buttons, DockItem, Hit},
    dock_menu::{self, Event, Item},
    dock_tips::Tips,
    edge::{self, Edges},
    eyes::{self, Eyes},
    graphics::{
        COVER_DIPS, Canvas, DockScene, DockSlot, Fx, Graphics, Grip, NotchScene, RailScene,
        ShelfSlot,
    },
    icons::{self, IconPixels, IconService},
    ipc::{Command, Controls},
    layout::{self, Bounds, DockStyle, SurfaceKind, placement_styled},
    limits::{self, Limits},
    limits_os,
    media::{self, Media, Progress, Track},
    motion::{self, Tween},
    native_window::{self, NativeWindow, WindowClass},
    picker::Picker,
    pins::Pins,
    pomo,
    power::{self, Power},
    prayer_os, prayer_tab, preview, privacy,
    settings::{self, Settings},
    settings_window::{SettingsWindow, SoundAction},
    shelf::{self, Check, Shelf},
    shelf_os::{self, DropEvent, Drops},
    sort,
    store::{self, Flush, Pending, Store},
    tabs::{Tab, Tabs},
    text_settings::TextSettings,
    theme::Palette,
    timer::{self, Timers},
    topology::{self, Monitor},
    water,
    weather::{self, Feed},
    weather_os,
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
    Foundation::{E_FAIL, HANDLE, HWND, LPARAM, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0},
    System::Diagnostics::Debug::MessageBeep,
    System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    System::Ole::{OleInitialize, OleUninitialize},
    System::Threading::INFINITE,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{
            GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            VK_CONTROL,
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

/// What the notches show besides the media session (features tabs-eyes): the tabs, and the mascot.
struct Pages {
    tabs: Tabs,
    /// `None` with "Mascot eyes" off: no state, no deadline and no drawing.
    eyes: Option<Eyes>,
    /// The consent-store watch (features B5): `None` with "Privacy indicator" off, which closes its handle.
    watch: Option<privacy::Watch>,
    /// The mic and camera dots: what is in use and their fade.
    dots: privacy::Dots,
    /// The last pointer input on a notch, and the local hour at the last clock tick: what the
    /// mascot's sleepy mood reads.
    last_input: Instant,
    hour: u8,
    /// The Weather page's text, rebuilt by `refresh_weather` only while that page shows, and the feed
    /// revision and minute it was built for.
    weather: weather::View,
    weather_key: Option<(u32, String)>,
    /// The Prayer page's text, rebuilt by `refresh_prayer` only while that page shows, and what it was built for.
    prayer: prayer_tab::View,
    prayer_key: Option<(String, u32, crate::prayer::Method, [i8; 6])>,
    /// The Limits page, rebuilt by `refresh_limits` only while that page shows, and what it was built for.
    limits: limits::View,
    limits_key: Option<(u32, bool, String)>,
    widgets: crate::widgets::WidgetConfig,
    /// The Pomodoro's phase rules; its running phase is a tagged timer in `Desktop::timers`.
    pomo: pomo::Pomo,
}
impl Pages {
    fn new(settings: &Settings, last: Tab) -> Self {
        let mut pages = Self {
            tabs: Tabs::new(settings.tabs, last),
            eyes: settings.eyes.then(|| {
                // Start-up is a wake: the eyes open once, then give way to the time.
                let mut eyes = Eyes::default();
                eyes.wake(Instant::now());
                eyes
            }),
            watch: None,
            dots: privacy::Dots::default(),
            last_input: Instant::now(),
            hour: 12,
            weather: weather::View::default(),
            weather_key: None,
            prayer: prayer_tab::View::default(),
            prayer_key: None,
            limits: limits::View::default(),
            limits_key: None,
            widgets: settings.widgets,
            pomo: pomo::Pomo::default(),
        };
        pages.set_privacy(settings.privacy, Instant::now());
        pages
    }
    /// The Timer page's content: the soonest countdown, named for its Pomodoro phase, or a paused phase
    /// when nothing runs.
    fn timer_view(&self, timers: &Timers, now: Instant) -> Option<timer::View> {
        let view = timers.view(now);
        if let Some(left) = self.pomo.left
            && view.as_ref().is_none_or(|view| view.done)
        {
            return Some(timer::View::paused(
                left,
                self.pomo.total,
                self.pomo.title(true),
                self.pomo.cycle(),
            ));
        }
        view.map(|mut view| {
            if view.tagged {
                view.label = self.pomo.title(false);
                view.total = self.pomo.total;
                view.cycle = Some(self.pomo.cycle());
            }
            view
        })
    }
    /// Starts or stops the registry watch with the setting and reads the devices once; true when the dots changed.
    fn set_privacy(&mut self, on: bool, now: Instant) -> bool {
        self.watch = on.then(privacy::Watch::new).flatten();
        let used = self.watch.as_ref().map(privacy::Watch::read);
        self.dots.set(used.unwrap_or_default(), now)
    }
    /// The track the notch shows: none with the Media tab off, so a session costs nothing then.
    fn track<'a>(&self, media: &'a Media) -> Option<&'a Track> {
        self.tabs
            .on(Tab::Media)
            .then(|| media.view().shown())
            .flatten()
    }
    /// The page the open panel shows. With every tab off the panel is the plain clock (the Media page).
    fn page(&self, media_live: bool) -> Tab {
        self.tabs.active(media_live).unwrap_or(Tab::Media)
    }
    /// The handle's layout; `None` with fewer than two tabs.
    fn rail(&self, media_live: bool) -> Option<layout::Rail> {
        let visible = self.tabs.visible();
        let active = visible.position(self.page(media_live))?;
        visible.handle().then_some(layout::Rail {
            count: visible.as_slice().len(),
            active,
        })
    }
    fn touch(&mut self) {
        self.last_input = Instant::now();
    }
}

struct Surface {
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
    /// A dock's slide in from below its bottom edge: 0 hidden, 1 settled. Unused by the notch.
    slide: Tween,
    /// A dock sliding down before it hides: it stays visible until `animate` sees the slide end.
    leaving: bool,
    /// Notch: the progress row last drawn, if any.
    played: Option<Progress>,
    /// Notch: the visualizer bar heights last drawn.
    bars: [i32; audio::BARS],
    /// Notch: what this pass has asked to repaint (`Desktop::paint_requested`).
    repaint: Repaint,
    /// Notch: the tab cell under the pointer (a position in the visible tabs), for its hover wash.
    tab_hover: Option<usize>,
    /// Notch: the pointer on the volume bar (hover, and a captured drag).
    grip: Grip,
    /// Notch: the volume bar's drawn fill (percent), easing toward the real level; a drag sets it at once.
    volume_fill: Tween,
    /// Notch: the page last drawn (its tab index), the slide and fade of a switch to the current one (1 at
    /// rest) and its direction, and the notice's entry with whether one showed.
    page_at: usize,
    /// The page a switch left, while its slide runs, and the last notice text (it fades out after it ends).
    page_from: Option<Tab>,
    last_notice: String,
    page_dir: f32,
    page_fx: Tween,
    notice_fx: Tween,
    notice_on: bool,
    /// Dock: a hover change has not been painted yet (`paint_dock` starts its clocks).
    hover_unpainted: bool,
    /// Dock: floating or attached to the bottom edge, and which taskbar buttons it shows.
    dock_style: DockStyle,
    dock_buttons: Buttons,
    /// Notch: shown over a full-screen app for a prayer alert only; hidden again once it has retracted.
    overlay: bool,
}
impl Surface {
    /// Ends a volume-bar drag: the thumb shrinks and the capture goes.
    fn end_grip(&mut self) {
        if std::mem::take(&mut self.grip.drag) {
            // SAFETY: releases this thread's own capture; harmless if it is already gone.
            let _ = unsafe { ReleaseCapture() };
            self.request(Repaint::Full);
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        if matches!(self.role, Role::Notch { .. }) {
            shelf_os::revoke(self.window.0);
        }
    }
}
/// State only one surface kind needs.
#[allow(clippy::large_enum_variant)] // one value per surface, never moved in bulk
enum Role {
    /// Pointer inside the hot band (not its shadow), the instant hover intent opens the notch, and the
    /// window region last applied (it follows the drawn shape frame by frame).
    Notch {
        shadow: NotchShadow,
        engaged: bool,
        intent: Option<Instant>,
        area: Vec<Bounds>,
    },
    /// Hovered cell, per-cell hover progress and the tooltips of its buttons (none if the control could not
    /// be made).
    Dock {
        hover: Option<usize>,
        slots: Vec<Tween>,
        tips: Option<Tips>,
    },
}
impl Surface {
    #[allow(clippy::too_many_arguments)]
    fn create(
        graphics: &Graphics,
        kind: SurfaceKind,
        monitor: &Monitor,
        dpi: u32,
        slots: usize,
        drops: &Drops,
        palette: Palette,
        dock_style: DockStyle,
        dock_buttons: Buttons,
    ) -> Result<Self> {
        let bounds = placement_styled(monitor.work, dpi, kind, slots, dock_style, dock_buttons);
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
                let tips = Tips::create(window.0)
                    .inspect(|tips| tips.place(dock_buttons, bounds.width, bounds.height, dpi))
                    .inspect_err(|error| eprintln!("Dock tooltips unavailable: {error}"))
                    .ok();
                Role::Dock {
                    hover: None,
                    slots: vec![Tween::rest(0.0); dock::cell_count(dock_buttons, slots)],
                    tips,
                }
            }
            // The first paint, which always precedes showing the window, applies the input region
            // of the plain or media pill.
            Some(shadow) => Role::Notch {
                shadow,
                engaged: false,
                intent: None,
                area: Vec::new(),
            },
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
            slide: Tween::rest(0.0),
            leaving: false,
            played: None,
            bars: [0; audio::BARS],
            repaint: Repaint::Nothing,
            tab_hover: None,
            grip: Grip::default(),
            volume_fill: Tween::rest(0.0),
            page_at: 0,
            page_from: None,
            last_notice: String::new(),
            page_dir: 1.0,
            page_fx: Tween::rest(1.0),
            notice_fx: Tween::rest(1.0),
            notice_on: false,
            hover_unpainted: false,
            overlay: false,
            dock_style,
            dock_buttons,
        })
    }
    /// Closed and not moving: nothing of the panel is drawn.
    fn collapsed(&self, now: Instant) -> bool {
        self.motion.target() <= 0.0 && !self.motion.running(now)
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
        if matches!(self.role, Role::Dock { .. }) && self.visible {
            // A visible dock slides down before its window hides (`animate` finishes it); showing again
            // while it leaves just turns the slide around.
            if !visible && !self.leaving {
                (self.leaving, self.live) = (true, true);
                self.slide.retarget(0.0, dock::SLIDE_TIME, motion::SETTLE);
            } else if visible && self.leaving {
                (self.leaving, self.live) = (false, true);
                self.slide.retarget(1.0, dock::SLIDE_TIME, motion::SETTLE);
            }
            if self.leaving || visible {
                return;
            }
        }
        self.apply_visible(visible);
    }
    fn apply_visible(&mut self, visible: bool) {
        self.leaving = false;
        if visible == self.visible {
            return;
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
        if visible && matches!(self.role, Role::Dock { .. }) {
            // The dock was painted below its window (slide 0): it rises over `SLIDE_TIME`.
            self.slide.retarget(1.0, dock::SLIDE_TIME, motion::SETTLE);
            self.live = true;
        }
        if !visible {
            // A hidden window gets no leave notification and needs no frames: settle at rest now.
            (self.hovered, self.live, self.motion) = (false, false, Tween::rest(0.0));
            self.slide = Tween::rest(0.0);
            self.tab_hover = None;
            match &mut self.role {
                Role::Notch {
                    engaged, intent, ..
                } => (*engaged, *intent) = (false, None),
                Role::Dock { hover, slots, .. } => {
                    *hover = None;
                    slots.fill(Tween::rest(0.0));
                }
            }
        }
    }
    /// Ends a timer completion's expansion: closes a notch nobody is pointing at. A pointer keeps it open,
    /// and the usual leave closes it.
    fn close_alert(&mut self) {
        if self.expanded() && matches!(self.role, Role::Notch { engaged: false, .. }) {
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
                self.tab_hover = None;
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
    /// Draws one frame.
    // shortcut: eight arguments, each a distinct borrowed part of `Desktop`; bundle them into a view
    // struct if a ninth is needed.
    #[allow(clippy::too_many_arguments)]
    fn paint_notch(
        &mut self,
        (time, date): (&str, &str),
        media: &Media,
        audio: &Audio,
        timers: &Timers,
        power: &Power,
        shelf: &ShelfView,
        pages: &Pages,
    ) -> Result<()> {
        let track = pages.track(media);
        let page = pages.page(track.is_some());
        layout::set_page_height(if page == Tab::Limits {
            layout::limits_height(pages.limits.rows.len())
        } else if page == Tab::Widgets {
            140
        } else {
            0
        });
        let now = Instant::now();
        let progress = self.motion.value(now);
        // A switched-off Timer tab draws and ticks nothing.
        let timer = pages
            .tabs
            .on(Tab::Timer)
            .then(|| pages.timer_view(timers, now))
            .flatten();
        let notice = power.text();
        // A switch of page while the panel is open slides and fades the new one in; a notice eases in.
        if let Some(dir) = motion::page_switch(self.page_at, page.index(), progress > 0.9)
            && self.visible
        {
            self.page_dir = dir;
            self.page_from = Tab::ALL.get(self.page_at).copied();
            self.page_fx = Tween::rest(0.0);
            self.page_fx.retarget(1.0, motion::PAGE_TIME, motion::SLIDE);
            self.live = true;
        }
        self.page_at = page.index();
        if let Some(text) = notice
            && self.last_notice != text
        {
            self.last_notice = text.to_owned();
        }
        if notice.is_some() != self.notice_on {
            self.notice_on = !self.notice_on;
            if self.visible {
                // Fades in, and out again when it ends (the text stays in `last_notice` for that).
                let (to, from) = if self.notice_on {
                    (1.0, 0.0)
                } else {
                    (0.0, 1.0)
                };
                self.notice_fx = Tween::rest(from);
                self.notice_fx
                    .retarget(to, motion::PAGE_TIME, motion::SLIDE);
                self.live = true;
            } else {
                self.notice_fx = Tween::rest(if self.notice_on { 1.0 } else { 0.0 });
            }
        }
        if !self.page_fx.running(now) {
            self.page_from = None;
        }
        let fading_out = notice.is_none() && self.notice_fx.running(now);
        let notice = notice.or(fading_out.then_some(self.last_notice.as_str()));
        // The plain panel makes room for the notice's row as it eases in, and gives it back as it leaves.
        layout::set_notice_shown(if notice.is_some() {
            self.notice_fx.value(now)
        } else {
            0.0
        });
        let level = audio.level().map_or(0.0, |level| f32::from(level.percent));
        if self.grip.drag {
            self.volume_fill = Tween::rest(level); // the thumb follows the pointer without lag
        } else {
            self.volume_fill
                .retarget(level, VOLUME_EASE, motion::SETTLE);
        }
        let volume_fill = self.volume_fill.value(now);
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
        let Role::Notch { shadow, area, .. } = &mut self.role else {
            return Ok(());
        };
        let visible = pages.tabs.visible();
        let rail = pages.rail(track.is_some());
        // Input and drawing use the same frame progress and shape, without a speculative growth lead.
        let rows = layout::notch_rows(self.bounds, self.dpi, progress, track.is_some(), rail);
        if *area != rows {
            self.window.set_notch_region(&rows)?;
            *area = rows;
        }
        let shelf_on = pages.tabs.on(Tab::Shelf);
        let mut slots: Vec<ShelfSlot> = shelf
            .shelf
            .visible()
            .iter()
            .filter(|_| shelf_on)
            .enumerate()
            .map(|(index, entry)| ShelfSlot {
                icon: shelf.icons.get(index).and_then(|icon| icon.as_deref()),
                name: &entry.name,
                missing: entry.check == Check::Missing,
                more: false,
                add: false,
            })
            .collect();
        if shelf_on && (shelf.shelf.overflow() || shelf.shelf.is_empty()) {
            slots.push(ShelfSlot {
                icon: None,
                name: "",
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
            played: self.played,
            volume: audio.level(),
            bars: self.bars,
            timer: timer.as_ref(),
            notice,
            shelf: &slots,
            shelf_undo: shelf.removed.is_some(),
            tab: page,
            widgets: pages.widgets,
            rail: rail.map(|layout| RailScene {
                layout,
                hover: self.tab_hover,
                tabs: visible.as_slice(),
            }),
            eyes: pages.eyes.map(|eyes| eyes.scene()),
            privacy: pages.dots.alpha(),
            grip: self.grip,
            volume_fill,
            weather: [Some(page), self.page_from]
                .contains(&Some(Tab::Weather))
                .then_some(&pages.weather),
            prayer: [Some(page), self.page_from]
                .contains(&Some(Tab::Prayer))
                .then_some(&pages.prayer),
            limits: [Some(page), self.page_from]
                .contains(&Some(Tab::Limits))
                .then_some(&pages.limits),
            leaving: self.page_from,
            fx: Fx {
                page: self.page_fx.value(now),
                dir: self.page_dir,
                notice: self.notice_fx.value(now),
            },
        };
        shadow.paint(&scene, self.window.0)?;
        let status = track.is_none().then(|| media.view().status()).flatten();
        self.canvas
            .paint_notch(&scene, status.filter(|_| pages.tabs.on(Tab::Media)))
    }
    /// Repaints only the visualizer slot with the new bar heights.
    fn paint_meter(&mut self, peaks: [f32; audio::BARS]) -> Result<()> {
        self.bars = layout::meter_heights(peaks, self.dpi);
        self.canvas.paint_meter(self.dpi, self.bars)
    }
    /// Pointer entered or left the notch. Entering swells the pill at once and opens it only after
    /// `INTENT_TIME` (a pointer passing along the screen edge never pops the panel open); leaving
    /// closes from wherever the motion is. The HWND itself never moves.
    fn set_notch_hover(&mut self, over: bool) {
        let Role::Notch {
            engaged, intent, ..
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
            self.motion.retarget(0.0, motion::CLOSE_TIME, motion::CLOSE);
        } else if motion::click_to_open() {
            // Click mode: the pill only swells (and never shrinks an open panel); a click opens it.
            if swells(self.motion.target()) {
                self.motion
                    .retarget(NOTCH_SWELL, motion::INTENT_TIME, motion::SETTLE);
            }
        } else if self.motion.value(Instant::now()) > NOTCH_SWELL {
            // Re-entered while still closing: reopen directly.
            open_notch(intent, &mut self.motion);
        } else {
            *intent = Some(Instant::now() + motion::time(motion::INTENT_TIME));
            self.motion
                .retarget(NOTCH_SWELL, motion::INTENT_TIME, motion::SETTLE);
        }
        self.live = true;
    }
    /// Highlights the hovered slot (or none) and the bar fill while the pointer is inside.
    fn hover_dock(&mut self, slot: Option<usize>, inside: bool) {
        let Role::Dock { hover, slots, .. } = &mut self.role else {
            return;
        };
        let ease = |tween: &mut Tween, on: bool, curve| {
            let (target, duration) = if on {
                (
                    1.0,
                    if curve == motion::POP {
                        motion::POP_TIME
                    } else {
                        motion::HOVER_IN_TIME
                    },
                )
            } else {
                (0.0, motion::HOVER_OUT_TIME)
            };
            tween.retarget(target, duration, curve);
        };
        ease(&mut self.motion, inside, motion::SETTLE);
        for (index, tween) in slots.iter_mut().enumerate() {
            ease(tween, slot == Some(index), motion::POP);
        }
        *hover = slot;
        self.hover_unpainted = true;
        self.live = true;
    }
    fn paint_dock(&mut self, items: &[DockItem], icons: &[Option<Arc<IconPixels>>]) -> Result<()> {
        let now = Instant::now();
        // A hover began at a mouse message; its first frame comes later. Start the clocks here so that frame
        // shows the rest value and the motion keeps its whole duration.
        if self.hover_unpainted {
            self.hover_unpainted = false;
            self.motion.restart(now);
            if let Role::Dock { slots, .. } = &mut self.role {
                slots.iter_mut().for_each(|tween| tween.restart(now));
            }
        }
        let Role::Dock { slots: tweens, .. } = &self.role else {
            return Ok(());
        };
        let count = items.len() + 1;
        let hover = |hit| {
            dock::cell_index(self.dock_buttons, count, hit)
                .and_then(|cell| tweens.get(cell))
                .map_or(0.0, |t| t.value(now))
        };
        let mut slots = vec![DockSlot {
            icon: None,
            active: false,
            picker: true,
            badge: dock::Badge::None,
            hover: hover(Hit::Slot(0)),
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
                    badge: item.badge,
                    hover: hover(Hit::Slot(index + 1)),
                }),
        );
        // Only the window rows from the content's top down are visible and take input: a region clips
        // the rest.
        let slide = dock::slide_offset(self.slide.value(now), self.bounds.height);
        let top = slide.min(self.bounds.height - 1);
        self.window.set_notch_region(&[Bounds {
            x: 0,
            y: top,
            width: self.bounds.width,
            height: self.bounds.height - top,
        }])?;
        self.canvas.paint_dock(&DockScene {
            slots,
            layout: self.dock_buttons,
            buttons: Button::ALL
                .into_iter()
                .filter(|button| self.dock_buttons.on(*button))
                .map(|button| (button, hover(Hit::Button(button))))
                .collect(),
            inside: self.motion.value(now),
            dpi: self.dpi,
            attached: self.dock_style == DockStyle::Attached,
            slide,
        })
    }
    /// The slot or button under the pointer.
    fn dock_hit(&self, lparam: LPARAM, slots: usize) -> Option<Hit> {
        let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
        dock::hit_at(self.dock_buttons, x, self.bounds.width, self.dpi, slots)
    }
    /// The pointer's cell index (the hover tween and UIA child), if over a slot or button.
    fn dock_cell(&self, lparam: LPARAM, slots: usize) -> Option<usize> {
        self.dock_hit(lparam, slots)
            .and_then(|hit| dock::cell_index(self.dock_buttons, slots, hit))
    }
}

/// Hover-intent swell: the pill grows ~11 x 3 DIP (the media pill ~7 x 6) to acknowledge the
/// pointer before opening.
const NOTCH_SWELL: f32 = 0.07;
/// In click mode the pointer entering swells the pill only when it is closed (its target is below the swell).
fn swells(target: f32) -> bool {
    target < NOTCH_SWELL
}
/// Opens the notch fully, ending any pending hover intent.
fn open_notch(intent: &mut Option<Instant>, tween: &mut Tween) {
    *intent = None;
    tween.retarget(1.0, motion::OPEN_TIME, motion::OPEN);
}
/// Client coordinates of a mouse message.
fn point(lparam: LPARAM) -> (i32, i32) {
    let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
    let y = i32::from((lparam.0 >> 16) as u16 as i16);
    (x, y)
}
/// Brings one of our own popups (a menu) to the foreground so it can end when it loses it; Windows may
/// refuse, as for any app not in front.
fn take_foreground(notch: HWND) -> bool {
    // SAFETY: scalar foreground request for our own live notch HWND; never forced.
    unsafe { SetForegroundWindow(notch).as_bool() || GetForegroundWindow() == notch }
}
/// The notch volume bar eases to a new level over this long.
const VOLUME_EASE: Duration = Duration::from_millis(120);
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
    removed: Option<(usize, shelf::Entry)>,
    shelf: Shelf,
    icons: Vec<Option<Arc<IconPixels>>>,
    stale: bool,
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

/// The DPI icons are extracted for: the densest dock's, else the densest notch's (a shelf needs icons with
/// the dock off), else none.
fn icon_dpi(dock: Option<u32>, notch: Option<u32>) -> Option<u32> {
    dock.or(notch)
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
    /// The display language reads right to left: the dock is mirrored (`dock::Buttons::rtl`).
    rtl: bool,
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
    /// The dock's right-click menu while open; nothing exists, and nothing is polled, otherwise.
    menu: Option<dock_menu::Menu>,
    /// Wheel movement left over after the last whole notch on the volume bar.
    volume_wheel: i32,
    /// The volume and brightness sliders on every monitor's edges, rebuilt to match the monitors and
    /// settings whenever either changes (`edges_dirty`).
    edges: Edges,
    edges_dirty: bool,
    /// The tabs and the mascot (features tabs-eyes).
    pages: Pages,
    /// The weather place, report and back-off, the worker channel and the locale's region (features W1).
    weather: Feed,
    weather_io: weather_os::Io,
    region: String,
    /// The plan limits, their worker channel and whether the page was open at the last pass (features U1).
    limits: Limits,
    limits_io: limits_os::Io,
    limits_open: bool,
    /// The prayer reminders' schedule and sound (features P1).
    prayer: PrayerRun,
    /// When the next water reminder is due; `None` until `refresh_water` computes it.
    water_due: Option<Instant>,
}
/// The reminder schedule for the day, the last pass it was checked at, and the MCI sound while one plays.
#[derive(Default)]
struct PrayerRun {
    /// The facts `events` were computed for: date, UTC offset, place, method and heads-up minutes.
    key: Option<(
        prayer_tab::Local,
        i32,
        i32,
        crate::prayer::Method,
        u32,
        [i8; 6],
    )>,
    events: Vec<prayer_tab::Event>,
    last: Option<((i32, u32, u32), i32)>,
    player: prayer_os::Player,
    sound_end: Option<Instant>,
    /// The reminder's notice ends here: the mascot's adhan mood lasts as long.
    adhan_until: Option<Instant>,
}
/// How long a prayer reminder holds the notch open.
const PRAYER_NOTICE: Duration = Duration::from_secs(8);
/// The system sound for a finished timer or a due reminder (callers check the sound settings).
fn chime() {
    // SAFETY: plays the user's own default sound asynchronously; no pointers.
    unsafe {
        let _ = MessageBeep(MB_ICONASTERISK);
    }
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
        if let Some(failure) = settings.apply_globals() {
            eprintln!("Settings: {failure}");
        }
        let mut shelf = Shelf::from_store(&store);
        if shelf.set_cap(settings.shelf_cap) {
            shelf.write_to(&mut store); // the cut list leaves the file with its next write
        }
        let pages = Pages::new(&settings, store.last_tab);
        let weather = Feed::new(store.place.clone(), store.report.clone(), settings.detect);
        let timers = Timers::restore(&store.timers, timer::wall_now(), now);
        // The section now mirrors what is running; stale entries leave the file with its next write.
        timers.write_to(&mut store, &mut Pending::default(), now);
        let effective_accent = if settings.match_accent {
            crate::theme::windows_accent_color().or_else(|| Some(text.accent()))
        } else {
            None
        };
        let mut desktop = Self {
            surfaces: Vec::new(),
            monitors,
            shown: Vec::new(),
            topology_due: None,
            topology_retries: 0,
            fullscreen: native_window::fullscreen_candidate()
                .filter(|_| settings.hide_fullscreen && native_window::shell_fullscreen())
                .into_iter()
                .collect(),
            fullscreen_generation: None,
            fullscreen_due: None,
            graphics: Graphics::create()?,
            rebuild: Rebuild::new(Instant::now()),
            picker: None,
            palette: crate::theme::system_palette(settings.theme, effective_accent),
            palette_dirty: false,
            settings_window: None,
            file_picker: Default::default(),
            settings,
            icons: IconService::create()?,
            dock_items: Vec::new(),
            rtl: dock::system_rtl(),
            dock_generation: None,
            pins: Pins::from_store(&store),
            shelf: ShelfView {
                removed: None,
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
            menu: None,
            volume_wheel: 0,
            edges: Edges::default(),
            edges_dirty: true,
            pages,
            weather,
            weather_io: weather_os::Io::new(),
            region: weather_os::region(),
            limits: Limits::default(),
            limits_io: limits_os::Io::new(),
            limits_open: false,
            prayer: PrayerRun::default(),
            water_due: None,
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
    /// The chosen buttons with the system reading direction applied.
    fn dock_buttons(&self) -> Buttons {
        Buttons {
            rtl: self.rtl,
            ..self.settings.dock_buttons
        }
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
            SurfaceKind::Notch => layout::notch_dpi(monitor.work, monitor.dpi, self.text_factor),
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
            self.settings.dock_style,
            self.dock_buttons(),
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
                &self.pages,
            )?;
            self.surfaces.push(surface);
        } else {
            self.surfaces.push(surface);
            // Sized after the push: icons are extracted once, for the densest dock.
            let icons = self.dock_icons();
            let last = self.surfaces.len() - 1;
            self.surfaces[last].paint_dock(&self.dock_items, &icons)?;
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
            self.pages.hour = clock.hour;
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
                    &self.pages,
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
        let media_page = self.page() == Tab::Media;
        for notch in self.surfaces.iter_mut().filter(|s| s.expanded()) {
            let Some(track) = self.pages.track(&self.media).filter(|_| media_page) else {
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
        let track = self.pages.track(&self.media);
        let (shown, playing) = (track.is_some(), track.is_some_and(|track| track.playing));
        // Only the Media page draws the visualizer; any other page, or no Media tab, makes no meter call.
        let open = self.page() == Tab::Media && self.surfaces.iter().any(Surface::expanded);
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
        if !self.pages.tabs.on(Tab::Timer) {
            return INFINITE; // switched off: no deadline, no alert, nothing to tick
        }
        let now = Instant::now();
        if self.timers.take_done(now) {
            self.timers
                .write_to(&mut self.store, &mut self.pending, now);
            self.complete_timer();
            if self.timers.take_pomo_done() {
                // The next phase follows by itself; Start or pause stops the round.
                let finished = self.pages.pomo.phase.label();
                self.pages.pomo.advance(&self.settings.pomo_cfg);
                let status = self.pomo_start(now);
                self.show_notice(format!("{finished} done. {status}"), now);
            }
            self.request_notches(Repaint::Full, |_| true);
        }
        if self.timers.alert_over(now) {
            self.surfaces.iter_mut().for_each(Surface::close_alert);
            self.request_notches(Repaint::Full, |_| true);
        }
        let expanded = self.page() == Tab::Timer && self.surfaces.iter().any(Surface::expanded);
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
    /// The page the open panel shows now.
    fn page(&self) -> Tab {
        self.pages.page(self.pages.track(&self.media).is_some())
    }
    /// A pick for the Media session's deadline only while the Media tab is on.
    fn media_wait(&self) -> u32 {
        if !self.pages.tabs.on(Tab::Media) {
            return INFINITE;
        }
        self.media
            .deadline()
            .map_or(INFINITE, |due| milliseconds_until(due, Instant::now()))
    }
    /// The user (or UI Automation, or a finished timer) picked `tab`: it shows now, becomes the last used
    /// tab (kept in the store) and retires older UI Automation elements.
    fn select_tab(&mut self, tab: Tab) {
        let live = self.pages.track(&self.media).is_some();
        if !self.pages.tabs.select(tab, live) {
            return;
        }
        if self.store.last_tab != tab {
            self.store.last_tab = tab;
            self.pending.mark(Instant::now());
        }
        self.request_notches(Repaint::Full, |_| true);
    }
    /// Weather (features W1): takes a finished worker's answer, starts a job when `Feed::wants` says so and
    /// keeps the page text current. It sets no deadline of its own: the minute clock's wake runs it, so a due
    /// refresh lands within a minute, and a switched-off tab costs nothing but this early return.
    #[inline(never)]
    fn refresh_weather(&mut self) {
        let now = Instant::now();
        while let Some((id, outcome)) = self.weather_io.poll() {
            if self.weather.finish(id, outcome, now) {
                self.store.place = self.weather.place.clone();
                self.store.report = self.weather.report.clone();
                self.pending.mark(now);
            }
        }
        self.weather.expire(now);
        let (on, prayer) = (
            self.pages.tabs.on(Tab::Weather),
            self.pages.tabs.on(Tab::Prayer),
        );
        if !weather::feed_wanted(on, prayer, self.weather.place.is_some()) {
            return; // no fetch, no worker, no page
        }
        let shown = self.page() == Tab::Weather;
        let (detect, configured) = (
            self.settings.detect,
            weather::configured(self.settings.detect, &self.settings.city),
        );
        let ctx = weather::Ctx {
            enabled: true,
            configured,
            visible: self
                .surfaces
                .iter()
                .any(|s| s.visible && s.kind() == SurfaceKind::Notch),
            open: shown && self.surfaces.iter().any(Surface::expanded),
            wall: timer::wall_now(),
        };
        let idle = || weather_os::idle_secs() >= weather_os::IDLE_AFTER_SECS;
        if self.weather.wants(&ctx, now, idle) {
            let job = self.weather.start(now, detect, &self.settings.city);
            let id = job.id;
            if !self.weather_io.spawn(job) {
                self.weather.request_failed(id, now);
            }
        }
        // The text is rebuilt when the data or the minute (the age) changed, and only while it shows.
        let current = |key: &Option<(u32, String)>| {
            key.as_ref()
                .is_some_and(|(rev, clock)| *rev == self.weather.rev && *clock == self.clock)
        };
        if shown && self.pages.tabs.on(Tab::Weather) && !current(&self.pages.weather_key) {
            let fahrenheit = self.settings.units.fahrenheit(&self.region);
            self.pages.weather = weather::view(
                &self.weather,
                configured,
                fahrenheit,
                ctx.wall,
                &weather_os::hour_label,
            );
            self.pages.weather_key = Some((self.weather.rev, self.clock.clone()));
            self.request_notches(Repaint::Full, |s| motion::reveal(s.motion.value(now)) > 0.0);
        }
    }
    /// Limits (features U1): takes a finished worker's answer, starts one job when the Limits page newly opens
    /// (the worker reads only what changed or aged out), and keeps the page text current while it shows. No
    /// timer and no watcher: a hidden or switched-off tab costs nothing but this early return.
    #[inline(never)]
    fn refresh_limits(&mut self) {
        self.limits.configure(
            (self.settings.limits_codex, self.settings.limits_agy),
            self.settings.limits_group,
        );
        let now = limits_os::now();
        while let Some(outcome) = self.limits_io.poll() {
            self.limits.finish(outcome, now);
        }
        let open = self.pages.tabs.on(Tab::Limits)
            && self.page() == Tab::Limits
            && self.surfaces.iter().any(Surface::expanded);
        if !open {
            self.limits_io.cancel();
        }
        if open && !self.limits_open {
            self.start_limits(now, false);
        }
        self.limits_open = open;
        if !open {
            self.pages.limits_key = None;
            return;
        }
        let key = (self.limits.rev, self.limits.busy(), self.clock.clone());
        if self.pages.limits_key.as_ref() != Some(&key) {
            self.pages.limits = limits::view(
                &self.limits.rows,
                now,
                &limits::clock(prayer_os::local_now().offset_min),
                key.1,
            );
            self.pages.limits_key = Some(key);
            self.request_notches(Repaint::Full, |s| s.expanded());
        }
    }
    /// A job for the Limits worker (`Limits::start` says no inside the cooldown or while one runs).
    fn start_limits(&mut self, now: i64, manual: bool) {
        let allow = (self.settings.limits_codex, self.settings.limits_agy);
        if let Some(job) = self
            .limits
            .start(now, manual, allow, self.settings.limits_group)
            && !self.limits_io.spawn(job)
        {
            // No thread: release the claim so a later open can try again.
            self.limits.finish(
                limits::Outcome {
                    id: job.id,
                    ..Default::default()
                },
                now,
            );
        }
    }
    /// Prayer (features P1): builds the page while it shows, then runs the reminders: one pass computes the day's
    /// events (when the date, offset, place, method or heads-up changed) and returns the wait until the next
    /// reminder or a second after midnight. A due reminder expands the notch and starts the sound. Also ends a
    /// finished sound. Off, or without a place, it has no deadline.
    #[inline(never)]
    fn refresh_prayer(&mut self) -> u32 {
        let now = Instant::now();
        if self.prayer.sound_end.is_some_and(|end| now >= end) {
            self.prayer.sound_end = None;
            self.prayer.player.stop();
        }
        let sound_wait = self
            .prayer
            .sound_end
            .map_or(INFINITE, |end| milliseconds_until(end, now));
        if !self.pages.tabs.on(Tab::Prayer) {
            return sound_wait;
        }
        let shown = self.page() == Tab::Prayer;
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
                    None => (
                        "Finding your locationÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦",
                        false,
                    ),
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
    /// A reminder is due: the sound plays unless Reminder sounds is off. The notch expands with the prayer's name; on a monitor a
    /// full-screen app covers it appears over the app (topmost, never activated) and hides again once retracted.
    fn fire_prayer(&mut self, event: prayer_tab::Event, now: Instant) {
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
    fn show_notice(&mut self, text: String, now: Instant) {
        self.power.announce(text, now, PRAYER_NOTICE);
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
    /// Water reminders (F5): one deadline to the next slot, none while the switch is off. A due slot shows
    /// the progress notice (silent once the goal is met) and the next deadline is computed from the clock.
    #[inline(never)]
    fn refresh_water(&mut self) -> u32 {
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
    fn water_today(&self) -> u8 {
        water::today(
            self.store.water,
            prayer_tab::day_number(prayer_os::local_now().ymd),
        )
    }
    /// "I drank a glass": today's count goes up and is stored.
    fn water_drink(&mut self) {
        let day = prayer_tab::day_number(prayer_os::local_now().ymd);
        self.store.water = (day, self.water_today().saturating_add(1));
        self.pending.mark(Instant::now());
    }
    fn sync_water(&self) {
        if let Some(window) = &self.settings_window {
            window.set_water(&water::text(self.water_today(), &self.settings.water_cfg));
        }
    }
    /// Plays the chosen sound (none when silent or no default exists); false when nothing played.
    fn play_prayer_sound(&mut self, now: Instant) -> bool {
        let found = prayer_os::discover();
        let Some(path) = prayer_tab::resolve(&self.settings.prayer_sound, &found) else {
            return false;
        };
        let length = self.prayer.player.play(path);
        self.prayer.sound_end = length.map(|length| now + length);
        length.is_some()
    }
    /// The mascot: picks the mood from the facts and steps the running show, or starts the idle one whose
    /// deadline passed. With the setting off it returns at once. Returns the wait until the next step or show.
    #[inline(never)]
    fn refresh_eyes(&mut self) -> u32 {
        let now = Instant::now();
        let facts = eyes::Facts {
            playing: self
                .pages
                .track(&self.media)
                .is_some_and(|track| track.playing),
            alert: self.power.text().is_some()
                || (self.pages.tabs.on(Tab::Timer) && self.timers.finished()),
            idle: now.saturating_duration_since(self.pages.last_input) >= eyes::IDLE_AFTER,
            adhan: self.prayer.adhan_until.is_some_and(|end| now < end),
            rain: self.pages.page(self.pages.track(&self.media).is_some()) == Tab::Weather
                && self.weather.report.as_ref().is_some_and(|report| {
                    matches!(
                        weather::Icon::from_wmo(report.code),
                        weather::Icon::Rain | weather::Icon::Storm
                    )
                }),
            hour: self.pages.hour,
        };
        let lean = if self.timers.running() {
            7.0
        } else if facts.playing {
            -8.0
        } else {
            0.0
        };
        let Some(eyes) = &mut self.pages.eyes else {
            return INFINITE;
        };
        eyes.set_lean(lean);
        let changed = eyes.set_mood(eyes::mood(facts), now) | eyes.tick(now);
        let due = eyes.due();
        if changed {
            // A running animation paints every frame already; an open panel does not draw the eyes.
            self.request_notches(Repaint::Full, |s| !s.live && s.motion.value(now) < 0.1);
        }
        milliseconds_until(due, now)
    }
    /// The collapsed pill's timer ring breathes: while a countdown runs and a pill is on screen, closed and not
    /// already animating, it is repainted every `BREATH_STEP`. Hidden, open, reduced motion or no timer: no wait.
    #[inline(never)]
    fn refresh_ring(&mut self) -> u32 {
        if motion::calm()
            || !self.pages.tabs.on(Tab::Timer)
            || !self.timers.running()
            || self.pages.track(&self.media).is_some()
        {
            return INFINITE;
        }
        let now = Instant::now();
        let mut shown = false;
        for notch in &mut self.surfaces {
            if notch.kind() == SurfaceKind::Notch && notch.visible && notch.collapsed(now) {
                shown = true;
                if !notch.live {
                    notch.request(Repaint::Full);
                }
            }
        }
        if shown {
            motion::BREATH_STEP.as_millis() as u32
        } else {
            INFINITE
        }
    }
    /// Steps the dots' fade; returns the wait until its next step (`INFINITE` at rest, the common case).
    #[inline(never)]
    fn refresh_privacy(&mut self) -> u32 {
        let now = Instant::now();
        if self.pages.dots.tick(now) {
            self.request_notches(Repaint::Full, |_| true);
        }
        self.pages
            .dots
            .due()
            .map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// The consent store changed: re-arm first so no later change is lost, then read it once.
    fn privacy_signal(&mut self) {
        let Some(watch) = &self.pages.watch else {
            return;
        };
        watch.arm();
        if self.pages.dots.set(watch.read(), Instant::now()) {
            self.request_notches(Repaint::Full, |_| true);
        }
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
        let slots = self.dock_slots();
        let hovered = self
            .surfaces
            .iter()
            .enumerate()
            .find_map(|(index, surface)| {
                let Role::Dock {
                    hover: Some(cell), ..
                } = surface.role
                else {
                    return None;
                };
                // Slot 0 is the picker and buttons have no windows; a pinned app that is not running has
                // nothing to show.
                let slot = dock::cell(surface.dock_buttons, slots, cell)?
                    .slot()
                    .filter(|slot| *slot >= 1 && surface.visible)?;
                self.dock_items
                    .get(slot - 1)
                    .filter(|item| !item.windows.is_empty())
                    .map(|item| (index, slot, item))
            });
        // A thumbnail would float over the open menu.
        let hovered = hovered.filter(|_| self.menu.is_none());
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
                let slot_x = dock::slot_center_x(
                    dock.bounds.x,
                    dock.dock_buttons,
                    dock.bounds.width,
                    slot,
                    dock.dpi,
                );
                preview::Popup::show(&item.windows, &app, slot_x, dock.bounds, work, dock.dpi).ok()
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
    /// The edge sliders' pass: rebuilds them when the monitors or settings changed, then applies their
    /// hover deadlines, brightness readings and paints. Returns the wait until the next deadline.
    #[inline(never)]
    fn refresh_edges(&mut self) -> u32 {
        let now = Instant::now();
        if std::mem::take(&mut self.edges_dirty) {
            self.edges.reconcile(&self.monitors, &self.settings);
        }
        let fullscreen = &self.fullscreen;
        let covered = |device: &str| fullscreen.iter().any(|(_, covered)| covered == device);
        let mut due = self
            .edges
            .refresh(now, &self.audio, &self.graphics, self.palette, covered);
        if !self.audio.active() && self.edges.wants_audio() {
            // A volume pill just opened: load Core Audio now (nothing else wakes the next pass) and draw
            // the pill again with the level.
            self.audio.start();
            self.audio.refresh();
            due = self
                .edges
                .refresh(now, &self.audio, &self.graphics, self.palette, covered);
        }
        due.map_or(INFINITE, |due| milliseconds_until(due, now))
    }
    /// A mouse or UIA message for an edge slider's window.
    fn edge_message(&mut self, message: &MSG) -> bool {
        self.edges.message(message, &self.audio, Instant::now())
    }
    /// A click on the dock ends the preview; it stays away until the pointer moves to another slot.
    fn dismiss_preview(&mut self) {
        self.popup = None;
        self.preview.dismiss();
    }
    /// A timer finished: every visible notch opens (once, however many finished together) and the
    /// system sound plays if enabled. No toast: notification packaging is M6.
    fn complete_timer(&mut self) {
        if self.pages.tabs.on(Tab::Timer) {
            // The open it causes shows the finished timer, whatever page was last used.
            self.pages.tabs.choice = Some(Tab::Timer);
        }
        self.open_notches();
        if self.settings.timer_beeps() {
            chime();
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
                        &self.pages,
                    )?,
                    SurfaceKind::Dock => surface.paint_dock(&self.dock_items, &icons)?,
                }
                surface.live = surface.motion.running(now)
                    || surface.volume_fill.running(now)
                    || surface.page_fx.running(now)
                    || surface.notice_fx.running(now)
                    || surface.slide.running(now)
                    || match &surface.role {
                        Role::Notch { intent, .. } => intent.is_some(),
                        Role::Dock { slots, .. } => slots.iter().any(|tween| tween.running(now)),
                    };
            }
            if surface.leaving && !surface.slide.running(now) {
                surface.apply_visible(false);
            }
            if surface.overlay && !surface.live && surface.motion.value(now) <= 0.0 {
                surface.overlay = false;
                surface.set_visible(false);
            }
            running |= surface.live;
        }
        Ok(running)
    }
    fn close(&mut self, kind: SurfaceKind) {
        self.shown.retain(|shown| *shown != kind);
        self.surfaces.retain(|surface| surface.kind() != kind);
    }
    fn is_notch(&self, hwnd: HWND) -> bool {
        self.surfaces
            .iter()
            .any(|s| s.kind() == SurfaceKind::Notch && s.window.0 == hwnd)
    }
    fn dock_at(&self, hwnd: HWND) -> Option<&Surface> {
        self.surfaces
            .iter()
            .find(|s| s.kind() == SurfaceKind::Dock && s.window.0 == hwnd)
    }
    /// Opens the settings window (or replaces an open one) on the connected monitors.
    fn open_settings(&mut self) -> Result<()> {
        self.settings_window.take();
        self.settings_window = Some(SettingsWindow::create(
            &self.settings,
            &self.monitors,
            self.text_factor,
        )?);
        self.sync_settings_pins(0);
        self.sync_water();
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
        let (change, start, movement, sound, drank, pomo) = (
            window.take_change(),
            window.take_start(),
            window.take_move(),
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
                let status = if self.play_prayer_sound(Instant::now()) {
                    "Playing the prayer sound."
                } else {
                    "No sound to play. Pick another file."
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
    fn choose_sound(&mut self, paths: Vec<String>) {
        let Some(path) = paths.into_iter().next() else {
            return;
        };
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
    fn apply_settings(&mut self, next: Settings, catalog: &mut Catalog) -> Result<()> {
        let devices: Vec<String> = self.monitors.iter().map(|m| m.device.clone()).collect();
        let next = next.clamped().keep_one_monitor(&devices);
        if next == self.settings {
            return Ok(());
        }
        let old = std::mem::replace(&mut self.settings, next);
        if let Some(failure) = self.settings.apply_globals()
            && let Some(window) = &self.settings_window
        {
            window.say(failure)?;
        }
        self.water_due = None; // the schedule follows the new hours
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
        self.edges_dirty = true;
        let widgets_changed = old.widgets != self.settings.widgets;
        self.pages.widgets = self.settings.widgets;
        let tabs_changed = old.tabs != self.settings.tabs;
        if tabs_changed {
            self.pages.tabs.flags = self.settings.tabs;
            // A tab back on picks up what it missed: icons for the shelf, an expired timer.
            self.shelf.stale = true;
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
    /// Starts a countdown the user asked for in the settings window; the text is the window's status line.
    /// Starts the current Pomodoro phase (or resumes a paused one) as a tagged timer.
    fn pomo_start(&mut self, now: Instant) -> String {
        let seconds = self
            .pages
            .pomo
            .left
            .take()
            .unwrap_or_else(|| self.pages.pomo.secs(&self.settings.pomo_cfg));
        let label = self.pages.pomo.phase.label();
        self.pages.pomo.track(&self.settings.pomo_cfg);
        if !self
            .timers
            .start_as(seconds, timer::wall_now(), now, Some(label))
        {
            self.pages.pomo.left = Some(seconds);
            return format!("{} timers are already running.", timer::MAX);
        }
        self.timers
            .write_to(&mut self.store, &mut self.pending, now);
        self.request_notches(Repaint::Full, |_| true);
        format!("{label}: {} min.", seconds.div_ceil(60))
    }
    /// Start or pause (false) or skip (true) from Settings; returns what to tell the user.
    fn pomo_act(&mut self, skip: bool) -> String {
        let now = Instant::now();
        let left = self.timers.stop_pomo(now);
        if skip {
            self.pages.pomo.advance(&self.settings.pomo_cfg);
        } else if left.is_some() {
            self.pages.pomo.left = left;
            self.timers
                .write_to(&mut self.store, &mut self.pending, now);
            self.request_notches(Repaint::Full, |_| true);
            return "Pomodoro paused.".to_owned();
        }
        self.pomo_start(now)
    }
    /// A Timer page button; what it did is announced as a notice (the presets just start, as before).
    fn timer_press(&mut self, control: timer::Ctl, now: Instant) {
        use timer::Ctl;
        let status = match control {
            Ctl::Preset(minutes) => {
                self.clear_notice();
                return drop(self.start_timer(minutes * 60));
            }
            Ctl::Focus | Ctl::Pause | Ctl::Resume => self.pomo_act(false),
            Ctl::Skip => self.pomo_act(true),
            Ctl::Cancel => {
                let view = self.pages.timer_view(&self.timers, now);
                if view.as_ref().is_some_and(|view| view.tagged) {
                    self.timers.stop_pomo(now);
                    self.pages.pomo = pomo::Pomo::default();
                    "Pomodoro cancelled.".to_owned()
                } else {
                    self.timers.cancel_soonest();
                    "Timer cancelled.".to_owned()
                }
            }
        };
        self.timers
            .write_to(&mut self.store, &mut self.pending, now);
        // A phase starting shows in the page's title line, so it needs no notice of its own, and an earlier
        // notice ("Pomodoro paused.") is stale by then.
        if pomo::is_start_status(&status) {
            self.clear_notice();
        } else {
            self.show_notice(status, now);
        }
    }
    /// Ends the notice row at once when what it said no longer holds.
    fn clear_notice(&mut self) {
        if self.power.clear() {
            self.request_notches(Repaint::Full, |_| true);
        }
    }
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
    fn toggle_picker(&mut self, catalog: &Catalog) -> Result<()> {
        if self.picker.take().is_some() {
            Ok(())
        } else {
            self.open_picker(catalog)
        }
    }
    fn effective_accent(&self) -> Option<crate::theme::Rgb> {
        if self.settings.match_accent {
            crate::theme::windows_accent_color().or_else(|| Some(self.text.accent()))
        } else {
            None
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        crate::hotkey::unregister_picker();
        crate::hotkey::unregister_audio();
    }
}

impl Desktop {
    /// Moves, resizes and repaints every dock for the current style, buttons and slot count. Same slots in a
    /// new place or width: no new surface and no timer; hover restarts (cells moved).
    fn relayout_docks(&mut self) -> Result<()> {
        let (style, buttons, slots) = (
            self.settings.dock_style,
            self.dock_buttons(),
            self.dock_slots(),
        );
        let icons = self.dock_icons();
        for dock in &mut self.surfaces {
            let Some(monitor) = self.monitors.iter().find(|m| m.device == dock.monitor) else {
                continue;
            };
            if dock.kind() != SurfaceKind::Dock {
                continue;
            }
            let Role::Dock {
                hover,
                slots: tweens,
                tips,
                ..
            } = &mut dock.role
            else {
                continue;
            };
            (*hover, *tweens) = (
                None,
                vec![Tween::rest(0.0); dock::cell_count(buttons, slots)],
            );
            (dock.dock_style, dock.dock_buttons) = (style, buttons);
            dock.bounds = placement_styled(
                monitor.work,
                dock.dpi,
                SurfaceKind::Dock,
                slots,
                style,
                buttons,
            );
            if let Some(tips) = tips {
                tips.place(buttons, dock.bounds.width, dock.bounds.height, dock.dpi);
            }
            dock.window.resize(dock.bounds)?;
            dock.canvas.resize(dock.bounds)?;
            dock.paint_dock(&self.dock_items, &icons)?;
        }
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
            let mut items = dock::items_with_pins(model, &self.pins, self.settings.desktop_only);
            if !self.settings.dock_badges {
                for item in &mut items {
                    item.badge = dock::Badge::None;
                }
            }
            let changed = items != self.dock_items;
            // Only changes to visible dock data require a repaint.
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
                return self.relayout_docks();
            }
        }
        let icons = self.dock_icons();
        for dock in self
            .surfaces
            .iter_mut()
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
    /// would empty the cache on every switch) and draws it smaller; a notch-only desktop uses the notch's DPI.
    fn icon_size(&self) -> Option<u32> {
        let densest = |kind| {
            self.surfaces
                .iter()
                .filter(|s| s.kind() == kind)
                .map(|s| s.dpi)
                .max()
        };
        icon_dpi(densest(SurfaceKind::Dock), densest(SurfaceKind::Notch))
            .map(|dpi| dock::px(dock::ICON_DIPS, dpi) as u32)
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
    /// Slot 0 opens the picker, other slots act on their real window group, a button does its job.
    fn activate(&mut self, hwnd: HWND, hit: Option<Hit>, catalog: &Catalog) -> Result<()> {
        match hit {
            Some(Hit::Slot(0)) => self.open_picker(catalog),
            Some(Hit::Slot(slot)) => {
                if let Some(action) = self
                    .dock_items
                    .get(slot - 1)
                    .and_then(|item| dock::action(item, dock::foreground_key()))
                {
                    self.run_dock_action(action);
                }
                Ok(())
            }
            Some(Hit::Button(button)) => {
                self.press_button(hwnd, button, catalog.model());
                Ok(())
            }
            None => Ok(()),
        }
    }
    fn run_dock_action(&mut self, action: dock::Action) {
        if let Some(failure) = dock::action_failure(action) {
            self.show_notice(failure.into(), Instant::now());
        }
    }
    /// A taskbar button: the shell shortcut it stands for (sent on this click only), or the minimized list.
    fn press_button(&mut self, hwnd: HWND, button: Button, model: &WindowModel) {
        match button.keys() {
            Some(keys) => {
                if !dock::send_keys(keys) {
                    eprintln!("Dock button: Windows refused the key input");
                }
            }
            None => self.open_minimized_menu(hwnd, model),
        }
    }
    /// The chevron's popup above it: the minimized windows right now (at most `MINIMIZED_MAX`), each with
    /// its icon from the cache; a click restores and activates that window. Nothing is listed, and nothing
    /// opens, when none is minimized.
    fn open_minimized_menu(&mut self, hwnd: HWND, model: &WindowModel) {
        self.dismiss_preview();
        self.close_menu();
        let Some(dock) = self.dock_at(hwnd).filter(|dock| dock.visible) else {
            return;
        };
        let (bounds, dpi, monitor, buttons) = (
            dock.bounds,
            dock.dpi,
            dock.monitor.clone(),
            dock.dock_buttons,
        );
        let return_to = dock::foreground_key();
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let windows: Vec<_> = dock::minimized_windows(model, self.settings.desktop_only)
            .into_iter()
            .take(dock::MINIMIZED_MAX)
            .collect();
        if windows.is_empty() {
            return;
        }
        let size = self.icon_size();
        let icons = windows
            .iter()
            .map(|window| {
                let sources = icons::sources(&window.identity, window.executable.as_deref());
                size.and_then(|size| self.icons.icon(&window.identity, sources, size))
            })
            .collect();
        let (left, width) = dock::span(buttons, bounds.width, dpi, Hit::Button(Button::Minimized));
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: 0,
            windows: Vec::new(),
            identity: Identity::Unavailable(0),
            items: windows.iter().map(|w| Item::Restore(w.key)).collect(),
            dock_bounds: bounds,
            work,
            dpi,
            app: "Minimized apps".into(),
            titles: windows
                .iter()
                .map(|w| {
                    if w.title.is_empty() {
                        "Window".to_owned()
                    } else {
                        w.title.clone()
                    }
                })
                .collect(),
            icons,
            x: bounds.x + left + width / 2,
            below: None,
            return_to,
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Minimized list: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Minimized list: {error}"),
        }
    }
    /// Pins the slot's app, or unpins it if pinned (the menu's Pin and Unpin). The dock is rebuilt on the
    /// next pass.
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
    /// Middle-click on a dock slot launches a new instance when the app can be launched.
    fn middle_click_slot(&mut self, slot: usize) {
        if let Some(index) = slot.checked_sub(1)
            && let Some(item) = self.dock_items.get(index)
            && let Some(launch) = crate::pins::launch_target(&item.identity)
        {
            self.run_dock_action(dock::Action::Launch(launch));
        }
    }
    /// Right-click on a dock slot opens its menu above it (the picker slot has none). It needs the
    /// foreground for dismissal; if Windows refuses, no menu opens rather than one that
    /// a click elsewhere could never dismiss.
    fn open_menu(&mut self, hwnd: HWND, slot: Option<usize>, lparam: LPARAM) {
        // Outside an app slot (the picker slot, a gap) the dock offers the app menu instead.
        if !slot.is_some_and(|slot| slot >= 1 && slot <= self.dock_items.len()) {
            return self.open_app_menu(hwnd, lparam);
        }
        self.dismiss_preview();
        self.close_menu();
        let Some((dock, slot)) = self
            .dock_at(hwnd)
            .zip(slot.filter(|&slot| slot >= 1))
            .filter(|(dock, _)| dock.visible)
        else {
            return;
        };
        let Some(item) = self.dock_items.get(slot - 1) else {
            return;
        };
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == dock.monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot,
            identity: item.identity.clone(),
            windows: item.windows.clone(),
            items: dock_menu::items(item, &self.pins),
            dock_bounds: dock.bounds,
            work,
            dpi: dock.dpi,
            app: crate::pins::display_name(&item.identity),
            titles: Vec::new(),
            icons: Vec::new(),
            x: dock::slot_center_x(
                dock.bounds.x,
                dock.dock_buttons,
                dock.bounds.width,
                slot,
                dock.dpi,
            ),
            below: None,
            return_to: dock::foreground_key(),
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Dock menu: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Dock menu: {error}"),
        }
    }
    /// Right-click on the notch, or on the dock outside an app slot: Settings and Quit, below the notch or
    /// above the dock. Needs the foreground like the slot menu.
    fn open_app_menu(&mut self, hwnd: HWND, lparam: LPARAM) {
        self.open_context_menu(hwnd, lparam, None);
    }

    fn open_context_menu(&mut self, hwnd: HWND, lparam: LPARAM, shelf_path: Option<String>) {
        self.dismiss_preview();
        self.close_menu();
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
        let below = (surface.kind() == SurfaceKind::Notch).then(|| {
            let media = self.pages.track(&self.media).is_some();
            let shape = layout::notch_shape(surface.motion.value(Instant::now()), media);
            surface.bounds.y + (shape.height * surface.dpi as f32 / 96.0).round() as i32
        });
        let mut items = if shelf_path.is_some() {
            vec![Item::RemoveShelf]
        } else {
            dock_menu::APP_ITEMS.to_vec()
        };
        let shelf_menu = surface.kind() == SurfaceKind::Notch && self.page() == Tab::Shelf;
        if shelf_menu && self.shelf.removed.is_some() {
            items.push(Item::UndoShelf);
        }
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: 0,
            windows: Vec::new(),
            identity: shelf_path.map_or(Identity::Unavailable(0), Identity::Executable),
            items,
            dock_bounds: surface.bounds,
            work,
            dpi: surface.dpi,
            app: if shelf_menu { "Shelf" } else { "ROQA" }.into(),
            titles: Vec::new(),
            icons: Vec::new(),
            x: surface.bounds.x + point(lparam).0,
            below,
            return_to: dock::foreground_key(),
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => {
                self.menu = Some(menu);
                // The panel the pointer had opened closes, so the menu does not sit on it.
                self.surfaces
                    .iter_mut()
                    .for_each(|s| s.set_notch_hover(false));
            }
            Ok(_) => eprintln!("App menu: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("App menu: {error}"),
        }
    }
    /// Ends the open menu, giving the foreground back to the window it came from if the menu still holds it.
    fn close_menu(&mut self) {
        if let Some(menu) = self.menu.take() {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
    }
    /// Input for the open menu. Returns true when the message is consumed.
    fn menu_message(&mut self, message: &MSG) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        let own = message.hwnd == menu.hwnd();
        match menu.message(message) {
            Event::Ignored => false,
            Event::Consumed => true,
            Event::Dismiss => {
                self.close_menu();
                own // a press on another ROQA window still reaches it
            }
            Event::Choose(item) => {
                self.run_menu(item);
                true
            }
        }
    }
    /// Does the chosen action on the slot's app, if the slot still holds it. A new instance leaves the
    /// foreground to the app it starts; the rest return it to the window the menu came from.
    fn run_menu(&mut self, item: Item) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        // A new instance or a restored window takes the foreground itself.
        if !matches!(item, Item::NewInstance | Item::Restore(_)) {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
        if menu.slot == 0 {
            match item {
                Item::Settings => {
                    if let Err(error) = self.open_settings() {
                        eprintln!("Settings: {error}");
                    }
                }
                // SAFETY: a scalar quit request to this thread's own message loop.
                Item::Quit => unsafe { PostQuitMessage(0) },
                Item::Restore(key) => {
                    self.run_dock_action(dock::Action::Activate(key));
                }
                Item::RemoveShelf => {
                    if let Identity::Executable(path) = &menu.identity {
                        self.remove_shelf_entry(path);
                    }
                }
                Item::UndoShelf => self.undo_shelf_removal(),
                _ => {}
            }
            return;
        }
        let target = self
            .dock_items
            .get(menu.slot - 1)
            .filter(|target| {
                dock_menu::describes(
                    &menu.identity,
                    menu.items(),
                    &menu.windows,
                    Some(target),
                    &self.pins,
                )
            })
            .cloned();
        let Some(target) = target else {
            return;
        };
        match item {
            Item::Close(key) => dock::close(&[key]),
            Item::CloseAll => dock::close(&menu.windows),
            Item::NewInstance => {
                if let Some(launch) = crate::pins::launch_target(&target.identity) {
                    self.run_dock_action(dock::Action::Launch(launch));
                }
            }
            Item::Pin | Item::Unpin => self.toggle_pin(Some(menu.slot)),
            Item::Settings
            | Item::Quit
            | Item::Restore(_)
            | Item::RemoveShelf
            | Item::UndoShelf => {}
        }
    }
    /// Ends the open menu once it no longer describes the dock (its slot changed, or a full-screen app hid
    /// the dock), and repaints it when its highlight moved. Nothing open costs one check.
    fn refresh_menu(&mut self) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        let shown = self
            .surfaces
            .iter()
            .any(|s| s.window.0 == menu.dock() && s.visible);
        // The app menu (slot 0) describes no dock item, so only its surface being shown matters.
        let current = shown
            && (menu.slot == 0
                || dock_menu::describes(
                    &menu.identity,
                    menu.items(),
                    &menu.windows,
                    self.dock_items.get(menu.slot - 1),
                    &self.pins,
                ));
        if !current || menu.paint().is_err() {
            self.close_menu();
        }
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
            dock::drop_index(
                dock.dock_buttons,
                x,
                dock.bounds.width,
                dock.dpi,
                slots,
                pins,
            )
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
                    let shelf = self.pages.tabs.on(Tab::Shelf);
                    if let Some(notch) = self.surfaces.iter_mut().find(|s| s.window.0 == hwnd) {
                        notch.set_notch_hover(over);
                        // A file over the notch is for the shelf: open it there (a disabled Shelf tab keeps
                        // the notch as it was).
                        if over
                            && shelf
                            && notch.visible
                            && let Role::Notch { intent, .. } = &mut notch.role
                        {
                            open_notch(intent, &mut notch.motion);
                            notch.live = true;
                        }
                    }
                    if over && shelf {
                        self.select_tab(Tab::Shelf);
                    }
                }
                DropEvent::Files(paths) if self.pages.tabs.on(Tab::Shelf) => {
                    self.add_shelf_files(&paths)
                }
                DropEvent::Files(_) => {}
            }
        }
        // An arrival while the panel is shut must survive until it opens: remember it as stale.
        self.shelf.stale |= self.icons_arrived;
        if self.shelf.shelf.is_empty()
            || !self.shelf.stale
            || self.page() != Tab::Shelf
            || !self.surfaces.iter().any(Surface::expanded)
        {
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
            self.add_shelf_files(&paths);
        }
    }
    fn add_shelf_files(&mut self, paths: &[String]) {
        let before = self.shelf.shelf.entries().len();
        let mut displaced = 0;
        self.change_shelf(|shelf| {
            let added = shelf.add_all(paths);
            displaced = before + added - shelf.entries().len();
            added > 0
        });
        if displaced > 0 {
            self.show_notice(
                format!("Shelf full. Removed {displaced} older."),
                Instant::now(),
            );
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
        }
    }
    /// The visible shelf entry under a window point of notch `notch`, if its slot is drawn and filled.
    fn shelf_slot_at(&self, notch: usize, lparam: LPARAM) -> Option<usize> {
        let surface = &self.surfaces[notch];
        let (x, y) = point(lparam);
        let progress = surface.motion.value(Instant::now());
        let media = self.pages.track(&self.media).is_some();
        if self.page() != Tab::Shelf {
            return None;
        }
        layout::shelf_at(
            surface.bounds.width,
            surface.dpi,
            progress,
            media,
            self.shelf.shelf.is_empty(),
            x,
            y,
        )
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
        } else {
            self.show_notice(
                "File missing. Add it from its new location.".into(),
                Instant::now(),
            );
        }
    }
    /// Opens the Shelf context menu for a drawn entry.
    fn shelf_menu_at(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        let notch = self
            .surfaces
            .iter()
            .position(|s| s.kind() == SurfaceKind::Notch && s.window.0 == hwnd);
        let Some(index) = notch.and_then(|notch| self.shelf_slot_at(notch, lparam)) else {
            return false;
        };
        let Some(entry) = self
            .shelf
            .shelf
            .absolute(index)
            .and_then(|index| self.shelf.shelf.entries().get(index))
        else {
            return false;
        };
        let path = entry.path.clone();
        self.open_context_menu(hwnd, lparam, Some(path));
        true
    }

    fn remove_shelf_entry(&mut self, path: &str) {
        let Some(index) = self
            .shelf
            .shelf
            .entries()
            .iter()
            .position(|entry| entry.path == path)
        else {
            return;
        };
        self.shelf.removed = Some((index, self.shelf.shelf.entries()[index].clone()));
        self.change_shelf(|shelf| shelf.remove(index));
        self.show_notice(
            "Removed from Shelf. Select Undo to restore.".into(),
            Instant::now(),
        );
    }

    fn undo_shelf_removal(&mut self) {
        if let Some((index, entry)) = self.shelf.removed.take() {
            let mut restored = false;
            self.change_shelf(|shelf| {
                restored = shelf.restore(index, entry.clone());
                restored
            });
            if restored {
                self.show_notice("Restored to Shelf.".into(), Instant::now());
            } else {
                self.shelf.removed = Some((index, entry));
                self.show_notice(
                    "Cannot restore. Check Shelf space and duplicates.".into(),
                    Instant::now(),
                );
            }
        }
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
    /// Media control input on a notch: a click on a control. Returns whether `message` is consumed.
    fn notch_input(&mut self, message: &MSG) -> bool {
        let Some(notch) = self
            .surfaces
            .iter()
            .position(|s| s.kind() == SurfaceKind::Notch && s.window.0 == message.hwnd)
        else {
            return false;
        };
        if message.message == WM_LBUTTONUP {
            self.pages.touch();
        }
        if message.message == WM_LBUTTONUP && self.surfaces[notch].grip.drag {
            self.surfaces[notch].end_grip();
            return true;
        }
        message.message == WM_LBUTTONUP && self.click_control(notch, message.lParam)
    }
    /// A click on a drawn media control sends its command, one on the progress bar seeks, and one
    /// on the volume bar sets the volume; the notch stays nonactivating.
    fn click_control(&mut self, notch: usize, lparam: LPARAM) -> bool {
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
        if let Some(rail) = self.pages.rail(live)
            && let Some(position) = layout::rail_tab_at(width, dpi, progress, live, rail, (x, y))
        {
            if let Some(&tab) = self.pages.tabs.visible().as_slice().get(position) {
                self.select_tab(tab);
            }
            return true;
        }
        if layout::gear_at(width, dpi, progress, live, x, y) {
            if let Err(error) = self.open_settings() {
                eprintln!("Settings: {error}");
            }
            return true;
        }
        let page = self.page();
        if page == Tab::Limits && layout::refresh_at(width, dpi, progress, live, x, y) {
            self.start_limits(limits_os::now(), true);
            return true;
        }
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
            if self.shelf.removed.is_some()
                && layout::shelf_action_at(width, dpi, progress, live, (x, y), true)
            {
                self.undo_shelf_removal();
                return true;
            }
            if !self.shelf.shelf.is_empty()
                && layout::shelf_action_at(width, dpi, progress, live, (x, y), false)
            {
                self.start_file_picker();
                return true;
            }
            if let Some(index) = layout::shelf_remove_at(width, dpi, progress, x, y)
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
        if control != audio::MUTE {
            self.media.command(control);
        } else if self.pages.track(&self.media).is_some() {
            self.audio.toggle_mute();
        }
    }
    fn message(&mut self, message: &MSG) -> Result<()> {
        if message.message == shelf_os::PICKED && message.hwnd.is_invalid() {
            if let Some(paths) = self.file_picker.take() {
                if self.file_picker.for_sound {
                    self.choose_sound(paths);
                } else {
                    self.add_shelf_files(&paths);
                }
            }
            return Ok(());
        }
        if message.message == native_window::POWER_STATUS {
            // One thread post however many windows got the broadcast; clear it before reading so a
            // change during the read posts again.
            native_window::power_read();
            if self.power.update(power::Raw::read(), Instant::now()) {
                self.open_notches();
                self.request_notches(Repaint::Full, |_| true);
            } else if self.page() == Tab::Widgets {
                self.request_notches(Repaint::Full, |s| s.expanded());
            }
            return Ok(());
        }
        if message.message == native_window::WAKE {
            native_window::wake_read();
            let now = Instant::now();
            if let Some(eyes) = &mut self.pages.eyes {
                eyes.wake(now);
                self.request_notches(Repaint::Full, |s| !s.live && s.motion.value(now) < 0.1);
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
            self.prayer.key = None; // the day's times are computed again
            self.water_due = None;
            self.pages.prayer_key = None;
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
                if let Some(eyes) = &mut self.pages.eyes {
                    eyes.wake(now);
                    self.request_notches(Repaint::Full, |s| !s.live && s.motion.value(now) < 0.1);
                }
            }
            return Ok(());
        }
        // The volume row answers the pointer only on the Media page with a live track.
        let volume_ui = self.page() == Tab::Media && self.pages.track(&self.media).is_some();
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
                    .dock_hit(message.lParam, slots)
                    .and_then(Hit::slot)
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
                    let cell = surface.dock_cell(message.lParam, slots);
                    if entered || cell != hover {
                        surface.hover_dock(cell, true);
                    }
                } else {
                    let (x, y) = point(message.lParam);
                    let now = Instant::now();
                    let progress = surface.motion.value(now);
                    let width = surface.bounds.width;
                    let media = self.pages.track(&self.media).is_some();
                    let rail = self.pages.rail(media);
                    let over = layout::notch_hit(width, surface.dpi, progress, media, rail, x, y);
                    // While a menu is open the notch does not open under it.
                    let was = matches!(surface.role, Role::Notch { engaged: true, .. });
                    surface.set_notch_hover(over && self.menu.is_none());
                    if over
                        && !was
                        && progress < 0.1
                        && let Some(eyes) = &mut self.pages.eyes
                        && eyes.poke(now)
                    {
                        surface.request(Repaint::Full);
                    }
                    // The tab cell under the pointer washes lighter.
                    let tab = rail.and_then(|rail| {
                        layout::rail_tab_at(width, surface.dpi, progress, media, rail, (x, y))
                    });
                    if tab != surface.tab_hover {
                        surface.tab_hover = tab;
                        surface.request(Repaint::Full);
                    }
                    self.pages.touch();
                    // Pointer activity restarts the doze deadline; eyes that dozed wave on the closed pill.
                    if let Some(eyes) = &mut self.pages.eyes
                        && eyes.touch(now, progress < 0.1)
                    {
                        surface.request(Repaint::Full);
                    }
                    if volume_ui {
                        let bar = layout::volume_at(width, surface.dpi, progress, x, y).is_some();
                        if surface.grip.drag {
                            if message.wParam.0 & MK_LBUTTON == 0 {
                                surface.end_grip(); // the release was lost
                            } else if let Some(volume) =
                                layout::volume_fraction(width, surface.dpi, progress, x)
                            {
                                self.audio.set(volume);
                            }
                        } else if bar != surface.grip.hover {
                            surface.grip.hover = bar;
                            surface.request(Repaint::Full);
                        }
                    }
                }
            }
            WM_LBUTTONDOWN if volume_ui && matches!(surface.role, Role::Notch { .. }) => {
                let (x, y) = point(message.lParam);
                let (width, progress) =
                    (surface.bounds.width, surface.motion.value(Instant::now()));
                if layout::volume_at(width, surface.dpi, progress, x, y).is_some()
                    && let Some(volume) = layout::volume_fraction(width, surface.dpi, progress, x)
                {
                    // SAFETY: captures the pointer for this thread's own live notch window; released when
                    // the button goes up or the capture is lost.
                    unsafe {
                        SetCapture(surface.window.0);
                    }
                    surface.grip = Grip {
                        hover: true,
                        drag: true,
                    };
                    surface.request(Repaint::Full);
                    self.audio.set(volume);
                }
            }
            WM_CAPTURECHANGED => surface.end_grip(),
            WM_MOUSEWHEEL if volume_ui && surface.grip.hover => {
                let delta = i32::from((message.wParam.0 >> 16) as u16 as i16);
                let (steps, rest) = edge::wheel_steps(self.volume_wheel, delta);
                self.volume_wheel = rest;
                if steps != 0
                    && let Some(level) = self.audio.level()
                {
                    let fine = message.wParam.0 & 0x0004 != 0; // MK_SHIFT
                    self.audio
                        .set(f32::from(edge::step_level(level.percent, steps, fine)) / 100.0);
                }
            }
            WM_MOUSELEAVE => {
                surface.leave();
                if !surface.grip.drag && surface.grip.hover {
                    surface.grip.hover = false;
                    surface.request(Repaint::Full);
                }
            }
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
        // With "Hide in full screen" off nothing is ever remembered, so every surface stays shown.
        let hide = self.settings.hide_fullscreen;
        self.fullscreen.retain(|(key, device)| {
            hide && native_window::fullscreen_monitor(*key).as_ref() == Some(device)
        });
        let foreground = dock::foreground_key().filter(|_| hide);
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
                        &self.pages,
                    )?,
                    SurfaceKind::Dock => surface.paint_dock(&self.dock_items, &icons)?,
                }
            }
            surface.overlay = false;
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
        self.edges_dirty = true;
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
        let broken = self.settings_window.as_mut().is_some_and(|window| {
            window
                .refresh_monitors(&self.monitors)
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
            let palette =
                crate::theme::system_palette(self.settings.theme, self.effective_accent());
            if palette != self.palette {
                self.palette = palette;
                self.menu = None;
                self.edges.release(); // an open pill takes the new colours when it next shows
                self.surfaces.clear();
                for kind in self.shown.clone() {
                    self.show(kind)?;
                }
            }
        }
        if self.text.take_changed() {
            self.rescale_text()?;
        }
        // A pick made during one open ends once every notch has closed.
        if self.pages.tabs.choice.is_some() {
            let now = Instant::now();
            if self
                .surfaces
                .iter()
                .filter(|s| s.kind() == SurfaceKind::Notch)
                .all(|s| s.collapsed(now))
            {
                self.pages.tabs.choice = None;
            }
        }
        let media_on = self.pages.tabs.on(Tab::Media);
        self.media
            .set_expanded(media_on && self.surfaces.iter().any(Surface::expanded));
        // One paint per visible notch when the drawn track or status changes or its pause grace
        // expires, whatever else this pass asks (`paint_requested`). With the Media tab off the events are
        // still applied but draw nothing.
        let status = self.media.view().status();
        let changed =
            self.media.refresh(self.cover_pixels()) || status != self.media.view().status();
        let redraw = changed && media_on;
        let shown = self.pages.track(&self.media).is_some();
        if shown || self.edges.wants_audio() {
            // Core Audio loads only once a media panel or the volume slider can show the level.
            self.audio.start();
        }
        let volume = self.audio.refresh();
        if redraw {
            self.request_notches(Repaint::Full, |_| true);
        } else if volume && shown {
            // Only an open or opening panel draws the volume row; a collapsed pill has nothing new.
            let now = Instant::now();
            self.request_notches(Repaint::Full, |s| motion::reveal(s.motion.value(now)) > 0.0);
        }
        self.refresh_dock(model)?;
        self.refresh_shelf();
        self.refresh_weather();
        self.refresh_limits();
        self.refresh_fullscreen(model.generation)?;
        let wait = self
            .refresh_clock()?
            .min(self.refresh_progress()?)
            .min(self.refresh_meter()?)
            .min(self.refresh_timers())
            .min(self.refresh_power())
            .min(self.refresh_preview())
            .min(self.refresh_edges())
            .min(self.refresh_eyes())
            .min(self.refresh_ring())
            .min(self.refresh_privacy())
            .min(self.refresh_prayer())
            .min(self.refresh_water());
        self.refresh_menu();
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
        self.menu = None;
        self.edges.release();
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
        self.menu = None;
        self.edges.release();
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
                Some(windows::Win32::Foundation::WPARAM(0)),
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
    let mut events = controls.handles();
    let base = events.len();
    loop {
        // The registry watch's handle goes last, and only while the setting is on.
        events.truncate(base);
        events.extend(desktop.pages.watch.as_ref().map(privacy::Watch::handle));
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
            .min(desktop.media_wait())
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
        if index == base {
            desktop.privacy_signal();
        } else if index < base {
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
            if message.message == WM_HOTKEY {
                let id = message.wParam.0 as i32;
                if id == crate::hotkey::HOTKEY_PICKER_ID {
                    desktop.toggle_picker(catalog)?;
                } else if id == crate::hotkey::HOTKEY_AUDIO_CYCLE_ID
                    && let Some(name) = desktop.audio.cycle_device()
                {
                    desktop.show_notice(format!("Audio: {name}"), Instant::now());
                }
                continue;
            }
            if catalog.message(&message)? {
                continue;
            }
            if desktop.menu_message(&message) {
                continue;
            }
            if matches!(
                message.message,
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
            ) && desktop.dock_at(message.hwnd).is_some()
            {
                desktop.dismiss_preview();
            }
            if message.message == WM_LBUTTONUP && desktop.dock_at(message.hwnd).is_some() {
                if !desktop.end_drag(message.hwnd, message.lParam) {
                    let hit = desktop
                        .dock_at(message.hwnd)
                        .and_then(|dock| dock.dock_hit(message.lParam, desktop.dock_slots()));
                    desktop.activate(message.hwnd, hit, catalog)?;
                }
                continue;
            }
            if message.message == WM_MBUTTONUP
                && let Some(dock) = desktop.dock_at(message.hwnd)
            {
                if let Some(slot) = dock
                    .dock_hit(message.lParam, desktop.dock_slots())
                    .and_then(Hit::slot)
                {
                    desktop.dismiss_preview();
                    desktop.middle_click_slot(slot);
                }
                continue;
            }
            if message.message == WM_RBUTTONUP
                && let Some(dock) = desktop.dock_at(message.hwnd)
            {
                let slot = dock
                    .dock_hit(message.lParam, desktop.dock_slots())
                    .and_then(Hit::slot);
                desktop.open_menu(message.hwnd, slot, message.lParam);
                continue;
            }
            if message.message == WM_RBUTTONUP
                && desktop.shelf_menu_at(message.hwnd, message.lParam)
            {
                continue;
            }
            if message.message == WM_RBUTTONUP && desktop.is_notch(message.hwnd) {
                desktop.open_app_menu(message.hwnd, message.lParam);
                continue;
            }
            if desktop.edge_message(&message) {
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
            if let Some(picker) = &mut desktop.picker
                && picker.message(&message)?
            {
                continue;
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
    sort::by(current, |a, b| a.1.cmp(&b.1));
}

#[cfg(test)]
mod clock_wait_tests {
    use super::*;
    use crate::window_model::Identity;

    #[test]
    fn icons_are_sized_for_the_dock_else_the_notch() {
        // A notch-only desktop (dock off) still gets shelf icons.
        assert_eq!(icon_dpi(Some(144), Some(96)), Some(144));
        assert_eq!(icon_dpi(None, Some(96)), Some(96));
        assert_eq!(icon_dpi(None, None), None);
    }

    #[test]
    fn click_mode_swells_a_closed_pill_and_never_shrinks_an_open_one() {
        assert!(swells(0.0));
        assert!(!swells(NOTCH_SWELL) && !swells(1.0));
    }

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
            removed: None,
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
                rect: Bounds {
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
    #[test]
    fn the_handle_follows_the_shown_tabs_and_the_active_page() {
        let mut settings = Settings::default();
        // Enabled tabs show a handle, on Media while a session lives, else on the last tab used.
        let pages = Pages::new(&settings, Tab::Shelf);
        let rail = |pages: &Pages, live| pages.rail(live).map(|r| (r.count, r.active));
        assert_eq!(rail(&pages, true), Some((6, 0)));
        assert_eq!(rail(&pages, false), Some((6, 4)));
        assert_eq!(pages.page(false), Tab::Shelf);
        // A pick during one open wins until it ends.
        settings.tabs[Tab::Timer.index()] = false;
        settings.tabs[Tab::Weather.index()] = false;
        let mut pages = Pages::new(&settings, Tab::Media);
        assert_eq!(rail(&pages, false), Some((4, 0)));
        assert!(pages.tabs.select(Tab::Shelf, true));
        assert_eq!(rail(&pages, true), Some((4, 2)));
        // One tab: no handle. None: the plain clock panel is the Media page.
        settings.tabs = [true, false, false, false, false, false, false];
        assert_eq!(rail(&Pages::new(&settings, Tab::Media), true), None);
        settings.tabs = [false; 7];
        let none = Pages::new(&settings, Tab::Shelf);
        assert_eq!((rail(&none, true), none.page(true)), (None, Tab::Media));
    }

    #[test]
    fn the_mascot_has_no_state_with_the_setting_off() {
        let mut settings = Settings::default();
        assert!(Pages::new(&settings, Tab::Media).eyes.is_some());
        settings.eyes = false;
        assert!(Pages::new(&settings, Tab::Media).eyes.is_none());
    }
}
