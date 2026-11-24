//! Small Windows switches the settings and quick settings reach: start at sign-in, keep awake, lock and sleep. Each is one
//! call made when the user asks; the only thing here that runs on its own is the Start menu shortcut at start-up.

use std::{ffi::OsStr, path::Path, path::PathBuf};

use windows::{
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_NOT_ALL_ASSIGNED, ERROR_SUCCESS, GENERIC_READ,
            GENERIC_WRITE, GetLastError, HANDLE,
        },
        Security::{
            AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME,
            TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
            },
            Power::{
                ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetSuspendState,
                SetThreadExecutionState,
            },
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
                RegOpenKeyExW, RegSetValueExW,
            },
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
        UI::Shell::{
            FOLDERID_LocalAppData, FOLDERID_Programs, IShellLinkW, KF_FLAG_DONT_VERIFY,
            SHGetKnownFolderPath, SLGP_RAWPATH, ShellLink,
        },
    },
    core::{HSTRING, Interface, w},
};

/// Puts ROQA.lnk in the user's Start menu Programs folder, so Windows search finds ROQA, when it is missing or
/// points at another exe (ROQA moved). One short worker at start-up; a test profile never touches the Start menu.
pub fn ensure_start_shortcut() {
    // A dev build never points the user's shortcut at itself.
    if cfg!(debug_assertions) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("roqa-shortcut".into())
        .spawn(|| {
            if let Err(error) = write_start_shortcut() {
                eprintln!("Start menu shortcut: {error}");
            }
        });
}

fn write_start_shortcut() -> windows::core::Result<()> {
    let (Ok(exe), Some(local), Some(programs)) = (
        std::env::current_exe(),
        known_folder(&FOLDERID_LocalAppData),
        known_folder(&FOLDERID_Programs),
    ) else {
        return Ok(());
    };
    if redirected(std::env::var_os("LOCALAPPDATA").as_deref(), &local) {
        return Ok(());
    }
    let shortcut = HSTRING::from(programs.join("ROQA.lnk").as_os_str());
    // SAFETY: this worker's own apartment, balanced below; the link objects are dropped before it ends, and the
    // path buffer is sized and terminated by GetPath.
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let file: IPersistFile = link.cast()?;
            let current = file.Load(&shortcut, STGM_READ).ok().and_then(|()| {
                let mut path = [0u16; 32768];
                link.GetPath(&mut path, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
                    .ok()?;
                let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
                Some(PathBuf::from(String::from_utf16_lossy(&path[..len])))
            });
            if !needs_write(current.as_deref(), &exe) {
                return Ok(());
            }
            link.SetPath(&HSTRING::from(exe.as_os_str()))?;
            file.Save(&shortcut, true)
        })();
        if com {
            CoUninitialize();
        }
        result
    }
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    // SAFETY: the returned string is copied, then freed with CoTaskMemFree as the API requires.
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None).ok()?;
        let owned = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0 as _));
        owned
    }
}

/// True when LOCALAPPDATA is not the user's real folder: the test and capture scripts point it at a private profile.
fn redirected(env: Option<&OsStr>, real: &Path) -> bool {
    let trim = |p: &OsStr| p.to_string_lossy().trim_end_matches('\\').to_owned();
    !env.is_some_and(|env| trim(env).eq_ignore_ascii_case(&trim(real.as_os_str())))
}

fn needs_write(current: Option<&Path>, exe: &Path) -> bool {
    !current.is_some_and(|path| path.as_os_str().eq_ignore_ascii_case(exe.as_os_str()))
}

/// Writes (on) or removes (off) ROQA's value under the user's Run key. Off by default and never touched
/// unless the switch changes, so a user who never turns it on gets no registry entry. False when the registry
/// refused, so the switch can say so and go back instead of promising a start that will not happen.
pub fn set_autostart(on: bool) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let command = format!("\"{}\"", exe.display());
    let data: Vec<u8> = command
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    // SAFETY: valid terminated key and value names, an out handle closed below, and a wide string
    // that includes its terminator.
    unsafe {
        let mut key = HKEY::default();
        let opened = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            KEY_SET_VALUE,
            &mut key,
        );
        if opened != ERROR_SUCCESS {
            return false;
        }
        let done = if on {
            RegSetValueExW(key, w!("ROQA"), None, REG_SZ, Some(&data)) == ERROR_SUCCESS
        } else {
            // Nothing to remove is as good as removed.
            matches!(
                RegDeleteValueW(key, w!("ROQA")),
                ERROR_SUCCESS | ERROR_FILE_NOT_FOUND
            )
        };
        let _ = RegCloseKey(key);
        done
    }
}

/// Keeps the screen and the PC awake while on. The request belongs to the calling (host) thread, so
/// Windows drops it when ROQA quits.
pub fn keep_awake(on: bool) {
    let flags = if on {
        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    // SAFETY: a scalar flag call with no pointers.
    unsafe {
        SetThreadExecutionState(flags);
    }
}

#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn DeviceIoControl(
        device: HANDLE,
        code: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
        returned: *mut u32,
        overlapped: *mut u8,
    ) -> i32;
}

/// Lock (it fails while a file is open, so nothing is lost), dismount, allow removal, eject: Safe Remove's steps on an
/// open volume handle. False when any step is refused.
fn release(volume: HANDLE) -> bool {
    const LOCK: u32 = 0x0009_0018;
    const DISMOUNT: u32 = 0x0009_0020;
    const ALLOW_REMOVAL: u32 = 0x002D_4804;
    const EJECT: u32 = 0x002D_4808;
    let step = |code: u32, input: &[u8]| control(volume, code, input, &mut []).is_some();
    step(LOCK, &[]) && step(DISMOUNT, &[]) && step(ALLOW_REMOVAL, &[0]) && step(EJECT, &[])
}

/// One `DeviceIoControl` on `handle`: the bytes written to `output`, or `None` when Windows refused.
fn control(handle: HANDLE, code: u32, input: &[u8], output: &mut [u8]) -> Option<usize> {
    let mut returned = 0;
    // SAFETY: both buffers are live slices and the call passes the lengths they state; the call is synchronous.
    let done = unsafe {
        DeviceIoControl(
            handle,
            code,
            input.as_ptr(),
            input.len() as u32,
            output.as_mut_ptr(),
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    (done != 0).then_some(returned as usize)
}

/// Opens a volume (`\\.\E:` or `\\?\Volume{GUID}` with no trailing backslash) for `access` (0: queries only).
fn open_volume(path: &str, access: u32) -> Option<crate::ipc::OwnedHandle> {
    // SAFETY: the path outlives the call; the handle is owned (closed on drop).
    unsafe {
        CreateFileW(
            &HSTRING::from(path),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()
    .map(crate::ipc::OwnedHandle)
}

/// A disk as the storage stack numbers it: device type, device number, partition number.
type Disk = (u32, u32, u32);

/// What one volume handle says about itself, read with the handle open for queries only.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Probe {
    disk: Disk,
    /// The mount manager's unique id for the volume (disk signature and offset, or the partition GUID).
    unique: Vec<u8>,
    usb: bool,
}

/// `STORAGE_BUS_TYPE` `BusTypeUsb`.
const BUS_USB: u32 = 7;

fn probe(handle: HANDLE) -> Option<Probe> {
    const GET_DEVICE_NUMBER: u32 = 0x002D_1080;
    const QUERY_PROPERTY: u32 = 0x002D_1400;
    const QUERY_UNIQUE_ID: u32 = 0x004D_0000;
    let word =
        |bytes: &[u8], at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let mut number = [0u8; 12];
    (control(handle, GET_DEVICE_NUMBER, &[], &mut number)? == number.len()).then_some(())?;
    // STORAGE_PROPERTY_QUERY: StorageDeviceProperty (0), PropertyStandardQuery (0), one spare byte, padded to 12.
    let mut descriptor = [0u8; 1024];
    let written = control(handle, QUERY_PROPERTY, &[0; 12], &mut descriptor)?;
    // STORAGE_DEVICE_DESCRIPTOR.BusType sits at byte 28.
    let bus = word(descriptor.get(..written)?, 28)?;
    let mut unique = [0u8; 256];
    let written = control(handle, QUERY_UNIQUE_ID, &[], &mut unique)?;
    let length = usize::from(u16::from_le_bytes([unique[0], unique[1]]));
    (length > 0 && written >= 2 + length).then_some(())?;
    Some(Probe {
        disk: (word(&number, 0)?, word(&number, 4)?, word(&number, 8)?),
        unique: unique[2..2 + length].to_vec(),
        usb: bus == BUS_USB,
    })
}

/// A USB volume's stable identity, captured while Places lists drives. The letter is display text only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub letter: char,
    /// `\\?\Volume{GUID}`, no trailing backslash: what the eject opens.
    guid: String,
    probe: Probe,
}

#[cfg(test)]
impl Volume {
    /// A made-up identity for tests that never touch hardware.
    pub fn for_test(letter: char) -> Self {
        Self {
            letter,
            guid: String::new(),
            probe: Probe {
                disk: (7, 0, 1),
                unique: vec![1],
                usb: true,
            },
        }
    }
}

/// The volume at `now` is the captured one: still USB, the same disk slot and the same mount manager id. Nothing
/// readable means not the same.
fn unchanged(was: &Probe, now: Option<&Probe>) -> bool {
    now.is_some_and(|now| now.usb && now == was)
}

/// Whether `mine` is the only volume on its physical device among `volumes` (every volume Windows lists, lettered
/// or not). A volume that could not be read might be on this device, so it rules the device out.
fn sole_volume(mine: Disk, volumes: &[Option<Disk>]) -> bool {
    volumes.iter().all(Option::is_some)
        && volumes
            .iter()
            .flatten()
            .filter(|disk| disk.0 == mine.0 && disk.1 == mine.1)
            .count()
            == 1
}

/// Every volume's disk, from the volume list Windows keeps (lettered, unlettered and folder-mounted), read with
/// query-only handles. `None` when the list cannot be walked to its end.
fn all_volumes() -> Option<Vec<Option<Disk>>> {
    use windows::Win32::{
        Foundation::ERROR_NO_MORE_FILES,
        Storage::FileSystem::{FindFirstVolumeW, FindNextVolumeW, FindVolumeClose},
    };
    let mut name = [0u16; 64];
    // SAFETY: `name` is a live, sized buffer for each call; the find handle is closed once below.
    let find = unsafe { FindFirstVolumeW(&mut name) }.ok()?;
    let mut disks = Vec::new();
    let complete = loop {
        let end = name.iter().position(|c| *c == 0).unwrap_or(0);
        let path = String::from_utf16_lossy(&name[..end]);
        disks.push(
            open_volume(path.trim_end_matches('\\'), 0)
                .and_then(|volume| probe(volume.0).map(|probe| probe.disk)),
        );
        // SAFETY: as above.
        if let Err(error) = unsafe { FindNextVolumeW(find, &mut name) } {
            break error.code() == ERROR_NO_MORE_FILES.to_hresult();
        }
    };
    // SAFETY: the handle came from FindFirstVolumeW and is closed once.
    let _ = unsafe { FindVolumeClose(find) };
    complete.then_some(disks)
}

/// The identity of drive `letter` when it can be proven to be one USB volume alone on its device, else `None` (no
/// Eject row). Blocking disk calls: run on the Places worker.
pub fn usb_volume(letter: char) -> Option<Volume> {
    use windows::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
    let mut name = [0u16; 64];
    // SAFETY: the root string and the sized buffer outlive the call.
    unsafe { GetVolumeNameForVolumeMountPointW(&HSTRING::from(format!("{letter}:\\")), &mut name) }
        .ok()?;
    let end = name.iter().position(|c| *c == 0)?;
    let guid = String::from_utf16_lossy(&name[..end])
        .trim_end_matches('\\')
        .to_owned();
    let probe = probe(open_volume(&guid, 0)?.0).filter(|probe| probe.usb)?;
    sole_volume(probe.disk, &all_volumes()?).then_some(Volume {
        letter,
        guid,
        probe,
    })
}

/// How an eject ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ejected {
    /// A step was refused (a file is open).
    Refused,
    Safe,
    /// The volume is gone, replaced or no longer alone on its USB device: nothing was locked or dismounted.
    Changed,
}

impl Ejected {
    /// The notice row's text for drive `letter`.
    pub fn notice(self, letter: char) -> String {
        match self {
            Self::Safe => format!("{letter}: safe to remove"),
            Self::Refused => format!("{letter}: still in use"),
            Self::Changed => "That drive isn't there now".into(),
        }
    }
}

/// Ejects the volume `volume` names. The GUID is opened (never the letter), and on that very handle the identity is
/// compared with what was captured before any lock; the single-volume rule is read again too.
fn eject_volume(volume: &Volume) -> Ejected {
    let Some(handle) = open_volume(&volume.guid, GENERIC_READ.0 | GENERIC_WRITE.0) else {
        return Ejected::Changed;
    };
    let now = probe(handle.0);
    if !unchanged(&volume.probe, now.as_ref())
        || !all_volumes().is_some_and(|all| sole_volume(volume.probe.disk, &all))
    {
        return Ejected::Changed;
    }
    if release(handle.0) {
        Ejected::Safe
    } else {
        Ejected::Refused
    }
}

/// The one eject slot: 0 free, `BUSY` while a worker runs, otherwise a finished answer waiting to be read
/// (`letter << 8 | 2` refused, `| 3` safe to remove, `| 4` changed). A new eject starts only from 0, so an unread
/// answer is never replaced and each answer names its drive.
static EJECT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const BUSY: u32 = 1;
/// Posted to the host thread when an eject finished; it only wakes the loop, `ejected` carries the answer.
const EJECTED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 61;

fn eject_begin() -> bool {
    use std::sync::atomic::Ordering::{AcqRel, Acquire};
    EJECT.compare_exchange(0, BUSY, AcqRel, Acquire).is_ok()
}

fn eject_finish(letter: char, answer: Ejected) {
    let code = match answer {
        Ejected::Refused => 2,
        Ejected::Safe => 3,
        Ejected::Changed => 4,
    };
    EJECT.store(
        (letter as u32) << 8 | code,
        std::sync::atomic::Ordering::Release,
    );
}

/// `job` on a short worker: locking and dismounting flush the drive, which can take seconds on a slow stick and
/// must not stop the dock and notch. The answer comes back through `ejected`. False, and nothing starts, while an
/// earlier eject is out or its answer is unread.
fn eject_on_worker(letter: char, job: impl FnOnce() -> Ejected + Send + 'static) -> bool {
    if !eject_begin() {
        return false;
    }
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("roqa-eject".into())
        .spawn(move || {
            eject_finish(letter, job());
            // A failed post is picked up on the host's next pass.
            let _ = crate::native_window::post_wake(host, EJECTED);
        });
    if spawned.is_err() {
        eject_finish(letter, Ejected::Refused);
    }
    true
}

/// Places' and the arrival alert's eject, by captured identity (`eject_volume`), never by letter.
pub fn eject_volume_in_background(volume: Volume) -> bool {
    eject_on_worker(volume.letter, move || eject_volume(&volume))
}

/// Arrived drives' identities read by `identify_in_background`, oldest first, until the host takes them: `None`
/// when a drive is not one USB volume alone on its device. A later arrival never replaces an earlier one.
static ARRIVED: std::sync::Mutex<Vec<Option<Volume>>> = std::sync::Mutex::new(Vec::new());

/// Reads arrived drive `letter`'s identity (`usb_volume`) on a short worker, as Places does on its own; the host
/// takes it through `arrived`. False when no worker could start.
pub fn identify_in_background(letter: char) -> bool {
    // SAFETY: a scalar thread id query.
    let host = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    std::thread::Builder::new()
        .name("roqa-drive".into())
        .spawn(move || {
            let volume = usb_volume(letter);
            ARRIVED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(volume);
            // A failed post is picked up on the host's next pass.
            let _ = crate::native_window::post_wake(host, EJECTED);
        })
        .is_ok()
}

/// The arrived drives' identities read since the last call, oldest first.
pub fn arrived() -> Vec<Option<Volume>> {
    std::mem::take(
        &mut *ARRIVED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// The finished eject's drive and how it ended, once.
pub fn ejected() -> Option<(char, Ejected)> {
    use std::sync::atomic::Ordering::{AcqRel, Acquire};
    let done = EJECT
        .fetch_update(AcqRel, Acquire, |state| (state > BUSY).then_some(0))
        .ok()?;
    let answer = match done & 0xFF {
        3 => Ejected::Safe,
        4 => Ejected::Changed,
        _ => Ejected::Refused,
    };
    Some((char::from_u32(done >> 8)?, answer))
}

#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    fn LockWorkStation() -> i32;
}

/// Locks the session, as Win+L does.
pub fn lock() {
    // SAFETY: no arguments; Windows refuses quietly when it cannot lock.
    unsafe {
        LockWorkStation();
    }
}

/// Puts the PC to sleep (not hibernate), asking running apps nothing; false when Windows refused. The shutdown
/// privilege `SetSuspendState` needs is enabled only around the call.
pub fn sleep() -> bool {
    let mut token = HANDLE::default();
    let mut wanted = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        ..Default::default()
    };
    wanted.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
    let mut previous = TOKEN_PRIVILEGES::default();
    let mut size = 0;
    // SAFETY: the token is this process's own, owned by the guard below; every pointer is a local that outlives
    // its call, and `previous` has room for the one privilege changed.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES, &mut token).is_err() {
            return false;
        }
        let token = crate::ipc::OwnedHandle(token);
        if LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut wanted.Privileges[0].Luid).is_err()
            || AdjustTokenPrivileges(
                token.0,
                false,
                Some(&wanted),
                std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
                Some(&mut previous),
                Some(&mut size),
            )
            .is_err()
            || GetLastError() == ERROR_NOT_ALL_ASSIGNED
        {
            return false;
        }
        let slept = SetSuspendState(false, false, false);
        let _ = AdjustTokenPrivileges(token.0, false, Some(&previous), 0, None, None);
        slept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_is_written_when_missing_or_pointing_elsewhere() {
        let exe = Path::new(r"G:\Code\ROQA\target\release\roqa.exe");
        assert!(needs_write(None, exe));
        assert!(needs_write(Some(Path::new(r"C:\Old\roqa.exe")), exe));
        assert!(!needs_write(
            Some(Path::new(r"g:\code\roqa\TARGET\release\ROQA.EXE")),
            exe
        ));
    }

    #[test]
    fn a_test_profile_is_recognised_by_its_redirected_local_app_data() {
        let real = Path::new(r"C:\Users\A\AppData\Local");
        assert!(!redirected(
            Some(OsStr::new(r"c:\users\a\appdata\local\")),
            real
        ));
        assert!(redirected(
            Some(OsStr::new(r"G:\Code\ROQA\artifacts\runtime\profile")),
            real
        ));
        assert!(redirected(None, real));
    }

    #[test]
    fn one_eject_at_a_time_keeps_the_first_unread_answer_and_names_its_drive() {
        assert_eq!(ejected(), None);
        assert!(eject_begin());
        assert!(!eject_begin(), "busy while the worker runs");
        assert_eq!(ejected(), None, "nothing to read yet");
        eject_finish('E', Ejected::Safe);
        assert!(!eject_begin(), "an unread answer is never replaced");
        assert_eq!(ejected(), Some(('E', Ejected::Safe)));
        assert_eq!(ejected(), None, "read once");
        assert!(eject_begin());
        eject_finish('F', Ejected::Refused);
        assert_eq!(ejected(), Some(('F', Ejected::Refused)));
        assert!(eject_begin());
        eject_finish('G', Ejected::Changed);
        assert_eq!(ejected(), Some(('G', Ejected::Changed)));
    }

    fn probe_of(unique: u8, number: u32, usb: bool) -> Probe {
        Probe {
            disk: (7, number, 1),
            unique: vec![unique],
            usb,
        }
    }

    #[test]
    fn an_eject_goes_by_identity_and_refuses_a_volume_it_cannot_prove() {
        // The identity's volume path does not open: nothing is locked, the drive "isn't there".
        assert_eq!(eject_volume(&Volume::for_test('E')), Ejected::Changed);
    }

    #[test]
    fn only_the_very_volume_captured_may_be_ejected() {
        let was = probe_of(1, 3, true);
        assert!(unchanged(&was, Some(&was.clone())));
        assert!(!unchanged(&was, None), "gone or unreadable");
        assert!(
            !unchanged(&was, Some(&probe_of(2, 3, true))),
            "another volume"
        );
        assert!(
            !unchanged(&was, Some(&probe_of(1, 4, true))),
            "disk slot reused"
        );
        assert!(
            !unchanged(&was, Some(&probe_of(1, 3, false))),
            "not USB now"
        );
    }

    #[test]
    fn a_device_must_carry_exactly_one_readable_volume() {
        let mine = (7, 3, 1);
        let other = Some((7, 0, 1));
        assert!(sole_volume(mine, &[other, Some(mine), Some((4, 3, 0))]));
        assert!(
            !sole_volume(mine, &[Some(mine), Some((7, 3, 2))]),
            "sibling"
        );
        assert!(!sole_volume(mine, &[Some(mine), None]), "unreadable volume");
        assert!(!sole_volume(mine, &[other]), "the volume itself is missing");
        assert!(!sole_volume(mine, &[]));
    }
}
