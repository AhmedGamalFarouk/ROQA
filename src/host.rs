use crate::{
    catalog_timing::milliseconds_until,
    clock,
    dock::{self, DockItem},
    dock_uia::{self, DockAccessibility},
    graphics::{Canvas, DockScene, DockSlot, Graphics, NotchScene},
    icons::{self, IconPixels, IconService},
    ipc::{Command, Controls},
    layout::{self, Bounds, SurfaceKind, placement},
    media::Media,
    motion::{self, Tween},
    native_window::{self, NativeWindow, WindowClass},
    picker::Picker,
    text_settings::TextSettings,
    topology::{self, Monitor},
    window_catalog::Catalog,
    window_model::WindowModel,
};
use std::{
    mem::size_of,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{E_FAIL, HANDLE, HWND, LPARAM, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WPARAM},
    System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
    System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    System::Threading::INFINITE,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        WindowsAndMessaging::*,
    },
};
use windows::core::{Error, Result, s, w};

#[derive(PartialEq, Eq)]
pub enum Probe {
    None,
    Initialization,
    Callback,
    /// Debug builds only: one synthetic device-removed failure right after startup.
    #[cfg(debug_assertions)]
    DeviceLoss,
}

struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: main thread owns all COM objects and drops them before Apartment.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances successful initialization on this same thread.
        unsafe {
            CoUninitialize();
        }
    }
}

struct NotchShadow {
    canvas: Canvas,
    window: NativeWindow,
}
impl NotchShadow {
    fn create(graphics: &Graphics, bounds: Bounds) -> Result<Self> {
        let window = NativeWindow::create_shadow(bounds)?;
        let canvas = Canvas::create(graphics, window.0, bounds)?;
        Ok(Self { canvas, window })
    }
    fn paint(&self, scene: &NotchScene, notch: HWND) -> Result<()> {
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
        self.canvas.paint_notch_shadow(scene)
    }
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
}
/// State only one surface kind needs.
enum Role {
    /// Pointer inside the hot band (not its shadow), the instant hover intent opens the notch, and
    /// the window region last applied (it follows the drawn shape frame by frame).
    Notch {
        shadow: NotchShadow,
        engaged: bool,
        intent: Option<Instant>,
        area: Vec<Bounds>,
    },
    /// Hovered slot, per-slot hover progress and this dock's UIA geometry/names.
    Dock {
        _accessible: DockAccessibility,
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
    ) -> Result<Self> {
        let bounds = placement(monitor.work, dpi, kind, slots);
        let shadow = if kind == SurfaceKind::Notch {
            Some(NotchShadow::create(graphics, bounds)?)
        } else {
            None
        };
        let window = NativeWindow::create(bounds, kind)?;
        let role = match shadow {
            None => {
                let uia = dock_uia::Shared::default();
                Role::Dock {
                    _accessible: DockAccessibility::create(window.0, &uia)?,
                    hover: None,
                    slots: vec![Tween::rest(0.0); slots],
                    uia,
                }
            }
            Some(shadow) => {
                // Collapsed at rest: only the visible notch takes input.
                let area = layout::notch_rows(bounds, dpi, 0.0);
                window.set_notch_region(&area)?;
                Role::Notch {
                    shadow,
                    engaged: false,
                    intent: None,
                    area,
                }
            }
        };
        let canvas = Canvas::create(graphics, window.0, bounds)?;
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
        })
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
                    engaged, intent, ..
                } => (*engaged, *intent) = (false, None),
                Role::Dock { hover, slots, .. } => {
                    *hover = None;
                    slots.fill(Tween::rest(0.0));
                }
            }
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
    fn paint_notch(&mut self, time: &str, date: &str) -> Result<()> {
        let Role::Notch { shadow, area, .. } = &mut self.role else {
            return Ok(());
        };
        let progress = self.motion.value(Instant::now());
        // Input and drawing use the same frame progress, without a speculative growth lead.
        let rows = layout::notch_rows(self.bounds, self.dpi, progress);
        if *area != rows {
            self.window.set_notch_region(&rows)?;
            *area = rows;
        }
        let scene = NotchScene {
            time,
            date,
            progress,
            dpi: self.dpi,
        };
        shadow.paint(&scene, self.window.0)?;
        self.canvas.paint_notch(&scene)
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

/// Hover-intent swell: the pill grows ~11 x 3 DIP to acknowledge the pointer before opening.
const NOTCH_SWELL: f32 = 0.07;
/// Opens the notch fully, ending any pending hover intent.
fn open_notch(intent: &mut Option<Instant>, tween: &mut Tween) {
    *intent = None;
    tween.retarget(1.0, motion::OPEN_TIME, motion::OPEN);
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

struct Desktop {
    surfaces: Vec<Surface>,
    /// Last settled topology; each monitor in it carries every shown kind.
    monitors: Vec<Monitor>,
    /// Kinds shown on every monitor; Show/Hide commands and closing a surface apply to all monitors.
    shown: Vec<SurfaceKind>,
    /// When the next topology read is due, and how many transient reads this change has retried.
    topology_due: Option<Instant>,
    topology_retries: u32,
    /// Monitor currently covered by a full-screen app; its surfaces are hidden, not released.
    fullscreen: Option<String>,
    /// Catalog generation seen by the last full-screen check, and the next shell poll, armed only
    /// while the foreground window covers its monitor.
    fullscreen_generation: Option<u64>,
    fullscreen_due: Option<Instant>,
    graphics: Graphics,
    rebuild: Rebuild,
    picker: Option<Picker>,
    icons: IconService,
    dock_items: Vec<DockItem>,
    dock_names: Vec<String>,
    dock_generation: Option<u64>,
    /// Media session state, no UI yet. Declared before `text` so its WinRT objects and event tokens
    /// are released before that field's `RoUninitialize`.
    media: Media,
    /// Windows text size (Accessibility > Text size) scales the whole notch, whose only content is text.
    text: TextSettings,
    text_factor: f64,
    /// Last drawn time text; the notches repaint only when it changes.
    clock: String,
    date: String,
    /// Just after the next minute boundary; until then a wake reuses the drawn text unformatted.
    clock_due: Instant,
}
impl Desktop {
    fn create(probe: &Probe) -> Result<Self> {
        let monitors = topology::settle(native_window::monitors()?)
            .ok_or_else(|| Error::new(E_FAIL, "No usable display monitor."))?;
        let text = TextSettings::create()?;
        let text_factor = text.factor()?;
        let mut desktop = Self {
            surfaces: Vec::new(),
            monitors,
            shown: Vec::new(),
            topology_due: None,
            topology_retries: 0,
            fullscreen: native_window::fullscreen_candidate()
                .filter(|_| native_window::shell_fullscreen()),
            fullscreen_generation: None,
            fullscreen_due: None,
            graphics: Graphics::create()?,
            rebuild: Rebuild::new(Instant::now()),
            picker: None,
            icons: IconService::create()?,
            dock_items: Vec::new(),
            dock_names: dock::names(&[], &WindowModel::default()),
            dock_generation: None,
            media: Media::create(),
            text,
            text_factor,
            clock: String::new(),
            date: String::new(),
            clock_due: Instant::now(),
        };
        desktop.show(SurfaceKind::Notch)?;
        if *probe == Probe::Initialization {
            return Err(Error::new(
                E_FAIL,
                "Injected failure after first composed window.",
            ));
        }
        desktop.show(SurfaceKind::Dock)?;
        // After the first paints, so loading the media component never delays them. WinRT is
        // already initialized on this thread by `TextSettings`.
        desktop.media.start();
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
        let mut surface = Surface::create(&self.graphics, kind, monitor, dpi, self.dock_slots())?;
        if kind == SurfaceKind::Notch {
            // Advances every existing notch first, so all monitors show the same minute.
            self.refresh_clock()?;
            surface.paint_notch(&self.clock, &self.date)?;
            self.surfaces.push(surface);
        } else {
            surface.publish_accessibility(&self.dock_names, false);
            self.surfaces.push(surface);
            // Sized after the push: icons are extracted once, for the densest dock.
            let icons = self.dock_icons();
            self.surfaces[self.surfaces.len() - 1].paint_dock(&self.dock_items, &icons)?;
        }
        // Shown after its first paint, and never on the monitor a full-screen app covers.
        let visible = self.fullscreen.as_ref() != Some(&monitor.device);
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
                for notch in self
                    .surfaces
                    .iter_mut()
                    .filter(|s| s.kind() == SurfaceKind::Notch && s.visible)
                {
                    notch.paint_notch(&self.clock, &self.date)?;
                }
            }
        }
        Ok(milliseconds_until(self.clock_due, now))
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
                    SurfaceKind::Notch => surface.paint_notch(&self.clock, &self.date)?,
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
    fn open_picker(&mut self, catalog: &Catalog) -> Result<()> {
        self.picker.take();
        self.picker = Some(Picker::create(catalog.model())?);
        Ok(())
    }
    /// Rebuilds dock items only when the catalog generation or an icon changed.
    fn refresh_dock(&mut self, model: &WindowModel) -> Result<()> {
        let icons_arrived = self.icons.collect();
        if self.dock_generation == Some(model.generation) && !icons_arrived {
            return Ok(());
        }
        if self.dock_generation != Some(model.generation) {
            self.dock_generation = Some(model.generation);
            let items = dock::items(model);
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
    /// One icon set for every dock, extracted for the densest dock; lower-DPI docks scale it down.
    // shortcut: mixed-DPI docks resample icons; keep one cache size per DPI if that looks soft on hardware.
    fn dock_icons(&mut self) -> Vec<Option<Arc<IconPixels>>> {
        let densest = self
            .surfaces
            .iter()
            .filter(|s| s.kind() == SurfaceKind::Dock)
            .map(|s| s.dpi)
            .max();
        let Some(dpi) = densest else {
            return Vec::new();
        };
        let size = dock::px(dock::ICON_DIPS, dpi) as u32;
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
                    dock::perform(action);
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
    fn command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::ShowPicker => {}
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
    fn message(&mut self, message: &MSG) -> Result<()> {
        if message.message == native_window::REPOSITION {
            // Every surface forwards display/DPI/settings/time changes, resume and taskbar recreation;
            // one coalesced read serves them all.
            self.request_topology();
            // A clock, time-zone or locale change, or a resume, can alter the text before the deadline.
            self.clock_due = Instant::now();
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
            WM_MOUSEMOVE => {
                let entered = surface.track_pointer()?;
                if let Role::Dock { hover, .. } = surface.role {
                    let slot = surface.dock_slot(message.lParam, slots);
                    if entered || slot != hover {
                        surface.hover_dock(slot, true);
                    }
                } else {
                    let x = i32::from((message.lParam.0 & 0xffff) as u16 as i16);
                    let y = i32::from((message.lParam.0 >> 16) as u16 as i16);
                    let progress = surface.motion.value(Instant::now());
                    let width = surface.bounds.width;
                    let over = layout::notch_hit(width, surface.dpi, progress, x, y);
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
        // Geometry first: the shell is asked, and polled, only while a window covers its monitor.
        let candidate = native_window::fullscreen_candidate();
        self.fullscreen_due = candidate.as_ref().map(|_| now + FULLSCREEN_POLL);
        let fullscreen = candidate.filter(|_| native_window::shell_fullscreen());
        if fullscreen == self.fullscreen {
            return Ok(());
        }
        self.fullscreen = fullscreen;
        let icons = self.dock_icons();
        for surface in &mut self.surfaces {
            let visible = self.fullscreen.as_ref() != Some(&surface.monitor);
            if visible && !surface.visible {
                match surface.kind() {
                    SurfaceKind::Notch => surface.paint_notch(&self.clock, &self.date)?,
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
    /// One pass of state refreshes and frames; returns the clock wait and whether animation runs.
    fn update(&mut self, model: &WindowModel) -> Result<(u32, bool)> {
        self.refresh_topology()?;
        if self.text.take_changed() {
            self.rescale_text()?;
        }
        self.media.refresh();
        self.refresh_dock(model)?;
        self.refresh_fullscreen(model.generation)?;
        let clock_wait = self.refresh_clock()?;
        if let Some(picker) = &mut self.picker {
            picker.synchronize(model)?;
            if picker.closed() {
                self.picker.take();
            }
        }
        Ok((clock_wait, self.animate()?))
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
        self.surfaces.retain(|s| s.kind() != SurfaceKind::Notch);
        if self.shown.contains(&SurfaceKind::Notch) {
            self.show(SurfaceKind::Notch)?;
        }
        Ok(())
    }
}

pub fn run(probe: Probe, diagnostics: Option<std::path::PathBuf>) -> Result<()> {
    let _apartment = Apartment::enter()?;
    let _class = WindowClass::register()?;
    let controls = Controls::create()?;
    let mut desktop = Desktop::create(&probe)?;
    #[cfg(debug_assertions)]
    if probe == Probe::DeviceLoss {
        let removed = windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED;
        desktop.survive(Err(Error::from(removed)), ())?;
    }
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
    let mut catalog = Catalog::create(diagnostics)?;
    let result = event_loop(&controls, &mut desktop, &mut catalog);
    result.and(catalog.shutdown())
}

fn event_loop(controls: &Controls, desktop: &mut Desktop, catalog: &mut Catalog) -> Result<()> {
    let events = controls.handles();
    loop {
        catalog.service()?;
        desktop.refresh_graphics()?;
        let pass = desktop.update(catalog.model());
        let (clock_wait, animating) = desktop.survive(pass, (INFINITE, false))?;
        let timeout = catalog
            .wait_milliseconds()
            .min(clock_wait)
            .min(desktop.topology_wait())
            .min(desktop.fullscreen_wait())
            .min(desktop.rebuild.wait(Instant::now()));
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
            if matches!(Command::ALL[index], Command::ShowPicker) {
                desktop.open_picker(catalog)?;
            } else {
                let shown = desktop.command(Command::ALL[index]);
                if !desktop.survive(shown, true)? {
                    return Ok(());
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
            if message.message == WM_LBUTTONUP
                && let Some(dock) = desktop.dock_at(message.hwnd)
            {
                let slot = dock.dock_slot(message.lParam, desktop.dock_slots());
                desktop.activate_slot(slot, catalog)?;
                continue;
            }
            if message.message == dock_uia::INVOKE && desktop.dock_at(message.hwnd).is_some() {
                let (slot, version) = (message.wParam.0, message.lParam.0 as u32);
                desktop.invoke_dock(message.hwnd, slot, version, catalog)?;
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

#[cfg(test)]
mod clock_wait_tests {
    use super::*;
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
