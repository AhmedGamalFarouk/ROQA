//! Bounded, fixed-context fallback for the GUI subsystem. No idle work or user strings.
use std::{
    fs::File,
    io::Write,
    sync::{Mutex, OnceLock},
};
static LOG: OnceLock<Mutex<Option<(File, usize)>>> = OnceLock::new();
static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn started() {
    STARTED.store(true, std::sync::atomic::Ordering::Relaxed);
}
const LIMIT: usize = 16 * 1024;
/// HRESULT of `ERROR_ALREADY_EXISTS`: a second launch exits quietly, before any log or dialog.
pub const ALREADY_RUNNING: i32 = 0x8007_00B7_u32 as i32;
fn wants_log(args: &[String]) -> bool {
    !args.iter().any(|arg| arg == "--diagnostics")
        && (args.len() == 1 || args.iter().any(|arg| arg.starts_with("--probe-")))
}
/// Call only after single-instance admission, so a second launch never truncates the live log.
pub fn initialize() {
    let args: Vec<String> = std::env::args().collect();
    let file = if !wants_log(&args) {
        None
    } else {
        std::env::var_os("LOCALAPPDATA").and_then(|base| {
            let directory = std::path::PathBuf::from(base).join("ROQA");
            std::fs::create_dir_all(&directory).ok()?;
            File::create(directory.join("error.log"))
                .ok()
                .map(|file| (file, 0))
        })
    };
    let _ = LOG.set(Mutex::new(file));
}
pub fn record(template: &'static str) {
    write_line(template.split('{').next().unwrap_or("ROQA error"));
}
fn write_line(context: &str) {
    let Some(Ok(mut log)) = LOG.get().map(Mutex::lock) else {
        return;
    };
    let Some((file, bytes)) = log.as_mut() else {
        return;
    };
    if *bytes + context.len() < LIMIT {
        let _ = writeln!(file, "{context}");
        *bytes += context.len() + 1;
    }
}
fn code_line(code: i32) -> String {
    format!("ROQA startup failure 0x{:08X}", code as u32)
}
/// Diagnostics, debug fault probes, a running host and a second launch must stay unattended.
fn dialog_wanted(code: i32, started: bool, args: &[String]) -> bool {
    code != ALREADY_RUNNING
        && !started
        && args.len() == 1
        && !args.iter().any(|arg| {
            arg == "--diagnostics" || (cfg!(debug_assertions) && arg.starts_with("--probe-"))
        })
}
pub fn startup_failure(code: i32) {
    write_line(&code_line(code));
    let args: Vec<String> = std::env::args().collect();
    if !dialog_wanted(
        code,
        STARTED.load(std::sync::atomic::Ordering::Relaxed),
        &args,
    ) {
        return;
    }
    let message: Vec<u16> = format!(
        "ROQA could not complete startup (0x{:08X}). See %LOCALAPPDATA%\\ROQA\\error.log.",
        code as u32
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    // SAFETY: fixed, terminated message; no retained pointers and no owner window on failed startup.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            windows::core::PCWSTR(message.as_ptr()),
            windows::core::w!("ROQA"),
            windows::Win32::UI::WindowsAndMessaging::MB_OK
                | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn second_launch_is_quiet_and_other_failures_still_show() {
        let plain = args(&["roqa"]);
        assert!(!dialog_wanted(ALREADY_RUNNING, false, &plain));
        assert!(dialog_wanted(0x8000_4005_u32 as i32, false, &plain));
        assert!(!dialog_wanted(0x8000_4005_u32 as i32, true, &plain));
        assert!(!dialog_wanted(
            1,
            false,
            &args(&["roqa", "--diagnostics", "x"])
        ));
    }

    #[test]
    fn log_is_opened_only_for_host_launches_and_records_the_code() {
        assert!(wants_log(&args(&["roqa"])));
        assert!(!wants_log(&args(&["roqa", "show"])));
        assert!(!wants_log(&args(&["roqa", "--diagnostics", "x"])));
        assert_eq!(
            code_line(ALREADY_RUNNING),
            "ROQA startup failure 0x800700B7"
        );
    }
}
