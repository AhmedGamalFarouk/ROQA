//! The shelf's operating-system edges (M5 S6): the notch's `IDropTarget`, the standard file dialog and the
//! shell open/reveal. Nothing here opens, reads or stats a file; paths only travel from the shell to the pure
//! `shelf` model, and back to the shell when the user activates an entry.

use crate::{
    dock,
    shelf::{MAX_DROP, MAX_PATH_CHARS},
};
use std::{cell::Cell, cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, POINTL},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, DVASPECT_CONTENT, FORMATETC, IDataObject,
                TYMED_HGLOBAL,
            },
            Ole::{
                CF_HDROP, DROPEFFECT, DROPEFFECT_LINK, DROPEFFECT_NONE, IDropTarget,
                IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{
            DragQueryFileW, FOS_ALLOWMULTISELECT, FOS_DONTADDTORECENT, FOS_FILEMUSTEXIST,
            FOS_FORCEFILESYSTEM, FileOpenDialog, HDROP, IFileOpenDialog, SIGDN_FILESYSPATH,
        },
    },
    core::{Ref, Result, implement},
};

/// What the drop target tells the host; read once per host pass, so a drop adds no wake of its own.
pub enum DropEvent {
    /// A file drag is over the notch (true) or left it (false): the notch opens, or closes again.
    Hover(HWND, bool),
    Files(Vec<String>),
}
pub type Drops = Rc<RefCell<Vec<DropEvent>>>;

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// The paths of a `CF_HDROP` drop, at most `MAX_DROP`. Reads names only: no file is opened or stat'ed,
/// so a UNC target cannot stall the host here.
fn dropped_files(data: &IDataObject) -> Vec<String> {
    // SAFETY: the medium is owned until `ReleaseStgMedium`; the HDROP is only read through
    // `DragQueryFileW`, with buffers sized from its own length answer.
    unsafe {
        let Ok(mut medium) = data.GetData(&hdrop_format()) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            let hdrop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(hdrop, u32::MAX, None).min(MAX_DROP as u32);
            for index in 0..count {
                let length = DragQueryFileW(hdrop, index, None) as usize;
                if length == 0 || length > MAX_PATH_CHARS {
                    continue;
                }
                let mut buffer = vec![0u16; length + 1];
                let copied = DragQueryFileW(hdrop, index, Some(&mut buffer)) as usize;
                names.extend(String::from_utf16(&buffer[..copied.min(length)]).ok());
            }
        }
        ReleaseStgMedium(&mut medium);
        names
    }
}

#[implement(IDropTarget)]
struct DropTarget {
    hwnd: HWND,
    drops: Drops,
    /// The drag now over the notch carries files; set at enter, so over/drop need not ask again.
    accepts: Cell<bool>,
}

fn answer(effect: *mut DROPEFFECT, accepts: bool) {
    if !effect.is_null() {
        // SAFETY: OLE passes a valid, writable effect for the call's duration; checked non-null above.
        unsafe {
            *effect = if accepts {
                DROPEFFECT_LINK
            } else {
                DROPEFFECT_NONE
            };
        }
    }
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        // SAFETY: `data` is the live object OLE hands this call; the format is a plain value.
        let files = data
            .as_ref()
            .is_some_and(|data| unsafe { data.QueryGetData(&hdrop_format()) }.is_ok());
        self.accepts.set(files);
        if files {
            self.drops
                .borrow_mut()
                .push(DropEvent::Hover(self.hwnd, true));
        }
        answer(effect, files);
        Ok(())
    }
    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        answer(effect, self.accepts.get());
        Ok(())
    }
    fn DragLeave(&self) -> Result<()> {
        if self.accepts.replace(false) {
            self.drops
                .borrow_mut()
                .push(DropEvent::Hover(self.hwnd, false));
        }
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let files = if self.accepts.replace(false) {
            data.as_ref().map(dropped_files).unwrap_or_default()
        } else {
            Vec::new()
        };
        answer(effect, !files.is_empty());
        if !files.is_empty() {
            // The pointer is still over the notch: it stays open until the pointer leaves.
            self.drops.borrow_mut().push(DropEvent::Files(files));
        } else {
            self.drops
                .borrow_mut()
                .push(DropEvent::Hover(self.hwnd, false));
        }
        Ok(())
    }
}

/// Makes `hwnd` (a notch) accept file drops. OLE holds its own reference until `revoke`.
pub fn register(hwnd: HWND, drops: &Drops) -> Result<()> {
    let target: IDropTarget = DropTarget {
        hwnd,
        drops: Rc::clone(drops),
        accepts: Cell::new(false),
    }
    .into();
    // SAFETY: `hwnd` is our own live window on this OLE-initialised STA thread.
    unsafe { RegisterDragDrop(hwnd, &target) }
}

/// Undoes `register`; call before destroying the window. A window that never registered just errors.
pub fn revoke(hwnd: HWND) {
    // SAFETY: scalar window handle of our own live window.
    let _ = unsafe { RevokeDragDrop(hwnd) };
}

/// The standard file dialog, multi-select, file-system items only. Cancel, or any failure, is an empty list.
pub fn pick_files(owner: HWND) -> Vec<String> {
    let picked = || -> Result<Vec<String>> {
        // SAFETY: the dialog and its result objects are used and released on this thread; each returned
        // name is freed with `CoTaskMemFree` after it is copied.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                dialog.GetOptions()?
                    | FOS_ALLOWMULTISELECT
                    | FOS_FORCEFILESYSTEM
                    | FOS_FILEMUSTEXIST
                    | FOS_DONTADDTORECENT,
            )?;
            dialog.Show(Some(owner))?;
            let items = dialog.GetResults()?;
            let count = items.GetCount()?.min(MAX_DROP as u32);
            let mut paths = Vec::new();
            for index in 0..count {
                let name = items.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)?;
                paths.extend(name.to_string().ok());
                CoTaskMemFree(Some(name.0.cast()));
            }
            Ok(paths)
        }
    };
    picked().unwrap_or_default()
}

/// Opens `path` with its default app, or with `reveal` selects it in Explorer. The shell does the work, on
/// the user's activation only. A UNC target may block on the network, so that call runs on its own short
/// thread (one per click) instead of the host's.
pub fn open(path: &str, reveal: bool, network: bool) {
    let (file, parameters) = if reveal {
        // `"` cannot occur: `shelf::validate` rejects it.
        (
            "explorer.exe".to_owned(),
            Some(format!("/select,\"{path}\"")),
        )
    } else {
        (path.to_owned(), None)
    };
    if !network {
        dock::launch_with(&file, parameters.as_deref());
        return;
    }
    let _ = std::thread::Builder::new()
        .name("roqa-shelf-open".into())
        .spawn(move || {
            // SAFETY: this thread's own STA for the shell call, balanced below.
            let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
            dock::launch_with(&file, parameters.as_deref());
            if com {
                // SAFETY: balances the successful initialisation above on this same thread.
                unsafe { CoUninitialize() };
            }
        });
}
