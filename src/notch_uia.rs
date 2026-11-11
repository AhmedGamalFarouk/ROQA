//! UI Automation for the notch: a ToolBar root with one Button (Invoke) per media control (previous,
//! play/pause, next, then the volume row's mute toggle) while a media session exists, then one Button (Invoke
//! opens the file) per visible shelf entry. Like
//! `dock_uia`, the providers read a small shared snapshot and post actions to the host loop, which
//! validates them again before acting.
#![allow(non_upper_case_globals)] // Matching on Windows SDK constant names.
use crate::{
    audio::Level,
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
/// Element ids of the shelf entries start here, after the media controls (including the mute toggle), so an
/// id means the same thing with or without a media session.
pub const SHELF_BASE: usize = 4;

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
    /// The drawn position, such as "1:23 of 4:56"; empty while no progress row is drawn.
    pub position: String,
    /// Why there is no media notch (`media::View::status`), when media access failed.
    pub status: Option<&'static str>,
    /// The running timers and a just-finished one, as `timer::View::name`; empty without any.
    pub timer: String,
    /// The visible shelf entries' names (`Shelf::label`); element id `SHELF_BASE + n` is entry n.
    pub shelf: Vec<String>,
}
pub type Shared = Arc<Mutex<State>>;

/// Clients can retain child providers after the owning window has been destroyed.
pub fn invalidate(state: &Shared) {
    if let Ok(mut state) = state.lock() {
        state.hwnd = 0;
        state.controls.clear();
        state.shelf.clear();
        state.timer.clear();
        state.focus = None;
        state.open = false;
    }
}

/// The root's name: the status line and the timers (`timer::View::name`, such as "Timer, 4:32
/// remaining"), when there are any, are part of it, as they are part of the panel.
fn root_name(status: Option<&str>, timer: &str) -> String {
    ["ROQA Notch", status.unwrap_or(""), timer]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The element ids that exist, in tree order: the media controls, then the shelf entries.
fn ids(state: &State) -> Vec<usize> {
    (0..state.controls.len())
        .chain(SHELF_BASE..SHELF_BASE + state.shelf.len())
        .collect()
}

/// The name and enabled state of element `id`. Shelf entries are always enabled.
fn entry(state: &State, id: usize) -> Option<(String, bool)> {
    match id.checked_sub(SHELF_BASE) {
        None => state.controls.get(id).cloned(),
        Some(n) => state.shelf.get(n).map(|name| (name.clone(), true)),
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

/// The mute toggle's name and enabled state: its action and the shown level, such as "Mute, volume
/// 40%". The level is read-only: there is no RangeValue. Without an endpoint it stays, disabled.
pub fn mute(level: Option<Level>) -> (String, bool) {
    match level {
        Some(Level { percent, muted }) => {
            let action = if muted { "Unmute" } else { "Mute" };
            (format!("{action}, volume {percent}%"), true)
        }
        None => ("Volume unavailable, no audio device".to_owned(), false),
    }
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
        let (found, version) = read(&self.state, |state| {
            (entry(state, index).is_some(), state.version)
        })?;
        if !found {
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
        let (status, timer) = read(&self.state, |state| (state.status, state.timer.clone()))?;
        Ok(match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_ToolBarControlTypeId.0),
            UIA_NamePropertyId => variant_text(&root_name(status, &timer)),
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
        let ids = read(&self.state, ids)?;
        match direction {
            NavigateDirection_FirstChild => ids.first().map_or_else(none, |&id| self.item(id)),
            NavigateDirection_LastChild => ids.last().map_or_else(none, |&id| self.item(id)),
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
        let (hwnd, dpi, open, media) = read(&self.state, |s| {
            (s.hwnd, s.dpi, s.open, !s.controls.is_empty())
        })?;
        let rect = window_rect(hwnd)?;
        let (x, y) = (x as i32 - rect.left, y as i32 - rect.top);
        let width = rect.right - rect.left;
        // Bounds are those of the open panel, as `Item::BoundingRectangle` reports them.
        let control = media
            .then(|| layout::media_control_at(width, dpi, 1.0, x, y))
            .flatten()
            .or_else(|| {
                layout::shelf_at(width, dpi, 1.0, media, x, y).map(|slot| SHELF_BASE + slot)
            });
        match control {
            Some(id) if open => self.item(id),
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
            entry(state, self.index)
                .filter(|_| state.version == self.version)
                .map(|control| view(state, &control))
        })?
        .ok_or_else(not_available)
    }
    /// The next (or previous) existing element after this one.
    fn sibling(&self, forward: bool) -> Result<IRawElementProviderFragment> {
        let mut ids = self.current(|state, _| ids(state))?.into_iter();
        let found = if forward {
            ids.find(|&id| id > self.index)
        } else {
            ids.rev().find(|&id| id < self.index)
        };
        let Some(index) = found else {
            return none();
        };
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
        let id = [
            "RoqaMediaPrevious",
            "RoqaMediaPlayPause",
            "RoqaMediaNext",
            "RoqaVolumeMute",
            "RoqaShelf0",
            "RoqaShelf1",
            "RoqaShelf2",
        ];
        // Shelf entries open files; keyboard focus stays on the media controls.
        let shelf = self.index >= SHELF_BASE;
        Ok(match property {
            UIA_ControlTypePropertyId => variant_i32(UIA_ButtonControlTypeId.0),
            UIA_NamePropertyId => variant_text(&name),
            UIA_AutomationIdPropertyId => variant_text(id.get(self.index).unwrap_or(&"")),
            // Disabled controls stay in the tree, exposed as disabled.
            UIA_IsEnabledPropertyId => variant_bool(enabled),
            UIA_IsKeyboardFocusablePropertyId => variant_bool(enabled && !shelf),
            UIA_HasKeyboardFocusPropertyId => variant_bool(focused),
            UIA_IsOffscreenPropertyId => variant_bool(!open),
            // Read-only: the position is shown and spoken, not adjusted with keys (user decision).
            UIA_ItemStatusPropertyId if self.index == PLAY_PAUSE => {
                let position = self.current(|state, _| state.position.clone())?;
                if position.is_empty() {
                    VARIANT::default()
                } else {
                    variant_text(&position)
                }
            }
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
            NavigateDirection_NextSibling => self.sibling(true),
            NavigateDirection_PreviousSibling => self.sibling(false),
            _ => none(),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        self.current(|_, _| ())?;
        dock_uia::runtime_id(self.version, self.index)
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let (hwnd, dpi, open, media) = self.current(|state, _| {
            (
                state.hwnd,
                state.dpi,
                state.open,
                !state.controls.is_empty(),
            )
        })?;
        // A collapsed panel draws no controls: they are offscreen, and Invoke still works.
        let control = match self.index.checked_sub(SHELF_BASE) {
            None => layout::media_controls(dpi).get(self.index).copied(),
            Some(slot) => Some(layout::shelf_slot(media, slot, dpi)),
        };
        let Some(control) = control else {
            return Err(not_available());
        };
        if !open {
            return Ok(UiaRect::default());
        }
        let rect = window_rect(hwnd)?;
        let shape = layout::notch_shape(1.0, media || self.index < SHELF_BASE);
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
        if self.index >= SHELF_BASE {
            return Err(not_enabled());
        }
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
    fn the_root_name_carries_the_status_line_only_when_there_is_one() {
        use crate::media::View;
        assert_eq!(root_name(View::NoPlayer.status(), ""), "ROQA Notch");
        for view in [View::Permission, View::Error] {
            let line = view.status().unwrap();
            assert_eq!(root_name(Some(line), ""), format!("ROQA Notch, {line}"));
        }
    }

    #[test]
    fn the_root_name_says_each_timer_and_its_state() {
        assert_eq!(
            root_name(None, "Timer, 4:32 remaining"),
            "ROQA Notch, Timer, 4:32 remaining"
        );
        assert_eq!(
            root_name(Some("No player"), "Timer finished"),
            "ROQA Notch, No player, Timer finished"
        );
    }

    #[test]
    fn clock_fallback_removes_children_and_rejects_retained_focus_and_invoke() {
        use crate::media::View;
        let recently_paused = View::Track(Track {
            title: "Song".into(),
            paused_until: Some(std::time::Instant::now() + std::time::Duration::from_secs(60)),
            controls: [true; 3],
            ..Track::default()
        });
        for hidden in [
            View::Track(Track {
                title: "Song".into(),
                controls: [true; 3],
                paused_until: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
                ..Track::default()
            }),
            View::Track(Track {
                title: "Song".into(),
                controls: [true; 3],
                ..Track::default()
            }),
            View::Track(Track {
                playing: true,
                controls: [true; 3],
                ..Track::default()
            }),
        ] {
            let state = Shared::default();
            {
                let mut state = state.lock().unwrap();
                state.hwnd = 1; // Sentinel: only error paths below; no HWND or foreground APIs.
                state.controls = recently_paused.shown().map(controls).unwrap_or_default();
            }
            let root: IRawElementProviderSimple = Root {
                state: Arc::clone(&state),
            }
            .into();
            let fragment: IRawElementProviderFragment = root.cast().unwrap();
            // SAFETY: local providers; Navigate/GetPropertyValue read only the shared snapshot.
            unsafe {
                let child = fragment.Navigate(NavigateDirection_FirstChild).unwrap();
                let simple: IRawElementProviderSimple = child.cast().unwrap();
                assert!(simple.GetPropertyValue(UIA_NamePropertyId).is_ok());
                state.lock().unwrap().controls = hidden.shown().map(controls).unwrap_or_default();
                assert!(fragment.Navigate(NavigateDirection_FirstChild).is_err());
                assert_eq!(
                    simple
                        .GetPropertyValue(UIA_IsKeyboardFocusablePropertyId)
                        .unwrap_err()
                        .code(),
                    not_available().code()
                );
                assert_eq!(child.SetFocus().unwrap_err().code(), not_available().code());
                assert_eq!(
                    simple
                        .cast::<IInvokeProvider>()
                        .unwrap()
                        .Invoke()
                        .unwrap_err()
                        .code(),
                    not_available().code()
                );
                assert_eq!(
                    fragment.SetFocus().unwrap_err().code(),
                    not_enabled().code()
                );
            }
        }
    }

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
    fn play_pause_reports_the_drawn_position_as_its_status() {
        let state = Shared::default();
        {
            let mut state = state.lock().unwrap();
            state.hwnd = 1; // A sentinel only: no method below calls a window API.
            state.controls = controls(&Track::default());
        }
        let root: IRawElementProviderFragmentRoot = Root {
            state: Arc::clone(&state),
        }
        .into();
        let item = |index| -> IRawElementProviderSimple {
            Item {
                state: Arc::clone(&state),
                root: root.clone(),
                index,
                version: 0,
            }
            .into()
        };
        let status = |index| {
            // SAFETY: a local provider that only reads the shared snapshot.
            let value = unsafe { item(index).GetPropertyValue(UIA_ItemStatusPropertyId) };
            value.unwrap().to_string()
        };
        assert_eq!(status(PLAY_PAUSE), "");
        state.lock().unwrap().position = "1:23 of 4:56".into();
        assert_eq!(status(PLAY_PAUSE), "1:23 of 4:56");
        assert_eq!(status(PREVIOUS), "");
        assert_eq!(status(NEXT), "");
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
        assert_eq!(
            mute(Some(Level {
                percent: 40,
                muted: false
            })),
            ("Mute, volume 40%".to_owned(), true)
        );
        assert_eq!(
            mute(Some(Level {
                percent: 0,
                muted: true
            })),
            ("Unmute, volume 0%".to_owned(), true)
        );
        assert_eq!(
            mute(None),
            ("Volume unavailable, no audio device".to_owned(), false)
        );
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
