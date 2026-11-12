//! Switching the default render endpoint: the one UNDOCUMENTED call in ROQA.
//!
//! Windows has no public API to change the default audio device (the Sound settings page and the volume
//! flyout do it through the private `IPolicyConfig` COM interface). The user explicitly asked for the
//! volume slider's icon to switch devices (decision recorded in `docs/development/features-edge-sliders.md`),
//! so this is the single place that touches it. The interface can change between Windows builds: any
//! failure here is returned and the caller opens `ms-settings:sound` instead.
#![allow(non_snake_case)] // The COM method names are the interface's own.
use windows::{
    Win32::{
        Media::Audio::{ERole, eCommunications, eConsole, eMultimedia},
        System::Com::{CLSCTX_ALL, CoCreateInstance},
    },
    core::{GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, PCWSTR, Result, interface},
};

/// `CPolicyConfigClient`.
const CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

// Vtable order of the Windows 7+ interface. Only `SetDefaultEndpoint` is called; the others are slot
// placeholders so it lands on the right one.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    unsafe fn GetMixFormat(&self) -> HRESULT;
    unsafe fn GetDeviceFormat(&self) -> HRESULT;
    unsafe fn ResetDeviceFormat(&self) -> HRESULT;
    unsafe fn SetDeviceFormat(&self) -> HRESULT;
    unsafe fn GetProcessingPeriod(&self) -> HRESULT;
    unsafe fn SetProcessingPeriod(&self) -> HRESULT;
    unsafe fn GetShareMode(&self) -> HRESULT;
    unsafe fn SetShareMode(&self) -> HRESULT;
    unsafe fn GetPropertyValue(&self) -> HRESULT;
    unsafe fn SetPropertyValue(&self) -> HRESULT;
    unsafe fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
}

/// Makes endpoint `id` the default render device for the console, multimedia and communications roles.
/// Host (STA) thread.
pub fn set_default(id: &str) -> Result<()> {
    // SAFETY: a COM call on the host STA thread; `id` is a NUL-terminated copy that outlives the calls.
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&CLIENT, None, CLSCTX_ALL)?;
        let id = HSTRING::from(id);
        for role in [eConsole, eMultimedia, eCommunications] {
            policy.SetDefaultEndpoint(PCWSTR(id.as_ptr()), role).ok()?;
        }
    }
    Ok(())
}
