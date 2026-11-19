//! The OS side of the Shelf's Copied list: the clipboard-update listener on a notch window (event driven, no
//! polling), reading and writing Unicode text. Text the source marks as private (password managers) is skipped.
//! Nothing here logs clipboard contents.

use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{
            DataExchange::{
                AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
                IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
                RemoveClipboardFormatListener, SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        },
    },
    core::w,
};

/// `CF_UNICODETEXT`.
const TEXT: u32 = 13;
/// Longest text read, in UTF-16 units: a few times `clipboard::MAX_CHARS`, so a huge copy is never scanned.
const READ_UNITS: usize = 2048;

/// Starts (or stops) `WM_CLIPBOARDUPDATE` for `hwnd`. Repeating either is harmless.
pub fn watch(hwnd: HWND, on: bool) {
    // SAFETY: our own live window handle.
    let _ = unsafe {
        if on {
            AddClipboardFormatListener(hwnd)
        } else {
            RemoveClipboardFormatListener(hwnd)
        }
    };
}

/// The clipboard may be held by its writer: one try, never a wait on the host thread; a busy clipboard skips the copy.
fn open(hwnd: HWND) -> bool {
    // SAFETY: our own live window handle.
    unsafe { OpenClipboard(Some(hwnd)) }.is_ok()
}

/// The current clipboard text; `None` for no text, a private source, or a clipboard that stayed busy.
pub fn read(hwnd: HWND) -> Option<String> {
    if !open(hwnd) {
        return None;
    }
    // SAFETY: the clipboard is open until the close below.
    let text = unsafe { read_open() };
    // SAFETY: balances the successful open above.
    let _ = unsafe { CloseClipboard() };
    text
}

/// # Safety
/// The clipboard must be open on this thread.
unsafe fn read_open() -> Option<String> {
    // SAFETY: the clipboard is open (caller contract); handles are used only while it is.
    unsafe {
        IsClipboardFormatAvailable(TEXT).ok()?;
        if private() {
            return None;
        }
        let global = HGLOBAL(GetClipboardData(TEXT).ok()?.0);
        let units = GlobalLock(global).cast::<u16>();
        if units.is_null() {
            return None;
        }
        let room = (GlobalSize(global) / 2).min(READ_UNITS);
        let mut len = 0;
        while len < room && *units.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(units, len));
        let _ = GlobalUnlock(global);
        Some(text)
    }
}

/// The copy source asked to stay out of monitors and history (`ExcludeClipboardContentFromMonitorProcessing`,
/// `CanIncludeInClipboardHistory` = 0).
///
/// # Safety
/// The clipboard must be open on this thread.
unsafe fn private() -> bool {
    // SAFETY: the clipboard is open (caller contract).
    unsafe {
        if IsClipboardFormatAvailable(RegisterClipboardFormatW(w!(
            "ExcludeClipboardContentFromMonitorProcessing"
        )))
        .is_ok()
        {
            return true;
        }
        let history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        if IsClipboardFormatAvailable(history).is_err() {
            return false;
        }
        let Ok(data) = GetClipboardData(history) else {
            return false;
        };
        let global = HGLOBAL(data.0);
        let value = GlobalLock(global).cast::<u32>();
        let allowed = value.is_null() || *value != 0;
        let _ = GlobalUnlock(global);
        !allowed
    }
}

/// Puts `text` on the clipboard as Unicode text; false when it could not.
pub fn write(hwnd: HWND, text: &str) -> bool {
    if !open(hwnd) {
        return false;
    }
    let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
    // SAFETY: the clipboard is open until the close below; the block is ours until SetClipboardData takes it.
    let done = unsafe {
        let _ = EmptyClipboard();
        match GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) {
            Ok(global) => {
                let target = GlobalLock(global).cast::<u16>();
                if target.is_null() {
                    let _ = GlobalFree(Some(global));
                    false
                } else {
                    std::ptr::copy_nonoverlapping(units.as_ptr(), target, units.len());
                    let _ = GlobalUnlock(global);
                    let set = SetClipboardData(TEXT, Some(HANDLE(global.0))).is_ok();
                    if !set {
                        let _ = GlobalFree(Some(global));
                    }
                    set
                }
            }
            Err(_) => false,
        }
    };
    // SAFETY: balances the successful open above.
    let _ = unsafe { CloseClipboard() };
    done
}
