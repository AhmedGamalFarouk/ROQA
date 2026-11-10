//! UI Automation for the notch: a ToolBar root with one Button (Invoke) per media control (previous,
//! play/pause, next) while a media session exists. Like `dock_uia`, the providers read a small shared
//! snapshot and post actions to the host loop, which validates them again before acting.
#![allow(non_upper_case_globals)] // Matching on Windows SDK constant names.
use crate::{
    dock_uia::{
        self, Accessibility, none, not_available, variant_bool, variant_i32, variant_text,
        window_rect,
    },
    layout,
    media::{NEXT, PLAY_PAUSE, PREVIOUS, Track},
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
    core::{Error, HRESULT, IUnknown, IUnknownImpl, Interface, Result, implement},
};

/// Posted to the notch HWND when a UIA client sets focus: wParam = control, or `FIRST` for the
/// first enabled one; lParam = the version the element belonged to.
pub const FOCUS: u32 = WM_APP + 5;
pub const FIRST: usize = usize::MAX;

#[derive(Default)]
pub struct State {
    pub hwnd: isize,
    pub dpi: u32,
    /// The media session generation the controls belong to; older elements are unavailable.
    pub version: u32,
    /// Name and enabled state of each control; empty without a media session.
    pub controls: Vec<(String, bool)>,
    pub focus: Option<usize>,
    /// The panel is open or opening, so the controls have screen bounds.
    pub open: bool,
}
pub type Shared = Arc<Mutex<State>>;

/// Clients can retain child providers after the owning window has been destroyed.
pub fn invalidate(state: &Shared) {
    if let Ok(mut state) = state.lock() {
        state.hwnd = 0;
        state.controls.clear();
        state.focus = None;
        state.open = false;
    }
}

/// Accessible names and enabled states, in drawing order. Play/pause names its action and the
/// track, such as "Pause, Song – Artist".
pub fn controls(track: &Track) -> Vec<(String, bool)> {
    let action = if track.playing { "Pause" } else { "Play" };
    let playing = [track.title.as_str(), track.artist.as_str()]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" – ");
    let play_pause = if playing.is_empty() {
        action.to_owned()
    } else {
        format!("{action}, {playing}")
    };
    vec![
        ("Previous track".to_owned(), track.controls[PREVIOUS]),
        (play_pause, track.controls[PLAY_PAUSE]),
        ("Next track".to_owned(), track.controls[NEXT]),
    ]
}

/// Serves the notch's UIA root for one HWND lifetime; drop before destroying the window.
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

/// Tells listening UIA clients that keyboard focus moved to `control`. Call without holding the
/// state lock: clients may read the element's properties before this returns.
pub fn focus_changed(accessibility: &Accessibility, state: &Shared, control: usize) {
    let root = accessibility
        .root()
        .cast::<IRawElementProviderFragmentRoot>();
    let (Ok(root), Ok(version)) = (root, read(state, |s| s.version)) else {
        return;
    };
    // SAFETY: a provider of this live notch, raised on the UI thread that owns it.
    unsafe {
        if UiaClientsAreListening().as_bool() {
            let item: IRawElementProviderSimple = Item {
                state: Arc::clone(state),
                root,
                index: control,
                version,
            }
            .into();
            let _ = UiaRaiseAutomationEvent(&item, UIA_AutomationFocusChangedEventId);
        }
    }
}

fn read<T>(state: &Shared, view: impl FnOnce(&State) -> T) -> Result<T> {
    state
        .lock()
        .map_err(|_| Error::new(E_FAIL, "Notch accessibility state unavailable."))
        .and_then(|state| {
            if state.hwnd == 0 {
                Err(not_available())
            } else {
                Ok(view(&state))
            }
        })
}

fn not_enabled() -> Error {
    Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTENABLED as i32))
}

fn post(hwnd: isize, message: u32, control: usize, version: u32) -> Result<()> {
    // SAFETY: scalar post to our own notch HWND; the host loop validates the control and version.
    unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            message,
            WPARAM(control),
            LPARAM(version as isize),
        )
    }
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
        let (count, version) = read(&self.state, |state| (state.controls.len(), state.version))?;
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
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_ProviderOwnsSetFocus)
    }
    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<IUnknown> {
        none()
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        read(&self.state, |_| ())?;
        Ok(match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_ToolBarControlTypeId.0),
            UIA_NamePropertyId => variant_text("ROQA Notch"),
            UIA_AutomationIdPropertyId => variant_text("RoqaNotch"),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        let hwnd = read(&self.state, |state| state.hwnd)?;
        // SAFETY: our own live notch HWND supplies the default window properties and tree position.
        unsafe { UiaHostProviderFromHwnd(HWND(hwnd as *mut _)) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let count = read(&self.state, |state| state.controls.len())?;
        match direction {
            NavigateDirection_FirstChild => self.item(0),
            NavigateDirection_LastChild if count > 0 => self.item(count - 1),
            // Parent and siblings of a hosted root come from the HWND host provider.
            _ => none(),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut()) // UIA derives a hosted root's runtime ID from its window.
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default()) // The HWND host provider supplies the window bounds.
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        let (hwnd, version, enabled) = read(&self.state, |state| {
            (
                state.hwnd,
                state.version,
                state.controls.iter().any(|(_, enabled)| *enabled),
            )
        })?;
        if !enabled {
            return Err(not_enabled());
        }
        post(hwnd, FOCUS, FIRST, version)
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let (hwnd, dpi, open) = read(&self.state, |s| (s.hwnd, s.dpi, s.open))?;
        let rect = window_rect(hwnd)?;
        let (x, y) = (x as i32 - rect.left, y as i32 - rect.top);
        // Bounds are those of the open panel, as `Item::BoundingRectangle` reports them.
        match layout::media_control_at(rect.right - rect.left, dpi, 1.0, x, y) {
            Some(control) if open => self.item(control),
            _ => Ok(self.to_interface()),
        }
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        match read(&self.state, |state| state.focus)? {
            Some(control) => self.item(control),
            None => none(),
        }
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
    /// Reads this control and the notch state, or fails with `UIA_E_ELEMENTNOTAVAILABLE` once its
    /// session has ended.
    fn current<T>(&self, view: impl FnOnce(&State, &(String, bool)) -> T) -> Result<T> {
        read(&self.state, |state| {
            state
                .controls
                .get(self.index)
                .filter(|_| state.version == self.version)
                .map(|control| view(state, control))
        })?
        .ok_or_else(not_available)
    }
    fn sibling(&self, index: usize) -> Result<IRawElementProviderFragment> {
        if index >= self.current(|state, _| state.controls.len())? {
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
    /// Posts `message` for this control when the player enables it.
    fn act(&self, message: u32) -> Result<()> {
        let (hwnd, enabled) = self.current(|state, (_, enabled)| (state.hwnd, *enabled))?;
        if !enabled {
            return Err(not_enabled());
        }
        post(hwnd, message, self.index, self.version)
    }
}

impl IRawElementProviderSimple_Impl for Item_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        self.current(|_, _| ())?;
        if pattern == UIA_InvokePatternId {
            Ok(self.to_interface::<IInvokeProvider>().into())
        } else {
            none()
        }
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let (name, enabled, focused, open) = self.current(|state, (name, enabled)| {
            let focused = state.focus == Some(self.index);
            (name.clone(), *enabled, focused, state.open)
        })?;
        let id = ["RoqaMediaPrevious", "RoqaMediaPlayPause", "RoqaMediaNext"];
        Ok(match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_ButtonControlTypeId.0),
            UIA_NamePropertyId => variant_text(&name),
            UIA_AutomationIdPropertyId => variant_text(id.get(self.index).unwrap_or(&"")),
            // Disabled controls stay in the tree, exposed as disabled.
            UIA_IsEnabledPropertyId | UIA_IsKeyboardFocusablePropertyId => variant_bool(enabled),
            UIA_HasKeyboardFocusPropertyId => variant_bool(focused),
            UIA_IsOffscreenPropertyId => variant_bool(!open),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        none() // Not hosted in its own window.
    }
}

impl IRawElementProviderFragment_Impl for Item_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        self.current(|_, _| ())?;
        match direction {
            NavigateDirection_Parent => self.root.cast(),
            NavigateDirection_NextSibling => self.sibling(self.index + 1),
            NavigateDirection_PreviousSibling if self.index > 0 => self.sibling(self.index - 1),
            _ => none(),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        self.current(|_, _| ())?;
        dock_uia::runtime_id(self.version, self.index)
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let (hwnd, dpi, open) = self.current(|state, _| (state.hwnd, state.dpi, state.open))?;
        // A collapsed panel draws no controls: they are offscreen, and Invoke still works.
        let Some(control) = layout::media_controls(dpi).get(self.index).copied() else {
            return Err(not_available());
        };
        if !open {
            return Ok(UiaRect::default());
        }
        let rect = window_rect(hwnd)?;
        let shape = layout::notch_shape(1.0, true);
        let body = layout::notch_body(rect.right - rect.left, dpi, shape);
        Ok(UiaRect {
            left: f64::from(rect.left + body.left + control.x),
            top: f64::from(rect.top + control.y),
            width: f64::from(control.width),
            height: f64::from(control.height),
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        self.act(FOCUS)
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        self.current(|_, _| ())?;
        Ok(self.root.clone())
    }
}

impl IInvokeProvider_Impl for Item_Impl {
    fn Invoke(&self) -> Result<()> {
        self.act(dock_uia::INVOKE)?;
        // SAFETY: raised on the UI thread for this live element.
        unsafe {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_owns_activation_and_destroyed_windows_invalidate_retained_children() {
        let state = Shared::default();
        {
            let mut state = state.lock().unwrap();
            state.hwnd = 1; // A sentinel only: no method below calls a window API.
            state.version = 7;
            state.controls = controls(&Track {
                controls: [true; 3],
                ..Track::default()
            });
        }
        let root: IRawElementProviderSimple = Root {
            state: Arc::clone(&state),
        }
        .into();
        let item: IRawElementProviderSimple = Item {
            state: Arc::clone(&state),
            root: root.cast().unwrap(),
            index: PLAY_PAUSE,
            version: 7,
        }
        .into();
        // SAFETY: local provider calls that only read the snapshot; no HWND or foreground access.
        unsafe {
            let options = root.ProviderOptions().unwrap();
            assert_eq!(
                options & ProviderOptions_ProviderOwnsSetFocus,
                ProviderOptions_ProviderOwnsSetFocus
            );
            assert!(item.GetPropertyValue(UIA_NamePropertyId).is_ok());
            invalidate(&state);
            assert_eq!(
                item.GetPropertyValue(UIA_NamePropertyId)
                    .unwrap_err()
                    .code(),
                not_available().code()
            );
            assert_eq!(
                item.cast::<IInvokeProvider>()
                    .unwrap()
                    .Invoke()
                    .unwrap_err()
                    .code(),
                not_available().code()
            );
            assert_eq!(
                root.cast::<IRawElementProviderFragment>()
                    .unwrap()
                    .SetFocus()
                    .unwrap_err()
                    .code(),
                not_available().code()
            );
        }
    }

    #[test]
    fn names_say_the_action_and_the_track_and_follow_the_enabled_controls() {
        let mut track = Track {
            title: "Song".into(),
            artist: "Artist".into(),
            playing: true,
            controls: [false, true, true],
            ..Track::default()
        };
        assert_eq!(
            controls(&track),
            [
                ("Previous track".to_owned(), false),
                ("Pause, Song – Artist".to_owned(), true),
                ("Next track".to_owned(), true),
            ]
        );
        track.playing = false;
        track.artist.clear();
        assert_eq!(controls(&track)[PLAY_PAUSE].0, "Play, Song");
        // Properties not read yet: the action alone, still exposed, and disabled if so reported.
        let pending = Track::default();
        assert_eq!(
            controls(&pending),
            [
                ("Previous track".to_owned(), false),
                ("Play".to_owned(), false),
                ("Next track".to_owned(), false),
            ]
        );
    }
}
