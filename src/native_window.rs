use crate::{
    dock,
    layout::{Bounds, SurfaceKind},
    topology::{self, Monitor},
    window_model::WindowKey,
};
use std::{
    mem::size_of,
    sync::atomic::{AtomicU32, Ordering},
};
use windows::Win32::{
    Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
    Graphics::Gdi::{
        DeleteObject, EnumDisplayMonitors, ExtCreateRegion, GetMonitorInfoW, HDC, HMONITOR,
        MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW,
        MonitorFromPoint, MonitorFromWindow, RDH_RECTANGLES, RGNDATA, RGNDATAHEADER, SetWindowRgn,
        ValidateRect,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState},
        WindowsAndMessaging::*,
    },
};
use windows::core::{BOOL, Error, HSTRING, Result, w};

pub const REPOSITION: u32 = WM_APP + 1;
pub const CLOSE_SURFACE: u32 = WM_APP + 2;
#[cfg(debug_assertions)]
pub const PANIC_PROBE: u32 = WM_APP + 3;
/// Posted when a surface loses the foreground; only a keyboard-focused notch ever has it.
pub const DEACTIVATED: u32 = WM_APP + 6;
/// Posted on a power-status change (plugged in, unplugged, low battery); the host reads the status.
pub const POWER_STATUS: u32 = WM_APP + 7;
pub const CLASS: windows::core::PCWSTR = w!("ROQA.NativeSurface.1");
/// Explorer broadcasts this registered message to every top-level window when it (re)creates the
/// taskbar. 0 until registered, and stays 0 if registration fails.
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

pub struct NativeWindow(pub HWND);
impl NativeWindow {
    pub fn create(bounds: Bounds, kind: SurfaceKind) -> Result<Self> {
        let title = match kind {
            SurfaceKind::Notch => w!("ROQA Notch"),
            SurfaceKind::Dock => w!("ROQA Dock"),
        };
        Self::create_hidden(bounds, title)
    }
    pub fn create_hidden(bounds: Bounds, title: windows::core::PCWSTR) -> Result<Self> {
        Self::create_styled(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            bounds,
            title,
        )
    }
    /// A hidden composited popup that can take the foreground (for the dock menu's keyboard).
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
            Ok(Self(hwnd))
        }
    }
    /// A hidden topmost popup that never takes focus, for a DWM thumbnail; its title is its UIA name.
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
            Ok(Self(hwnd))
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
        // SAFETY: this owned HWND is live; hiding keeps it, its canvas and its UIA provider alive.
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
    handles
        .into_iter()
        .map(|handle| {
            let info = monitor_info(handle)?;
            let work = bounds(info.monitorInfo.rcWork);
            Ok(Monitor {
                device: device_name(&info),
                work,
                dpi: dpi_at(work)?,
            })
        })
        .collect()
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
            let _ = PostMessageW(Some(hwnd), POWER_STATUS, WPARAM(0), LPARAM(0));
            return LRESULT(1); // TRUE grants the broadcast
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
            WM_GETOBJECT => crate::dock_uia::get_object(hwnd, wparam, lparam)
                .unwrap_or_else(|| DefWindowProcW(hwnd, message, wparam, lparam)),
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
    use super::*;
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
