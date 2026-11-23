//! The shelf's operating-system edges (M5 S6): the notch's `IDropTarget`, the standard file dialog and the
//! shell open/reveal and the share sheet. Nothing here opens, reads or stats a file; paths only travel from the shell to the pure
//! `shelf` model, and back to the shell when the user activates an entry.

use crate::{
    dock,
    shelf::{MAX_DROP, MAX_PATH_CHARS},
};
use std::{cell::Cell, cell::RefCell, rc::Rc};
use windows::{
    ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
    Foundation::TypedEventHandler,
    Storage::{IStorageItem, StorageFile},
    Win32::{
        Foundation::{COLORREF, RECT},
        UI::{
            Shell::IDataTransferManagerInterop,
            WindowsAndMessaging::{
                CreateWindowExW, GetWindowRect, HWND_TOPMOST, LWA_ALPHA, SWP_SHOWWINDOW,
                SetForegroundWindow, SetLayeredWindowAttributes, SetWindowPos, WS_EX_LAYERED,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::w,
};
use windows::{
    Win32::{
        Foundation::{HWND, POINTL},
        Storage::FileSystem::GetDriveTypeW,
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
    core::{HSTRING, Interface, PCWSTR, Ref, Result, implement},
};
use windows_collections::IIterable;
use windows_future::{AsyncOperationCompletedHandler, IAsyncOperation};

/// What the drop target tells the host; read once per host pass, so a drop adds no wake of its own.
pub enum DropEvent {
    /// A file drag is over the notch (true) or left it (false): the notch opens, or closes again.
    Hover(HWND, bool),
    Files(Vec<String>),
    /// Files dropped on a dock at this screen column: the app under it opens them.
    Dock(HWND, i32, Vec<String>),
    /// A file drag is over a dock at this screen column, or (`None`) has left it or dropped.
    DockOver(HWND, Option<i32>),
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
    /// A dock takes the drop but never opens for the hover (the notch does, for the shelf).
    dock: bool,
    /// The drag now over the notch carries files; set at enter, so over/drop need not ask again.
    accepts: Cell<bool>,
    /// The screen column last reported over a dock, so a drag resting still adds no events.
    column: Cell<i32>,
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

impl DropTarget {
    fn over(&self, x: i32) {
        self.column.set(x);
        self.drops
            .borrow_mut()
            .push(DropEvent::DockOver(self.hwnd, Some(x)));
    }
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        // SAFETY: `data` is the live object OLE hands this call; the format is a plain value.
        let files = data
            .as_ref()
            .is_some_and(|data| unsafe { data.QueryGetData(&hdrop_format()) }.is_ok());
        self.accepts.set(files);
        if files && !self.dock {
            self.drops
                .borrow_mut()
                .push(DropEvent::Hover(self.hwnd, true));
        }
        if files && self.dock {
            self.over(point.x);
        }
        answer(effect, files);
        Ok(())
    }
    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        if self.dock && self.accepts.get() && point.x != self.column.get() {
            self.over(point.x);
        }
        answer(effect, self.accepts.get());
        Ok(())
    }
    fn DragLeave(&self) -> Result<()> {
        if self.accepts.replace(false) {
            if self.dock {
                self.drops
                    .borrow_mut()
                    .push(DropEvent::DockOver(self.hwnd, None));
            } else {
                self.drops
                    .borrow_mut()
                    .push(DropEvent::Hover(self.hwnd, false));
            }
        }
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let accepted = self.accepts.replace(false);
        let files = if accepted {
            data.as_ref().map(dropped_files).unwrap_or_default()
        } else {
            Vec::new()
        };
        if accepted && self.dock {
            self.drops
                .borrow_mut()
                .push(DropEvent::DockOver(self.hwnd, None));
        }
        answer(effect, !files.is_empty());
        if self.dock {
            if !files.is_empty() {
                self.drops
                    .borrow_mut()
                    .push(DropEvent::Dock(self.hwnd, point.x, files));
            }
        } else if !files.is_empty() {
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

/// Makes `hwnd` (a notch, or with `dock` a dock) accept file drops. OLE holds its own reference until `revoke`.
pub fn register(hwnd: HWND, drops: &Drops, dock: bool) -> Result<()> {
    let target: IDropTarget = DropTarget {
        hwnd,
        drops: Rc::clone(drops),
        dock,
        accepts: Cell::new(false),
        column: Cell::new(i32::MIN),
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

/// Multi-select file-system dialog. Cancellation is empty; other failures are reported without paths.
fn pick_files(cancel: &std::sync::Arc<std::sync::Mutex<DialogState>>, one: bool) -> Vec<String> {
    let picked = || -> Result<Vec<String>> {
        // SAFETY: the dialog and its result objects are used and released on this thread; each returned
        // name is freed with `CoTaskMemFree` after it is copied.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                dialog.GetOptions()?
                    | if one {
                        Default::default()
                    } else {
                        FOS_ALLOWMULTISELECT
                    }
                    | FOS_FORCEFILESYSTEM
                    | FOS_FILEMUSTEXIST
                    | FOS_DONTADDTORECENT,
            )?;
            let events: windows::Win32::UI::Shell::IFileDialogEvents = DialogEvents {
                state: cancel.clone(),
            }
            .into();
            let cookie = dialog.Advise(&events)?;
            let shown = if cancel.lock().map_or(true, |state| state.cancelled) {
                Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800704C7u32 as i32,
                )))
            } else {
                dialog.Show(None)
            };
            if let Ok(mut state) = cancel.lock() {
                state.hwnd = 0;
            }
            if let Err(error) = dialog.Unadvise(cookie) {
                eprintln!("Shelf dialog event cleanup: {error}");
            }
            shown?;
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
    match picked() {
        Ok(paths) => paths,
        Err(error) => {
            if error.code().0 != 0x800704C7u32 as i32 {
                eprintln!("Shelf file picker: {error}");
            }
            Vec::new()
        }
    }
}

/// Opens `path` with its default app, or with `reveal` selects it in Explorer. The shell does the work, on
/// the user's activation only. A UNC target may block on the network, so that call runs on its own short
/// worker instead of the host. Busy requests are ignored: no waiting queue is retained.
static NETWORK_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct NetworkPermit;
impl NetworkPermit {
    fn acquire() -> Option<Self> {
        NETWORK_BUSY
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()
            .map(|_| Self)
    }
}
struct Sta;
impl Sta {
    fn enter() -> Result<Self> {
        // SAFETY: each caller is a newly created worker, and this guard never crosses threads.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Sta {
    fn drop(&mut self) {
        // SAFETY: balances successful initialization on this same thread, after its COM objects drop.
        unsafe {
            CoUninitialize();
        }
    }
}
impl Drop for NetworkPermit {
    fn drop(&mut self) {
        NETWORK_BUSY.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Whether it started (a network open is only handed to its worker; its failure comes later, `network_failed`).
pub fn open(path: &str, reveal: bool, network: bool) -> bool {
    let (file, parameters) = if reveal {
        // `"` cannot occur: `shelf::validate` rejects it.
        (
            "explorer.exe".to_owned(),
            Some(format!("/select,\"{path}\"")),
        )
    } else {
        (path.to_owned(), None)
    };
    if network {
        open_network(&file, parameters.as_deref())
    } else {
        dock::shell(&file, parameters.as_deref(), false)
    }
}

/// Set when a network launch failed on its worker; `network_failed` reads it once.
static NETWORK_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Posted to the host when that happens; it only wakes the loop.
const FAILED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 63;

/// Whether a network launch failed since the last call.
pub fn network_failed() -> bool {
    NETWORK_FAILED.swap(false, std::sync::atomic::Ordering::AcqRel)
}

/// Starts `file` through the shell on the one network worker, never on the host thread. False when one is still
/// running or no worker could start; a failure on the worker is read with `network_failed`.
pub fn open_network(file: &str, parameters: Option<&str>) -> bool {
    let Some(permit) = NetworkPermit::acquire() else {
        return false;
    };
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let (file, parameters) = (file.to_owned(), parameters.map(str::to_owned));
    let spawned = std::thread::Builder::new()
        .name("roqa-shelf-open".into())
        .spawn(move || {
            let _permit = permit; // released on completion or unwind; failed spawn drops the closure.
            let opened = match Sta::enter() {
                Ok(_sta) => dock::shell(&file, parameters.as_deref(), true),
                Err(error) => {
                    eprintln!("Shelf network COM initialization: {error}");
                    false
                }
            };
            if !opened {
                eprintln!("Shelf network shell launch failed");
                NETWORK_FAILED.store(true, std::sync::atomic::Ordering::Release);
                let _ = crate::native_window::post_wake(host, FAILED);
            }
        });
    if spawned.is_err() {
        eprintln!("Shelf network launch worker unavailable");
    }
    spawned.is_ok()
}

/// `GetDriveType` of the path's drive letter, 0 (unknown) for anything else. It asks the mount manager, not
/// the network, so a mapped drive is classified (`shelf::DRIVE_REMOTE`) without touching its share.
pub fn drive_type(path: &str) -> u32 {
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return 0;
    }
    let root = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: `root` is a NUL-terminated `X:\` that outlives the call.
    unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) }
}

/// A UNC path or a mapped drive: nothing may touch it on the host thread (`open` with `network`).
pub fn is_network(path: &str) -> bool {
    path.starts_with("\\\\") || drive_type(path) == crate::shelf::DRIVE_REMOTE
}

pub const PICKED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 60;

thread_local! {
    /// The share sheet's owner window, request handler and its token. Windows shows the sheet only for a visible
    /// window that can be activated, which the notch never is: a 1 px, see-through, click-through popup at the
    /// notch stands in. It is made once and reused; a new share replaces the handler, so none pile up.
    static SHARING: RefCell<Option<(HWND, DataTransferManager, i64)>> = const { RefCell::new(None) };
}

/// Opens the Windows share sheet for one file at `notch` (works without app packaging). The sheet asks for the
/// file later; it is resolved on a WinRT worker, never on the host thread.
pub fn share(notch: HWND, path: &str) -> Result<()> {
    let mut rect = RECT::default();
    // SAFETY: `notch` is a live window of this thread; `rect` outlives the call.
    unsafe { GetWindowRect(notch, &mut rect)? };
    let owner = match SHARING.with_borrow(|slot| slot.as_ref().map(|slot| slot.0)) {
        Some(owner) => owner,
        // SAFETY: a system class, no parent and no retained pointers; the layered alpha is set before it shows.
        None => unsafe {
            let owner = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT,
                w!("STATIC"),
                w!("ROQA Share"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )?;
            SetLayeredWindowAttributes(owner, COLORREF(0), 1, LWA_ALPHA)?;
            owner
        },
    };
    // SAFETY: our own popup; the click that asked for the share gives this process the foreground.
    unsafe {
        let centre = (rect.left + rect.right) / 2;
        SetWindowPos(
            owner,
            Some(HWND_TOPMOST),
            centre,
            rect.top,
            1,
            1,
            SWP_SHOWWINDOW,
        )?;
        let _ = SetForegroundWindow(owner);
    }
    let interop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
    // SAFETY: `owner` is a live top-level window of this thread.
    let manager: DataTransferManager = unsafe { interop.GetForWindow(owner)? };
    let title = HSTRING::from(path.rsplit('\\').next().unwrap_or(path));
    let path = HSTRING::from(path);
    let handler =
        TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| {
            let Some(args) = args.as_ref() else {
                return Ok(());
            };
            let request = args.Request()?;
            let data = request.Data()?;
            data.Properties()?.SetTitle(&title)?;
            let deferral = request.GetDeferral()?;
            StorageFile::GetFileFromPathAsync(&path)?.SetCompleted(
                &AsyncOperationCompletedHandler::new(move |file, _| {
                    // A file gone since it was shelved leaves the sheet empty; Windows says so itself.
                    let file = file.as_ref();
                    if let Some(file) =
                        file.and_then(|file: &IAsyncOperation<StorageFile>| file.GetResults().ok())
                    {
                        let items: IIterable<IStorageItem> = vec![Some(file.cast()?)].into();
                        data.SetStorageItemsReadOnly(&items)?;
                    }
                    deferral.Complete()
                }),
            )
        });
    let token = manager.DataRequested(&handler)?;
    let old = SHARING.with_borrow_mut(|slot| slot.replace((owner, manager, token)));
    if let Some((_, old, token)) = old {
        old.RemoveDataRequested(token)?;
    }
    // SAFETY: as above.
    unsafe { interop.ShowShareUIForWindow(owner) }
}

// A single on-demand STA owns the dialog and all of its COM results. Completion posts one host wake.
// Cancellation carries only a window handle under a lock, never an apartment-bound COM interface.
#[derive(Default)]
struct DialogState {
    cancelled: bool,
    hwnd: isize,
    thread: u32,
}
#[implement(windows::Win32::UI::Shell::IFileDialogEvents)]
struct DialogEvents {
    state: std::sync::Arc<std::sync::Mutex<DialogState>>,
}
impl DialogEvents_Impl {
    fn ready(&self, dialog: Ref<'_, windows::Win32::UI::Shell::IFileDialog>) -> Result<()> {
        let Some(dialog) = dialog.as_ref() else {
            return Ok(());
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
        // SAFETY: this callback runs on the dialog's STA. IOleWindow supplies its live HWND.
        unsafe {
            if let Ok(window) = dialog
                .cast::<windows::Win32::System::Ole::IOleWindow>()
                .and_then(|window| window.GetWindow())
            {
                state.hwnd = window.0 as isize;
            }
            let cancelled = state.cancelled;
            drop(state); // Close may synchronously call a dialog event again.
            if cancelled {
                let _ = dialog.Close(windows::core::HRESULT(0x800704C7u32 as i32));
            }
        }
        Ok(())
    }
}
impl windows::Win32::UI::Shell::IFileDialogEvents_Impl for DialogEvents_Impl {
    fn OnFileOk(&self, dialog: Ref<'_, windows::Win32::UI::Shell::IFileDialog>) -> Result<()> {
        self.ready(dialog)
    }
    fn OnFolderChanging(
        &self,
        dialog: Ref<'_, windows::Win32::UI::Shell::IFileDialog>,
        _: Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> Result<()> {
        self.ready(dialog)
    }
    fn OnFolderChange(
        &self,
        dialog: Ref<'_, windows::Win32::UI::Shell::IFileDialog>,
    ) -> Result<()> {
        self.ready(dialog)
    }
    fn OnSelectionChange(
        &self,
        dialog: Ref<'_, windows::Win32::UI::Shell::IFileDialog>,
    ) -> Result<()> {
        self.ready(dialog)
    }
    fn OnShareViolation(
        &self,
        _: Ref<'_, windows::Win32::UI::Shell::IFileDialog>,
        _: Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> Result<windows::Win32::UI::Shell::FDE_SHAREVIOLATION_RESPONSE> {
        Ok(windows::Win32::UI::Shell::FDESVR_DEFAULT)
    }
    fn OnTypeChange(&self, dialog: Ref<'_, windows::Win32::UI::Shell::IFileDialog>) -> Result<()> {
        self.ready(dialog)
    }
    fn OnOverwrite(
        &self,
        _: Ref<'_, windows::Win32::UI::Shell::IFileDialog>,
        _: Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> Result<windows::Win32::UI::Shell::FDE_OVERWRITE_RESPONSE> {
        Ok(windows::Win32::UI::Shell::FDEOR_DEFAULT)
    }
}
#[derive(Default)]
pub struct FilePicker {
    /// The open or last dialog chooses the prayer sound (one file) instead of shelf files.
    pub for_sound: bool,
    worker: Option<std::thread::JoinHandle<()>>,
    result: Option<crate::chan::Receiver<Vec<String>>>,
    state: std::sync::Arc<std::sync::Mutex<DialogState>>,
}
impl FilePicker {
    /// Starts the dialog. With one already open this does nothing; if it finished but its wake was lost
    /// (a failed `PostThreadMessageW`), the result is returned here so the picker never stays dead.
    pub fn start(&mut self) -> Option<Vec<String>> {
        self.start_with(false)
    }
    pub fn start_with(&mut self, sound: bool) -> Option<Vec<String>> {
        if self.worker.is_some() {
            return self.take();
        }
        self.for_sound = sound;
        self.state = Default::default();
        let state = self.state.clone();
        let (sender, receiver) = crate::chan::channel(1);
        // SAFETY: host thread id; its queue is already created by the native surfaces.
        let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        match std::thread::Builder::new()
            .name("roqa-file-picker".into())
            .spawn(move || {
                // `pick_files` reports every failure as an empty result; it has no panic path.
                let paths = match Sta::enter() {
                    Ok(_sta) => {
                        if let Ok(mut state) = state.lock() {
                            // SAFETY: scalar identity for cancellation validation on this owned worker.
                            state.thread =
                                unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
                        }
                        pick_files(&state, sound)
                    }
                    Err(error) => {
                        eprintln!("Shelf picker COM initialization: {error}");
                        Vec::new()
                    }
                };
                // Retire the HWND before a completion can release this worker's ownership.
                if let Ok(mut state) = state.lock() {
                    state.hwnd = 0;
                }
                let _ = sender.send(paths);
                if !crate::native_window::post_wake(host, PICKED) {
                    eprintln!("Shelf picker completion wake failed");
                }
            }) {
            Ok(worker) => {
                self.worker = Some(worker);
                self.result = Some(receiver);
            }
            Err(_) => eprintln!("Shelf file picker worker unavailable"),
        }
        None
    }
    pub fn take(&mut self) -> Option<Vec<String>> {
        let result = self.result.as_ref()?.try_recv();
        match result {
            Ok(paths) => {
                self.result = None;
                self.worker.take();
                Some(paths)
            }
            Err(crate::chan::TryRecvError::Disconnected) => {
                self.result = None;
                self.worker.take();
                Some(Vec::new())
            }
            Err(crate::chan::TryRecvError::Empty) => None,
        }
    }
}
impl Drop for FilePicker {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.cancelled = true;
            // SAFETY: scalar query, used only to reject a retired/recycled window handle.
            let owned = state.hwnd != 0
                && unsafe {
                    windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                        HWND(state.hwnd as *mut _),
                        None,
                    )
                } == state.thread;
            if owned {
                // SAFETY: only our live dialog, under the same lock used to retire its HWND.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(HWND(state.hwnd as *mut _)),
                        windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
        }
        // Never join a shell dialog on the host thread. At most one worker exists, only on user request.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_share_is_known_without_touching_it() {
        assert!(is_network(r"\\server\share\docs"));
        assert!(!is_network("shell:RecycleBinFolder") && !is_network("notepad.exe"));
        assert!(!is_network(r"C:\Windows"), "a fixed drive is local");
    }
    #[test]
    fn network_admission_releases_on_completion_and_unwind() {
        let permit = NetworkPermit::acquire().unwrap();
        assert!(NetworkPermit::acquire().is_none());
        drop(permit);
        let unwind = std::panic::catch_unwind(|| {
            let _permit = NetworkPermit::acquire().unwrap();
            panic!("test unwind");
        });
        assert!(unwind.is_err());
        assert!(NetworkPermit::acquire().is_some());
    }
}
