use std::{mem::size_of, ptr};
use windows::Win32::{
    Foundation::{
        CloseHandle, E_ACCESSDENIED, E_INVALIDARG, ERROR_ALREADY_EXISTS, ERROR_INSUFFICIENT_BUFFER,
        GetLastError, HANDLE, HLOCAL, LocalFree,
    },
    Security::{
        Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    },
    System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, GetCurrentProcess, OpenEventW,
        OpenProcessToken, SetEvent,
    },
    UI::WindowsAndMessaging::{AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId},
};
use windows::core::{Error, HRESULT, PCWSTR, PWSTR, Result};

pub struct OwnedHandle(pub HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: every OwnedHandle contains one valid owned kernel handle.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Clone, Copy)]
pub enum Command {
    Quit,
    ShowNotch,
    HideNotch,
    ShowDock,
    HideDock,
    ShowPicker,
    ShowSettings,
}
impl Command {
    pub const ALL: [Self; 7] = [
        Self::Quit,
        Self::ShowNotch,
        Self::HideNotch,
        Self::ShowDock,
        Self::HideDock,
        Self::ShowPicker,
        Self::ShowSettings,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::ShowNotch => "show-notch",
            Self::HideNotch => "hide-notch",
            Self::ShowDock => "show-dock",
            Self::HideDock => "hide-dock",
            Self::ShowPicker => "show-picker",
            Self::ShowSettings => "show-settings",
        }
    }
    pub fn parse(argument: &str) -> Result<Self> {
        Self::ALL.into_iter().find(|command| argument == format!("--{}", command.name()))
            .ok_or_else(|| Error::new(E_INVALIDARG, "Unknown command. Use --quit, --show-notch, --hide-notch, --show-dock, --hide-dock, --show-picker or --show-settings."))
    }
    pub fn signal(self) -> Result<()> {
        let name = object_name(self.name())?;
        // SAFETY: the name is NUL-terminated; OpenEvent requests only signaling rights.
        unsafe {
            let event = OwnedHandle(OpenEventW(
                EVENT_MODIFY_STATE,
                false,
                PCWSTR(name.as_ptr()),
            )?);
            if matches!(self, Self::ShowPicker | Self::ShowSettings) {
                grant_foreground_to_resident();
            }
            SetEvent(event.0)
        }
    }
}

/// An explicit picker or settings request comes from whatever app the user is in, so this client usually holds the
/// foreground right; pass it to the resident ROQA process only. Without the right Windows refuses,
/// and the picker reports the refusal instead of forcing focus.
fn grant_foreground_to_resident() {
    // SAFETY: scalar lookups of this session's ROQA surface; no messages or foreign memory.
    unsafe {
        let Ok(surface) = FindWindowW(crate::native_window::CLASS, None) else {
            return;
        };
        let mut process = 0;
        GetWindowThreadProcessId(surface, Some(&mut process));
        if process != 0 {
            let _ = AllowSetForegroundWindow(process);
        }
    }
}

pub struct Controls {
    pub events: Vec<OwnedHandle>,
    _instance: OwnedHandle,
}
impl Controls {
    pub fn create() -> Result<Self> {
        let name = object_name("instance")?;
        // SAFETY: default token DACL restricts access; Local namespace scopes the session.
        let created = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) };
        // An instance at another integrity level denies access to its mutex: still "already running".
        if created.as_ref().is_err_and(|e| e.code() == E_ACCESSDENIED) {
            return Err(Error::new(
                HRESULT(crate::error_log::ALREADY_RUNNING),
                "ROQA is already running in this user session.",
            ));
        }
        let instance = OwnedHandle(created?);
        // SAFETY: checked immediately after CreateMutexW, before any unrelated Win32 call.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(Error::new(
                HRESULT(crate::error_log::ALREADY_RUNNING),
                "ROQA is already running in this user session.",
            ));
        }
        let events = Command::ALL
            .into_iter()
            .map(create_event)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            events,
            _instance: instance,
        })
    }
    pub fn handles(&self) -> Vec<HANDLE> {
        self.events.iter().map(|event| event.0).collect()
    }
}

fn create_event(command: Command) -> Result<OwnedHandle> {
    let name = object_name(command.name())?;
    // SAFETY: valid terminated name; auto-reset event, no inheritance, default user DACL.
    unsafe {
        Ok(OwnedHandle(CreateEventW(
            None,
            false,
            false,
            PCWSTR(name.as_ptr()),
        )?))
    }
}

fn object_name(action: &str) -> Result<Vec<u16>> {
    Ok(format!("Local\\ROQA.{}.{}", user_sid()?, action)
        .encode_utf16()
        .chain(Some(0))
        .collect())
}

fn user_sid() -> Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: token output and current-process pseudo-handle are valid.
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
    }
    let token = OwnedHandle(token);
    let bytes = token_user(&token)?;
    // SAFETY: token_user returns aligned storage containing a complete TOKEN_USER.
    unsafe { sid_text((bytes.as_ptr() as *const TOKEN_USER).read().User.Sid) }
}

fn token_user(token: &OwnedHandle) -> Result<Vec<usize>> {
    let mut required = 0;
    // SAFETY: first call queries size; second writes only within aligned owned storage.
    unsafe {
        let query = GetTokenInformation(token.0, TokenUser, None, 0, &mut required);
        if query.as_ref().err().map(Error::code) != Some(ERROR_INSUFFICIENT_BUFFER.to_hresult()) {
            return Err(query.err().unwrap_or_else(Error::from_thread));
        }
        let mut bytes = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(bytes.as_mut_ptr().cast()),
            required,
            &mut required,
        )?;
        Ok(bytes)
    }
}

unsafe fn sid_text(sid: windows::Win32::Security::PSID) -> Result<String> {
    let mut text = PWSTR(ptr::null_mut());
    // SAFETY: SID belongs to the still-live token buffer; conversion allocates with LocalAlloc.
    unsafe {
        ConvertSidToStringSidW(sid, &mut text)?;
    }
    // SAFETY: the API returned a terminated UTF-16 string that stays live until LocalFree.
    let converted =
        unsafe { text.to_string() }.map_err(|_| Error::new(E_INVALIDARG, "Invalid SID text."));
    // SAFETY: matches the allocation contract of ConvertSidToStringSidW.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    converted
}
