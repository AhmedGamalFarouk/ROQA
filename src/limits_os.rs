//! The Limits tab (features U1): the Windows half. One short-lived thread per job reads the small files and,
//! only with the user's permission, runs the user's own `agy` or `codex app-server` once (hidden, in a Job
//! object, bounded in time and output). Nothing here opens `auth.json`, `.credentials.json` or
//! `oauth_creds.json`, calls a vendor endpoint, or logs a path or a value. Nothing has been run against the
//! real tools by the tests.

use crate::chan::{Receiver, Sender, channel};
use crate::limits::{
    self, Job, NO_AGY, NO_CODEX, Outcome, Reading, Snap, Stamp, agy_text, claude_best,
    claude_cache, claude_file, codex_rpc, codex_tail,
};
use std::{
    ffi::c_void,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, LPARAM, SetHandleInformation,
            WPARAM,
        },
        Security::SECURITY_ATTRIBUTES,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Pipes::{CreatePipe, PeekNamedPipe},
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
                EXTENDED_STARTUPINFO_PRESENT, GetCurrentThreadId,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread,
                STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
                WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
    },
    core::{PCWSTR, PWSTR},
};

/// Thread message that only wakes the host loop; the answer waits in the channel.
const WAKE: u32 = WM_APP + 54;
const AGY_TIMEOUT: Duration = Duration::from_secs(8);
const AGY_CAP: usize = 8 * 1024;
const CODEX_TIMEOUT: Duration = Duration::from_secs(5);
const CODEX_CAP: usize = 64 * 1024;
/// The newest Codex log is read from its last 256 KB.
const TAIL: u64 = 256 * 1024;
/// `~/.claude.json` is a few hundred KB; a file this big is not read.
const CLAUDE_CAP: u64 = 16 * 1024 * 1024;

/// The host's end: starts jobs and collects their answers.
pub struct Io {
    host: u32,
    done: Sender<Outcome>,
    results: Receiver<Outcome>,
    active: Arc<AtomicBool>,
}
impl Io {
    /// Created on the host thread, whose id the workers wake.
    pub fn new() -> Self {
        let (done, results) = channel(usize::MAX);
        // SAFETY: plain query of the calling thread's id.
        let host = unsafe { GetCurrentThreadId() };
        Self {
            host,
            done,
            results,
            active: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Starts the job on its own thread; false if the thread cannot be created.
    pub fn spawn(&self, job: Job) -> bool {
        let (done, host) = (self.done.clone(), self.host);
        self.active.store(true, Ordering::Relaxed);
        let active = self.active.clone();
        std::thread::Builder::new()
            .name("roqa-limits".into())
            .spawn(move || {
                if done.send(run(&job, &active)).is_ok() {
                    // SAFETY: a scalar thread message with no pointers; a lost post only delays the poll.
                    let _ = unsafe { PostThreadMessageW(host, WAKE, WPARAM(0), LPARAM(0)) };
                }
            })
            .is_ok()
    }
    pub fn poll(&self) -> Option<Outcome> {
        self.results.try_recv().ok()
    }
    pub fn cancel(&self) {
        self.active.store(false, Ordering::Relaxed);
    }
}
impl Drop for Io {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn run(job: &Job, active: &AtomicBool) -> Outcome {
    let mut out = Outcome {
        id: job.id,
        group: job.group,
        ..Outcome::default()
    };
    if active.load(Ordering::Relaxed) {
        out.got[0] = claude(job);
    }
    if active.load(Ordering::Relaxed) {
        out.got[1] = codex(job, active);
    }
    if active.load(Ordering::Relaxed) {
        out.got[2] = agy(job, active);
    }
    out
}

// ---- Files ----

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}
fn local_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}
fn stamp(path: &Path) -> Stamp {
    fs::metadata(path).map_or(Stamp::default(), |meta| Stamp {
        mtime: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64),
        size: meta.len(),
    })
}
fn read_capped(path: &Path, cap: u64) -> Option<String> {
    if fs::metadata(path).ok()?.len() > cap {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= cap).then(|| String::from_utf8_lossy(&bytes).into_owned())
}
/// A parse that fails once is tried again after a moment: the writer may have been mid-write.
fn parse_twice(path: &Path, parse: fn(&str) -> Option<Snap>) -> Option<Snap> {
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(60));
        }
        let text = read_capped(path, CLAUDE_CAP)?;
        if let Some(snap) = parse(&text) {
            return Some(snap);
        }
    }
    None
}

fn claude(job: &Job) -> Option<(Reading, Stamp)> {
    let cache = home()?.join(".claude.json");
    let file = local_data()?.join("ROQA").join("claude-limits.json");
    let (a, b) = (stamp(&cache), stamp(&file));
    let now = Stamp {
        mtime: a.mtime.wrapping_add(b.mtime),
        size: a.size.wrapping_add(b.size),
    };
    if !job.manual && !job.due[0] && now == job.prev[0] {
        return None;
    }
    let reading = claude_best(
        parse_twice(&cache, claude_cache),
        parse_twice(&file, claude_file),
    );
    Some((reading, now))
}

/// Retains one newest log by modification time, within a shared bounded directory-entry budget.
fn newest_log(dir: &Path, depth: u8, budget: &mut usize, newest: &mut Option<(PathBuf, Stamp)>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if depth == 0 {
            if kind.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
                let stamp = stamp(&path);
                if newest
                    .as_ref()
                    .is_none_or(|(previous, old)| (stamp.mtime, &path) > (old.mtime, previous))
                {
                    *newest = Some((path, stamp));
                }
            }
        } else if kind.is_dir() && !kind.is_symlink() {
            newest_log(&path, depth - 1, budget, newest);
        }
    }
}
/// The last `TAIL` bytes of a log, from a whole line on.
fn tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let start = file.metadata().ok()?.len().saturating_sub(TAIL);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(match text.find('\n') {
        Some(at) if start > 0 => text[at + 1..].to_owned(),
        _ => text,
    })
}

fn codex(job: &Job, active: &AtomicBool) -> Option<(Reading, Stamp)> {
    let (mut newest, mut budget) = (None, 4097usize);
    if let Some(home) = home() {
        newest_log(
            &home.join(".codex").join("sessions"),
            3,
            &mut budget,
            &mut newest,
        );
    }
    if budget == 0 {
        newest = None;
    }
    let now = newest
        .as_ref()
        .map_or(Stamp::default(), |(_, stamp)| *stamp);
    let ask = job.manual && job.allow_codex;
    if !ask && !job.manual && !job.due[1] && now == job.prev[1] {
        return None;
    }
    let mut reading = Err(NO_CODEX);
    if let Some((log, _)) = newest {
        reading = tail(&log).map_or(Err(NO_CODEX), |text| codex_tail(&text));
        if reading
            .is_ok_and(|snap| job.now - snap.as_of > limits::STALE_SECS || snap.as_of > job.now)
        {
            reading = Err(NO_CODEX);
        }
    }
    if ask
        && active.load(Ordering::Relaxed)
        && let Ok(live) = codex_live(job.now, active)
    {
        reading = Ok(live);
    }
    Some((reading, now))
}

// ---- Processes ----

/// A Job object that kills what it holds when closed.
struct Jail(HANDLE);
impl Jail {
    fn new() -> Option<Self> {
        // SAFETY: a fresh anonymous job; the limit struct is a local the call only reads.
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
            let jail = Self(job);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()?;
            Some(jail)
        }
    }
}
impl Drop for Jail {
    fn drop(&mut self) {
        // SAFETY: the handle this struct created and owns; closing it kills the job's processes.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this guard exclusively owns the handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn pipe(parent_reads: bool) -> Option<(File, Owned)> {
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    // SAFETY: local output handles and security descriptor; each successful handle gets one owner.
    unsafe {
        CreatePipe(&mut read, &mut write, Some(&security), 4096).ok()?;
        let (parent, child) = if parent_reads {
            (Owned(read), Owned(write))
        } else {
            (Owned(write), Owned(read))
        };
        SetHandleInformation(parent.0, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)).ok()?;
        let file = File::from_raw_handle(parent.0.0);
        std::mem::forget(parent);
        Some((file, child))
    }
}

struct Attributes(Vec<usize>);
impl Attributes {
    fn list(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.0.as_ptr().cast_mut().cast())
    }
    fn new(handles: &[HANDLE]) -> Option<Self> {
        let mut size = 0;
        // SAFETY: size query followed by aligned, owned storage; the handle slice survives CreateProcessW.
        unsafe {
            let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
            if size == 0 {
                return None;
            }
            let storage = vec![0usize; size.div_ceil(size_of::<usize>())];
            let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_ptr().cast_mut().cast());
            InitializeProcThreadAttributeList(Some(list), 1, None, &mut size).ok()?;
            let attributes = Self(storage);
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr().cast()),
                size_of_val(handles),
                None,
                None,
            )
            .ok()?;
            Some(attributes)
        }
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: initialized attribute list, deleted before its backing storage is freed.
        unsafe { DeleteProcThreadAttributeList(self.list()) };
    }
}

struct Child {
    process: Owned,
    thread: Owned,
    _jail: Jail,
}
impl Drop for Child {
    fn drop(&mut self) {
        // SAFETY: exclusively owned process; termination also handles a still-suspended setup failure.
        unsafe {
            let _ = TerminateProcess(self.process.0, 1);
            let _ = WaitForSingleObject(self.process.0, 1000);
        }
    }
}

fn command(exe: &Path, args: &[&str]) -> Option<(Vec<u16>, Vec<u16>)> {
    let path = exe.to_str()?;
    let batch = exe
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"));
    if path.contains(['"', '\r', '\n', '\0'])
        || args.iter().any(|arg| arg.contains(['"', '\r', '\n', '\0']))
        || (batch
            && (path.contains(['%', '!', '^', '&', '|', '<', '>'])
                || args
                    .iter()
                    .any(|arg| arg.contains(['%', '!', '^', '&', '|', '<', '>']))))
    {
        return None;
    }
    let arguments = args
        .iter()
        .map(|arg| format!(" \"{arg}\""))
        .collect::<String>();
    let (application, line) = if batch {
        let shell = PathBuf::from(std::env::var_os("SystemRoot")?)
            .join("System32")
            .join("cmd.exe");
        let line = format!(
            "\"{}\" /d /v:off /s /c \"\"{path}\"{arguments}\"",
            shell.to_str()?
        );
        (shell, line)
    } else {
        (exe.to_owned(), format!("\"{path}\"{arguments}"))
    };
    Some((
        application.as_os_str().encode_wide().chain([0]).collect(),
        line.encode_utf16().chain([0]).collect(),
    ))
}

fn suspended(exe: &Path, args: &[&str], handles: [HANDLE; 2]) -> Option<Child> {
    let jail = Jail::new()?;
    let attributes = Attributes::new(&handles)?;
    let (application, mut line) = command(exe, args)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[1];
    startup.lpAttributeList = attributes.list();
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: NUL-terminated application, writable command line, live startup/attribute storage and handles.
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(line.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
            None,
            PCWSTR::null(),
            &startup.StartupInfo,
            &mut process,
        )
        .ok()?;
        let child = Child {
            process: Owned(process.hProcess),
            thread: Owned(process.hThread),
            _jail: jail,
        };
        AssignProcessToJobObject(child._jail.0, child.process.0).ok()?;
        Some(child)
    }
}

/// Runs `exe` hidden with `input` on stdin, collecting stdout until `done` says enough, the process ends,
/// `cap` bytes arrive or `timeout` passes; then everything it started is killed. `None` if it cannot start.
struct Run<'a> {
    exe: &'a Path,
    args: &'a [&'a str],
    input: &'a [u8],
    timeout: Duration,
    cap: usize,
    active: &'a AtomicBool,
}

fn capped(
    request: Run<'_>,
    mut done: impl FnMut(&[u8], &mut File) -> Option<bool>,
) -> Option<String> {
    let Run {
        exe,
        args,
        input,
        timeout,
        cap,
        active,
    } = request;
    if input.len() > 4096 || cap == 0 || !active.load(Ordering::Relaxed) {
        return None;
    }
    let (mut stdin, child_stdin) = pipe(false)?;
    let (mut stdout, child_stdout) = pipe(true)?;
    let child = suspended(exe, args, [child_stdin.0, child_stdout.0])?;
    stdin.write_all(input).ok()?;
    // SAFETY: setup and Job assignment succeeded, so no child code can run outside the kill-on-close Job.
    if !active.load(Ordering::Relaxed) || unsafe { ResumeThread(child.thread.0) } == u32::MAX {
        return None;
    }
    drop(child_stdin);
    drop(child_stdout);
    use std::os::windows::io::AsRawHandle;
    let pipe = HANDLE(stdout.as_raw_handle());
    let (mut bytes, start) = (Vec::new(), Instant::now());
    loop {
        if start.elapsed() >= timeout || !active.load(Ordering::Relaxed) {
            return None;
        }
        let mut available = 0u32;
        // SAFETY: a live pipe handle owned by `stdout`; only the available-byte count is asked for.
        let open =
            unsafe { PeekNamedPipe(pipe, None, 0, None, Some(&mut available), None) }.is_ok();
        if open && available > 0 {
            let mut chunk = [0u8; 1024];
            let want = (available as usize).min(chunk.len()).min(cap - bytes.len());
            match stdout.read(&mut chunk[..want]) {
                Ok(got) if got > 0 => bytes.extend_from_slice(&chunk[..got]),
                _ => break,
            }
            if bytes.len() >= cap {
                return None;
            }
            if done(&bytes, &mut stdin)? {
                break;
            }
            continue;
        }
        if !open || start.elapsed() >= timeout {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    drop(stdin);
    drop(child);
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// `name` as `.exe` or `.cmd` in the given extra directory, then in each PATH directory.
fn find(name: &str, extra: Option<PathBuf>) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    extra
        .into_iter()
        .chain(std::env::split_paths(&path))
        .find_map(|dir| {
            ["exe", "cmd"]
                .into_iter()
                .map(|ext| dir.join(format!("{name}.{ext}")))
                .find(|candidate| candidate.is_file())
        })
}

fn agy(job: &Job, active: &AtomicBool) -> Option<(Reading, Stamp)> {
    if !job.allow_agy {
        return job.due[2].then_some((Err(limits::OFF_AGY), Stamp::default()));
    }
    if !job.manual {
        return None;
    }
    let reading = find("agy", local_data().map(|d| d.join("agy").join("bin")))
        .and_then(|exe| {
            capped(
                Run {
                    exe: &exe,
                    args: &["-p", "/usage", "--output-format", "json"],
                    input: &[],
                    timeout: AGY_TIMEOUT,
                    cap: AGY_CAP,
                    active,
                },
                |_, _| Some(false),
            )
        })
        .map_or(Err(NO_AGY), |text| agy_text(&text, job.group, job.now));
    Some((reading, Stamp::default()))
}

/// `codex app-server` over stdio, once: initialize, then `account/rateLimits/read` (newline-delimited JSON,
/// no `jsonrpc` field), stopped as soon as the reply with id 1 arrives.
fn codex_live(now: i64, active: &AtomicBool) -> Result<Snap, &'static str> {
    let exe = find("codex", None).ok_or(NO_CODEX)?;
    let input = "{\"method\":\"initialize\",\"id\":0,\"params\":{\"clientInfo\":{\"name\":\"roqa\",\"title\":\"ROQA\",\"version\":\"0.1.0\"}}}\n";
    let request = concat!(
        "{\"method\":\"initialized\",\"params\":{}}\n",
        "{\"method\":\"account/rateLimits/read\",\"id\":1}\n"
    );
    let mut initialized = false;
    let text = capped(
        Run {
            exe: &exe,
            args: &["app-server"],
            input: input.as_bytes(),
            timeout: CODEX_TIMEOUT,
            cap: CODEX_CAP,
            active,
        },
        |bytes, stdin| {
            // Only whole lines count: the reply may still be arriving.
            let whole = bytes.iter().rposition(|&b| b == 10).map_or(0, |at| at + 1);
            let Ok(text) = std::str::from_utf8(&bytes[..whole]) else {
                return Some(false);
            };
            if !initialized && let Some(reply) = limits::rpc_reply(text, 0) {
                if reply.contains("\"error\"") {
                    return None;
                }
                stdin.write_all(request.as_bytes()).ok()?;
                initialized = true;
            }
            Some(initialized && limits::rpc_reply(text, 1).is_some())
        },
    )
    .ok_or(NO_CODEX)?;
    codex_rpc(&text, now)
}

/// Seconds since 1970 now.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_command_is_explicit_and_batch_metacharacters_fail_closed() {
        let (application, line) =
            command(Path::new("C:\\Tools\\codex.exe"), &["app-server"]).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&application),
            "C:\\Tools\\codex.exe\0"
        );
        assert_eq!(
            String::from_utf16_lossy(&line),
            "\"C:\\Tools\\codex.exe\" \"app-server\"\0"
        );
        assert!(command(Path::new("C:\\bad%name\\agy.cmd"), &[]).is_none());
        assert!(command(Path::new("C:\\bad&name\\agy.cmd"), &[]).is_none());
        assert!(command(Path::new("C:\\Tools\\agy.exe"), &["bad\"argument"]).is_none());
    }

    #[test]
    fn automatic_refresh_never_spawns_agy() {
        let job = Job {
            id: 1,
            now: 1000,
            manual: false,
            due: [true; 3],
            allow_codex: false,
            allow_agy: true,
            group: 0,
            prev: [Stamp::default(); 2],
        };
        let active = AtomicBool::new(true);
        assert!(agy(&job, &active).is_none());
        assert_eq!(
            agy(
                &Job {
                    allow_agy: false,
                    ..job
                },
                &active
            )
            .unwrap()
            .0,
            Err(limits::OFF_AGY)
        );
    }
}
