//! Media state from the Windows global media controls (GSMTC). M4 slice 1: state, events and
//! diagnostics only; nothing is drawn yet.
//!
//! Every GSMTC read and mutation happens on the host (STA) thread. Event handlers and async
//! completions may run on any thread: they only set dirty bits, fill their request's one-shot slot
//! and post one scalar wake, so they never block, panic or touch `Desktop`, HWNDs or Direct2D.
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU32, Ordering},
};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as Properties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Win32::{
        Foundation::{E_ACCESSDENIED, LPARAM, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
    },
    core::{Error, HRESULT, HSTRING, IUnknown, Interface, Result, RuntimeType},
};
use windows_future::IAsyncOperation;

/// Thread message that only wakes the host loop; the pending work is in `Wake::dirty`.
const WAKE: u32 = WM_APP + 50;
const SESSION: u32 = 1;
const PROPERTIES: u32 = 2;
/// Playback info or timeline changed: both are re-read together.
const PLAYBACK: u32 = 4;
/// An async request completed and filled its slot.
const READY: u32 = 8;
/// Title and artist are cut to this many UTF-16 units, so one session holds at most a few KiB.
const TEXT_LIMIT: usize = 512;

/// WinRT 100 ns ticks; `updated` is a UTC `DateTime` (ticks since 1601).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeline {
    pub start: i64,
    pub end: i64,
    pub position: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub rate: f64,
    pub timeline: Timeline,
}
impl Track {
    /// Position at `now` (UTC ticks): `Position` is current as of `LastUpdatedTime`, so a playing
    /// track advances by the elapsed time times the rate. The result stays within the timeline; an
    /// `end` at or before `start` means the player gave no duration and only `start` bounds it.
    pub fn position(&self, now: i64) -> i64 {
        let timeline = self.timeline;
        let elapsed = if self.playing && timeline.updated > 0 {
            now.saturating_sub(timeline.updated).max(0)
        } else {
            0
        };
        let position = timeline
            .position
            .saturating_add((elapsed as f64 * self.rate) as i64);
        let position = if timeline.end > timeline.start {
            position.min(timeline.end)
        } else {
            position
        };
        position.max(timeline.start)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum View {
    #[default]
    NoPlayer,
    /// Windows refused media-session access, as for an app without the `globalMediaControl`
    /// capability.
    Permission,
    /// The session manager could not be obtained for another reason.
    Error,
    Track(Track),
}

/// One observation from the owner, applied in order on the host thread.
pub enum Change {
    /// Obtaining the session manager failed with this code.
    Unavailable(HRESULT),
    /// The current session was replaced, or is gone (`false`); starts a new generation.
    Session(bool),
    Playback {
        playing: bool,
        rate: f64,
        timeline: Timeline,
    },
    /// Title and artist read for session `generation`.
    Properties {
        generation: u64,
        title: String,
        artist: String,
    },
}

#[derive(Default)]
pub struct State {
    /// Bumped on every session replacement; reads tagged with an older one are discarded.
    pub generation: u64,
    pub view: View,
}
impl State {
    pub fn apply(&mut self, change: Change) {
        match change {
            Change::Unavailable(code) => {
                self.view = if code == E_ACCESSDENIED {
                    View::Permission
                } else {
                    View::Error
                };
            }
            Change::Session(present) => {
                self.generation += 1;
                self.view = if present {
                    View::Track(Track::default())
                } else {
                    View::NoPlayer
                };
            }
            Change::Playback {
                playing,
                rate,
                timeline,
            } => {
                if let View::Track(track) = &mut self.view {
                    (track.playing, track.rate, track.timeline) = (playing, rate, timeline);
                }
            }
            Change::Properties {
                generation,
                title,
                artist,
            } => {
                if generation == self.generation
                    && let View::Track(track) = &mut self.view
                {
                    (track.title, track.artist) = (title, artist);
                }
            }
        }
    }
}

/// Whether two views differ in more than the timeline, which players may update every second.
fn differs(before: &View, after: &View) -> bool {
    match (before, after) {
        (View::Track(a), View::Track(b)) => {
            (&a.title, &a.artist, a.playing, a.rate) != (&b.title, &b.artist, b.playing, b.rate)
        }
        _ => before != after,
    }
}

/// The only state callbacks touch: pending work bits, the host thread to wake, and the last
/// callback's thread (diagnostics for which threads GSMTC calls back on; an already-complete
/// `when` runs on the host, so only `callback_on_host=false` is conclusive).
struct Wake {
    dirty: AtomicU32,
    host: u32,
    caller: AtomicU32,
}
impl Wake {
    /// Callback side; never blocks or panics. Posts only when no wake is pending already. A lost
    /// post leaves the bits set, and the loop's next finite deadline (the clock's, at most a
    /// minute) handles them.
    fn raise(&self, bit: u32) {
        // SAFETY: reads the calling thread's ID; no pointers.
        self.caller
            .store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        // Release: a completion's slot write is visible to the host once it sees this bit.
        if self.dirty.fetch_or(bit, Ordering::Release) == 0 {
            // SAFETY: a scalar thread message with no pointers.
            let _ = unsafe { PostThreadMessageW(self.host, WAKE, WPARAM(0), LPARAM(0)) };
        }
    }
}

fn handler<S: RuntimeType + 'static, A: RuntimeType + 'static>(
    wake: &Arc<Wake>,
    bit: u32,
) -> TypedEventHandler<S, A> {
    let wake = Arc::clone(wake);
    TypedEventHandler::new(move |_, _| {
        wake.raise(bit);
        Ok(())
    })
}

/// The generated `Remove*Changed` method matching one event token.
type Remove<T> = fn(&T, i64) -> Result<()>;
/// A GSMTC object and its event tokens. Dropping it removes every token, newest first, before the
/// object is released; a failed removal is logged and the rest still run.
struct Subscribed<T> {
    source: T,
    tokens: Vec<(i64, Remove<T>)>,
}
impl<T> Drop for Subscribed<T> {
    fn drop(&mut self) {
        for (token, remove) in self.tokens.drain(..).rev() {
            if let Err(error) = remove(&self.source, token) {
                eprintln!("Media event cleanup: {error}");
            }
        }
    }
}

fn subscribe_manager(manager: Manager, wake: &Arc<Wake>) -> Result<Subscribed<Manager>> {
    let mut subscribed = Subscribed {
        source: manager,
        tokens: Vec::with_capacity(2),
    };
    // Both: reports differ on whether CurrentSessionChanged alone fires when the last player closes.
    let token = subscribed
        .source
        .CurrentSessionChanged(&handler(wake, SESSION))?;
    subscribed
        .tokens
        .push((token, Manager::RemoveCurrentSessionChanged));
    let token = subscribed.source.SessionsChanged(&handler(wake, SESSION))?;
    subscribed
        .tokens
        .push((token, Manager::RemoveSessionsChanged));
    Ok(subscribed)
}

fn subscribe_session(session: Session, wake: &Arc<Wake>) -> Result<Subscribed<Session>> {
    let mut subscribed = Subscribed {
        source: session,
        tokens: Vec::with_capacity(3),
    };
    let token = subscribed
        .source
        .MediaPropertiesChanged(&handler(wake, PROPERTIES))?;
    subscribed
        .tokens
        .push((token, Session::RemoveMediaPropertiesChanged));
    let token = subscribed
        .source
        .PlaybackInfoChanged(&handler(wake, PLAYBACK))?;
    subscribed
        .tokens
        .push((token, Session::RemovePlaybackInfoChanged));
    let token = subscribed
        .source
        .TimelinePropertiesChanged(&handler(wake, PLAYBACK))?;
    subscribed
        .tokens
        .push((token, Session::RemoveTimelinePropertiesChanged));
    Ok(subscribed)
}

/// An async read in flight. Its completion fills this request's own slot exactly once, which never
/// waits (no other writer exists); a dropped request's late result is simply discarded. A slot
/// rather than a capacity-1 `sync_channel`: same semantics, 10.7 KB less binary (measured).
struct Request<T: RuntimeType + 'static> {
    operation: IAsyncOperation<T>,
    generation: u64,
    result: Arc<OnceLock<Result<T>>>,
}
impl<T: RuntimeType + Send + Sync + 'static> Request<T> {
    fn start(operation: IAsyncOperation<T>, generation: u64, wake: &Arc<Wake>) -> Result<Self> {
        let result = Arc::new(OnceLock::new());
        let slot = Arc::clone(&result);
        let wake = Arc::clone(wake);
        operation.when(move |outcome| {
            let _ = slot.set(outcome);
            wake.raise(READY);
        })?;
        Ok(Self {
            operation,
            generation,
            result,
        })
    }
    fn cancel(self) {
        // Best effort: the result is discarded either way.
        let _ = self.operation.Cancel();
    }
}

fn bounded(text: HSTRING) -> String {
    let units: &[u16] = &text;
    let mut end = units.len().min(TEXT_LIMIT);
    // Never split a surrogate pair at the cut.
    if end < units.len() && (0xD800..0xDC00).contains(&units[end - 1]) {
        end -= 1;
    }
    String::from_utf16_lossy(&units[..end])
}

fn read_playback(session: &Session) -> Result<Change> {
    let info = session.GetPlaybackInfo()?;
    let timeline = session.GetTimelineProperties()?;
    // shortcut: a player that leaves PlaybackRate unset is taken as 1x; check real players before progress (slice 4).
    let rate = info.PlaybackRate().and_then(|rate| rate.Value());
    Ok(Change::Playback {
        playing: info.PlaybackStatus()? == Status::Playing,
        rate: rate.unwrap_or(1.0),
        timeline: Timeline {
            start: timeline.StartTime()?.Duration,
            end: timeline.EndTime()?.Duration,
            position: timeline.Position()?.Duration,
            updated: timeline.LastUpdatedTime()?.UniversalTime,
        },
    })
}

fn now_ticks() -> i64 {
    // WinRT DateTime counts 100 ns from 1601-01-01 UTC, 11,644,473,600 s before the Unix epoch.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            (since.as_nanos() / 100) as i64 + 116_444_736_000_000_000
        })
}

/// Host-thread owner of the GSMTC manager, the current session and their event tokens.
/// Must be dropped on the host thread before its `RoInitialize` and COM apartment guards.
pub struct Media {
    // Teardown is field order after `drop` cancels pending reads: the session's tokens are removed
    // and it is released before the manager's.
    session: Option<Subscribed<Session>>,
    manager: Option<Subscribed<Manager>>,
    request: Option<Request<Manager>>,
    fetch: Option<Request<Properties>>,
    /// Properties changed while a read was in flight: read again when it lands.
    again: bool,
    state: State,
    wake: Arc<Wake>,
    /// Media wakes handled, for diagnostics.
    wakes: u32,
}
impl Media {
    /// Idle until `start`; the calling thread is the host thread that `refresh` runs on.
    pub fn create() -> Self {
        Self {
            session: None,
            manager: None,
            request: None,
            fetch: None,
            again: false,
            state: State::default(),
            wake: Arc::new(Wake {
                dirty: AtomicU32::new(0),
                // SAFETY: reads this thread's ID; no pointers.
                host: unsafe { GetCurrentThreadId() },
                caller: AtomicU32::new(0),
            }),
            wakes: 0,
        }
    }
    /// Requests the session manager; the caller must be the WinRT-initialized host STA. Never
    /// fails: refused access becomes the permission view, and nothing is retried.
    pub fn start(&mut self) {
        match Manager::RequestAsync().and_then(|request| Request::start(request, 0, &self.wake)) {
            Ok(request) => self.request = Some(request),
            Err(error) => {
                self.unavailable("RequestAsync", &error);
                self.log();
            }
        }
    }
    /// Applies pending media events. Without any it costs one atomic swap: no polling, no deadline.
    pub fn refresh(&mut self) {
        let mut dirty = self.wake.dirty.swap(0, Ordering::Acquire);
        if dirty == 0 {
            return;
        }
        self.wakes = self.wakes.wrapping_add(1);
        let before = self.state.view.clone();
        if let Some(outcome) = received(&self.request) {
            self.request = None;
            match outcome {
                Ok(manager) => {
                    eprintln!("Media: RequestAsync succeeded");
                    match subscribe_manager(manager, &self.wake) {
                        Ok(manager) => {
                            self.manager = Some(manager);
                            dirty |= SESSION;
                        }
                        Err(error) => self.unavailable("manager events", &error),
                    }
                }
                Err(error) => self.unavailable("RequestAsync", &error),
            }
        }
        if let Some(outcome) = received(&self.fetch) {
            let generation = self.fetch.take().map_or(0, |fetch| fetch.generation);
            if std::mem::take(&mut self.again) {
                dirty |= PROPERTIES;
            }
            match outcome.and_then(|properties| Ok((properties.Title()?, properties.Artist()?))) {
                Ok((title, artist)) => self.state.apply(Change::Properties {
                    generation,
                    title: bounded(title),
                    artist: bounded(artist),
                }),
                Err(error) => eprintln!("Media: properties: {error}"),
            }
        }
        if dirty & SESSION != 0 {
            dirty |= self.replace_session();
        }
        if let Some(session) = &self.session {
            // One read in flight at a time, so results land in order and a burst of changes cannot
            // keep cancelling them; a change meanwhile queues exactly one more read.
            if dirty & PROPERTIES != 0 && self.fetch.is_some() {
                self.again = true;
            } else if dirty & PROPERTIES != 0 {
                let generation = self.state.generation;
                match session
                    .source
                    .TryGetMediaPropertiesAsync()
                    .and_then(|read| Request::start(read, generation, &self.wake))
                {
                    Ok(fetch) => self.fetch = Some(fetch),
                    Err(error) => eprintln!("Media: properties request: {error}"),
                }
            }
            if dirty & PLAYBACK != 0 {
                match read_playback(&session.source) {
                    Ok(change) => self.state.apply(change),
                    Err(error) => eprintln!("Media: playback: {error}"),
                }
            }
        }
        if differs(&before, &self.state.view) {
            self.log();
        }
    }
    /// Re-reads the current session; on a change, ends the old one, starts a new generation and
    /// returns the reads the new session needs.
    fn replace_session(&mut self) -> u32 {
        let Some(manager) = &self.manager else {
            return 0;
        };
        let current = match manager.source.GetCurrentSession() {
            Ok(session) => Some(session),
            // A null session (no player) arrives as an empty error.
            Err(error) if error.code().is_ok() => None,
            Err(error) => {
                eprintln!("Media: current session: {error}");
                return 0;
            }
        };
        // COM identity: the same session may come back through another interface pointer.
        let identity = |session: &Session| {
            session
                .cast::<IUnknown>()
                .map_or(session.as_raw(), |unknown| unknown.as_raw())
        };
        if current.as_ref().map(identity) == self.session.as_ref().map(|s| identity(&s.source)) {
            return 0;
        }
        self.end_session();
        let next = current.and_then(|session| {
            subscribe_session(session, &self.wake)
                .inspect_err(|error| eprintln!("Media: session events: {error}"))
                .ok()
        });
        self.state.apply(Change::Session(next.is_some()));
        self.session = next;
        PROPERTIES | PLAYBACK
    }
    /// Cancels the session's pending read, then removes its tokens and releases it.
    fn end_session(&mut self) {
        if let Some(fetch) = self.fetch.take() {
            fetch.cancel();
        }
        self.again = false;
        self.session = None;
    }
    fn unavailable(&mut self, step: &str, error: &Error) {
        eprintln!("Media: {step} failed, {:#010X}: {error}", error.code().0);
        self.state.apply(Change::Unavailable(error.code()));
    }
    /// One line per shown change. Text lengths only, never titles or app names. Integers only:
    /// float and `Debug` formatting would add to the binary.
    fn log(&self) {
        // SAFETY: reads this thread's ID; no pointers.
        let on_host = self.wake.caller.load(Ordering::Relaxed) == unsafe { GetCurrentThreadId() };
        let (generation, wakes) = (self.state.generation, self.wakes);
        let name = match &self.state.view {
            View::Track(track) => {
                return eprintln!(
                    "Media: {} session={generation} title_utf16={} artist_utf16={} rate_milli={} position_s={} end_s={} wakes={wakes} callback_on_host={on_host}",
                    if track.playing { "playing" } else { "paused" },
                    track.title.encode_utf16().count(),
                    track.artist.encode_utf16().count(),
                    (track.rate * 1000.0) as i64,
                    track.position(now_ticks()) / 10_000_000,
                    track.timeline.end / 10_000_000,
                );
            }
            View::NoPlayer => "no player",
            View::Permission => "permission",
            View::Error => "error",
        };
        eprintln!("Media: {name} session={generation} wakes={wakes} callback_on_host={on_host}");
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        self.end_session();
        if let Some(request) = self.request.take() {
            request.cancel();
        }
        eprintln!("Media: stopped wakes={}", self.wakes);
    }
}

/// A completed request's outcome, if it has arrived.
fn received<T: RuntimeType + 'static>(request: &Option<Request<T>>) -> Option<Result<T>> {
    request.as_ref()?.result.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use windows::Win32::Foundation::E_FAIL;

    const SECOND: i64 = 10_000_000;

    fn track(playing: bool, rate: f64, start: i64, end: i64, position: i64) -> Track {
        Track {
            playing,
            rate,
            timeline: Timeline {
                start,
                end,
                position,
                updated: 1000 * SECOND,
            },
            ..Track::default()
        }
    }
    fn properties(generation: u64, title: &str) -> Change {
        Change::Properties {
            generation,
            title: title.into(),
            artist: "Artist".into(),
        }
    }

    #[test]
    fn session_replacement_clears_the_track_and_discards_stale_reads() {
        let mut state = State::default();
        state.apply(Change::Session(true));
        let first = state.generation;
        state.apply(properties(first, "One"));
        state.apply(Change::Playback {
            playing: true,
            rate: 1.0,
            timeline: Timeline::default(),
        });
        let View::Track(track) = &state.view else {
            panic!("expected a track");
        };
        assert_eq!((track.title.as_str(), track.playing), ("One", true));
        state.apply(Change::Session(true));
        assert_eq!(state.generation, first + 1);
        assert_eq!(state.view, View::Track(Track::default()));
        // A read started for the replaced session lands late and is dropped.
        state.apply(properties(first, "Stale"));
        assert_eq!(state.view, View::Track(Track::default()));
        state.apply(properties(first + 1, "Two"));
        assert!(matches!(&state.view, View::Track(t) if t.title == "Two"));
    }

    #[test]
    fn no_player_ignores_reads_until_a_session_appears() {
        let mut state = State::default();
        assert_eq!(state.view, View::NoPlayer);
        state.apply(Change::Session(true));
        state.apply(Change::Session(false));
        let generation = state.generation;
        state.apply(properties(generation, "Ghost"));
        state.apply(Change::Playback {
            playing: true,
            rate: 1.0,
            timeline: Timeline::default(),
        });
        assert_eq!(state.view, View::NoPlayer);
    }

    #[test]
    fn denied_access_is_permission_and_other_failures_are_errors() {
        let mut state = State::default();
        state.apply(Change::Unavailable(E_ACCESSDENIED));
        assert_eq!(state.view, View::Permission);
        state.apply(Change::Unavailable(E_FAIL));
        assert_eq!(state.view, View::Error);
    }

    #[test]
    fn playing_position_extrapolates_and_clamps_to_the_timeline() {
        let now = 1010 * SECOND;
        assert_eq!(
            track(true, 1.0, 0, 300 * SECOND, 20 * SECOND).position(now),
            30 * SECOND
        );
        assert_eq!(
            track(true, 2.0, 0, 300 * SECOND, 20 * SECOND).position(now),
            40 * SECOND
        );
        assert_eq!(
            track(true, 1.0, 0, 25 * SECOND, 20 * SECOND).position(now),
            25 * SECOND
        );
        // Rewinding past the start, or an update stamped in the future, never leaves the timeline.
        assert_eq!(
            track(true, -4.0, 0, 300 * SECOND, 20 * SECOND).position(now),
            0
        );
        let early = 990 * SECOND;
        assert_eq!(
            track(true, 1.0, 0, 300 * SECOND, 20 * SECOND).position(early),
            20 * SECOND
        );
        // No LastUpdatedTime: nothing to extrapolate from.
        let mut unstamped = track(true, 1.0, 0, 300 * SECOND, 20 * SECOND);
        unstamped.timeline.updated = 0;
        assert_eq!(unstamped.position(now), 20 * SECOND);
    }

    #[test]
    fn paused_or_zero_rate_holds_the_position() {
        let now = 1100 * SECOND;
        assert_eq!(
            track(false, 1.0, 0, 300 * SECOND, 20 * SECOND).position(now),
            20 * SECOND
        );
        assert_eq!(
            track(true, 0.0, 0, 300 * SECOND, 20 * SECOND).position(now),
            20 * SECOND
        );
    }

    #[test]
    fn missing_end_time_leaves_the_position_unbounded_above() {
        let now = 1500 * SECOND;
        assert_eq!(
            track(true, 1.0, 0, 0, 20 * SECOND).position(now),
            520 * SECOND
        );
        assert_eq!(
            track(true, 1.0, 50 * SECOND, 0, 0).position(1000 * SECOND),
            50 * SECOND
        );
    }

    #[test]
    fn only_non_timeline_changes_are_logged() {
        let a = View::Track(track(true, 1.0, 0, 300 * SECOND, 20 * SECOND));
        let b = View::Track(track(true, 1.0, 0, 300 * SECOND, 21 * SECOND));
        assert!(!differs(&a, &b));
        assert!(differs(&a, &View::Track(track(false, 1.0, 0, 0, 0))));
        assert!(differs(&a, &View::NoPlayer));
    }

    #[test]
    fn text_is_capped_without_splitting_a_surrogate_pair() {
        assert_eq!(bounded(HSTRING::from("Song")), "Song");
        let long = "a".repeat(TEXT_LIMIT + 10);
        assert_eq!(bounded(HSTRING::from(long.as_str())).len(), TEXT_LIMIT);
        // An emoji (two UTF-16 units) straddling the cut is dropped whole, not turned into U+FFFD.
        let straddling = format!("{}😀b", "a".repeat(TEXT_LIMIT - 1));
        assert_eq!(
            bounded(HSTRING::from(straddling.as_str())),
            "a".repeat(TEXT_LIMIT - 1)
        );
    }

    /// Stands in for a GSMTC object: records token removals and its own release.
    struct Fake {
        name: &'static str,
        log: Rc<RefCell<Vec<String>>>,
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.log
                .borrow_mut()
                .push(format!("{} released", self.name));
        }
    }
    fn remove(fake: &Fake, token: i64) -> Result<()> {
        fake.log
            .borrow_mut()
            .push(format!("{} removed {token}", fake.name));
        if token == 2 {
            return Err(E_FAIL.into());
        }
        Ok(())
    }
    fn subscribed(name: &'static str, log: &Rc<RefCell<Vec<String>>>) -> Subscribed<Fake> {
        Subscribed {
            source: Fake {
                name,
                log: Rc::clone(log),
            },
            tokens: vec![(1, remove), (2, remove), (3, remove)],
        }
    }

    /// Mirrors `Media`'s field order, the mechanism its teardown relies on.
    struct FakeOwner {
        session: Option<Subscribed<Fake>>,
        _manager: Option<Subscribed<Fake>>,
    }

    #[test]
    fn teardown_removes_session_tokens_then_manager_tokens_before_each_release() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut owner = FakeOwner {
            session: Some(subscribed("old", &log)),
            _manager: Some(subscribed("manager", &log)),
        };
        // Replacement: the old session is fully torn down before the new one is held.
        owner.session = None;
        owner.session = Some(subscribed("new", &log));
        drop(owner);
        let expected = [
            "old removed 3",
            "old removed 2",
            "old removed 1",
            "old released",
            "new removed 3",
            "new removed 2",
            "new removed 1",
            "new released",
            "manager removed 3",
            "manager removed 2",
            "manager removed 1",
            "manager released",
        ];
        assert_eq!(*log.borrow(), expected);
    }
}
