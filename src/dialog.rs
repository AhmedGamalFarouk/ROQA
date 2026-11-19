use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Controls::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result},
};

pub(crate) const ACTION: u32 = WM_APP + 40;
pub(crate) const RELAYOUT: u32 = WM_APP + 41;
pub(crate) const SCROLL: u32 = WM_APP + 64;
pub(crate) const CLOSE: usize = 2;
pub(crate) const DIALOG_STYLE: u32 = WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_BORDER.0;

#[repr(C, align(4))]
struct EmptyDialog {
    header: DLGTEMPLATE,
    menu: u16,
    class: u16,
    title: u16,
}

/// A visible child control of `parent` (standard classes only); the settings window shares it.
pub(crate) fn control(
    parent: HWND,
    class: PCWSTR,
    title: &str,
    id: usize,
    style: u32,
    extended: WINDOW_EX_STYLE,
) -> Result<HWND> {
    let title = wide(title);
    // SAFETY: parent lives throughout synchronous creation; class/title strings are terminated and not retained.
    unsafe {
        CreateWindowExW(
            extended,
            class,
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
            0,
            0,
            1,
            1,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(HINSTANCE(GetModuleHandleW(None)?.0)),
            None,
        )
    }
}
/// An empty standard dialog (caption and close box) whose commands `dialog_proc` posts to the host loop.
/// The settings window reuses it.
pub(crate) fn empty_dialog(cx: i16, cy: i16, proc: DLGPROC) -> Result<HWND> {
    // SAFETY: initialize standard themed classes before creating this process's first interactive controls.
    unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        })
        .ok()?;
    }
    let template = EmptyDialog {
        header: DLGTEMPLATE {
            style: DIALOG_STYLE,
            cx,
            cy,
            ..Default::default()
        },
        menu: 0,
        class: 0,
        title: 0,
    };
    // SAFETY: properly aligned, zero-child standard template remains live throughout synchronous creation.
    let dialog = unsafe {
        CreateDialogIndirectParamW(
            Some(HINSTANCE(GetModuleHandleW(None)?.0)),
            &template.header,
            None,
            proc,
            LPARAM(0),
        )
    }?;
    crate::native_window::capture_affinity(dialog);
    Ok(dialog)
}
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
pub(crate) unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    _: LPARAM,
) -> isize {
    let (kind, argument) = match message {
        WM_COMMAND => (ACTION, wparam),
        WM_CLOSE => (ACTION, WPARAM(CLOSE)),
        WM_DPICHANGED | WM_SETTINGCHANGE => (RELAYOUT, WPARAM(0)),
        WM_VSCROLL => (SCROLL, wparam),
        WM_MOUSEWHEEL => (
            SCROLL,
            WPARAM(if (wparam.0 >> 16) as i16 > 0 {
                SB_LINEUP.0 as usize
            } else {
                SB_LINEDOWN.0 as usize
            }),
        ),
        _ => return 0,
    };
    // SAFETY: no Rust pointers/callback state. All mutation happens later in the outer loop, never reentrantly.
    if unsafe { PostMessageW(Some(hwnd), kind, argument, LPARAM(0)) }.is_err() {
        // SAFETY: orderly failure on the owning UI thread.
        unsafe {
            PostQuitMessage(71);
        }
    }
    1
}
