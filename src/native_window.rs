use crate::layout::{Bounds, SurfaceKind};
use std::{
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
};
use windows::Win32::{
    Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::{
        CreateRectRgn, DeleteObject, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
        MonitorFromPoint, SetWindowRgn, ValidateRect,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};
use windows::core::{Error, Result, w};

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
    /// Limits input and visible pixels to a window-relative rectangle; `None` restores the whole window.
    /// Rounded outlines are drawn anti-aliased by Direct2D, so the region only has to contain them.
    pub fn set_region(&self, area: Option<Bounds>) -> Result<()> {
        // SAFETY: owned live HWND; a created region's ownership transfers to the window only on success.
        unsafe {
            let Some(area) = area else {
                return match SetWindowRgn(self.0, None, false) {
                    0 => Err(Error::from_thread()),
                    _ => Ok(()),
                };
            };
            let region = CreateRectRgn(area.x, area.y, area.x + area.width, area.y + area.height);
            if region.0.is_null() {
                return Err(Error::from_thread());
            }
            if SetWindowRgn(self.0, Some(region), false) == 0 {
                let failure = Error::from_thread();
                let _ = DeleteObject(region.into());
                return Err(failure);
            }
            Ok(())
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
    let rect = monitor.rcWork;
    Ok(Bounds {
        x: rect.left,
        y: rect.top,
        width: rect.right - rect.left,
        height: rect.bottom - rect.top,
    })
}

pub fn primary_dpi(work: Bounds) -> Result<u32> {
    // A hidden primary-monitor HWND observes current DPI after scale changes since sign-in.
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
    // SAFETY: the embedded PerMonitorV2 manifest applies to this owned live primary-monitor HWND.
    let dpi = unsafe { GetDpiForWindow(probe.0) };
    if dpi == 0 {
        return Err(Error::new(
            E_FAIL,
            "Cannot read current primary-monitor DPI.",
        ));
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
