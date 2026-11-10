use crate::{
    dock,
    layout::{Bounds, SurfaceKind},
    topology::{self, Monitor},
};
use std::{
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
};
use windows::Win32::{
    Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC,
        HMONITOR, MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW,
        MonitorFromPoint, MonitorFromWindow, RGN_OR, SetRectRgn, SetWindowRgn, ValidateRect,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState},
        WindowsAndMessaging::*,
    },
};
use windows::core::{BOOL, Error, Result, w};

pub const REPOSITION: u32 = WM_APP + 1;
pub const CLOSE_SURFACE: u32 = WM_APP + 2;
pub const PANIC_PROBE: u32 = WM_APP + 3;
pub const CLASS: windows::core::PCWSTR = w!("ROQA.NativeSurface.1");

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
    fn create_hidden(bounds: Bounds, title: windows::core::PCWSTR) -> Result<Self> {
        // SAFETY: registered class; no retained user pointers; validated geometry and current module.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
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
    /// Transfers a silhouette region to Windows; the scratch row and any failed result stay owned here.
    pub fn set_notch_region(&self, rows: &[Bounds]) -> Result<()> {
        // SAFETY: owned HWND; all rows are bounded window-relative pixels. Only a successful
        // SetWindowRgn transfers ownership. Both temporary GDI regions are cleaned on every error.
        unsafe {
            let region = CreateRectRgn(0, 0, 0, 0);
            if region.0.is_null() {
                return Err(Error::from_thread());
            }
            let row = CreateRectRgn(0, 0, 0, 0);
            if row.0.is_null() {
                let failure = Error::from_thread();
                let _ = DeleteObject(region.into());
                return Err(failure);
            }
            let applied = (|| {
                for span in rows {
                    SetRectRgn(row, span.x, span.y, span.x + span.width, span.y + 1).ok()?;
                    if CombineRgn(Some(region), Some(region), Some(row), RGN_OR).0 == 0 {
                        return Err(Error::from_thread());
                    }
                }
                if SetWindowRgn(self.0, Some(region), false) == 0 {
                    return Err(Error::from_thread());
                }
                Ok(())
            })();
            let _ = DeleteObject(row.into());
            if applied.is_err() {
                let _ = DeleteObject(region.into());
            }
            applied
        }
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
            let (device, info) = monitor_info(handle)?;
            let work = bounds(info.rcWork);
            Ok(Monitor {
                device,
                work,
                dpi: dpi_at(work)?,
            })
        })
        .collect()
}

/// Device name and geometry of one monitor; fails once its HMONITOR no longer exists.
fn monitor_info(handle: HMONITOR) -> Result<(String, MONITORINFO)> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: cbSize selects the MONITORINFOEXW layout, which begins with MONITORINFO.
    unsafe { GetMonitorInfoW(handle, &raw mut info as *mut MONITORINFO).ok()? };
    let name = &info.szDevice;
    let length = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    Ok((String::from_utf16_lossy(&name[..length]), info.monitorInfo))
}

/// Device name of the monitor whose whole area the foreground window covers while Windows reports a
/// full-screen app. Windows sends no notification when full-screen apps start or stop, so callers
/// re-check on events and a slow schedule. Any failed query reads as "not full screen", which keeps
/// the surfaces visible rather than hiding them on a guess.
pub fn fullscreen_device() -> Option<String> {
    // SAFETY: shell state query with an out value owned by the binding.
    let state = unsafe { SHQueryUserNotificationState() }.ok()?;
    if state != QUNS_BUSY && state != QUNS_RUNNING_D3D_FULL_SCREEN {
        return None;
    }
    // Visible foreign top-level window only; ROQA's own windows never suppress ROQA.
    let key = dock::foreground_key().filter(|key| dock::valid_target(*key))?;
    let window = HWND(key.handle as *mut _);
    // QUNS_BUSY also covers Presentation Settings, where the desktop spans every monitor.
    // shortcut: a maximized window can still read as full screen under Presentation Settings
    // when the work area equals the monitor (auto-hide taskbar); compare frames if users hit it.
    let mut class = [0u16; 16];
    // SAFETY: bounded writable buffer; a stale HWND yields 0 characters.
    let length = unsafe { GetClassNameW(window, &mut class) }.max(0) as usize;
    if matches!(
        String::from_utf16_lossy(&class[..length]).as_str(),
        "Progman" | "WorkerW"
    ) {
        return None;
    }
    let mut rect = RECT::default();
    // SAFETY: scalar geometry queries on a transient foreign HWND; a stale one fails cleanly.
    let monitor = unsafe {
        GetWindowRect(window, &mut rect).ok()?;
        MonitorFromWindow(window, MONITOR_DEFAULTTONULL)
    };
    let (device, info) = monitor_info(monitor).ok()?;
    topology::covers(bounds(rect), bounds(info.rcMonitor)).then_some(device)
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
    let outcome = catch_unwind(AssertUnwindSafe(|| dispatch(hwnd, message, wparam, lparam)));
    outcome.unwrap_or_else(|_| {
        // SAFETY: queues a nonzero exit on this UI thread; Rust unwinding never crosses the callback.
        unsafe {
            PostQuitMessage(70);
        }
        LRESULT(0)
    })
}

fn dispatch(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: callback HWND/parameters come from Windows; no pointer parameters are dereferenced here.
    unsafe {
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
            WM_DPICHANGED | WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
                let dpi = if message == WM_DPICHANGED {
                    wparam
                } else {
                    WPARAM(0)
                };
                let _ = PostMessageW(Some(hwnd), REPOSITION, dpi, LPARAM(0));
                LRESULT(0)
            }
            WM_GETOBJECT => crate::dock_uia::get_object(hwnd, wparam, lparam)
                .unwrap_or_else(|| DefWindowProcW(hwnd, message, wparam, lparam)),
            PANIC_PROBE => panic!("Intentional callback-boundary probe"),
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
