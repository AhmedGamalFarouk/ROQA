//! UI Automation for the dock context menu: a Menu root on the popup's HWND with one MenuItem (Invoke)
//! per action. Like `dock_uia`, it reads a small shared snapshot (fixed for the menu's lifetime, since the
//! host closes the menu when its actions change) and posts an invoke to the host loop, which applies it.
#![allow(non_upper_case_globals)] // Matching on Windows SDK constant names.
use crate::{
    dock_uia::{
        Accessibility, none, not_available, runtime_id, variant_bool, variant_i32, variant_text,
        window_rect,
    },
    layout::Bounds,
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
    core::{Error, IUnknown, IUnknownImpl, Interface, Result, implement},
};

/// Posted to the menu HWND by a UIA client's Invoke: wParam = the item index.
pub const INVOKE: u32 = WM_APP + 9;

#[derive(Default)]
pub struct State {
    pub hwnd: isize,
    pub name: String,
    /// Each item's label and its row in window pixels.
    pub rows: Vec<(String, Bounds)>,
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
    Accessibility::register(
        hwnd,
        Root {
            state: Arc::clone(state),
        }
        .into(),
    )
}

/// A view of the snapshot, or `UIA_E_ELEMENTNOTAVAILABLE` once the menu is gone.
fn read<T>(state: &Shared, view: impl FnOnce(&State) -> T) -> Result<T> {
    let state = state
        .lock()
        .map_err(|_| Error::new(E_FAIL, "Menu accessibility state unavailable."))?;
    if state.hwnd == 0 {
        return Err(not_available());
    }
    Ok(view(&state))
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
struct Root {
    state: Shared,
}
impl Root_Impl {
    fn item(&self, index: usize) -> Result<IRawElementProviderFragment> {
        if index >= read(&self.state, |state| state.rows.len())? {
            return none();
        }
        Ok(Item {
            state: Arc::clone(&self.state),
            root: self.to_interface(),
            index,
        }
        .into())
    }
}
impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<IUnknown> {
        none()
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        read(&self.state, |state| match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_MenuControlTypeId.0),
            UIA_NamePropertyId => variant_text(&state.name),
            UIA_AutomationIdPropertyId => variant_text("RoqaDockMenu"),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        let hwnd = read(&self.state, |state| state.hwnd)?;
        // SAFETY: our own live menu HWND supplies the default window properties and tree position.
        unsafe { UiaHostProviderFromHwnd(HWND(hwnd as *mut _)) }
    }
}
impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let count = read(&self.state, |state| state.rows.len())?;
        match direction {
            NavigateDirection_FirstChild => self.item(0),
            NavigateDirection_LastChild if count > 0 => self.item(count - 1),
            _ => none(), // The HWND host provider supplies parent and siblings.
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut()) // A hosted root: UIA derives it from the window.
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default()) // The HWND host provider supplies the window bounds.
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(()) // The menu already holds the foreground while it is open.
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}
impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let hwnd = read(&self.state, |state| state.hwnd)?;
        let rect = window_rect(hwnd)?;
        let (x, y) = (x as i32 - rect.left, y as i32 - rect.top);
        let hit = read(&self.state, |state| {
            state.rows.iter().position(|(_, row)| {
                (row.x..row.x + row.width).contains(&x) && (row.y..row.y + row.height).contains(&y)
            })
        })?;
        match hit {
            Some(index) => self.item(index),
            None => Ok(self.to_interface()),
        }
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        none()
    }
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IInvokeProvider
)]
struct Item {
    state: Shared,
    root: IRawElementProviderFragmentRoot,
    index: usize,
}
impl Item_Impl {
    /// The item's label, its row and the menu HWND.
    fn current(&self) -> Result<(String, Bounds, isize)> {
        read(&self.state, |state| {
            state
                .rows
                .get(self.index)
                .map(|(name, row)| (name.clone(), *row, state.hwnd))
        })?
        .ok_or_else(not_available)
    }
    fn sibling(&self, index: usize) -> Result<IRawElementProviderFragment> {
        if index >= read(&self.state, |state| state.rows.len())? {
            return none();
        }
        Ok(Item {
            state: Arc::clone(&self.state),
            root: self.root.clone(),
            index,
        }
        .into())
    }
}
impl IRawElementProviderSimple_Impl for Item_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        if pattern == UIA_InvokePatternId {
            Ok(self.to_interface::<IInvokeProvider>().into())
        } else {
            none()
        }
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let (name, ..) = self.current()?;
        Ok(match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_MenuItemControlTypeId.0),
            UIA_NamePropertyId => variant_text(&name),
            UIA_AutomationIdPropertyId => variant_text(&format!("RoqaDockMenuItem{}", self.index)),
            UIA_IsEnabledPropertyId | UIA_IsKeyboardFocusablePropertyId => variant_bool(true),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        none() // Not hosted in its own window.
    }
}
impl IRawElementProviderFragment_Impl for Item_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        match direction {
            NavigateDirection_Parent => self.root.cast(),
            NavigateDirection_NextSibling => self.sibling(self.index + 1),
            NavigateDirection_PreviousSibling if self.index > 0 => self.sibling(self.index - 1),
            _ => none(),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        self.current()?;
        runtime_id(0, self.index)
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let (_, row, hwnd) = self.current()?;
        let rect = window_rect(hwnd)?;
        Ok(UiaRect {
            left: f64::from(rect.left + row.x),
            top: f64::from(rect.top + row.y),
            width: f64::from(row.width),
            height: f64::from(row.height),
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.root.clone())
    }
}
impl IInvokeProvider_Impl for Item_Impl {
    fn Invoke(&self) -> Result<()> {
        let (.., hwnd) = self.current()?;
        // SAFETY: scalar post to our own menu HWND; the host loop applies it, or ignores a stale one.
        unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                INVOKE,
                WPARAM(self.index),
                LPARAM(0),
            )
        }
    }
}
