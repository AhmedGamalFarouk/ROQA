use crate::{
    dock,
    layout::{Bounds, SurfaceKind},
    topology::{self, Monitor},
    window_model::WindowKey,
};
use std::{
    mem::size_of,
    sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
};
use windows::Win32::{
    Foundation::{COLORREF, E_FAIL, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
    Graphics::Gdi::{
        DeleteObject, EnumDisplayMonitors, ExtCreateRegion, GetMonitorInfoW, HDC, HMONITOR,
        MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW,
        MonitorFromPoint, MonitorFromWindow, RDH_RECTANGLES, RGNDATA, RGNDATAHEADER, SetBkColor,
        SetTextColor, SetWindowRgn, ValidateRect,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Power::{
            HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
            UnregisterPowerSettingNotification,
        },
        RemoteDesktop::{
            NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
            WTSUnRegisterSessionNotification,
        },
        SystemServices::GUID_CONSOLE_DISPLAY_STATE,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{
            ABE_BOTTOM, ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE, APPBARDATA, QUNS_BUSY,
            QUNS_RUNNING_D3D_FULL_SCREEN, SHAppBarMessage, SHQueryUserNotificationState,
        },
        WindowsAndMessaging::*,
    },
};
use windows::core::{BOOL, Error, GUID, HSTRING, Result, w};

pub const REPOSITION: u32 = WM_APP + 1;
pub const CLOSE_SURFACE: u32 = WM_APP + 2;
#[cfg(debug_assertions)]
pub const PANIC_PROBE: u32 = WM_APP + 3;
/// Posted when a window loses the foreground; only an open menu (the one window that takes it) ever has it.
pub const DEACTIVATED: u32 = WM_APP + 6;
/// Posted on a power-status change (plugged in, unplugged, low battery); the host reads the status.
pub const POWER_STATUS: u32 = WM_APP + 7;
/// A `POWER_STATUS` post is queued and the host has not read the status yet.
static POWER_PENDING: AtomicBool = AtomicBool::new(false);

/// ROQA's windows are left out of screen recordings and shares (the Settings switch).
static HIDE_CAPTURE: AtomicBool = AtomicBool::new(false);

/// Applies the capture choice to one window; every window made after the switch calls it too.
pub fn capture_affinity(window: HWND) {
    let mode = if HIDE_CAPTURE.load(Ordering::Relaxed) {
        WDA_EXCLUDEFROMCAPTURE
    } else {
        WDA_NONE
    };
    // SAFETY: a scalar call on one of our own windows; older Windows refuses it and the window stays visible.
    let _ = unsafe { SetWindowDisplayAffinity(window, mode) };
}

unsafe extern "system" fn capture_one(window: HWND, _: LPARAM) -> BOOL {
    capture_affinity(window);
    BOOL(1)
}

/// Switches the choice for every open ROQA window (all live on the host thread) and each made later.
pub fn hide_from_capture(on: bool) {
    if HIDE_CAPTURE.swap(on, Ordering::Relaxed) != on {
        // SAFETY: the callback only reads the static and calls `capture_affinity` on the window it is given.
        let _ = unsafe {
            EnumThreadWindows(
                windows::Win32::System::Threading::GetCurrentThreadId(),
                Some(capture_one),
                LPARAM(0),
            )
        };
    }
}

/// Sets `flag`; true for the caller that found it clear (and so must post).
fn claim(flag: &AtomicBool) -> bool {
    !flag.swap(true, Ordering::AcqRel)
}

/// True once per power-status change: the host reads the status on every pass, so a dropped post (a modal
/// loop eats thread messages) costs nothing; the next broadcast posts again.
pub fn power_read() -> bool {
    POWER_PENDING.swap(false, Ordering::AcqRel)
}
/// Posted (once until read) when a drive, camera or audio output arrives; `device_read` says which.
pub const DEVICE: u32 = WM_APP + 13;
pub const DRIVE_BIT: u32 = 1;
pub const CAMERA_BIT: u32 = 2;
pub const SPEAKER_BIT: u32 = 4;
static DEVICE_BITS: AtomicU32 = AtomicU32::new(0);
/// The drive letters (bit 0 is A) of the arrivals since the last `drive_read`.
static DRIVE_MASK: AtomicU32 = AtomicU32::new(0);
/// The letter of the first drive that arrived since the last read, for the alert's Eject button.
pub fn drive_read() -> Option<char> {
    let mask = DRIVE_MASK.swap(0, Ordering::AcqRel);
    (mask != 0).then(|| char::from(b'A' + mask.trailing_zeros() as u8))
}
const CAMERA: GUID = GUID::from_u128(0xe5323777_f976_4f5b_9b55_b94699c46e44);
const SPEAKER: GUID = GUID::from_u128(0x65e8773e_8f56_11d0_a3b9_00a0c9223196);
/// The arrivals since the last read; the next arrival posts again.
pub fn device_read() -> u32 {
    DEVICE_BITS.swap(0, Ordering::AcqRel)
}
/// Which of the three alerts a device broadcast is, or 0: a drive (not a disc going in, not a network share
/// mapped at logon: `not_a_drive`), a camera, a speaker.
fn device_bit(kind: u32, interface: GUID, not_a_drive: bool) -> u32 {
    match kind {
        k if k == DBT_DEVTYP_VOLUME.0 && !not_a_drive => DRIVE_BIT,
        k if k == DBT_DEVTYP_DEVICEINTERFACE.0 && interface == CAMERA => CAMERA_BIT,
        k if k == DBT_DEVTYP_DEVICEINTERFACE.0 && interface == SPEAKER => SPEAKER_BIT,
        _ => 0,
    }
}
/// Posted (once until read) when the session unlocks or the display turns on: the mascot wakes.
pub const WAKE: u32 = WM_APP + 8;
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);
/// True once per wake event, read on every pass; the next event posts again.
pub fn wake_read() -> bool {
    WAKE_PENDING.swap(false, Ordering::AcqRel)
}
/// `WM_WTSSESSION_CHANGE` unlock, or a console-display-state setting that reads "on" (1).
fn wakes(message: u32, wparam: usize, display: Option<(windows::core::GUID, u8)>) -> bool {
    match message {
        WM_WTSSESSION_CHANGE => wparam == WTS_SESSION_UNLOCK as usize,
        WM_POWERBROADCAST => {
            display.is_some_and(|(guid, on)| guid == GUID_CONSOLE_DISPLAY_STATE && on == 1)
        }
        _ => false,
    }
}
pub const CLASS: windows::core::PCWSTR = w!("ROQA.NativeSurface.1");
/// Explorer broadcasts this registered message to every top-level window when it (re)creates the
/// taskbar. 0 until registered, and stays 0 if registration fails.
/// Posted (once until read) when the clipboard changes; the host reads it if the Copied list is on.
pub const CLIPBOARD: u32 = WM_APP + 9;
static CLIP_PENDING: AtomicBool = AtomicBool::new(false);
/// True once per burst of clipboard changes, read on every pass; the next change posts again.
pub fn clip_read() -> bool {
    CLIP_PENDING.swap(false, Ordering::AcqRel)
}
/// Whether the busy clipboard of the latest change was already tried a second time.
static CLIP_RETRIED: AtomicBool = AtomicBool::new(false);
/// The clipboard was busy: the next pass reads it once more, and only once for this change (a writer that holds it
/// longer loses that copy; a new change gets its own retry).
pub fn clip_again() {
    if !CLIP_RETRIED.swap(true, Ordering::AcqRel) {
        CLIP_PENDING.store(true, Ordering::Release);
    }
}

/// The hidden window the workers' wakes and the Caps Lock hotkey go to. A message box or a menu runs its own message
/// loop that dispatches a window's messages to its procedure but drops a thread's; 0 until the host makes it.
static WAKE_WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Records the window `post_wake` posts to and the Caps Lock hotkey belongs to.
pub fn set_wake_window(window: HWND) {
    WAKE_WINDOW.store(window.0 as isize, Ordering::Release);
}

/// The wake window, once the host made it.
pub fn wake_window() -> Option<HWND> {
    let raw = WAKE_WINDOW.load(Ordering::Acquire);
    (raw != 0).then_some(HWND(raw as *mut _))
}

/// Wakes the host loop with `message`, from any thread: a post to the wake window, or to the host `thread` before
/// that window exists. The host reads every result on its next pass, so the message carries nothing.
pub fn post_wake(thread: u32, message: u32) -> bool {
    // SAFETY: scalar messages; the window handle is the host's own, and a closed one just fails the post.
    #[expect(
        clippy::disallowed_methods,
        reason = "the fallback for the moments before the wake window exists; every later wake is a window post"
    )]
    unsafe {
        match wake_window() {
            Some(window) => PostMessageW(Some(window), message, WPARAM(0), LPARAM(0)),
            None => PostThreadMessageW(thread, message, WPARAM(0), LPARAM(0)),
        }
    }
    .is_ok()
}

/// Caps Lock was pressed while a modal loop (a message box) ran and was handed back there: the host says it after.
static CAPS_PENDING: AtomicBool = AtomicBool::new(false);
/// True once after such presses (the pill says the state now, not each press), read on every pass.
pub fn caps_read() -> bool {
    CAPS_PENDING.swap(false, Ordering::AcqRel)
}
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub struct WindowClass(HINSTANCE);
impl WindowClass {
    pub fn register() -> Result<Self> {
        // SAFETY: module and class definition are valid for this process; callback has static lifetime.
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None)?.0);
            let descriptor = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hIcon: LoadIconW(Some(instance), w!("ROQA_ICON"))?,
                ..Default::default()
            };
            if RegisterClassW(&descriptor) == 0 {
                return Err(Error::from_thread());
            }
            // On failure (0) an Explorer restart is still seen through the work-area/display messages.
            TASKBAR_CREATED.store(
                RegisterWindowMessageW(w!("TaskbarCreated")),
                Ordering::Relaxed,
            );
            Ok(Self(instance))
        }
    }
}
impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: run scopes ensure every surface is destroyed before unregistering its class.
        unsafe {
            if let Err(error) = UnregisterClassW(CLASS, Some(self.0)) {
                eprintln!("Class cleanup: {error}");
            }
        }
    }
}

/// The window and, for a notch, its display-state notification (unlock is a session notification, released by Drop).
pub struct NativeWindow(pub HWND, Option<HPOWERNOTIFY>, Vec<HDEVNOTIFY>);
impl NativeWindow {
    pub fn create(bounds: Bounds, kind: SurfaceKind) -> Result<Self> {
        let title = match kind {
            SurfaceKind::Notch => w!("ROQA Notch"),
            SurfaceKind::Dock => w!("ROQA Dock"),
        };
        let mut window = Self::create_hidden(bounds, title)?;
        if kind == SurfaceKind::Notch {
            window.watch_wake();
        }
        Ok(window)
    }
    /// Asks Windows to tell this window about a session unlock and the display coming on (no polling). A
    /// failed registration only loses the wake show.
    fn watch_wake(&mut self) {
        // SAFETY: our own live HWND; the registrations end in Drop, before the window is destroyed.
        unsafe {
            let _ = WTSRegisterSessionNotification(self.0, NOTIFY_FOR_THIS_SESSION);
            self.1 = RegisterPowerSettingNotification(
                HANDLE(self.0.0),
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
            .ok();
            // Camera and speaker interfaces arrive here; a drive letter arrives as a plain broadcast.
            for class in [CAMERA, SPEAKER] {
                let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                    dbcc_size: size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                    dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE.0,
                    dbcc_classguid: class,
                    ..Default::default()
                };
                if let Ok(handle) = RegisterDeviceNotificationW(
                    HANDLE(self.0.0),
                    (&raw const filter).cast(),
                    DEVICE_NOTIFY_WINDOW_HANDLE,
                ) {
                    self.2.push(handle);
                }
            }
        }
    }
    pub fn create_hidden(bounds: Bounds, title: windows::core::PCWSTR) -> Result<Self> {
        Self::create_styled(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            bounds,
            title,
        )
    }
    /// A hidden composited popup that can take the foreground (for the dock menu, which ends when it loses it).
    pub fn create_menu(bounds: Bounds, title: &str) -> Result<Self> {
        let title = HSTRING::from(title);
        Self::create_styled(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            bounds,
            windows::core::PCWSTR(title.as_ptr()),
        )
    }
    fn create_styled(
        style: WINDOW_EX_STYLE,
        bounds: Bounds,
        title: windows::core::PCWSTR,
    ) -> Result<Self> {
        // SAFETY: registered class; no retained user pointers; validated geometry and current module.
        unsafe {
            let hwnd = CreateWindowExW(
                style,
                CLASS,
                title,
                WS_POPUP,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                None,
                None,
                Some(HINSTANCE(GetModuleHandleW(None)?.0)),
                None,
            )?;
            capture_affinity(hwnd);
            Ok(Self(hwnd, None, Vec::new()))
        }
    }
    /// A hidden topmost popup that never takes focus, for a DWM thumbnail.
    pub fn create_preview(title: &str) -> Result<Self> {
        let title = HSTRING::from(title);
        // SAFETY: registered class; no retained user pointers; the title outlives the call.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                CLASS,
                &title,
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HINSTANCE(GetModuleHandleW(None)?.0)),
                None,
            )?;
            capture_affinity(hwnd);
            Ok(Self(hwnd, None, Vec::new()))
        }
    }
    /// A layered transparent HWND lets input reach windows in other processes, unlike HTTRANSPARENT.
    pub fn create_shadow(bounds: Bounds) -> Result<Self> {
        let window = Self::create_hidden(bounds, w!("ROQA Notch Shadow"))?;
        // SAFETY: owned hidden HWND; layered + transparent is Windows' cross-thread input bypass.
        unsafe {
            let style = GetWindowLongPtrW(window.0, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(
                window.0,
                GWL_EXSTYLE,
                (style | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize,
            );
            SetLayeredWindowAttributes(window.0, Default::default(), 255, LWA_ALPHA)?;
        }
        Ok(window)
    }
    /// Moves/resizes in place without activation or z-order change.
    pub fn resize(&self, bounds: Bounds) -> Result<()> {
        // SAFETY: owned live HWND; positive validated layout; explicitly nonactivating.
        unsafe {
            SetWindowPos(
                self.0,
                None,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
    }
    pub fn show(&self) {
        // SAFETY: this owned HWND is live; explicitly avoids activation.
        unsafe {
            let _ = ShowWindow(self.0, SW_SHOWNOACTIVATE);
        }
    }
    pub fn hide(&self) {
        // SAFETY: this owned HWND is live; hiding keeps it and its canvas alive.
        unsafe {
            let _ = ShowWindow(self.0, SW_HIDE);
        }
    }
    /// Transfers a silhouette region to Windows in one GDI call, rather than unioning every row.
    pub fn set_notch_region(&self, rows: &[Bounds]) -> Result<()> {
        let bytes = size_of::<RGNDATAHEADER>() + rows.len() * size_of::<RECT>();
        let mut data = vec![0u32; bytes.max(size_of::<RGNDATA>()).div_ceil(size_of::<u32>())];
        let header = RGNDATAHEADER {
            dwSize: size_of::<RGNDATAHEADER>() as u32,
            iType: RDH_RECTANGLES,
            nCount: rows.len() as u32,
            nRgnSize: (rows.len() * size_of::<RECT>()) as u32,
            rcBound: RECT {
                left: rows.iter().map(|r| r.x).min().unwrap_or(0),
                top: rows.first().map_or(0, |r| r.y),
                right: rows.iter().map(|r| r.x + r.width).max().unwrap_or(0),
                bottom: rows.last().map_or(0, |r| r.y + r.height),
            },
        };
        // SAFETY: u32 storage has the alignment of these Win32 structs and room for the header
        // and all sorted, nonoverlapping RECTs. SetWindowRgn alone transfers region ownership.
        unsafe {
            data.as_mut_ptr().cast::<RGNDATAHEADER>().write(header);
            let rectangles = data
                .as_mut_ptr()
                .cast::<u8>()
                .add(size_of::<RGNDATAHEADER>())
                .cast::<RECT>();
            for (index, row) in rows.iter().enumerate() {
                rectangles.add(index).write(RECT {
                    left: row.x,
                    top: row.y,
                    right: row.x + row.width,
                    bottom: row.y + row.height,
                });
            }
            let region = ExtCreateRegion(None, bytes as u32, data.as_ptr().cast());
            if region.0.is_null() {
                return Err(Error::from_thread());
            }
            if SetWindowRgn(self.0, Some(region), false) == 0 {
                let failure = Error::from_thread();
                let _ = DeleteObject(region.into());
                return Err(failure);
            }
        }
        Ok(())
    }
}
impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: sole owner destroys its HWND on its creating thread, after composition teardown.
        unsafe {
            if let Some(handle) = self.1.take() {
                let _ = UnregisterPowerSettingNotification(handle);
                let _ = WTSUnRegisterSessionNotification(self.0);
            }
            for handle in self.2.drain(..) {
                let _ = UnregisterDeviceNotification(handle);
            }
            if let Err(error) = DestroyWindow(self.0) {
                eprintln!("Window cleanup: {error}");
            }
        }
    }
}

fn bounds(rect: RECT) -> Bounds {
    Bounds {
        x: rect.left,
        y: rect.top,
        width: rect.right - rect.left,
        height: rect.bottom - rect.top,
    }
}

pub fn primary_work_area() -> Result<Bounds> {
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: MONITORINFO is initialized with the correct size; the primary monitor owns (0,0).
    unsafe {
        GetMonitorInfoW(
            MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY),
            &mut monitor,
        )
        .ok()?;
    }
    Ok(bounds(monitor.rcWork))
}

/// Upper bound on monitors per snapshot; more are ignored rather than growing work per topology event.
const MAX_MONITORS: usize = 16;

/// One enumeration pass: device name, work area and current DPI of each monitor. HMONITORs are used
/// only inside this call. A monitor that disappears mid-pass fails the pass; the caller retries.
pub fn monitors() -> Result<Vec<Monitor>> {
    let mut handles = Vec::<HMONITOR>::with_capacity(MAX_MONITORS);
    // SAFETY: synchronous enumeration; the callback writes only into `handles`, which outlives it.
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&raw mut handles as isize),
        )
        .ok()?;
    }
    let hides = taskbar_hides_at_bottom();
    handles
        .into_iter()
        .map(|handle| {
            let info = monitor_info(handle)?;
            let work = bounds(info.monitorInfo.rcWork);
            let rect = bounds(info.monitorInfo.rcMonitor);
            let dpi = dpi_at(work)?;
            Ok(Monitor {
                device: device_name(&info),
                work,
                rect,
                dpi,
                // The work area then reaches the screen's last row, and the taskbar (48 DIPs) slides over it.
                taskbar: if hides && work.y + work.height == rect.y + rect.height {
                    dock::px(48, dpi)
                } else {
                    0
                },
            })
        })
        .collect()
}

/// True when the taskbar is set to hide and sits at the bottom edge.
fn taskbar_hides_at_bottom() -> bool {
    let mut data = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    // SAFETY: `data` is a live, sized APPBARDATA for both queries; the shell fills it in place.
    unsafe {
        if SHAppBarMessage(ABM_GETSTATE, &mut data) as u32 & ABS_AUTOHIDE == 0 {
            return false;
        }
        SHAppBarMessage(ABM_GETTASKBARPOS, &mut data);
    }
    data.uEdge == ABE_BOTTOM
}

/// Geometry and device name of one monitor; fails once its HMONITOR no longer exists.
fn monitor_info(handle: HMONITOR) -> Result<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: cbSize selects the MONITORINFOEXW layout, which begins with MONITORINFO.
    unsafe { GetMonitorInfoW(handle, &raw mut info as *mut MONITORINFO).ok()? };
    Ok(info)
}

fn device_name(info: &MONITORINFOEXW) -> String {
    let name = &info.szDevice;
    let length = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    String::from_utf16_lossy(&name[..length])
}

/// A foreground candidate is admitted by the shell; retained candidates only need geometry checks.
pub fn fullscreen_candidate() -> Option<(WindowKey, String)> {
    let key = dock::foreground_key()?;
    fullscreen_monitor(key).map(|device| (key, device))
}

/// Monitor of a foreign foreground window; ROQA focus must preserve remembered suppression.
pub fn foreground_monitor(key: WindowKey) -> Option<String> {
    if !dock::valid_target(key) {
        return None;
    }
    // SAFETY: scalar query on a revalidated transient foreign HWND.
    let monitor = unsafe { MonitorFromWindow(HWND(key.handle as *mut _), MONITOR_DEFAULTTONULL) };
    monitor_info(monitor).ok().map(|info| device_name(&info))
}

/// The monitor and frame of a visible, un-minimized, non-desktop foreign window; `None` otherwise.
pub fn window_frame(key: WindowKey) -> Option<(String, Bounds)> {
    if !dock::valid_target(key) {
        return None;
    }
    let window = HWND(key.handle as *mut _);
    let mut class = [0u16; 16];
    // SAFETY: bounded writable buffer; a stale HWND yields 0 characters.
    let length = usize::try_from(unsafe { GetClassNameW(window, &mut class) }).ok()?;
    let named = |name: &str| name.encode_utf16().eq(class[..length].iter().copied());
    let mut rect = RECT::default();
    // SAFETY: scalar geometry queries on a transient foreign HWND; a stale one fails cleanly.
    let (iconic, monitor) = unsafe {
        GetWindowRect(window, &mut rect).ok()?;
        (
            IsIconic(window).as_bool(),
            MonitorFromWindow(window, MONITOR_DEFAULTTONULL),
        )
    };
    // A window on another virtual desktop is cloaked: it covers nothing here.
    let mut cloaked = 0u32;
    // SAFETY: DWM writes one DWORD to this owned buffer; a failed read counts as not cloaked.
    let _ = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            &raw mut cloaked as *mut _,
            size_of::<u32>() as u32,
        )
    };
    if iconic || cloaked != 0 || length == 0 || named("Progman") || named("WorkerW") {
        return None;
    }
    let info = monitor_info(monitor).ok()?;
    Some((device_name(&info), bounds(rect)))
}

/// A stale, hidden, cloaked or minimized window cannot keep a monitor suppressed. Revalidate its PID too,
/// so an HWND reused by another process cannot inherit suppression.
pub fn fullscreen_monitor(key: WindowKey) -> Option<String> {
    if !dock::valid_target(key) {
        return None;
    }
    let window = HWND(key.handle as *mut _);
    // SAFETY: scalar query on a revalidated transient foreign HWND.
    let mut cloaked = 0u32;
    // SAFETY: DWM writes one DWORD to this owned buffer; unknown visibility cannot suppress ROQA.
    let visible = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            &raw mut cloaked as *mut _,
            size_of::<u32>() as u32,
        )
    };
    // SAFETY: scalar query on the revalidated transient foreign HWND.
    if visible.is_err() || cloaked != 0 || unsafe { IsIconic(window) }.as_bool() {
        return None;
    }
    // The desktop spans every monitor; under Presentation Settings (also QUNS_BUSY) it would read
    // as full screen.
    // shortcut: a maximized window can still read as full screen under Presentation Settings
    // when the work area equals the monitor (auto-hide taskbar); compare frames if users hit it.
    let mut class = [0u16; 16];
    // SAFETY: bounded writable buffer; a stale HWND yields 0 characters.
    let length = usize::try_from(unsafe { GetClassNameW(window, &mut class) }).ok()?;
    let class = &class[..length];
    let named = |name: &str| name.encode_utf16().eq(class.iter().copied());
    if length == 0 || named("Progman") || named("WorkerW") {
        return None;
    }
    let mut rect = RECT::default();
    // SAFETY: scalar geometry queries on a transient foreign HWND; a stale one fails cleanly.
    let monitor = unsafe {
        GetWindowRect(window, &mut rect).ok()?;
        MonitorFromWindow(window, MONITOR_DEFAULTTONULL)
    };
    let info = monitor_info(monitor).ok()?;
    let area = bounds(info.monitorInfo.rcMonitor);
    topology::covers(bounds(rect), area).then(|| device_name(&info))
}

/// Whether Windows reports a full-screen app (or exclusive Direct3D). Windows sends no notification
/// when one starts or stops, and a failed query reads as false.
pub fn shell_fullscreen() -> bool {
    // SAFETY: shell state query with an out value owned by the binding.
    unsafe { SHQueryUserNotificationState() }
        .is_ok_and(|state| state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN)
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    handles: LPARAM,
) -> BOOL {
    // SAFETY: `monitors` passes its exclusively borrowed Vec for the duration of the enumeration.
    let handles = unsafe { &mut *(handles.0 as *mut Vec<HMONITOR>) };
    // Pushing within the reserved capacity never reallocates, so nothing here can unwind.
    if handles.len() < handles.capacity() {
        handles.push(monitor);
    }
    BOOL::from(true)
}

fn dpi_at(work: Bounds) -> Result<u32> {
    // A hidden HWND on the monitor observes its current DPI after scale changes since sign-in.
    // It is never shown or composed, and RAII destroys it on this same thread.
    let probe = NativeWindow::create_hidden(
        Bounds {
            x: work.x,
            y: work.y,
            width: 1,
            height: 1,
        },
        w!("ROQA DPI Probe"),
    )?;
    // SAFETY: the embedded PerMonitorV2 manifest applies to this owned live HWND.
    let dpi = unsafe { GetDpiForWindow(probe.0) };
    if dpi == 0 {
        return Err(Error::new(E_FAIL, "Cannot read current monitor DPI."));
    }
    Ok(dpi)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // A panic here never unwinds: release is `panic = "abort"` and the hook exits 70 first.
    dispatch(hwnd, message, wparam, lparam)
}

fn dispatch(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: callback HWND/parameters come from Windows; no pointer parameters are dereferenced here.
    unsafe {
        if message == WM_POWERBROADCAST && wparam.0 == PBT_APMPOWERSTATUSCHANGE as usize {
            // Every window gets the broadcast: one thread post until the host has read the status.
            if claim(&POWER_PENDING)
                && PostMessageW(None, POWER_STATUS, WPARAM(0), LPARAM(0)).is_err()
            {
                POWER_PENDING.store(false, Ordering::Release);
            }
            return LRESULT(1); // TRUE grants the broadcast
        }
        if message == WM_DEVICECHANGE && wparam.0 == DBT_DEVICEARRIVAL as usize && lparam.0 != 0 {
            let header = &*(lparam.0 as *const DEV_BROADCAST_HDR);
            let kind = header.dbch_devicetype.0;
            let interface = if kind == DBT_DEVTYP_DEVICEINTERFACE.0 {
                (*(lparam.0 as *const DEV_BROADCAST_DEVICEINTERFACE_W)).dbcc_classguid
            } else {
                GUID::zeroed()
            };
            let not_a_drive = kind == DBT_DEVTYP_VOLUME.0
                && (*(lparam.0 as *const DEV_BROADCAST_VOLUME)).dbcv_flags.0
                    & (DBTF_MEDIA.0 | DBTF_NET.0)
                    != 0;
            let bit = device_bit(kind, interface, not_a_drive);
            if bit == DRIVE_BIT {
                let volume = &*(lparam.0 as *const DEV_BROADCAST_VOLUME);
                DRIVE_MASK.fetch_or(volume.dbcv_unitmask & 0x03FF_FFFF, Ordering::AcqRel);
            }
            if bit != 0
                && DEVICE_BITS.fetch_or(bit, Ordering::AcqRel) == 0
                && PostMessageW(None, DEVICE, WPARAM(0), LPARAM(0)).is_err()
            {
                DEVICE_BITS.store(0, Ordering::Release);
            }
            return LRESULT(1);
        }
        if message == WM_HOTKEY && crate::keys::Keys::is_caps(wparam.0) {
            // Only a modal loop (a message box, a menu) dispatches it here; the host loop takes the hotkey before dispatch.
            // The key goes back at once so it toggles now, however many presses; only its pill waits for the host.
            if crate::keys::hand_back() {
                CAPS_PENDING.store(true, Ordering::Release);
            }
            return LRESULT(0);
        }
        if message == WM_ENDSESSION && wparam.0 != 0 {
            // Signing out or shutting down: the process may end as soon as this returns.
            crate::store::save_unsaved();
            return LRESULT(0);
        }
        if message == WM_CLIPBOARDUPDATE {
            CLIP_RETRIED.store(false, Ordering::Release);
            if claim(&CLIP_PENDING) && PostMessageW(None, CLIPBOARD, WPARAM(0), LPARAM(0)).is_err()
            {
                CLIP_PENDING.store(false, Ordering::Release);
            }
            return LRESULT(0);
        }
        let display = (message == WM_POWERBROADCAST
            && wparam.0 == PBT_POWERSETTINGCHANGE as usize
            && lparam.0 != 0)
            .then(|| {
                let setting = &*(lparam.0 as *const POWERBROADCAST_SETTING);
                (setting.PowerSetting, setting.Data[0])
            });
        if wakes(message, wparam.0, display) {
            if claim(&WAKE_PENDING) && PostMessageW(None, WAKE, WPARAM(0), LPARAM(0)).is_err() {
                WAKE_PENDING.store(false, Ordering::Release);
            }
            return LRESULT(isize::from(message == WM_POWERBROADCAST));
        }
        if let Some(reconnect) =
            repositions(message, wparam, TASKBAR_CREATED.load(Ordering::Relaxed))
        {
            let _ = PostMessageW(
                Some(hwnd),
                REPOSITION,
                WPARAM(reconnect.into()),
                LPARAM(message as isize),
            );
            // WM_POWERBROADCAST is handled by returning TRUE; the others by returning 0.
            return LRESULT(isize::from(message == WM_POWERBROADCAST));
        }
        match message {
            WM_CTLCOLOREDIT => {
                let brush = GetPropW(hwnd, w!("ROQA.EditBrush"));
                if !brush.is_invalid() {
                    let context = HDC(wparam.0 as *mut _);
                    SetTextColor(
                        context,
                        COLORREF(GetPropW(hwnd, w!("ROQA.EditInk")).0 as usize as u32),
                    );
                    SetBkColor(
                        context,
                        COLORREF(GetPropW(hwnd, w!("ROQA.EditFill")).0 as usize as u32),
                    );
                    LRESULT(brush.0 as isize)
                } else {
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = PostMessageW(Some(hwnd), CLOSE_SURFACE, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if wparam.0 & 0xffff == WA_INACTIVE as usize {
                    let _ = PostMessageW(Some(hwnd), DEACTIVATED, WPARAM(0), LPARAM(0));
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            #[cfg(debug_assertions)]
            PANIC_PROBE => panic!("Intentional callback-boundary probe"),
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

/// Messages after which the host re-reads the topology and the clock: display, DPI, settings and
/// time changes; resume, which re-arms the clock whether or not the monotonic clock counted the
/// sleep (wait timeouts do not) and re-reads monitors changed while asleep; and a recreated
/// taskbar (Explorer restart), which can change the work area. Every surface receives these
/// broadcasts; the host coalesces the posts. `Some(true)` for resume and a new taskbar, after which
/// media and audio may have missed events and reconnect.
fn repositions(message: u32, wparam: WPARAM, taskbar_created: u32) -> Option<bool> {
    match message {
        WM_DPICHANGED | WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_TIMECHANGE => {
            Some(false)
        }
        // Sent on every resume, with or without a user present; suspend changes nothing visible.
        WM_POWERBROADCAST => (wparam.0 == PBT_APMRESUMEAUTOMATIC as usize).then_some(true),
        _ => (taskbar_created != 0 && message == taskbar_created).then_some(true),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn device_arrivals_map_to_one_alert_each() {
        use super::*;
        let none = GUID::zeroed();
        assert_eq!(device_bit(DBT_DEVTYP_VOLUME.0, none, false), DRIVE_BIT);
        assert_eq!(device_bit(DBT_DEVTYP_VOLUME.0, none, true), 0); // a disc going in
        assert_eq!(
            device_bit(DBT_DEVTYP_DEVICEINTERFACE.0, CAMERA, false),
            CAMERA_BIT
        );
        assert_eq!(
            device_bit(DBT_DEVTYP_DEVICEINTERFACE.0, SPEAKER, false),
            SPEAKER_BIT
        );
        assert_eq!(device_bit(DBT_DEVTYP_DEVICEINTERFACE.0, none, false), 0);
    }
    use super::*;
    #[test]
    fn one_power_post_serves_every_window_until_the_host_reads() {
        let pending = AtomicBool::new(false);
        // Many windows get the same broadcast: only the first posts.
        let posts = (0..6).filter(|_| claim(&pending)).count();
        assert_eq!(posts, 1);
        // After the host reads, the next change posts again, once.
        pending.store(false, Ordering::Release);
        assert!(claim(&pending) && !claim(&pending));
    }

    #[test]
    fn a_flag_whose_post_was_dropped_is_still_read_once_by_the_pass() {
        // The post was lost (a modal loop): the flag stays set, no later change posts, and the pass reads it.
        assert!(claim(&CLIP_PENDING) && !claim(&CLIP_PENDING));
        assert!(clip_read() && !clip_read(), "read once");
        assert!(claim(&CLIP_PENDING), "the next change posts again");
        assert!(clip_read());
    }

    #[test]
    fn unlock_and_display_on_wake_and_nothing_else_does() {
        let on = |v| Some((GUID_CONSOLE_DISPLAY_STATE, v));
        assert!(wakes(
            WM_WTSSESSION_CHANGE,
            WTS_SESSION_UNLOCK as usize,
            None
        ));
        assert!(!wakes(WM_WTSSESSION_CHANGE, 7, None)); // lock
        assert!(wakes(WM_POWERBROADCAST, 0, on(1)));
        assert!(!wakes(WM_POWERBROADCAST, 0, on(0)) && !wakes(WM_POWERBROADCAST, 0, on(2)));
        assert!(!wakes(
            WM_POWERBROADCAST,
            0,
            Some((windows::core::GUID::zeroed(), 1))
        ));
        assert!(!wakes(WM_NULL, 0, None));
    }

    #[test]
    fn repositions_on_display_resume_and_taskbar_recreation_and_only_the_last_two_reconnect() {
        let taskbar = 0xC123;
        for message in [
            WM_DPICHANGED,
            WM_DISPLAYCHANGE,
            WM_SETTINGCHANGE,
            WM_SYSCOLORCHANGE,
            WM_TIMECHANGE,
        ] {
            assert_eq!(repositions(message, WPARAM(0), taskbar), Some(false));
        }
        // Only resume and a new taskbar also reconnect media and audio.
        let resume = WPARAM(PBT_APMRESUMEAUTOMATIC as usize);
        assert_eq!(repositions(WM_POWERBROADCAST, resume, taskbar), Some(true));
        for event in [PBT_APMSUSPEND, PBT_APMRESUMESUSPEND, PBT_POWERSETTINGCHANGE] {
            let event = WPARAM(event as usize);
            assert_eq!(repositions(WM_POWERBROADCAST, event, taskbar), None);
        }
        assert_eq!(repositions(taskbar, WPARAM(0), taskbar), Some(true));
        // An unregistered message (0) must never match WM_NULL.
        assert_eq!(repositions(WM_NULL, WPARAM(0), 0), None);
        assert_eq!(repositions(WM_MOUSEMOVE, WPARAM(0), taskbar), None);
    }
}
