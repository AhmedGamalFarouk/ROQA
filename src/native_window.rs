use crate::layout::{Bounds, SurfaceKind};
use std::{
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
};
use windows::Win32::{
    Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY,
        MONITORINFO, MonitorFromPoint, RGN_OR, SetRectRgn, SetWindowRgn, ValidateRect,
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
