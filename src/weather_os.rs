//! Weather (features W1): the Windows half. One short-lived thread per job does the work that can block:
//! the WinHTTP calls (OS TLS, every wait bounded) and the WinRT `Geolocator`. The answer waits in a channel
//! and a thread message wakes the host loop, which polls it (`brightness.rs` pattern). Coordinates are never
//! logged; the only diagnostics are fixed strings. Nothing here has been run against the network.

use crate::{
    picker::wide,
    weather::{
        FORECAST_HOST, Fail, GEOCODE_HOST, Job, Outcome, Place, Source, forecast_path,
        geocode_path, parse_forecast, parse_place, sources,
    },
};
use std::{
    ffi::c_void,
    mem::size_of,
    sync::mpsc::{Receiver, Sender, channel},
};
use windows::{
    Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy},
    Foundation::TimeSpan,
    Win32::{
        Foundation::{LPARAM, SYSTEMTIME, WPARAM},
        Globalization::{GetTimeFormatEx, GetUserDefaultLocaleName, TIME_NOMINUTESORSECONDS},
        Networking::WinHttp::*,
        System::{
            SystemInformation::GetTickCount,
            Threading::GetCurrentThreadId,
            WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        },
        UI::{
            Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
            WindowsAndMessaging::{PostThreadMessageW, WM_APP},
        },
    },
    core::{PCWSTR, w},
};

/// Thread message that only wakes the host loop; the answer waits in the channel.
const WAKE: u32 = WM_APP + 53;
/// Resolve, connect, send and receive each wait at most this long.
const TIMEOUT_MS: i32 = 8_000;
/// A reply is a few KB; anything bigger is refused.
const MAX_BODY: usize = 64 * 1024;
/// No keyboard or mouse input for this long counts as away: no background fetch.
pub const IDLE_AFTER_SECS: u64 = 600;

/// The host's end: starts jobs and collects their answers.
pub struct Io {
    host: u32,
    done: Sender<(u64, Outcome)>,
    results: Receiver<(u64, Outcome)>,
}
impl Io {
    /// Created on the host thread, whose id the workers wake.
    pub fn new() -> Self {
        let (done, results) = channel();
        // SAFETY: plain query of the calling thread's id.
        let host = unsafe { GetCurrentThreadId() };
        Self {
            host,
            done,
            results,
        }
    }
    /// Starts the job on its own thread; false if the thread cannot be created.
    pub fn spawn(&self, job: Job) -> bool {
        let (done, host) = (self.done.clone(), self.host);
        std::thread::Builder::new()
            .name("roqa-weather".into())
            .spawn(move || {
                let id = job.id;
                let outcome = run(job);
                if done.send((id, outcome)).is_ok() {
                    // SAFETY: a scalar thread message with no pointers; a lost post only delays the poll.
                    let _ = unsafe { PostThreadMessageW(host, WAKE, WPARAM(0), LPARAM(0)) };
                }
            })
            .is_ok()
    }
    pub fn poll(&self) -> Option<(u64, Outcome)> {
        self.results.try_recv().ok()
    }
}

fn run(job: Job) -> Outcome {
    let mut place = None;
    for source in sources(job.detect, job.known.is_some(), &job.city) {
        place = match source {
            Source::Detect => locate(),
            Source::Known => job.known.clone(),
            Source::City => match geocode(&job.city) {
                Ok(place) => Some(place),
                Err(Fail::Unlocated) => None,
                Err(fail) => {
                    return Outcome {
                        place: None,
                        result: Err(fail),
                    };
                }
            },
        };
        if place.is_some() {
            break;
        }
    }
    let Some(place) = place else {
        return Outcome {
            place: None,
            result: Err(Fail::Unlocated),
        };
    };
    let result = get(FORECAST_HOST, &forecast_path(&place))
        .and_then(|body| parse_forecast(&body, crate::timer::wall_now()).ok_or(Fail::Data));
    Outcome {
        place: Some(place),
        result,
    }
}

fn geocode(city: &str) -> Result<Place, Fail> {
    let path = geocode_path(city).ok_or(Fail::Unlocated)?;
    parse_place(&get(GEOCODE_HOST, &path)?).ok_or(Fail::Unlocated)
}

/// A WinHTTP handle, closed on drop.
struct Handle(*mut c_void);
impl Handle {
    fn open(raw: *mut c_void) -> Result<Self, Fail> {
        (!raw.is_null()).then_some(Self(raw)).ok_or(Fail::Network)
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle this struct opened and owns; closing it once, last.
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// `https://host/path` through the OS stack: the system proxy and certificate checks apply.
fn get(host: &str, path: &str) -> Result<String, Fail> {
    let (host, path) = (wide(host), wide(path));
    // SAFETY: every handle is owned by a `Handle` that outlives its children (declared in order, dropped in
    // reverse); the wide strings and the buffers below outlive the calls using them.
    unsafe {
        let session = Handle::open(WinHttpOpen(
            w!("ROQA"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS)
            .map_err(|_| Fail::Network)?;
        let connection = Handle::open(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0))?;
        let request = Handle::open(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|_| Fail::Network)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|_| Fail::Network)?;
        let (mut status, mut size) = (0u32, size_of::<u32>() as u32);
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(std::ptr::from_mut(&mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|_| Fail::Network)?;
        if status != 200 {
            return Err(Fail::Network);
        }
        let (mut body, mut chunk) = (Vec::new(), [0u8; 4096]);
        loop {
            let mut got = 0;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut got,
            )
            .map_err(|_| Fail::Network)?;
            if got == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..got as usize]);
            if body.len() > MAX_BODY {
                return Err(Fail::Data);
            }
        }
        String::from_utf8(body).map_err(|_| Fail::Data)
    }
}

/// The device's position, coarse is enough; `None` when location is off, denied or unavailable. The
/// Windows privacy setting decides whether this is allowed.
fn locate() -> Option<Place> {
    // SAFETY: balanced below on this same thread when it succeeded; the apartment is this thread's own.
    let started = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
    let found = (|| -> windows::core::Result<Option<Place>> {
        if Geolocator::RequestAccessAsync()?.join()? != GeolocationAccessStatus::Allowed {
            return Ok(None);
        }
        let locator = Geolocator::new()?;
        locator.SetDesiredAccuracy(PositionAccuracy::Default)?;
        let seconds = |s: i64| TimeSpan {
            Duration: s * 10_000_000,
        };
        let position = locator
            .GetGeopositionAsyncWithAgeAndTimeout(seconds(600), seconds(8))?
            .join()?
            .Coordinate()?
            .Point()?
            .Position()?;
        Ok(Place::located(position.Latitude, position.Longitude))
    })();
    if started {
        // SAFETY: pairs the successful RoInitialize above.
        unsafe { RoUninitialize() };
    }
    found.ok().flatten()
}

/// The two-letter region of the user's Windows locale ("US" for en-US), else empty.
pub fn region() -> String {
    let mut name = [0u16; 85];
    // SAFETY: the buffer is the documented LOCALE_NAME_MAX_LENGTH.
    let length = unsafe { GetUserDefaultLocaleName(&mut name) }.max(1) as usize - 1;
    String::from_utf16_lossy(&name[..length.min(name.len())])
        .rsplit('-')
        .next()
        .filter(|part| part.len() == 2)
        .map_or_else(String::new, str::to_uppercase)
}

/// Seconds since the last keyboard or mouse input anywhere in the session; 0 if Windows cannot say.
pub fn idle_secs() -> u64 {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a local with its size set, as the call requires.
    unsafe {
        if GetLastInputInfo(&mut info).as_bool() {
            u64::from(GetTickCount().wrapping_sub(info.dwTime)) / 1000
        } else {
            0
        }
    }
}

/// An hour of the day as the user's locale writes it ("2 PM", or "14").
pub fn hour_label(hour: u8) -> String {
    let time = SYSTEMTIME {
        wHour: u16::from(hour.min(23)),
        ..Default::default()
    };
    let mut text = [0u16; 32];
    // SAFETY: NULL locale = user default; the output is a bounded slice whose length includes the NUL.
    let length = unsafe {
        GetTimeFormatEx(
            PCWSTR::null(),
            TIME_NOMINUTESORSECONDS,
            Some(&time),
            PCWSTR::null(),
            Some(&mut text),
        )
    };
    String::from_utf16_lossy(&text[..(length.max(1) as usize - 1).min(text.len())])
}
