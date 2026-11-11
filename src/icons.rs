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
    Storage::Streams::IRandomAccessStream,
    Win32::{
        Foundation::{E_FAIL, LPARAM, SIZE, WPARAM},
        Graphics::{
            Gdi::{
                BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
                GetDIBits, GetObjectW, HBITMAP, ReleaseDC,
            },
            Imaging::{
                CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
                WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
                WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand, WICRect,
            },
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IStream,
            },
            Threading::GetCurrentThreadId,
            WinRT::CreateStreamOverRandomAccessStream,
        },
        UI::{
            Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY},
            WindowsAndMessaging::{PostThreadMessageW, WM_APP},
        },
    },
    core::{AgileReference, Error, HSTRING, Result},
};

const ICON_READY: u32 = WM_APP + 12;
const QUEUE: usize = 64;
// Bound retained pixels and admitted work together; closed apps are evicted on catalog changes.
pub const CACHE_LIMIT: usize = 256;
/// Album-art streams larger than this are rejected before decoding.
const ART_BYTES: u64 = 4 << 20;
/// Decoded art is at most this many pixels a side (256 KiB of BGRA).
const ART_SIDE: u32 = 256;
/// Frames with a larger side are rejected from the header alone: a small stream can still declare a
/// huge frame, which WIC may buffer whole (about 36 MiB of BGRA at this cap). 3000 x 3000 store
/// artwork still fits.
const ART_FRAME: u32 = 3072;

/// Premultiplied BGRA, top-down rows of `width * 4` bytes.
pub struct IconPixels {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

type Extracted = (Identity, u32, Option<Arc<IconPixels>>);

pub struct Request {
    identity: Identity,
    sources: Vec<String>,
    size: u32,
}

/// Work for the icon worker, run one at a time in queue order.
pub enum Job {
    Icon(Request),
    /// Album art: the opened thumbnail stream, the side it is drawn at in physical pixels, and
    /// where the pixels go. `done` runs on the worker, and only when decoding succeeds.
    Art {
        stream: AgileReference<IRandomAccessStream>,
        side: u32,
        done: Box<dyn FnOnce(IconPixels) + Send>,
    },
}

pub struct IconService {
    requests: Option<SyncSender<Job>>,
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
        let (request_tx, request_rx) = mpsc::sync_channel::<Job>(QUEUE);
        let (result_tx, result_rx) = mpsc::sync_channel(QUEUE);
        let worker = thread::Builder::new()
            .name("roqa-icons".into())
            .spawn(move || {
                // SAFETY: worker-owned STA for in-process shell item objects; balanced below on this thread.
                let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
                while let Ok(job) = request_rx.recv() {
                    let request = match job {
                        Job::Icon(request) => request,
                        Job::Art { stream, side, done } => {
                            let pixels = if com {
                                art(stream.resolve(), side)
                            } else {
                                None
                            };
                            // The resolved stream is already released; release its reference too
                            // before replying.
                            drop(stream);
                            if let Some(pixels) = pixels {
                                done(pixels);
                            }
                            continue;
                        }
                    };
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
                // Jobs still queued after an early exit hold agile references: release them before COM.
                drop(request_rx);
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
            self.publish();
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
        if self.requests.as_ref()?.try_send(Job::Icon(request)).is_ok() {
            self.pending.insert(identity.clone());
        }
        None
    }

    /// The worker's queue, for album art. The worker stops only once every clone is dropped.
    pub fn jobs(&self) -> Option<SyncSender<Job>> {
        self.requests.clone()
    }

    /// Drops cached and pending identities that no longer belong to the dock.
    pub fn retain(&mut self, keep: impl Fn(&Identity) -> bool) {
        self.cache.retain(|identity, _| keep(identity));
        self.pending.retain(|identity| keep(identity));
        self.publish();
    }

    /// Cached-icon count for the diagnostics report.
    fn publish(&self) {
        crate::diagnostics::ICON_CACHE.store(
            self.cache.len() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
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
        if changed {
            self.publish();
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

/// Decoded album art, or `None` when the stream cannot be resolved, is over a cap or fails to
/// decode. The reason is logged, never the content.
fn art(stream: Result<IRandomAccessStream>, side: u32) -> Option<IconPixels> {
    stream
        .and_then(|stream| decode_art(&stream, side))
        .inspect_err(|error| eprintln!("Album art skipped: {error}"))
        .ok()
}

fn decode_art(stream: &IRandomAccessStream, side: u32) -> Result<IconPixels> {
    if stream.Size()? > ART_BYTES {
        return Err(Error::new(E_FAIL, "Album art stream over 4 MiB."));
    }
    // SAFETY: COM is initialized on this thread; every WIC object is released on return, and the
    // output buffer holds exactly `side` rows of `side * 4` bytes.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let source: IStream = CreateStreamOverRandomAccessStream(stream)?;
        let frame = factory
            .CreateDecoderFromStream(&source, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?
            .GetFrame(0)?;
        let (mut width, mut height) = (0, 0);
        frame.GetSize(&mut width, &mut height)?;
        let (crop, side) = art_square(width, height, side)
            .ok_or_else(|| Error::new(E_FAIL, "Album art frame size out of range."))?;
        let clipper = factory.CreateBitmapClipper()?;
        clipper.Initialize(&frame, &crop)?;
        // Premultiply before scaling, so transparent edges do not pick up dark fringes.
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &clipper,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(&converter, side, side, WICBitmapInterpolationModeFant)?;
        let mut pixels = vec![0u8; (side * side * 4) as usize];
        scaler.CopyPixels(std::ptr::null(), side * 4, &mut pixels)?;
        Ok(IconPixels {
            width: side,
            height: side,
            pixels,
        })
    }
}

/// The centred square crop of a `width` x `height` frame that fills the cover slot, and the side to
/// decode it at: the drawn `side`, but never above the crop or `ART_SIDE`. `None` for an empty or
/// oversized frame.
fn art_square(width: u32, height: u32, side: u32) -> Option<(WICRect, u32)> {
    if width == 0 || height == 0 || width > ART_FRAME || height > ART_FRAME || side == 0 {
        return None;
    }
    let square = width.min(height);
    let crop = WICRect {
        X: ((width - square) / 2) as i32,
        Y: ((height - square) / 2) as i32,
        Width: square as i32,
        Height: square as i32,
    };
    Some((crop, side.min(square).min(ART_SIDE)))
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

    fn service() -> (IconService, Receiver<Job>, SyncSender<Extracted>) {
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

    fn icon(job: Job) -> Request {
        match job {
            Job::Icon(request) => request,
            Job::Art { .. } => panic!("expected an icon request"),
        }
    }

    #[test]
    fn art_crops_the_centred_square_and_caps_the_decoded_side() {
        let square = |width, height, side| {
            art_square(width, height, side).map(|(c, side)| (c.X, c.Y, c.Width, c.Height, side))
        };
        // A 16:9 video thumbnail fills the slot from its centre; the drawn side is decoded.
        assert_eq!(square(1280, 720, 108), Some((280, 0, 720, 720, 108)));
        assert_eq!(square(300, 500, 72), Some((0, 100, 300, 300, 72)));
        // Never upscaled past the source, never above 256 x 256.
        assert_eq!(square(64, 64, 108), Some((0, 0, 64, 64, 64)));
        assert_eq!(
            square(ART_FRAME, ART_FRAME, 324),
            Some((0, 0, 3072, 3072, ART_SIDE))
        );
        // Empty or oversized frames, and a zero side, are rejected.
        for (width, height, side) in [
            (0, 10, 72),
            (10, 0, 72),
            (ART_FRAME + 1, 10, 72),
            (10, 10, 0),
        ] {
            assert_eq!(square(width, height, side), None);
        }
    }

    #[test]
    fn art_over_the_stream_cap_or_unresolved_is_skipped_before_decoding() {
        // Activating the stream first joins this test thread to the implicit MTA that WIC needs.
        use windows::Storage::Streams::InMemoryRandomAccessStream;
        let stream = |bytes| -> IRandomAccessStream {
            let stream = InMemoryRandomAccessStream::new().unwrap();
            stream.SetSize(bytes).unwrap();
            stream.into()
        };
        let error = decode_art(&stream(ART_BYTES + 1), 72).err().unwrap();
        assert_eq!(error.message(), "Album art stream over 4 MiB.");
        // At the cap the stream reaches the decoder, which rejects zero bytes as an image.
        let error = decode_art(&stream(ART_BYTES), 72).err().unwrap();
        assert_ne!(error.message(), "Album art stream over 4 MiB.");
        assert!(art(Ok(stream(ART_BYTES + 1)), 72).is_none());
        // An agile reference that cannot be resolved skips the art.
        assert!(art(Err(E_FAIL.into()), 72).is_none());
    }

    /// A 2 x 1 24-bit BMP: blue-green-red (10, 20, 30) then (40, 50, 60), rows padded to 8 bytes.
    fn bmp() -> IRandomAccessStream {
        let mut bytes = b"BM".to_vec();
        for field in [62u32, 0, 54, 40, 2, 1] {
            bytes.extend(field.to_le_bytes());
        }
        bytes.extend([1, 0, 24, 0]); // planes, bits per pixel
        for field in [0u32, 8, 2835, 2835, 0, 0] {
            bytes.extend(field.to_le_bytes());
        }
        bytes.extend([10, 20, 30, 40, 50, 60, 0, 0]);
        let stream: IRandomAccessStream =
            windows::Storage::Streams::InMemoryRandomAccessStream::new()
                .unwrap()
                .into();
        // SAFETY: the bridge stream is used only here; `bytes` outlives the write it is read by.
        unsafe {
            let writer: IStream = CreateStreamOverRandomAccessStream(&stream).unwrap();
            writer
                .Write(bytes.as_ptr().cast(), bytes.len() as u32, None)
                .ok()
                .unwrap();
        }
        stream.Seek(0).unwrap();
        stream
    }

    #[test]
    fn art_decodes_the_centred_square_as_premultiplied_bgra() {
        // `bmp()` activates a WinRT stream first, and windows-core then joins this uninitialized
        // test thread to the implicit MTA, which WIC needs. Keep activation before decoding.
        let art = decode_art(&bmp(), 72).unwrap();
        // The centred 1 x 1 crop of a 2 x 1 frame is its left pixel, never upscaled; opaque.
        assert_eq!((art.width, art.height), (1, 1));
        assert_eq!(art.pixels, [10, 20, 30, 255]);
    }

    #[test]
    fn the_worker_replies_to_art_only_when_decoding_succeeds() {
        let service = IconService::create().unwrap();
        let jobs = service.jobs().unwrap();
        let (decoded, replies) = mpsc::channel();
        for stream in [
            IRandomAccessStream::from(
                windows::Storage::Streams::InMemoryRandomAccessStream::new().unwrap(),
            ),
            bmp(),
        ] {
            let decoded = decoded.clone();
            let job = Job::Art {
                stream: AgileReference::new(&stream).unwrap(),
                side: 72,
                done: Box::new(move |art: IconPixels| decoded.send(art.pixels).unwrap()),
            };
            jobs.send(job).unwrap();
        }
        // The empty stream fails without a reply; the queue runs in order, so the first reply is the BMP.
        let first = replies.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(first, [10, 20, 30, 255]);
        drop((jobs, decoded));
        drop(service);
        assert!(replies.try_recv().is_err());
    }

    #[test]
    fn demand_beyond_capacity_settles_without_resetting_cache() {
        let (mut service, incoming, completed) = service();
        for _ in 0..(CACHE_LIMIT / QUEUE + 2) {
            for index in 0..CACHE_LIMIT + 20 {
                request(&mut service, &identity(index), 40);
            }
            for request in incoming.try_iter().map(icon) {
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
        let requests: Vec<_> = incoming.try_iter().map(icon).collect();
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
        assert_eq!(icon(incoming.try_recv().unwrap()).identity, identity(0));
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
        assert!(
            incoming
                .try_iter()
                .map(icon)
                .any(|request| request.identity == retry)
        );
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
