use crate::chan::{Receiver, Sender, TryRecvError, channel};
use crate::{
    catalog_timing::CatalogTiming,
    diagnostics,
    window_model::{ScanSchedule, Snapshot, SourceStats, WindowModel},
    window_source,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND, LPARAM, WPARAM},
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
            Threading::GetCurrentThreadId,
        },
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            WindowsAndMessaging::*,
        },
    },
    core::{Error, Result},
};

const DIRTY: u32 = WM_APP + 10;
const COMPLETE: u32 = WM_APP + 11;
static TARGET: AtomicU32 = AtomicU32::new(0);
static NOTIFIED: AtomicBool = AtomicBool::new(false);
const RING_CAPACITY: usize = 256;
static HANDLES: [AtomicUsize; RING_CAPACITY] = [const { AtomicUsize::new(0) }; RING_CAPACITY];
static HANDLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static OVERFLOW: AtomicBool = AtomicBool::new(false);
struct Request {
    generation: u64,
    full: bool,
    handles: Vec<usize>,
}

unsafe extern "system" fn changed(
    _: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if event >= EVENT_OBJECT_CREATE && (object != 0 || child != 0) {
        return;
    }
    let target = TARGET.load(Ordering::Relaxed);
    if target == 0 {
        return;
    }
    let slot = HANDLE_COUNT.fetch_add(1, Ordering::Relaxed);
    if slot < RING_CAPACITY {
        HANDLES[slot].store(hwnd.0 as usize, Ordering::Relaxed);
    } else {
        OVERFLOW.store(true, Ordering::Relaxed);
    }
    if !NOTIFIED.swap(true, Ordering::Relaxed) {
        // SAFETY: scalar-only notification; no window memory or retained callback data. No panicking operations.
        if unsafe { PostThreadMessageW(target, DIRTY, WPARAM(0), LPARAM(0)) }.is_err() {
            NOTIFIED.store(false, Ordering::Relaxed);
        }
    }
}

struct Hook(HWINEVENTHOOK);
impl Hook {
    fn release(&mut self) -> Result<()> {
        if !self.0.0.is_null() {
            // SAFETY: unhook runs on the registering UI thread, once for this owned registration.
            unsafe {
                UnhookWinEvent(self.0).ok()?;
            }
            self.0 = HWINEVENTHOOK::default();
        }
        Ok(())
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            eprintln!("Window event cleanup: {error}");
        }
    }
}

pub struct Catalog {
    hooks: Vec<Hook>,
    requests: Option<Sender<Request>>,
    results: Option<Receiver<Snapshot>>,
    worker: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    timing: CatalogTiming,
    schedule: ScanSchedule,
    model: WindowModel,
    scans: u64,
    discarded: u64,
    invalidations: u64,
    reconciliations: u64,
    diagnostics: Option<PathBuf>,
    pending: BTreeSet<usize>,
    full: bool,
    source_stats: SourceStats,
}
impl Catalog {
    pub fn create(diagnostics: Option<PathBuf>) -> Result<Self> {
        // SAFETY: caller already owns HWNDs/a message queue and pumps it throughout this lifetime.
        let target = unsafe { GetCurrentThreadId() };
        if TARGET
            .compare_exchange(0, target, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Err(Error::new(E_FAIL, "A window catalog already exists."));
        }
        NOTIFIED.store(false, Ordering::Relaxed);
        HANDLE_COUNT.store(0, Ordering::Relaxed);
        OVERFLOW.store(false, Ordering::Relaxed);
        let mut catalog = Self {
            hooks: Vec::new(),
            requests: None,
            results: None,
            worker: None,
            cancel: Arc::new(AtomicBool::new(false)),
            timing: CatalogTiming::new(Instant::now()),
            schedule: ScanSchedule::default(),
            model: WindowModel::default(),
            scans: 0,
            discarded: 0,
            invalidations: 0,
            reconciliations: 0,
            diagnostics,
            pending: BTreeSet::new(),
            full: true,
            source_stats: SourceStats::default(),
        };
        let (request_tx, request_rx) = channel::<Request>(1);
        let (result_tx, result_rx) = channel(1);
        let cancel = Arc::clone(&catalog.cancel);
        catalog.worker = Some(
            thread::Builder::new()
                .name("roqa-window-source".into())
                .spawn(move || {
                    // SAFETY: worker-owned MTA for shell window-property reads, balanced on this thread below.
                    // Without it, AppUserModelID lookups fail and grouping falls back to executables.
                    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                    let mut source = window_source::SourceState::default();
                    while let Ok(request) = request_rx.recv() {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let snapshot = source.reconcile(
                            request.generation,
                            request.full,
                            request.handles,
                            &cancel,
                        );
                        if cancel.load(Ordering::Relaxed) || result_tx.send(snapshot).is_err() {
                            break;
                        }
                        // SAFETY: scalar wake for the owning thread. A missed wake is recovered by reconciliation.
                        let _ =
                            unsafe { PostThreadMessageW(target, COMPLETE, WPARAM(0), LPARAM(0)) };
                    }
                    drop(source); // its virtual-desktop manager is a COM object
                    if com {
                        // SAFETY: balances the successful initialization above; the source's COM object is released.
                        unsafe { CoUninitialize() };
                    }
                })
                .map_err(|error| {
                    Error::new(E_FAIL, format!("Window worker startup failed: {error}"))
                })?,
        );
        catalog.requests = Some(request_tx);
        catalog.results = Some(result_rx);
        for (first, last) in [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
            (EVENT_OBJECT_STATECHANGE, EVENT_OBJECT_STATECHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ] {
            // SAFETY: out-of-context notifications, no DLL injection or input interception; UI-thread message delivery.
            let hook = unsafe {
                SetWinEventHook(
                    first,
                    last,
                    None,
                    Some(changed),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if hook.0.is_null() {
                return Err(Error::new(E_FAIL, "Window event subscription failed."));
            }
            catalog.hooks.push(Hook(hook));
        }
        catalog.schedule.invalidate();
        catalog.request()?;
        Ok(catalog)
    }
    pub fn model(&self) -> &WindowModel {
        &self.model
    }
    pub fn message(&mut self, message: &MSG) -> Result<bool> {
        if !message.hwnd.0.is_null() {
            return Ok(false);
        }
        let now = Instant::now();
        match message.message {
            DIRTY => self.drain_ingress(now),
            COMPLETE => self.collect(now)?,
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn drain_ingress(&mut self, now: Instant) {
        let (handles, overflow) = take_ingress();
        if handles.is_empty() && !overflow {
            return;
        }
        self.invalidations += 1;
        self.full |= overflow;
        for handle in handles {
            if self.pending.len() < 512 {
                self.pending.insert(handle);
            } else {
                self.full = true;
            }
        }
        self.schedule.invalidate();
        self.timing.arm(now);
    }
    pub fn service(&mut self) -> Result<()> {
        self.service_at(Instant::now())
    }
    fn service_at(&mut self, now: Instant) -> Result<()> {
        // WinEvent callbacks and this drain run on the same thread, including recovery after a lost wake.
        self.drain_ingress(now);
        self.collect(now)?;
        if self.timing.repair_due(now) {
            self.reconciliations += 1;
            self.full = true;
            self.schedule.invalidate();
            self.timing.arm(now);
        }
        if self.timing.refresh_due(now) {
            self.request()?;
        }
        Ok(())
    }
    pub fn wait_milliseconds(&self) -> u32 {
        self.timing.wait_milliseconds(Instant::now())
    }
    fn request(&mut self) -> Result<()> {
        if let Some(generation) = self.schedule.request() {
            let sender = self
                .requests
                .as_ref()
                .ok_or_else(|| Error::new(E_FAIL, "Window worker is stopped."))?;
            let request = Request {
                generation,
                full: std::mem::replace(&mut self.full, false),
                handles: std::mem::take(&mut self.pending).into_iter().collect(),
            };
            sender.try_send(request).map_err(|error| {
                Error::new(E_FAIL, format!("Window worker request failed: {error}"))
            })?;
            self.scans += 1;
        }
        Ok(())
    }
    fn collect(&mut self, now: Instant) -> Result<()> {
        loop {
            let Some(receiver) = &self.results else {
                break;
            };
            let snapshot = match receiver.try_recv() {
                Ok(snapshot) => snapshot,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(Error::new(
                        E_FAIL,
                        "Window worker disconnected unexpectedly.",
                    ));
                }
            };
            self.source_stats = snapshot.stats;
            if self.schedule.complete(snapshot.generation) {
                let complete = snapshot.complete;
                self.model.apply(snapshot);
                self.timing.completed(complete, now);
                if !complete {
                    self.full = true;
                    self.schedule.invalidate();
                }
                self.write_diagnostics(false)?;
            } else {
                self.discarded += 1;
            }
        }
        // Every follow-up observes the same debounce as event-triggered work.
        if self.schedule.has_pending() {
            self.timing.arm(now);
        }
        Ok(())
    }
    /// Settings changed the opt-in report file (`None` stops writing it).
    pub fn set_diagnostics(&mut self, path: Option<PathBuf>) {
        self.diagnostics = path;
    }
    pub fn shutdown(&mut self) -> Result<()> {
        TARGET.store(0, Ordering::Relaxed);
        let mut failure = None;
        for hook in &mut self.hooks {
            if let Err(error) = hook.release() {
                failure = Some(error);
            }
        }
        self.hooks.clear();
        self.cancel.store(true, Ordering::Relaxed);
        self.requests.take();
        self.results.take(); // Release a worker blocked sending its single bounded result before joining.
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            failure = Some(Error::new(E_FAIL, "Window worker panicked."));
        }
        if let Some(error) = failure {
            return Err(error);
        }
        self.write_diagnostics(true)
    }
    fn write_diagnostics(&self, stopped: bool) -> Result<()> {
        let Some(path) = &self.diagnostics else {
            return Ok(());
        };
        // Explicit opt-in only; the redacted format lives in `diagnostics`.
        let view = diagnostics::CatalogView {
            model: &self.model,
            stats: self.source_stats,
            scans: self.scans,
            discarded: self.discarded,
            invalidations: self.invalidations,
            reconciliations: self.reconciliations,
            hooks: self.hooks.len(),
        };
        diagnostics::write(
            path,
            diagnostics::report(&view, &diagnostics::Counters::read(), stopped),
        )
    }
}
impl Drop for Catalog {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("Window catalog cleanup: {error}");
        }
    }
}
// Out-of-context callbacks are delivered on the registering UI thread; no callback can
// interleave these swaps unless this helper starts pumping messages (which it must not).
fn take_ingress() -> (Vec<usize>, bool) {
    NOTIFIED.store(false, Ordering::Relaxed);
    let count = HANDLE_COUNT.swap(0, Ordering::Relaxed);
    let overflow = OVERFLOW.swap(false, Ordering::Relaxed) || count > RING_CAPACITY;
    let handles = HANDLES
        .iter()
        .take(count.min(RING_CAPACITY))
        .map(|slot| slot.swap(0, Ordering::Relaxed))
        .filter(|handle| *handle != 0)
        .collect();
    (handles, overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_wakes_overflow_followup_repair_and_disconnect_need_no_timer_message() {
        let now = Instant::now();
        let (request_tx, request_rx) = channel(1);
        let (result_tx, result_rx) = channel(1);
        let mut catalog = Catalog {
            hooks: Vec::new(),
            requests: Some(request_tx),
            results: Some(result_rx),
            worker: None,
            cancel: Arc::new(AtomicBool::new(false)),
            timing: CatalogTiming::new(now),
            schedule: ScanSchedule::default(),
            model: WindowModel::default(),
            scans: 0,
            discarded: 0,
            invalidations: 0,
            reconciliations: 0,
            diagnostics: None,
            pending: BTreeSet::new(),
            full: false,
            source_stats: SourceStats::default(),
        };
        // No native Catalog exists in unit tests. This is the only test owning callback statics.
        NOTIFIED.store(true, Ordering::Relaxed);
        HANDLE_COUNT.store(RING_CAPACITY + 7, Ordering::Relaxed);
        for (index, slot) in HANDLES.iter().enumerate() {
            slot.store(index + 1, Ordering::Relaxed);
        }
        catalog.service_at(now).unwrap();
        assert!(!NOTIFIED.load(Ordering::Relaxed));
        assert!(catalog.full);
        assert_eq!(catalog.pending.len(), RING_CAPACITY);
        let (handles, overflow) = take_ingress();
        assert!(handles.is_empty() && !overflow);
        let fifty = std::time::Duration::from_millis(50);
        catalog.service_at(now + fifty).unwrap();
        let request = request_rx.try_recv().unwrap();
        assert!(request.full);
        assert_eq!(request.handles.len(), RING_CAPACITY);
        catalog.schedule.invalidate();
        result_tx
            .send(Snapshot {
                generation: request.generation,
                stats: SourceStats::default(),
                windows: Vec::new(),
                complete: true,
                reason: None,
            })
            .unwrap();
        // Lost COMPLETE: the outer-loop service still publishes, then batches the pending update.
        catalog.service_at(now + fifty).unwrap();
        assert_eq!(catalog.model.generation, request.generation);
        assert!(request_rx.try_recv().is_err());
        catalog.service_at(now + fifty + fifty).unwrap();
        let followup = request_rx.try_recv().unwrap();
        assert!(!followup.full);
        result_tx
            .send(Snapshot {
                generation: followup.generation,
                stats: SourceStats::default(),
                windows: Vec::new(),
                complete: false,
                reason: Some("injected incomplete scan".into()),
            })
            .unwrap();
        catalog.service_at(now + fifty + fifty).unwrap();
        catalog
            .service_at(now + std::time::Duration::from_millis(1099))
            .unwrap();
        assert!(request_rx.try_recv().is_err());
        catalog
            .service_at(now + std::time::Duration::from_millis(1100))
            .unwrap();
        let retry = request_rx.try_recv().unwrap();
        assert!(retry.full);
        result_tx
            .send(Snapshot {
                generation: retry.generation,
                stats: SourceStats::default(),
                windows: Vec::new(),
                complete: true,
                reason: None,
            })
            .unwrap();
        catalog
            .service_at(now + std::time::Duration::from_secs(30))
            .unwrap();
        assert_eq!(catalog.reconciliations, 1);
        catalog
            .service_at(now + std::time::Duration::from_secs(30) + fifty)
            .unwrap();
        assert!(request_rx.try_recv().unwrap().full);
        drop(result_tx);
        assert!(
            catalog
                .service_at(now + std::time::Duration::from_secs(60))
                .is_err()
        );
    }
}
