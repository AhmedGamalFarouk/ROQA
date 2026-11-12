//! UI Automation for the custom-drawn dock: a ToolBar root with one Button (Invoke) per slot, plus
//! the per-HWND root registration and value helpers the notch's provider (`notch_uia`) shares.
//! UIA calls these providers re-entrantly on the UI thread while it pumps messages, so they read only a
//! small shared snapshot and post actions back to the host loop; they never touch host state directly.
#![allow(non_upper_case_globals)] // Matching on Windows SDK constant names.
use crate::dock;
use std::{
    cell::{Cell, RefCell},
    mem::ManuallyDrop,
    sync::{Arc, Mutex},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_OUTOFMEMORY, HWND, LPARAM, LRESULT, RECT, VARIANT_TRUE, WPARAM},
        System::{
            Com::SAFEARRAY,
            Ole::{SafeArrayCreateVector, SafeArrayPutElement},
            Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_I4},
        },
        UI::{
            Accessibility::*,
            WindowsAndMessaging::{GetWindowRect, PostMessageW, WM_APP},
        },
    },
    core::{BSTR, Error, HRESULT, IUnknown, IUnknownImpl, Interface, Result, implement},
};

/// Posted to the dock or notch HWND: wParam = slot or control, lParam = layout version the element
/// belonged to.
pub const INVOKE: u32 = WM_APP + 4;

#[derive(Default)]
pub struct State {
    pub hwnd: isize,
    pub dpi: u32,
    /// Bumped whenever slots change, so elements from an older layout report themselves unavailable.
    pub version: u32,
    pub names: Vec<String>,
}
pub type Shared = Arc<Mutex<State>>;

fn read<T>(state: &Shared, view: impl FnOnce(&State) -> T) -> Result<T> {
    state
        .lock()
        .map(|state| view(&state))
        .map_err(|_| Error::new(E_FAIL, "Dock accessibility state unavailable."))
}

pub(crate) fn not_available() -> Error {
    Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

/// S_OK with a NULL result: UIA's convention for "no such pattern / no element in that direction".
pub(crate) fn none<T>() -> Result<T> {
    Err(Error::empty())
}

pub(crate) fn window_rect(hwnd: isize) -> Result<RECT> {
    let mut rect = RECT::default();
    // SAFETY: scalar query of our own dock HWND; fails cleanly if the window is gone.
    unsafe { GetWindowRect(HWND(hwnd as *mut _), &mut rect)? };
    Ok(rect)
}

/// A child element's runtime ID, unique under its HWND root: the layout version and its index.
pub(crate) fn runtime_id(version: u32, index: usize) -> Result<*mut SAFEARRAY> {
    let parts = [UiaAppendRuntimeId as i32, version as i32, index as i32];
    // SAFETY: a VT_I4 vector of exactly parts.len() elements; ownership passes to the UIA caller.
    unsafe {
        let array = SafeArrayCreateVector(VT_I4, 0, parts.len() as u32);
        if array.is_null() {
            return Err(Error::from_hresult(E_OUTOFMEMORY));
        }
        for (index, part) in parts.iter().enumerate() {
            SafeArrayPutElement(array, &(index as i32), (part as *const i32).cast())?;
        }
        Ok(array)
    }
}

pub(crate) fn variant_i32(value: i32) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: a fresh VT_EMPTY variant gets its tag and the matching union member.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = value;
    }
    variant
}

pub(crate) fn variant_bool(value: bool) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: as above; VARIANT_BOOL uses -1 for true.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = if value {
            VARIANT_TRUE
        } else {
            Default::default()
        };
    }
    variant
}

pub(crate) fn variant_text(text: &str) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: as above; the BSTR's ownership passes to the caller, which clears the variant.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_BSTR;
        inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(text));
    }
    variant
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
        let (count, version) = read(&self.state, |state| (state.names.len(), state.version))?;
        if index >= count {
            return none();
        }
        Ok(Item {
            state: Arc::clone(&self.state),
            root: self.to_interface(),
            index,
            version,
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
        Ok(match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_ToolBarControlTypeId.0),
            UIA_NamePropertyId => variant_text("ROQA Dock"),
            UIA_AutomationIdPropertyId => variant_text("RoqaDock"),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        let hwnd = read(&self.state, |state| state.hwnd)?;
        // SAFETY: our own live dock HWND supplies the default window properties and position in the tree.
        unsafe { UiaHostProviderFromHwnd(HWND(hwnd as *mut _)) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let count = read(&self.state, |state| state.names.len())?;
        match direction {
            NavigateDirection_FirstChild => self.item(0),
            NavigateDirection_LastChild if count > 0 => self.item(count - 1),
            // Parent and siblings of a hosted root come from the HWND host provider.
            _ => none(),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        // A root hosted in an HWND returns NULL; UIA derives its runtime ID from the window.
        Ok(std::ptr::null_mut())
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default()) // The HWND host provider supplies the window bounds.
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(()) // Passive, nonactivating surface: focus is never taken.
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let (hwnd, dpi, count) = read(&self.state, |s| (s.hwnd, s.dpi, s.names.len()))?;
        let rect = window_rect(hwnd)?;
        let inside = (rect.top as f64..rect.bottom as f64).contains(&y);
        match dock::slot_at(x as i32 - rect.left, rect.right - rect.left, dpi, count) {
            Some(slot) if inside => self.item(slot),
            _ => Ok(self.to_interface()),
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
    version: u32,
}

impl Item_Impl {
    /// The slot's name and dock geometry, or `UIA_E_ELEMENTNOTAVAILABLE` once the layout changed.
    fn current(&self) -> Result<(String, isize, u32, usize)> {
        read(&self.state, |state| {
            (state.version == self.version)
                .then(|| state.names.get(self.index).cloned())
                .flatten()
                .map(|name| (name, state.hwnd, state.dpi, state.names.len()))
        })?
        .ok_or_else(not_available)
    }
    fn sibling(&self, index: usize) -> Result<IRawElementProviderFragment> {
        let count = self.current()?.3;
        if index >= count {
            return none();
        }
        Ok(Item {
            state: Arc::clone(&self.state),
            root: self.root.clone(),
            index,
            version: self.version,
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
            UIA_ControlTypePropertyId => variant_i32(UIA_ButtonControlTypeId.0),
            UIA_NamePropertyId => variant_text(&name),
            UIA_AutomationIdPropertyId => variant_text(&format!("RoqaDockSlot{}", self.index)),
            UIA_IsEnabledPropertyId => variant_bool(true),
            UIA_IsKeyboardFocusablePropertyId => variant_bool(false),
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
        runtime_id(self.version, self.index)
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let (_, hwnd, dpi, count) = self.current()?;
        let rect = window_rect(hwnd)?;
        if self.index >= dock::visible_slots(rect.right - rect.left, dpi, count) {
            return Ok(UiaRect::default()); // Clipped slot: offscreen, still reachable through the picker.
        }
        let slot = dock::px(dock::SLOT_DIPS, dpi);
        Ok(UiaRect {
            left: f64::from(rect.left + dock::px(dock::PAD_DIPS, dpi) + slot * self.index as i32),
            top: f64::from(rect.top),
            width: f64::from(slot),
            height: f64::from(rect.bottom - rect.top),
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
        let (_, hwnd, ..) = self.current()?;
        // SAFETY: scalar post to our own dock HWND; the host loop validates the slot and version again.
        unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                INVOKE,
                WPARAM(self.index),
                LPARAM(self.version as isize),
            )?;
            if UiaClientsAreListening().as_bool() {
                let _ = UiaRaiseAutomationEvent(
                    &self.to_interface::<IRawElementProviderSimple>(),
                    UIA_Invoke_InvokedEventId,
                );
            }
        }
        Ok(())
    }
}

thread_local! {
    // WM_GETOBJECT arrives in the stateless window procedure; this maps the dock HWND to its root.
    // Each root also holds a message to post to its HWND on the first query (0 for none), then 0.
    static ROOTS: RefCell<Vec<(isize, IRawElementProviderSimple, Cell<u32>)>> = const { RefCell::new(Vec::new()) };
}

/// Answers UIA's WM_GETOBJECT for a registered surface; `None` lets DefWindowProc handle everything else.
pub fn get_object(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if lparam.0 as i32 != UiaRootObjectId {
        return None;
    }
    let (provider, notice) = ROOTS.with(|roots| {
        roots
            .try_borrow()
            .ok()?
            .iter()
            .find(|(window, ..)| *window == hwnd.0 as isize)
            .map(|(_, provider, notice)| (provider.clone(), notice.take()))
    })?;
    if notice != 0 {
        // SAFETY: scalar post to the queried live HWND; a lost post only delays the host's reaction.
        let _ = unsafe { PostMessageW(Some(hwnd), notice, WPARAM(0), LPARAM(0)) };
    }
    // SAFETY: the provider is registered only while its HWND is alive on this thread.
    Some(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider) })
}

/// Owns a surface's UIA root for one HWND lifetime; drop before destroying the window.
pub struct Accessibility {
    hwnd: HWND,
    root: Option<IRawElementProviderSimple>,
}
impl Accessibility {
    pub fn dock(hwnd: HWND, state: &Shared) -> Result<Self> {
        if let Ok(mut state) = state.lock() {
            state.hwnd = hwnd.0 as isize;
        }
        Self::register(
            hwnd,
            Root {
                state: Arc::clone(state),
            }
            .into(),
        )
    }
    /// Serves `root` for `hwnd`'s WM_GETOBJECT until dropped.
    pub fn register(hwnd: HWND, root: IRawElementProviderSimple) -> Result<Self> {
        Self::register_noticed(hwnd, root, 0)
    }
    /// Like `register`, and posts `notice` to `hwnd` the first time a client queries it, so the host learns
    /// that something is listening without polling `UiaClientsAreListening`.
    pub fn register_noticed(
        hwnd: HWND,
        root: IRawElementProviderSimple,
        notice: u32,
    ) -> Result<Self> {
        ROOTS.with(|roots| {
            roots
                .try_borrow_mut()
                .map(|mut roots| roots.push((hwnd.0 as isize, root.clone(), Cell::new(notice))))
                .map_err(|_| Error::new(E_FAIL, "Accessibility registry busy."))
        })?;
        Ok(Self {
            hwnd,
            root: Some(root),
        })
    }
    pub fn root(&self) -> &IRawElementProviderSimple {
        self.root.as_ref().expect("live accessibility root")
    }
    /// Retire registry access before COM disconnect, while the host identity is still valid.
    pub fn disconnect(&mut self) {
        if self.root.is_none() {
            return;
        }
        let hwnd = self.hwnd.0 as isize;
        ROOTS.with(|roots| {
            roots.borrow_mut().retain(|(window, ..)| *window != hwnd);
        });
        let root = self.root.take().expect("registered accessibility root");
        // SAFETY: documented teardown while the HWND is still alive: release UIA's references, then disconnect.
        unsafe {
            UiaReturnRawElementProvider(self.hwnd, WPARAM(0), LPARAM(0), None);
            if let Err(error) = UiaDisconnectProvider(&root) {
                eprintln!("Accessibility cleanup: {error}");
            }
        }
    }
}
impl Drop for Accessibility {
    fn drop(&mut self) {
        self.disconnect();
    }
}
