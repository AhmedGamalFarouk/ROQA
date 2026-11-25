//! The notification-area icon. Right-click opens a plain Windows menu (owner decision 2026-11-25): it is
//! modal, so ROQA's frames pause only while it is open. The icon lives on the host's hidden wake window.
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, POINT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{LIM_SMALL, LoadIconMetric},
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, HICON,
            MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_NONOTIFY,
            TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU, WM_NULL,
            WM_RBUTTONUP,
        },
    },
};
use windows::core::{HSTRING, PCWSTR, w};

/// The shell's callback for the icon; its mouse event is the low word of `lparam`.
pub const CALLBACK: u32 = WM_APP + 122;
/// Posted by the window procedure on a right-click, so the host loop opens the menu.
pub const MENU: u32 = WM_APP + 123;

/// Set by Restart: `main` starts a new ROQA once this one has ended and let go of its instance lock.
static RESTART: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Settings,
    Hide,
    Restart,
    Quit,
}

/// The menu's rows, top to bottom; `hidden` names the visibility row for what it will do.
fn rows(hidden: bool) -> [(Choice, &'static str); 4] {
    [
        (Choice::Settings, "Settings"),
        (
            Choice::Hide,
            if hidden {
                "Show dock and notch"
            } else {
                "Hide dock and notch"
            },
        ),
        (Choice::Restart, "Restart ROQA"),
        (Choice::Quit, "Quit ROQA"),
    ]
}

/// Whether a shell callback is a right-click (or the keyboard's menu key) on the icon.
pub fn wants_menu(lparam: LPARAM) -> bool {
    matches!(lparam.0 as u32 & 0xFFFF, WM_RBUTTONUP | WM_CONTEXTMENU)
}

pub struct Tray {
    window: HWND,
    icon: HICON,
}

impl Tray {
    /// Puts ROQA's icon in the notification area; `None` only when the icon cannot be loaded.
    pub fn add(window: HWND) -> Option<Self> {
        // SAFETY: this module's own icon resource; the handle is destroyed in Drop.
        let icon = unsafe {
            let module = GetModuleHandleW(None).ok()?;
            LoadIconMetric(Some(HINSTANCE(module.0)), w!("ROQA_ICON"), LIM_SMALL).ok()?
        };
        let tray = Self { window, icon };
        tray.show();
        Some(tray)
    }

    /// Adds the icon; called again after Explorer restarts, whose new taskbar forgets every icon.
    /// An icon that is already there makes this a harmless no-op.
    pub fn show(&self) {
        // SAFETY: a fully initialised, correctly sized struct for our own live window.
        let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &self.data()) };
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        for (slot, unit) in data.szTip.iter_mut().zip("ROQA".encode_utf16()) {
            *slot = unit;
        }
        data
    }

    /// Shows the menu at the pointer and returns the row picked, if any.
    pub fn menu(&self, hidden: bool) -> Option<Choice> {
        let rows = rows(hidden);
        // SAFETY: a menu made, tracked and destroyed here, owned by our own live window.
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            for (index, (choice, label)) in rows.iter().enumerate() {
                if *choice == Choice::Restart {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                let _ = AppendMenuW(menu, MF_STRING, index + 1, &HSTRING::from(*label));
            }
            let mut at = POINT::default();
            let _ = GetCursorPos(&mut at);
            // Without the foreground a click elsewhere would not close the menu (a documented TrackPopupMenu rule).
            let _ = SetForegroundWindow(self.window);
            let picked = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
                at.x,
                at.y,
                None,
                self.window,
                None,
            )
            .0 as usize;
            let _ = PostMessageW(Some(self.window), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            picked
                .checked_sub(1)
                .and_then(|index| rows.get(index))
                .map(|(choice, _)| *choice)
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: removes our own icon, then frees the handle it used.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data());
            let _ = DestroyIcon(self.icon);
        }
    }
}

/// Asks `main` to start ROQA again after this run ends.
pub fn ask_restart() {
    RESTART.store(true, Ordering::Release);
}

/// Starts a new ROQA if Restart was picked; called after the run has released everything.
pub fn restart_if_asked() {
    if RESTART.load(Ordering::Acquire)
        && let Err(error) =
            std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn())
    {
        eprintln!("Restart: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_click_and_menu_key_open_the_menu_and_the_hide_row_names_its_action() {
        assert!(wants_menu(LPARAM(WM_RBUTTONUP as isize)));
        assert!(wants_menu(LPARAM(WM_CONTEXTMENU as isize | 1 << 16)));
        assert!(!wants_menu(LPARAM(0x0202))); // left button up
        assert_eq!(rows(false)[1].1, "Hide dock and notch");
        assert_eq!(rows(true)[1].1, "Show dock and notch");
        assert_eq!(rows(false).map(|row| row.0)[3], Choice::Quit);
    }
}
