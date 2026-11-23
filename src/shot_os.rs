//! The screenshot pill's OS side: a Windows screenshot tool's clipboard image is read once on a short worker
//! (a delay-rendered image would make the host wait on the tool), and a shot is dragged out from memory: an app
//! gets the image or a virtual `Screenshot HH-MM.png` file whose bytes never touch the disk, the Shelf its id.
//! Nothing here logs or saves an image.

use crate::shots::{self, Shot};
use std::{
    mem::ManuallyDrop,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{COLORREF, E_FAIL, GlobalFree, HGLOBAL, HWND, POINT, SIZE},
        Graphics::{
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS,
                DeleteObject,
            },
            Imaging::{
                CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat24bppBGR,
                IWICImagingFactory, WICBitmapEncoderNoCache,
            },
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, CoCreateInstance, DVASPECT_CONTENT, FORMATETC, IDataObject,
                STGMEDIUM, STGMEDIUM_0, STREAM_SEEK_CUR,
                StructuredStorage::{CreateStreamOnHGlobal, GetHGlobalFromStream},
                TYMED_HGLOBAL,
            },
            DataExchange::{
                CloseClipboard, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber,
                IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{DROPEFFECT_COPY, DROPEFFECT_NONE},
            SystemInformation::GetLocalTime,
            Threading::GetCurrentThreadId,
        },
        UI::{
            Shell::{
                CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CLSID_DragDropHelper, FD_FILESIZE,
                FD_PROGRESSUI, FILEDESCRIPTORW, IDragSourceHelper, SHCreateDataObject, SHDRAGIMAGE,
                SHDoDragDrop,
            },
            WindowsAndMessaging::GetWindowThreadProcessId,
        },
    },
    core::{Error, Result, w},
};

/// `CF_DIB`.
const DIB: u16 = 8;
/// The processes whose clipboard image is a screenshot: Win+Shift+S, PrtScn and the Snipping Tool, and
/// Win+PrtScn. An image with no owner (the system's own PrtScn) counts too.
const TOOLS: [&str; 4] = [
    "snippingtool.exe",
    "screenclippinghost.exe",
    "screensketch.exe",
    "explorer.exe",
];

static BUSY: AtomicBool = AtomicBool::new(false);
static SEEN: AtomicU32 = AtomicU32::new(0);
static NEXT: AtomicU64 = AtomicU64::new(0);
static READY: Mutex<Vec<Shot>> = Mutex::new(Vec::new());

/// The clipboard changed: a new image starts one read on a worker, which wakes the host when it has a shot.
pub fn check() {
    // SAFETY: plain clipboard queries; neither opens the clipboard.
    let (seq, image) = unsafe {
        (
            GetClipboardSequenceNumber(),
            IsClipboardFormatAvailable(u32::from(DIB)).is_ok(),
        )
    };
    if !image || SEEN.swap(seq, Ordering::AcqRel) == seq || BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    // SAFETY: the calling thread's id, a plain value.
    let host = unsafe { GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("shot".into())
        .spawn(move || {
            if let Some(shot) = read() {
                READY
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(shot);
                let _ = crate::native_window::post_wake(host, crate::native_window::CLIPBOARD);
            }
            BUSY.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        BUSY.store(false, Ordering::Release);
    }
}

/// The shots the worker finished since the last pass.
pub fn take() -> Vec<Shot> {
    std::mem::take(&mut *READY.lock().unwrap_or_else(PoisonError::into_inner))
}

/// The clipboard's image when a screenshot tool put it there and did not mark it private.
fn read() -> Option<Shot> {
    // SAFETY: the owner is only queried for its process id; the clipboard is open between the open and close
    // below, and the image's global is read only while locked.
    let bytes = unsafe {
        if let Ok(owner) = GetClipboardOwner()
            && !owner.is_invalid()
        {
            let mut process = 0;
            GetWindowThreadProcessId(owner, Some(&mut process));
            let path = crate::window_source::process_executable(process)?;
            let name = path.rsplit('\\').next().unwrap_or(&path);
            if !TOOLS.contains(&name) {
                return None;
            }
        }
        OpenClipboard(None).ok()?;
        let bytes = (|| {
            if crate::clipboard_os::private() {
                return None;
            }
            let global = HGLOBAL(GetClipboardData(u32::from(DIB)).ok()?.0);
            let start = GlobalLock(global).cast::<u8>();
            if start.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(start, GlobalSize(global)).to_vec();
            let _ = GlobalUnlock(global);
            Some(bytes)
        })();
        let _ = CloseClipboard();
        bytes?
    };
    let info = shots::dib(&bytes)?;
    let thumb = shots::thumb(&bytes, &info);
    // SAFETY: a plain clock read.
    let time = unsafe { GetLocalTime() };
    Some(Shot {
        id: NEXT.fetch_add(1, Ordering::Relaxed) + 1,
        dib: bytes,
        thumb,
        time: format!("{:02}-{:02}", time.wHour, time.wMinute),
    })
}

/// The Shelf's own format: the dragged shot's id.
pub fn shot_format() -> u16 {
    // SAFETY: registers (or finds) a named format; a plain value.
    unsafe { RegisterClipboardFormatW(w!("ROQA.Shot")) as u16 }
}

pub fn format(format: u16, lindex: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// Hands `data` a moveable global holding `bytes` under `format`.
fn put(data: &IDataObject, format: u16, lindex: i32, bytes: &[u8]) -> Result<()> {
    // SAFETY: the new global is filled while locked; SetData takes it on success, it is freed otherwise.
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let target = GlobalLock(global).cast::<u8>();
        if target.is_null() {
            let _ = GlobalFree(Some(global));
            return Err(E_FAIL.into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        let _ = GlobalUnlock(global);
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: global },
            pUnkForRelease: ManuallyDrop::new(None),
        };
        let set = data.SetData(&self::format(format, lindex), &medium, true);
        if set.is_err() {
            let _ = GlobalFree(Some(global));
        }
        set
    }
}

/// The shot as a PNG, encoded in memory (24 bits: a screenshot's fourth byte is not alpha).
fn png(dib: &[u8], info: &shots::Dib) -> Result<Vec<u8>> {
    // SAFETY: WIC objects on this OLE-initialised thread; the stream owns its global, read only while locked.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let (mut frame, mut options) = (None, None);
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or_else(|| Error::from(E_FAIL))?;
        frame.Initialize(options.as_ref())?;
        frame.SetSize(info.width as u32, info.height as u32)?;
        let mut pixel = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut pixel)?;
        if pixel != GUID_WICPixelFormat24bppBGR {
            return Err(E_FAIL.into());
        }
        let mut line = Vec::with_capacity(info.width * 3);
        for y in 0..info.height {
            line.clear();
            for px in dib[info.row(y)].chunks_exact(info.bytes) {
                line.extend_from_slice(&px[..3]);
            }
            frame.WritePixels(1, line.len() as u32, &line)?;
        }
        frame.Commit()?;
        encoder.Commit()?;
        let mut length = 0;
        stream.Seek(0, STREAM_SEEK_CUR, Some(&mut length))?;
        let global = GetHGlobalFromStream(&stream)?;
        let start = GlobalLock(global).cast::<u8>();
        if start.is_null() {
            return Err(E_FAIL.into());
        }
        let bytes = std::slice::from_raw_parts(start, length as usize).to_vec();
        let _ = GlobalUnlock(global);
        Ok(bytes)
    }
}

/// One virtual file, `name`, of `size` bytes (`FILEGROUPDESCRIPTORW` with one item).
fn descriptor(name: &str, size: usize) -> Vec<u8> {
    #[repr(C)]
    struct Group {
        count: u32,
        file: FILEDESCRIPTORW,
    }
    let mut units = [0u16; 260];
    for (slot, unit) in units.iter_mut().take(259).zip(name.encode_utf16()) {
        *slot = unit;
    }
    let file = FILEDESCRIPTORW {
        dwFlags: (FD_FILESIZE.0 | FD_PROGRESSUI.0) as u32,
        nFileSizeLow: size as u32,
        nFileSizeHigh: (size as u64 >> 32) as u32,
        cFileName: units,
        ..Default::default()
    };
    let group = Group { count: 1, file };
    // SAFETY: a plain repr(C) value read as its own bytes.
    unsafe {
        std::slice::from_raw_parts(
            (&raw const group).cast::<u8>(),
            std::mem::size_of::<Group>(),
        )
        .to_vec()
    }
}

/// The drag image: the thumbnail at 128 x 72 DIP, radius 14, opacity .9, premultiplied.
fn ghost(shot: &Shot, dpi: u32, data: &IDataObject) -> Result<()> {
    let scale = dpi as f32 / 96.0;
    let (width, height) = ((128.0 * scale) as i32, (72.0 * scale) as i32);
    let radius = 14.0 * scale;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let thumb = &shot.thumb;
    // SAFETY: the section's bits hold `width * height` pixels and are written before the helper takes the
    // bitmap; a refused bitmap is deleted here.
    unsafe {
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let out = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                // Coverage inside the rounded rectangle, with a one-pixel soft edge.
                let (cx, cy) = (
                    (x as f32 + 0.5).clamp(radius, width as f32 - radius),
                    (y as f32 + 0.5).clamp(radius, height as f32 - radius),
                );
                let distance = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
                let alpha = 0.9 * (radius + 0.5 - distance).clamp(0.0, 1.0);
                let sx = (x as u32 * thumb.width / width as u32).min(thumb.width - 1);
                let sy = (y as u32 * thumb.height / height as u32).min(thumb.height - 1);
                let from = ((sy * thumb.width + sx) * 4) as usize;
                let at = ((y * width + x) * 4) as usize;
                for channel in 0..3 {
                    out[at + channel] = (f32::from(thumb.pixels[from + channel]) * alpha) as u8;
                }
                out[at + 3] = (alpha * 255.0) as u8;
            }
        }
        let image = SHDRAGIMAGE {
            sizeDragImage: SIZE {
                cx: width,
                cy: height,
            },
            ptOffset: POINT {
                x: width / 2,
                y: height / 2,
            },
            hbmpDragImage: bitmap,
            crColorKey: COLORREF(0xFFFF_FFFF),
        };
        let helper: IDragSourceHelper =
            CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER)?;
        let shown = helper.InitializeFromBitmap(&image, data);
        if shown.is_err() {
            let _ = DeleteObject(bitmap.into());
        }
        shown
    }
}

/// Drags `shot` out of `hwnd` in the system's drag loop; true when something took it (an app, or the Shelf).
pub fn drag(hwnd: HWND, shot: &Shot, dpi: u32) -> Result<bool> {
    let info = shots::dib(&shot.dib).ok_or_else(|| Error::from(E_FAIL))?;
    // shortcut: the PNG is encoded as the drag starts (the drop target asks only during the loop), a pause on
    // a very large shot; make it lazy with an own IDataObject if that is felt.
    let png = png(&shot.dib, &info)?;
    // SAFETY: shell objects on this OLE-initialised STA thread; every format is copied into the object.
    unsafe {
        let data: IDataObject = SHCreateDataObject(None, None, None)?;
        put(&data, DIB, -1, &shot.dib)?;
        put(&data, RegisterClipboardFormatW(w!("PNG")) as u16, -1, &png)?;
        let name = format!("Screenshot {}.png", shot.time);
        put(
            &data,
            RegisterClipboardFormatW(CFSTR_FILEDESCRIPTORW) as u16,
            -1,
            &descriptor(&name, png.len()),
        )?;
        put(
            &data,
            RegisterClipboardFormatW(CFSTR_FILECONTENTS) as u16,
            0,
            &png,
        )?;
        put(&data, shot_format(), -1, &shot.id.to_le_bytes())?;
        drop(png);
        // A missing ghost still drags.
        let _ = ghost(shot, dpi, &data);
        let effect = SHDoDragDrop(Some(hwnd), &data, None, DROPEFFECT_COPY)?;
        Ok(effect != DROPEFFECT_NONE)
    }
}
