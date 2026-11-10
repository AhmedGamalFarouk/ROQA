use crate::window_model::Identity;
use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, LPARAM, SIZE, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
            GetDIBits, GetObjectW, HBITMAP, ReleaseDC,
        },
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
            Threading::GetCurrentThreadId,
        },
        UI::{
            Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY},
            WindowsAndMessaging::{PostThreadMessageW, WM_APP},
        },
    },
    core::{Error, HSTRING, Result},
};

const ICON_READY: u32 = WM_APP + 12;
const QUEUE: usize = 64;
// Bound retained pixels and admitted work together; closed apps are evicted on catalog changes.
const CACHE_LIMIT: usize = 256;

/// Premultiplied BGRA, top-down rows of `width * 4` bytes.
pub struct IconPixels {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

type Extracted = (Identity, u32, Option<Arc<IconPixels>>);

struct Request {
    identity: Identity,
    sources: Vec<String>,
    size: u32,
}

pub struct IconService {
    requests: Option<SyncSender<Request>>,
    results: Option<Receiver<Extracted>>,
    worker: Option<JoinHandle<()>>,
    /// `None` records a definitive failure so it is not retried every refresh.
    cache: HashMap<Identity, Option<Arc<IconPixels>>>,
    pending: HashSet<Identity>,
    size: u32,
}

impl IconService {
    pub fn create() -> Result<Self> {
        // SAFETY: scalar thread ID of the UI thread that owns the message queue for wakes.
        let target = unsafe { GetCurrentThreadId() };
        let (request_tx, request_rx) = mpsc::sync_channel::<Request>(QUEUE);
        let (result_tx, result_rx) = mpsc::sync_channel(QUEUE);
        let worker = thread::Builder::new()
            .name("roqa-icons".into())
            .spawn(move || {
                // SAFETY: worker-owned STA for in-process shell item objects; balanced below on this thread.
                let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
                while let Ok(request) = request_rx.recv() {
                    let icon = if com {
                        request
                            .sources
                            .iter()
                            .find_map(|source| extract(source, request.size).ok())
                            .map(Arc::new)
                    } else {
                        None
                    };
                    if result_tx
                        .send((request.identity, request.size, icon))
                        .is_err()
                    {
                        break;
                    }
                    // SAFETY: scalar wake only; the UI loop also polls results on every pass.
                    let _ = unsafe { PostThreadMessageW(target, ICON_READY, WPARAM(0), LPARAM(0)) };
                }
                if com {
                    // SAFETY: balances the successful initialization above; all COM objects were released.
                    unsafe { CoUninitialize() };
                }
            })
            .map_err(|error| Error::new(E_FAIL, format!("Icon worker startup failed: {error}")))?;
        Ok(Self {
            requests: Some(request_tx),
            results: Some(result_rx),
            worker: Some(worker),
            cache: HashMap::new(),
            pending: HashSet::new(),
            size: 0,
        })
    }

    /// Cached icon, or queues one bounded extraction and returns `None` until it arrives.
    pub fn icon(
        &mut self,
        identity: &Identity,
        sources: Vec<String>,
        size: u32,
    ) -> Option<Arc<IconPixels>> {
        if size != self.size {
            self.size = size;
            self.cache.clear();
            self.pending.clear();
        }
        if let Some(icon) = self.cache.get(identity) {
            return icon.clone();
        }
        if sources.is_empty()
            || self.pending.contains(identity)
            || self.cache.len() + self.pending.len() >= CACHE_LIMIT
        {
            return None;
        }
        let request = Request {
            identity: identity.clone(),
            sources,
            size,
        };
        // A full queue is retried on a later dock refresh; it never blocks the UI thread.
        if self.requests.as_ref()?.try_send(request).is_ok() {
            self.pending.insert(identity.clone());
        }
        None
    }

    /// Drops cached and pending identities that no longer belong to the dock.
    pub fn retain(&mut self, keep: impl Fn(&Identity) -> bool) {
        self.cache.retain(|identity, _| keep(identity));
        self.pending.retain(|identity| keep(identity));
    }

    /// Moves finished extractions into the cache; true when the dock should repaint.
    pub fn collect(&mut self) -> bool {
        let mut changed = false;
        let Some(results) = &self.results else {
            return false;
        };
        while let Ok((identity, size, icon)) = results.try_recv() {
            if size == self.size && self.pending.remove(&identity) {
                self.cache.insert(identity, icon);
                changed = true;
            }
        }
        changed
    }
}

impl Drop for IconService {
    fn drop(&mut self) {
        self.requests.take();
        self.results.take(); // Unblocks a worker waiting to send, so it observes shutdown.
        let Some(worker) = self.worker.take() else {
            return;
        };
        // A shell extraction cannot be cancelled; bound shutdown instead of hanging on a stuck source.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if worker.is_finished() {
            if worker.join().is_err() {
                eprintln!("Icon worker panicked.");
            }
        } else {
            eprintln!("Icon worker still extracting after 1 s; detached at shutdown.");
        }
    }
}

fn extract(source: &str, size: u32) -> Result<IconPixels> {
    // SAFETY: COM is initialized on this thread; the factory and bitmap are released on every path.
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(source), None)?;
        let side = size as i32;
        let bitmap = factory.GetImage(SIZE { cx: side, cy: side }, SIIGBF_ICONONLY)?;
        let pixels = bitmap_pixels(bitmap);
        let _ = DeleteObject(bitmap.into());
        pixels
    }
}

fn bitmap_pixels(bitmap: HBITMAP) -> Result<IconPixels> {
    let mut info = BITMAP::default();
    // SAFETY: BITMAP output is sized correctly; the HBITMAP is owned by the caller for this call.
    let read = unsafe {
        GetObjectW(
            bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some((&mut info as *mut BITMAP).cast()),
        )
    };
    if read == 0
        || info.bmWidth <= 0
        || info.bmHeight <= 0
        || info.bmWidth > 512
        || info.bmHeight > 512
    {
        return Err(Error::new(E_FAIL, "Unusable icon bitmap."));
    }
    let (width, height) = (info.bmWidth as u32, info.bmHeight as u32);
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    // SAFETY: buffer holds exactly height rows of 32-bpp pixels; screen DC is released immediately.
    let lines = unsafe {
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        lines
    };
    if lines != height as i32 {
        return Err(Error::new(E_FAIL, "Cannot read icon pixels."));
    }
    premultiply(&mut pixels);
    Ok(IconPixels {
        width,
        height,
        pixels,
    })
}

/// Shell bitmaps may arrive straight-alpha, premultiplied, or with no alpha at all; normalize to premultiplied.
fn premultiply(pixels: &mut [u8]) {
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
    if !has_alpha {
        pixels.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        return;
    }
    let straight = pixels
        .chunks_exact(4)
        .any(|p| p[0] > p[3] || p[1] > p[3] || p[2] > p[3]);
    if straight {
        for p in pixels.chunks_exact_mut(4) {
            let alpha = u16::from(p[3]);
            for channel in &mut p[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
    }
}

/// Shell parsing names to try for a group, most specific first.
pub fn sources(identity: &Identity, executable: Option<&str>) -> Vec<String> {
    let mut sources = Vec::new();
    match identity {
        Identity::Application(id) => sources.push(format!("shell:AppsFolder\\{id}")),
        Identity::Executable(path) => sources.push(path.clone()),
        Identity::Unavailable(_) => {}
    }
    if let Some(path) = executable
        && !sources.iter().any(|source| source == path)
    {
        sources.push(path.to_owned());
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (IconService, Receiver<Request>, SyncSender<Extracted>) {
        let (requests, incoming) = mpsc::sync_channel(QUEUE);
        let (completed, results) = mpsc::sync_channel(QUEUE);
        (
            IconService {
                requests: Some(requests),
                results: Some(results),
                worker: None,
                cache: HashMap::new(),
                pending: HashSet::new(),
                size: 40,
            },
            incoming,
            completed,
        )
    }

    fn identity(index: usize) -> Identity {
        Identity::Executable(format!("app-{index}.exe"))
    }

    fn request(service: &mut IconService, id: &Identity, size: u32) {
        service.icon(id, vec!["test.exe".into()], size);
    }

    #[test]
    fn demand_beyond_capacity_settles_without_resetting_cache() {
        let (mut service, incoming, completed) = service();
        for _ in 0..(CACHE_LIMIT / QUEUE + 2) {
            for index in 0..CACHE_LIMIT + 20 {
                request(&mut service, &identity(index), 40);
            }
            for request in incoming.try_iter() {
                completed
                    .send((request.identity, request.size, None))
                    .unwrap();
            }
            service.collect();
        }
        assert_eq!(service.cache.len(), CACHE_LIMIT);
        assert!(service.pending.is_empty());
        for index in 0..CACHE_LIMIT + 20 {
            request(&mut service, &identity(index), 40);
        }
        assert!(incoming.try_recv().is_err());
        assert!(!service.collect());
    }

    #[test]
    fn retain_evicts_closed_cached_and_pending_apps() {
        let (mut service, incoming, completed) = service();
        for index in 0..3 {
            request(&mut service, &identity(index), 40);
        }
        let requests: Vec<_> = incoming.try_iter().collect();
        for request in &requests[..2] {
            completed
                .send((request.identity.clone(), 40, None))
                .unwrap();
        }
        assert!(service.collect());
        service.retain(|id| id == &identity(1));
        assert_eq!(service.cache.len(), 1);
        assert!(service.pending.is_empty());
        completed.send((identity(2), 40, None)).unwrap();
        assert!(!service.collect());
        request(&mut service, &identity(1), 40);
        assert!(incoming.try_recv().is_err());
        request(&mut service, &identity(0), 40);
        assert_eq!(incoming.try_recv().unwrap().identity, identity(0));
    }

    #[test]
    fn old_size_completion_preserves_current_pending_request() {
        let (mut service, incoming, completed) = service();
        let id = identity(0);
        request(&mut service, &id, 40);
        request(&mut service, &id, 60);
        assert_eq!(incoming.try_iter().count(), 2);
        completed.send((id.clone(), 40, None)).unwrap();
        assert!(!service.collect());
        request(&mut service, &id, 60);
        assert!(incoming.try_recv().is_err());
        assert!(service.pending.contains(&id));
        completed.send((id.clone(), 60, None)).unwrap();
        assert!(service.collect());
        assert!(service.cache.contains_key(&id));
        assert!(service.pending.is_empty());
    }

    #[test]
    fn full_request_queue_leaves_identity_retryable() {
        let (mut service, incoming, _completed) = service();
        for index in 0..QUEUE {
            request(&mut service, &identity(index), 40);
        }
        let retry = identity(QUEUE);
        request(&mut service, &retry, 40);
        assert!(!service.pending.contains(&retry));
        incoming.try_recv().unwrap();
        request(&mut service, &retry, 40);
        assert!(service.pending.contains(&retry));
        assert!(incoming.try_iter().any(|request| request.identity == retry));
    }
    #[test]
    fn normalizes_straight_premultiplied_and_alphaless_pixels() {
        let mut straight = vec![200, 100, 50, 128, 0, 0, 0, 0];
        premultiply(&mut straight);
        assert_eq!(straight, vec![100, 50, 25, 128, 0, 0, 0, 0]);
        let mut already = vec![100, 50, 25, 128];
        premultiply(&mut already);
        assert_eq!(already, vec![100, 50, 25, 128]);
        let mut opaque = vec![10, 20, 30, 0, 40, 50, 60, 0];
        premultiply(&mut opaque);
        assert_eq!(opaque, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn icon_sources_prefer_packaged_identity_then_executable() {
        assert_eq!(
            sources(
                &Identity::Application("A.B!App".into()),
                Some("c:\\host.exe")
            ),
            vec![
                "shell:AppsFolder\\A.B!App".to_string(),
                "c:\\host.exe".into()
            ]
        );
        assert_eq!(
            sources(&Identity::Executable("c:\\a.exe".into()), Some("c:\\a.exe")),
            vec!["c:\\a.exe".to_string()]
        );
        assert!(sources(&Identity::Unavailable(4), None).is_empty());
    }
}
