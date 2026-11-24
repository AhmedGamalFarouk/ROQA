#![allow(non_snake_case)]
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use windows::{
    Win32::{
        Foundation::E_FAIL,
        System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance},
    },
    core::{BOOL, GUID, HRESULT, IUnknown, IUnknown_Vtbl, Result, interface},
};

const MANAGER: GUID = GUID::from_u128(0x581333f6_28db_41be_bc7a_ff201f12f3f6);
static STATE: AtomicU8 = AtomicU8::new(0);
static FALLBACK: AtomicBool = AtomicBool::new(false);

#[interface("db3afbfb-08e6-46c6-aa70-bf9a34c30ab7")]
unsafe trait IRadioManager: IUnknown {
    unsafe fn IsRMSupported(&self, supported: *mut u32) -> HRESULT;
    unsafe fn GetUIRadioInstances(&self, instances: *mut *mut core::ffi::c_void) -> HRESULT;
    unsafe fn GetSystemRadioState(
        &self,
        radios_enabled: *mut BOOL,
        secondary: *mut i32,
        reason: *mut u32,
    ) -> HRESULT;
    unsafe fn SetSystemRadioState(&self, radios_enabled: BOOL) -> HRESULT;
}

pub(super) fn state() -> Option<bool> {
    match STATE.load(Ordering::Acquire) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

fn read(flip: bool) -> Result<bool> {
    // SAFETY: the caller initialized COM; all three ABI outputs point to writable scalars.
    unsafe {
        let manager: IRadioManager = CoCreateInstance(&MANAGER, None, CLSCTX_LOCAL_SERVER)?;
        let (mut enabled, mut secondary, mut reason) = (BOOL(0), 0, 0);
        manager
            .GetSystemRadioState(&mut enabled, &mut secondary, &mut reason)
            .ok()?;
        if flip {
            let target = !enabled.as_bool();
            manager.SetSystemRadioState(BOOL::from(target)).ok()?;
            manager
                .GetSystemRadioState(&mut enabled, &mut secondary, &mut reason)
                .ok()?;
            if enabled.as_bool() != target {
                return Err(E_FAIL.into());
            }
        }
        Ok(!enabled.as_bool())
    }
}

pub(super) fn refresh(flip: bool) {
    match read(flip) {
        Ok(on) => STATE.store(1 + u8::from(on), Ordering::Release),
        Err(error) => {
            STATE.store(0, Ordering::Release);
            if flip {
                eprintln!("Airplane mode: {error}");
                FALLBACK.store(true, Ordering::Release);
            }
        }
    }
}

pub(super) fn fallback() -> bool {
    FALLBACK.swap(false, Ordering::AcqRel)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "reads the private interface on the current Windows machine without changing radio state"]
    fn airplane_read_only_spike() {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
        // SAFETY: initializes COM on this test thread only.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
        }
        let state = super::read(false);
        // SAFETY: balances the successful initialization on this thread.
        unsafe {
            CoUninitialize();
        }
        eprintln!("Airplane mode reported by Windows: {state:?}");
        assert!(state.is_ok());
    }
}
