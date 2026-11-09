use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows::{
    Foundation::TypedEventHandler,
    UI::ViewManagement::UISettings,
    Win32::{
        Foundation::{LPARAM, WPARAM},
        System::{
            Threading::GetCurrentThreadId,
            WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize},
        },
        UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
    },
    core::Result,
};

struct Runtime;
impl Runtime {
    fn enter() -> Result<Self> {
        // SAFETY: caller is the existing STA UI thread. Every successful call is balanced by this guard.
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
        }
        Ok(Self)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // SAFETY: UISettings and its subscription are released before this same-thread guard.
        unsafe {
            RoUninitialize();
        }
    }
}

pub struct TextSettings {
    settings: UISettings,
    token: i64,
    changed: Arc<AtomicBool>,
    _runtime: Runtime,
}
impl TextSettings {
    pub fn create() -> Result<Self> {
        let runtime = Runtime::enter()?;
        let settings = UISettings::new()?;
        let changed = Arc::new(AtomicBool::new(false));
        let pending = Arc::clone(&changed);
        // SAFETY: capture a scalar UI thread ID, never a borrowed Rust object or HWND.
        let thread = unsafe { GetCurrentThreadId() };
        let token = settings.TextScaleFactorChanged(&TypedEventHandler::new(move |_, _| {
            pending.store(true, Ordering::Relaxed);
            // SAFETY: scalar wake only. A lost wake is recovered by the main loop's finite wait.
            let _ = unsafe { PostThreadMessageW(thread, WM_APP + 30, WPARAM(0), LPARAM(0)) };
            Ok(())
        }))?;
        Ok(Self {
            settings,
            token,
            changed,
            _runtime: runtime,
        })
    }
    pub fn factor(&self) -> Result<f64> {
        self.settings.TextScaleFactor()
    }
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}
impl Drop for TextSettings {
    fn drop(&mut self) {
        if let Err(error) = self.settings.RemoveTextScaleFactorChanged(self.token) {
            eprintln!("Text scale event cleanup: {error}");
        }
    }
}
