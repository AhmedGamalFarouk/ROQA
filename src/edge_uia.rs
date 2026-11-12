//! UI Automation for an edge slider: one Slider element on the slider's HWND with RangeValue (0 to 100),
//! named such as "Volume 40%, Speakers" or "Brightness 70%". Like `dock_uia`, it reads a small shared
//! snapshot and posts changes to the host loop, which applies them; it never touches host state. The
//! element exists whether or not the pill is drawn, so assistive technology can adjust a level without
//! hovering.
#![allow(non_upper_case_globals)] // Matching on Windows SDK constant names.
use crate::dock_uia::{
    Accessibility, none, not_available, variant_bool, variant_i32, variant_text, window_rect,
};
use std::sync::{Arc, Mutex};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND, LPARAM, WPARAM},
        System::{Com::SAFEARRAY, Variant::VARIANT},
        UI::{
            Accessibility::*,
            WindowsAndMessaging::{PostMessageW, WM_APP},
        },
    },
    core::{BOOL, Error, IUnknown, IUnknownImpl, Result, implement},
};

/// Posted to the slider HWND by a UIA client's `SetValue`: wParam = the percent, 0 to 100.
pub const SET: u32 = WM_APP + 8;
/// Posted to the slider HWND the first time a UIA client asks for its element (`WM_GETOBJECT`).
pub const QUERIED: u32 = WM_APP + 13;

#[derive(Default)]
pub struct State {
    pub hwnd: isize,
    pub id: &'static str,
    pub name: String,
    pub percent: u8,
    pub enabled: bool,
}
pub type Shared = Arc<Mutex<State>>;

/// Clients can retain the provider after its window is destroyed.
pub fn invalidate(state: &Shared) {
    if let Ok(mut state) = state.lock() {
        state.hwnd = 0;
    }
}

pub fn accessibility(hwnd: HWND, state: &Shared) -> Result<Accessibility> {
    if let Ok(mut state) = state.lock() {
        state.hwnd = hwnd.0 as isize;
    }
    Accessibility::register_noticed(
        hwnd,
        Root {
            state: Arc::clone(state),
        }
        .into(),
        QUERIED,
    )
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    IRangeValueProvider
)]
struct Root {
    state: Shared,
}
impl Root_Impl {
    fn read<T>(&self, view: impl FnOnce(&State) -> T) -> Result<T> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::new(E_FAIL, "Slider accessibility state unavailable."))?;
        if state.hwnd == 0 {
            return Err(not_available());
        }
        Ok(view(&state))
    }
}

impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        if pattern == UIA_RangeValuePatternId {
            Ok(self.to_interface::<IRangeValueProvider>().into())
        } else {
            none()
        }
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        self.read(|state| match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_SliderControlTypeId.0),
            UIA_NamePropertyId => variant_text(&state.name),
            UIA_AutomationIdPropertyId => variant_text(state.id),
            UIA_IsEnabledPropertyId => variant_bool(state.enabled),
            UIA_IsKeyboardFocusablePropertyId => variant_bool(false),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        let hwnd = self.read(|state| state.hwnd)?;
        // SAFETY: our own live slider HWND supplies the default window properties and tree position.
        unsafe { UiaHostProviderFromHwnd(HWND(hwnd as *mut _)) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, _: NavigateDirection) -> Result<IRawElementProviderFragment> {
        none() // A leaf; the HWND host provider supplies parent and siblings.
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut()) // A hosted root: UIA derives it from the window.
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let hwnd = self.read(|state| state.hwnd)?;
        let rect = window_rect(hwnd)?;
        Ok(UiaRect {
            left: f64::from(rect.left),
            top: f64::from(rect.top),
            width: f64::from(rect.right - rect.left),
            height: f64::from(rect.bottom - rect.top),
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(()) // Passive, nonactivating window: focus is never taken.
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, _: f64, _: f64) -> Result<IRawElementProviderFragment> {
        Ok(self.to_interface())
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        none()
    }
}

impl IRangeValueProvider_Impl for Root_Impl {
    fn SetValue(&self, value: f64) -> Result<()> {
        let (hwnd, enabled) = self.read(|state| (state.hwnd, state.enabled))?;
        if !enabled {
            return Err(Error::from_hresult(windows::core::HRESULT(
                UIA_E_ELEMENTNOTENABLED as i32,
            )));
        }
        if !(0.0..=100.0).contains(&value) {
            return Err(Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        // SAFETY: scalar post to our own slider HWND; the host loop applies it.
        unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                SET,
                WPARAM(value.round() as usize),
                LPARAM(0),
            )
        }
    }
    fn Value(&self) -> Result<f64> {
        self.read(|state| f64::from(state.percent))
    }
    fn IsReadOnly(&self) -> Result<BOOL> {
        self.read(|state| BOOL::from(!state.enabled))
    }
    fn Maximum(&self) -> Result<f64> {
        Ok(100.0)
    }
    fn Minimum(&self) -> Result<f64> {
        Ok(0.0)
    }
    fn LargeChange(&self) -> Result<f64> {
        Ok(10.0)
    }
    fn SmallChange(&self) -> Result<f64> {
        Ok(5.0)
    }
}
