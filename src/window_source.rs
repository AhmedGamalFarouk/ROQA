use crate::{
    ipc::OwnedHandle,
    window_model::{Identity, Snapshot, SourceStats, WindowKey, WindowRecord},
};
use std::{
    collections::BTreeMap,
    mem::size_of,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        Graphics::Dwm::{DWM_CLOAKED_SHELL, DWMWA_CLOAKED, DwmGetWindowAttribute},
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        System::{
            Com::{
                CLSCTX_ALL, CoCreateInstance,
                StructuredStorage::{PropVariantClear, PropVariantToBSTR},
            },
            Threading::{
                GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
        UI::{
            Shell::{
                IVirtualDesktopManager,
                PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
                VirtualDesktopManager,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PWSTR},
};

const MAX_HANDLES: usize = 4096;
const MAX_WINDOWS: usize = 512;
struct Enumeration<'a> {
    handles: Vec<usize>,
    cancel: &'a AtomicBool,
    incomplete: bool,
}

unsafe extern "system" fn enumerate(hwnd: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows synchronously borrows this Enumeration until returning on the worker thread.
    let state = unsafe { &mut *(parameter.0 as *mut Enumeration<'_>) };
    if state.cancel.load(Ordering::Relaxed) || state.handles.len() == MAX_HANDLES {
        state.incomplete = true;
        return BOOL(0);
    }
    state.handles.push(hwnd.0 as usize);
    BOOL(1)
}

/// The shell's virtual-desktop manager: created once, on the first shell-cloaked window; `None` until then.
type Desktops = Option<IVirtualDesktopManager>;

/// Whether a window the shell cloaked is on another virtual desktop (not merely suspended on this one). A
/// failed call drops the manager so Explorer's next incarnation gets a fresh one; the window stays omitted.
fn on_other_desktop(desktops: &mut Desktops, hwnd: HWND) -> bool {
    // SAFETY: free-threaded shell COM object owned by this worker; scalar HWND argument.
    unsafe {
        if desktops.is_none() {
            *desktops = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok();
        }
        let Some(manager) = desktops.as_ref() else {
            return false;
        };
        match manager.IsWindowOnCurrentVirtualDesktop(hwnd) {
            Ok(current) => !current.as_bool(),
            Err(_) => {
                *desktops = None;
                false
            }
        }
    }
}

fn full_scan(generation: u64, cancel: &AtomicBool, desktops: &mut Desktops) -> Snapshot {
    let started = Instant::now();
    let mut state = Enumeration {
        handles: Vec::new(),
        cancel,
        incomplete: false,
    };
    // SAFETY: callback does not retain the stack pointer and cannot panic.
    let enumeration =
        unsafe { EnumWindows(Some(enumerate), LPARAM(&mut state as *mut _ as isize)) };
    let mut snapshot = Snapshot {
        generation,
        stats: SourceStats::default(),
        windows: Vec::new(),
        complete: !state.incomplete && enumeration.is_ok(),
        reason: None,
    };
    let mut executables = BTreeMap::new();
    // SAFETY: these queries return scalar identities and do not borrow foreign memory.
    let own_process = unsafe { GetCurrentProcessId() };
    // SAFETY: foreground handle is used only as an identity comparison in this snapshot.
    let foreground = unsafe { GetForegroundWindow() };
    for handle in state.handles {
        if cancel.load(Ordering::Relaxed)
            || started.elapsed() > Duration::from_millis(250)
            || snapshot.windows.len() == MAX_WINDOWS
        {
            snapshot.complete = false;
            break;
        }
        snapshot.stats.inspected += 1;
        if let Some(window) = inspect(
            HWND(handle as *mut _),
            own_process,
            foreground,
            &mut executables,
            desktops,
        ) {
            snapshot.windows.push(window);
        }
    }
    if !snapshot.complete {
        snapshot.reason = Some("Window scan incomplete: cancelled, native enumeration failed, or bounded capacity/time reached.".into());
    }
    snapshot
}

fn inspect(
    hwnd: HWND,
    own_process: u32,
    foreground: HWND,
    executables: &mut BTreeMap<u32, Option<String>>,
    desktops: &mut Desktops,
) -> Option<WindowRecord> {
    // SAFETY: only documented, non-message metadata queries against possibly transient foreign HWNDs.
    // Every failed/destroyed window is skipped, never dereferenced. Own HWNDs are excluded before text queries.
    unsafe {
        if GetAncestor(hwnd, GA_ROOT) != hwnd {
            return None;
        }
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process));
        if process == 0 || process == own_process || !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_EX_TOOLWINDOW.0 != 0 {
            return None;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null())
            && style & WS_EX_APPWINDOW.0 == 0
        {
            return None;
        }
        let mut cloaked = 0u32;
        // Unknown cloaking state is conservatively omitted; no fabricated visible state.
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            size_of::<u32>() as u32,
        )
        .is_err()
        {
            return None;
        }
        // A window on another virtual desktop is cloaked by the shell: it stays, flagged (the dock may filter
        // it). Any other cloak (a suspended app, a hidden frame) still omits it.
        let elsewhere = cloaked == DWM_CLOAKED_SHELL && on_other_desktop(desktops, hwnd);
        if cloaked != 0 && !elsewhere {
            return None;
        }
        // Windows' "Not Responding" ghost duplicates a hung window; the taskbar shows one button, so do we.
        let mut class = [0u16; 8];
        let class_length = GetClassNameW(hwnd, &mut class);
        if class_length > 0 && String::from_utf16_lossy(&class[..class_length as usize]) == "Ghost"
        {
            return None;
        }
        let mut title = [0u16; 1024];
        let length = GetWindowTextW(hwnd, &mut title);
        if length <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&title[..length as usize]);
        let executable = executables
            .entry(process)
            .or_insert_with(|| process_executable(process))
            .clone();
        let identity = match (application_id(hwnd), &executable) {
            (Some(id), _) => Identity::Application(id),
            (None, Some(path)) => Identity::Executable(path.clone()),
            (None, None) => Identity::Unavailable(process),
        };
        let mut current_process = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut current_process));
        if current_process != process || !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        Some(WindowRecord {
            key: WindowKey {
                handle: hwnd.0 as usize,
                process,
            },
            title,
            identity,
            executable,
            minimized: IsIconic(hwnd).as_bool(),
            foreground: hwnd == foreground,
            current_desktop: !elsewhere,
        })
    }
}

/// Explicit AppUserModelID set on the window (packaged apps, frame hosts, self-grouping apps).
fn application_id(hwnd: HWND) -> Option<String> {
    // SAFETY: reads a window property through the documented shell store on this COM-initialized worker;
    // the PROPVARIANT is cleared on every path after conversion.
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let mut value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let text = PropVariantToBSTR(&value).ok().map(|text| text.to_string());
        let _ = PropVariantClear(&mut value);
        text.filter(|id| !id.is_empty())
    }
}

fn process_executable(process: u32) -> Option<String> {
    // SAFETY: requests only limited query rights; owned handle closes on every return path.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process).ok()?;
        let handle = OwnedHandle(handle);
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        if QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .is_err()
        {
            return None;
        }
        Some(String::from_utf16_lossy(&path[..length as usize]).to_lowercase())
    }
}

#[derive(Default)]
pub struct SourceState {
    windows: BTreeMap<usize, WindowRecord>,
    stats: SourceStats,
    needs_full: bool,
    desktops: Desktops,
}
impl SourceState {
    pub fn reconcile(
        &mut self,
        generation: u64,
        full: bool,
        handles: Vec<usize>,
        cancel: &AtomicBool,
    ) -> Snapshot {
        if full || self.needs_full {
            let mut snapshot = full_scan(generation, cancel, &mut self.desktops);
            self.stats.full_scans += 1;
            self.stats.inspected += snapshot.stats.inspected;
            self.needs_full = !snapshot.complete;
            if snapshot.complete {
                self.windows = snapshot
                    .windows
                    .iter()
                    .cloned()
                    .map(|window| (window.key.handle, window))
                    .collect();
            }
            snapshot.stats = self.stats;
            return snapshot;
        }
        self.stats.delta_batches += 1;
        let started = Instant::now();
        let mut executables = BTreeMap::new();
        // SAFETY: scalar queries; no foreign messages or ownership transfer.
        let own_process = unsafe { GetCurrentProcessId() };
        // SAFETY: foreground HWND is only compared, never dereferenced.
        let foreground = unsafe { GetForegroundWindow() };
        let mut complete = true;
        for handle in handles {
            if cancel.load(Ordering::Relaxed) || started.elapsed() > Duration::from_millis(250) {
                complete = false;
                break;
            }
            self.stats.inspected += 1;
            if let Some(window) = inspect(
                HWND(handle as *mut _),
                own_process,
                foreground,
                &mut executables,
                &mut self.desktops,
            ) {
                if !self.windows.contains_key(&handle) && self.windows.len() == MAX_WINDOWS {
                    complete = false;
                    break;
                }
                self.windows.insert(handle, window);
            } else {
                self.windows.remove(&handle);
            }
        }
        for window in self.windows.values_mut() {
            window.foreground = window.key.handle == foreground.0 as usize;
        }
        self.needs_full = !complete;
        Snapshot {
            generation,
            stats: self.stats,
            windows: self.windows.values().cloned().collect(),
            complete,
            reason: if complete {
                None
            } else {
                Some("Incremental window update incomplete; full repair required.".into())
            },
        }
    }
}
