//! Media state from the Windows global media controls (GSMTC): state, events and diagnostics, and
//! which part of it the notch draws.
//!
//! Every GSMTC read and mutation happens on the host (STA) thread. Event handlers and async
//! completions may run on any thread: they only set dirty bits, fill their request's one-shot slot
//! and post one scalar wake, so they never block, panic or touch `Desktop`, HWNDs or Direct2D.
use crate::chan::Sender;
use crate::icons::{IconPixels, Job};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU32, Ordering},
};
use std::time::{Duration, Instant};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as Properties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Storage::Streams::{
        IRandomAccessStream, IRandomAccessStreamReference, IRandomAccessStreamWithContentType,
    },
    Win32::{
        Foundation::{E_ACCESSDENIED, LPARAM, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
    },
    core::{AgileReference, Error, HRESULT, HSTRING, IUnknown, Interface, Result, RuntimeType},
};
use windows_future::{AsyncStatus, IAsyncOperation};

/// Thread message that only wakes the host loop; the pending work is in `Wake::dirty`.
const WAKE: u32 = WM_APP + 50;
const SESSION: u32 = 1;
const PROPERTIES: u32 = 2;
/// Playback info changed.
const PLAYBACK: u32 = 4;
/// An async request completed and filled its slot.
const READY: u32 = 8;
/// Title and artist are cut to this many UTF-16 units, so one session holds at most a few KiB.
const TEXT_LIMIT: usize = 512;
const PAUSED_GRACE: Duration = Duration::from_secs(5 * 60);
/// One second in WinRT 100 ns ticks.
const SECOND: i64 = 10_000_000;
/// Longer reported lengths (up to `i64::MAX` from a live stream, say) count as none: no bar and no
/// seek. Keeps ticks times bar pixels far inside i64.
const MAX_LENGTH: i64 = 1000 * 3600 * SECOND;

/// WinRT 100 ns ticks; `updated` is a UTC `DateTime` (ticks since 1601).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeline {
    pub start: i64,
    pub end: i64,
    pub position: i64,
    pub updated: i64,
}

/// Indexes of `Track::controls`, in drawing and UI Automation order.
pub const PREVIOUS: usize = 0;
pub const PLAY_PAUSE: usize = 1;
pub const NEXT: usize = 2;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// One expiry after leaving Playing; repeated pause/stop reports do not extend it.
    pub paused_until: Option<Instant>,
    /// Whether the player accepts previous, play/pause and next. Play/pause is the action the
    /// button performs now: pause while playing, play otherwise.
    pub controls: [bool; 3],
    /// The player accepts a new playback position (`IsPlaybackPositionEnabled`).
    pub seekable: bool,
    pub rate: f64,
    pub timeline: Timeline,
}

/// What the panel's progress row draws: whole seconds elapsed and in total, and the bar's filled
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub elapsed: i64,
    pub total: i64,
    pub filled: i32,
}
impl Progress {
    /// Elapsed and total time as `m:ss`.
    pub fn times(self) -> [String; 2] {
        [self.elapsed, self.total].map(|seconds| format!("{}:{:02}", seconds / 60, seconds % 60))
    }
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
    /// The track length; a player that reports no usable `EndTime` gets no bar and no seek.
    fn duration(&self) -> Option<i64> {
        Some(self.timeline.end.saturating_sub(self.timeline.start))
            .filter(|total| (1..=MAX_LENGTH).contains(total))
    }
    /// The progress row at `now` (UTC ticks) for a bar `width` pixels wide.
    pub fn progress(&self, now: i64, width: i32) -> Option<Progress> {
        let total = self.duration()?;
        let elapsed = self.position(now) - self.timeline.start;
        Some(Progress {
            elapsed: elapsed / SECOND,
            total: total / SECOND,
            filled: (elapsed * i64::from(width.max(0)) / total) as i32,
        })
    }
    /// How long until the progress row at `now` should repaint: its next whole second or bar pixel,
    /// at the playback rate. At rate 1 or below a whole second never waits, so the time neither
    /// lags nor skips; a pixel-only change waits for `floor` (a second after the previous tick) and
    /// is taken only if the next second is still a second after it, so ticks stay a second apart.
    /// Above rate 1 every change waits for `floor`. None while the row cannot advance: paused, a
    /// rate of 0 or below, no `LastUpdatedTime` or length to extrapolate within, or at the end.
    pub fn next_tick(&self, now: i64, width: i32, floor: Duration) -> Option<Duration> {
        let usable = self.playing && self.rate > 0.0 && self.timeline.updated > 0;
        let total = self.duration().filter(|_| usable)?;
        let elapsed = self.position(now) - self.timeline.start;
        if elapsed >= total {
            return None;
        }
        let width = i64::from(width.max(1));
        // The first tick at which floor(elapsed * width / total) grows by one pixel.
        let pixel = ((elapsed * width / total + 1) * total + width - 1) / width;
        let wait = |next: i64| {
            let ticks = next.min(total) - elapsed;
            Duration::from_nanos((ticks as f64 * 100.0 / self.rate).ceil() as u64)
        };
        let (second, pixel) = (wait((elapsed / SECOND + 1) * SECOND), wait(pixel));
        Some(if self.rate > 1.0 {
            second.min(pixel).max(floor)
        } else if pixel.max(floor) + Duration::from_secs(1) <= second {
            pixel.max(floor)
        } else {
            second
        })
    }
    /// The position at `fraction` (0 to 1) of the bar; none without a duration.
    pub fn seek_target(&self, fraction: f64) -> Option<i64> {
        let total = self.duration()?;
        Some(self.timeline.start + (total as f64 * fraction.clamp(0.0, 1.0)) as i64)
    }
    /// A seek to `target`, asked for at `asked` (UTC ticks), was accepted. A player may report the
    /// new timeline only after that: until it does, show the target from `now`.
    fn sought(&mut self, target: i64, asked: i64, now: i64) {
        if self.timeline.updated < asked {
            (self.timeline.position, self.timeline.updated) = (target, now);
        }
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
impl View {
    /// The track the notch draws; every other view draws the plain clock notch.
    pub fn shown(&self) -> Option<&Track> {
        match self {
            View::Track(track)
                if (track.playing
                    || track.paused_until.is_some_and(|due| Instant::now() < due))
                    && (!track.title.is_empty() || !track.artist.is_empty()) =>
            {
                Some(track)
            }
            _ => None,
        }
    }
    /// The one line the open plain panel shows when media access failed, so that notch is not
    /// mistaken for "no player"; it is also the UIA root name's suffix.
    pub fn status(&self) -> Option<&'static str> {
        match self {
            View::Permission => Some("Windows blocked media access"),
            View::Error => Some("Media controls unavailable"),
            _ => None,
        }
    }
}

/// Whether the notch must repaint: a session appeared or ended, or its title, artist, play state or
/// enabled controls changed. Timeline and rate updates, and changes among the no-media views, draw
/// nothing new. Every track field the notch draws must be compared here.
pub fn redraw(before: &View, after: &View) -> bool {
    fn drawn(view: &View) -> Option<(&str, &str, bool, [bool; 3])> {
        view.shown()
            .map(|t| (t.title.as_str(), t.artist.as_str(), t.playing, t.controls))
    }
    drawn(before) != drawn(after)
}

/// One observation from the owner, applied in order on the host thread.
pub enum Change {
    /// Obtaining the session manager failed with this code.
    Unavailable(HRESULT),
    /// The current session was replaced, or is gone (`false`); starts a new generation.
    Session(bool),
    Playback {
        status: Status,
        observed: Instant,
        controls: [bool; 3],
        seekable: bool,
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
            // A transient state between tracks or loads: keep what is shown, so it neither flickers
            // the notch nor ends the paused grace early.
            Change::Playback { status, .. } if status == Status::Changing => {}
            Change::Playback {
                status,
                observed,
                controls,
                seekable,
                rate,
                timeline,
            } => {
                if let View::Track(track) = &mut self.view {
                    let playing = status == Status::Playing;
                    if playing || (status != Status::Paused && status != Status::Stopped) {
                        track.paused_until = None;
                    } else if track.playing {
                        // Playback was known to continue until this pause/stop observation.
                        track.paused_until = Some(observed + PAUSED_GRACE);
                    }
                    (track.playing, track.controls, track.seekable) = (playing, controls, seekable);
                    (track.rate, track.timeline) = (rate, timeline);
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
    /// Consumes the single grace deadline; expiry must repaint even though shown() now hides it.
    fn expire(&mut self, now: Instant) -> bool {
        if let View::Track(track) = &mut self.view
            && track.paused_until.is_some_and(|due| now >= due)
        {
            track.paused_until = None;
            return !track.title.is_empty() || !track.artist.is_empty();
        }
        false
    }
}

/// Whether two views differ in more than the timeline, which players may update every second.
fn differs(before: &View, after: &View) -> bool {
    match (before, after) {
        (View::Track(a), View::Track(b)) => {
            (&a.title, &a.artist, a.playing, a.controls, a.rate)
                != (&b.title, &b.artist, b.playing, b.controls, b.rate)
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
        tokens: Vec::with_capacity(2),
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
    // No TimelinePropertiesChanged: players may raise it every second. The timeline is read on
    // opening a notch and after a command (`Media::set_expanded`), with every playback and (while
    // open) properties event, and extrapolated between reads.
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
    fn cancel(&self) {
        // Cancellation is a request, not evidence that the operation has terminated.
        let _ = self.operation.Cancel();
    }
}

/// Cancelled commands that may still be running: a dead player's never completes, so at most this
/// many are kept before new commands wait, instead of one hung command disabling every session.
const ORPHANS: usize = 4;

fn running(operation: &IAsyncOperation<bool>) -> bool {
    operation.Status() == Ok(AsyncStatus::Started)
}

/// One command at a time for the current session, and at most `ORPHANS` unfinished ones from ended
/// sessions; a late completion of an orphan lands in its own dropped slot and is ignored.
fn admit_command(
    slot: &mut Option<Request<bool>>,
    orphans: &mut Vec<IAsyncOperation<bool>>,
    generation: u64,
    wake: &Arc<Wake>,
    start: impl FnOnce() -> Result<IAsyncOperation<bool>>,
) -> Result<()> {
    orphans.retain(running);
    if slot.is_some() || orphans.len() >= ORPHANS {
        return Ok(());
    }
    let operation = start()?;
    match Request::start(operation.clone(), generation, wake) {
        Ok(request) => *slot = Some(request),
        Err(error) => {
            // No completion subscription: cancel, and count it until it finishes on its own.
            let _ = operation.Cancel();
            orphans.push(operation);
            return Err(error);
        }
    }
    Ok(())
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

fn read_timeline(session: &Session) -> Result<Timeline> {
    let timeline = session.GetTimelineProperties()?;
    Ok(Timeline {
        start: timeline.StartTime()?.Duration,
        end: timeline.EndTime()?.Duration,
        position: timeline.Position()?.Duration,
        updated: timeline.LastUpdatedTime()?.UniversalTime,
    })
}

fn read_playback(session: &Session, observed: Instant) -> Result<Change> {
    let info = session.GetPlaybackInfo()?;
    // shortcut: a player that leaves PlaybackRate unset is taken as 1x; check real players before progress (slice 4).
    let rate = info.PlaybackRate().and_then(|rate| rate.Value());
    let status = info.PlaybackStatus()?;
    let playing = status == Status::Playing;
    let controls = info.Controls()?;
    // shortcut: a player offering only the play/pause toggle shows a disabled button; use TryTogglePlayPauseAsync if a real player does.
    let play_pause = if playing {
        controls.IsPauseEnabled()?
    } else {
        controls.IsPlayEnabled()?
    };
    Ok(Change::Playback {
        status,
        observed,
        controls: [
            controls.IsPreviousEnabled()?,
            play_pause,
            controls.IsNextEnabled()?,
        ],
        seekable: controls.IsPlaybackPositionEnabled()?,
        rate: rate.unwrap_or(1.0),
        timeline: read_timeline(session)?,
    })
}

/// Now as a WinRT `DateTime` (UTC ticks), the clock `Track::position` extrapolates against.
pub fn now_ticks() -> i64 {
    // WinRT DateTime counts 100 ns from 1601-01-01 UTC, 11,644,473,600 s before the Unix epoch.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            (since.as_nanos() / 100) as i64 + 116_444_736_000_000_000
        })
}

/// The current track's album art, the only entry kept. Replacing it cancels its read and orphans
/// its slot, so a late decode for an older track is never drawn.
#[derive(Default)]
struct Art {
    /// Session generation, title and artist of the track it belongs to.
    key: (u64, String, String),
    read: Option<IAsyncOperation<IRandomAccessStreamWithContentType>>,
    /// Filled once, on the icon worker, when decoding succeeds; a failed track stays empty.
    pixels: Arc<OnceLock<IconPixels>>,
    /// The host has handled the pixels' arrival, which repaints exactly once. The worker fills the
    /// slot before its wake, so comparing the slot across one refresh would miss it.
    seen: bool,
}
impl Drop for Art {
    fn drop(&mut self) {
        if let Some(read) = &self.read {
            // A request only: a read that still completes decodes into an orphaned slot.
            let _ = read.Cancel();
        }
    }
}

/// Completion side of a thumbnail read: queues the opened stream for decoding without blocking.
/// A reference that cannot be created, or a full queue, drops the request, so the track keeps the
/// no-art layout. The worker calls `done` with the decoded pixels.
fn hand_off(
    opened: Result<AgileReference<IRandomAccessStream>>,
    jobs: &Sender<Job>,
    side: u32,
    done: Box<dyn FnOnce(IconPixels) + Send>,
) {
    if let Ok(stream) = opened {
        let _ = jobs.try_send(Job::Art { stream, side, done });
    }
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
    /// The current session's one outstanding command.
    command: Option<Request<bool>>,
    /// Cancelled commands of ended sessions that may still run (`admit_command`).
    orphans: Vec<IAsyncOperation<bool>>,
    /// An album art read was skipped because no notch surface existed; the next pass with one
    /// re-reads the properties.
    art_skipped: bool,
    /// Properties changed while a read was in flight: read again when it lands.
    again: bool,
    /// A visible notch is open, so the drawn progress needs a current timeline.
    expanded: bool,
    /// The target and request time (UTC ticks) of the seek in `command`, if it is one.
    seek: Option<(i64, i64)>,
    art: Art,
    /// Album art entries started; the current entry's number keys its uploaded bitmap.
    arts: u64,
    /// The icon worker's queue, which decodes album art; none until `start`.
    jobs: Option<Sender<Job>>,
    state: State,
    wake: Arc<Wake>,
    /// Media wakes handled, for diagnostics.
    wakes: u32,
    /// Refreshes that did work (a wake or a grace expiry), not host calls.
    refreshes: u32,
}
impl Media {
    /// Idle until `start`; the calling thread is the host thread that `refresh` runs on.
    pub fn create() -> Self {
        Self {
            session: None,
            manager: None,
            request: None,
            fetch: None,
            command: None,
            orphans: Vec::new(),
            art_skipped: false,
            again: false,
            expanded: false,
            seek: None,
            art: Art::default(),
            arts: 0,
            jobs: None,
            state: State::default(),
            wake: Arc::new(Wake {
                dirty: AtomicU32::new(0),
                // SAFETY: reads this thread's ID; no pointers.
                host: unsafe { GetCurrentThreadId() },
                caller: AtomicU32::new(0),
            }),
            wakes: 0,
            refreshes: 0,
        }
    }
    /// Requests the session manager; the caller must be the WinRT-initialized host STA. Never
    /// fails: refused access becomes the permission view, and nothing is retried. Album art is
    /// decoded by `jobs`, or not drawn without it.
    pub fn start(&mut self, jobs: Option<Sender<Job>>) {
        self.jobs = jobs;
        match Manager::RequestAsync().and_then(|request| Request::start(request, 0, &self.wake)) {
            Ok(request) => self.request = Some(request),
            Err(error) => {
                self.unavailable("RequestAsync", &error);
                self.log();
            }
        }
    }
    pub fn view(&self) -> &View {
        &self.state.view
    }
    /// The shown track's album art, once decoded, and its entry number, which changes with the art.
    pub fn art(&self) -> Option<(u64, &IconPixels)> {
        let pixels = self.state.view.shown().and(self.art.pixels.get())?;
        Some((self.arts, pixels))
    }
    /// Work is pending that no wake may have announced: a post can fail, or be consumed by a modal
    /// loop, while the bits stay set.
    pub fn pending(&self) -> bool {
        self.wake.dirty.load(Ordering::Relaxed) != 0
    }
    /// After resume or an Explorer restart, events may have been missed: re-reads the current
    /// session, its playback state and timeline, and its properties (new art only for a new track).
    pub fn reconnect(&self) {
        self.wake
            .dirty
            .fetch_or(SESSION | PROPERTIES | PLAYBACK, Ordering::Relaxed);
    }
    /// One host wake at grace expiry, and none for a view with nothing to show.
    pub fn deadline(&self) -> Option<Instant> {
        match &self.state.view {
            View::Track(track) if !track.title.is_empty() || !track.artist.is_empty() => {
                track.paused_until
            }
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.state.generation
    }
    /// Sends `control` (`PREVIOUS`, `PLAY_PAUSE` or `NEXT`) to the current session if its player
    /// enables it. The notch changes only when the player reports the outcome through its events.
    pub fn command(&mut self, control: usize) {
        let (Some(session), Some(track)) = (&self.session, self.state.view.shown()) else {
            return;
        };
        if !track.controls.get(control).copied().unwrap_or(false) {
            return;
        }
        let source = &session.source;
        let start = || match control {
            PREVIOUS => source.TrySkipPreviousAsync(),
            PLAY_PAUSE if track.playing => source.TryPauseAsync(),
            PLAY_PAUSE => source.TryPlayAsync(),
            _ => source.TrySkipNextAsync(),
        };
        // shortcut: presses while a command is unfinished are ignored; no queue or replacement work.
        let generation = self.state.generation;
        if let Err(error) = admit_command(
            &mut self.command,
            &mut self.orphans,
            generation,
            &self.wake,
            start,
        ) {
            eprintln!("Media: command {control}: {error}");
        }
    }
    /// Seeks the current session to `fraction` (0 to 1) of the track when its player accepts it.
    /// While a notch is open, the new position is read when the command completes.
    pub fn seek(&mut self, fraction: f64) {
        let (Some(session), Some(track)) = (&self.session, self.state.view.shown()) else {
            return;
        };
        let Some(target) = track.seek_target(fraction).filter(|_| track.seekable) else {
            return;
        };
        let idle = self.command.is_none();
        let start = || session.source.TryChangePlaybackPositionAsync(target);
        let generation = self.state.generation;
        match admit_command(
            &mut self.command,
            &mut self.orphans,
            generation,
            &self.wake,
            start,
        ) {
            Ok(()) if idle && self.command.is_some() => self.seek = Some((target, now_ticks())),
            Ok(()) => {}
            Err(error) => eprintln!("Media: seek: {error}"),
        }
    }
    /// Whether any visible notch is open. Opening reads the timeline once: no event keeps it
    /// current, and a collapsed notch draws no progress.
    pub fn set_expanded(&mut self, expanded: bool) {
        if expanded && !self.expanded && self.state.view.shown().is_some() {
            let started = Instant::now();
            self.reread_timeline();
            slow("timeline read", started);
        }
        self.expanded = expanded;
    }
    fn reread_timeline(&mut self) {
        let (Some(session), View::Track(track)) = (&self.session, &mut self.state.view) else {
            return;
        };
        match read_timeline(&session.source) {
            Ok(timeline) => track.timeline = timeline,
            Err(error) => eprintln!("Media: timeline: {error}"),
        }
    }
    /// Applies pending media events and returns whether the drawn view or its album art changed.
    /// A new track's art is decoded for a `cover` pixels square. Without any events it costs one
    /// atomic swap, plus a clock check only while a grace deadline exists; the caller's `cover`
    /// is a scan of a few surfaces.
    pub fn refresh(&mut self, cover: u32) -> bool {
        let expired = self
            .deadline()
            .is_some_and(|_| self.state.expire(Instant::now()));
        // A notch exists again: read the properties (and so the art) skipped without one.
        if cover > 0 && std::mem::take(&mut self.art_skipped) {
            self.wake.dirty.fetch_or(PROPERTIES, Ordering::Relaxed);
        }
        let mut dirty = self.wake.dirty.swap(0, Ordering::Acquire);
        // The host calls this every pass (meter, frames, any message): only a wake or an expiry is a refresh.
        if dirty == 0 && !expired {
            return false;
        }
        self.refreshes = self.refreshes.wrapping_add(1);
        crate::diagnostics::MEDIA_REFRESHES.fetch_add(1, Ordering::Relaxed);
        if dirty == 0 {
            return true;
        }
        let started = Instant::now();
        self.wakes = self.wakes.wrapping_add(1);
        crate::diagnostics::MEDIA_WAKES.fetch_add(1, Ordering::Relaxed);
        let finished = |read: &mut IAsyncOperation<_>| read.Status() != Ok(AsyncStatus::Started);
        // A finished read keeps its result stream; dropping our reference frees it. Never `Close()`
        // it: the completion may still be about to call `GetResults()`.
        self.art.read.take_if(finished);
        let before = self.state.view.clone();
        if let Some(outcome) = received(&self.request) {
            self.request = None;
            match outcome {
                Ok(manager) => {
                    std::eprintln!("Media: RequestAsync succeeded");
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
        // While open, a track change or a finished command (a seek or skip) may move the position
        // without a playback-info event.
        let mut timeline = self.expanded && dirty & PROPERTIES != 0;
        let mut sought = None;
        if let Some(outcome) = received(&self.command) {
            let generation = self.command.take().map_or(0, |command| command.generation);
            let seek = self
                .seek
                .take()
                .filter(|_| generation == self.state.generation);
            timeline |= self.expanded;
            match outcome {
                Ok(true) => sought = seek,
                Ok(false) => eprintln!("Media: the player refused a command"),
                Err(error) => eprintln!("Media: command: {error}"),
            }
        }
        if let Some(outcome) = received(&self.fetch) {
            let generation = self.fetch.take().map_or(0, |fetch| fetch.generation);
            if std::mem::take(&mut self.again) {
                dirty |= PROPERTIES;
            }
            match outcome
                .and_then(|properties| Ok((properties.Title()?, properties.Artist()?, properties)))
            {
                Ok((title, artist, properties)) => {
                    let (title, artist) = (bounded(title), bounded(artist));
                    // A track without title or artist is never shown, so it needs no art; with no
                    // notch (`cover` 0) nothing draws it yet.
                    if generation == self.state.generation
                        && !(title.is_empty() && artist.is_empty())
                    {
                        if cover == 0 {
                            self.art_skipped = true;
                        } else {
                            let key = (generation, title.clone(), artist.clone());
                            self.read_art(key, || properties.Thumbnail(), cover);
                        }
                    }
                    self.state.apply(Change::Properties {
                        generation,
                        title,
                        artist,
                    });
                }
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
                match read_playback(&session.source, started) {
                    Ok(change) => self.state.apply(change),
                    Err(error) => {
                        eprintln!("Media: playback: {error}");
                        // A session failing its reads may be gone without an event: check it once.
                        self.wake.raise(SESSION);
                    }
                }
            }
        }
        if timeline && dirty & PLAYBACK == 0 {
            self.reread_timeline();
        }
        if let (Some((target, asked)), View::Track(track)) = (sought, &mut self.state.view) {
            track.sought(target, asked, now_ticks());
        }
        if differs(&before, &self.state.view) {
            self.log();
        }
        slow("host refresh", started);
        let arrived = !self.art.seen && self.art.pixels.get().is_some();
        self.art.seen |= arrived;
        // Art for a track that is not drawn changes nothing on screen.
        expired
            || redraw(&before, &self.state.view)
            || (arrived && self.state.view.shown().is_some())
    }
    /// Starts reading a new track's `thumbnail` for a `side` pixels decode. Any step that fails
    /// leaves the track without art, and the same track is never read again.
    fn read_art(
        &mut self,
        key: (u64, String, String),
        thumbnail: impl FnOnce() -> Result<IRandomAccessStreamReference>,
        side: u32,
    ) {
        if key == self.art.key {
            return;
        }
        self.arts += 1;
        self.art = Art {
            key,
            read: None,
            pixels: Arc::default(),
            seen: false,
        };
        let Some(jobs) = self.jobs.clone() else {
            return;
        };
        let (pixels, landed) = (Arc::clone(&self.art.pixels), Arc::clone(&self.wake));
        let done = Box::new(move |art| {
            let _ = pixels.set(art);
            landed.raise(READY);
        });
        let wake = Arc::clone(&self.wake);
        let read = thumbnail()
            .and_then(|thumbnail| thumbnail.OpenReadAsync())
            .and_then(|read| {
                read.when(move |opened| {
                    let opened = opened.and_then(|stream| AgileReference::new(&stream.cast()?));
                    hand_off(opened, &jobs, side, done);
                    // Lets the host release the finished read, which holds the stream, at once.
                    wake.raise(READY);
                })?;
                Ok(read)
            });
        match read {
            Ok(read) => self.art.read = Some(read),
            Err(error) => eprintln!("Media: no album art: {error}"),
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
    /// Cancels the session's pending reads and command, drops its album art, then removes its tokens
    /// and releases it. The command is orphaned, not kept: a hung one must not outlive its session's
    /// budget (`admit_command`).
    fn end_session(&mut self) {
        if let Some(fetch) = self.fetch.take() {
            fetch.cancel();
        }
        if let Some(command) = self.command.take() {
            command.cancel();
            self.orphans.push(command.operation);
        }
        self.seek = None;
        self.again = false;
        self.art = Art::default();
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
                return std::eprintln!(
                    "Media: {} session={generation} title_utf16={} artist_utf16={} controls={}{}{} rate_milli={} position_s={} end_s={} wakes={wakes} callback_on_host={on_host}",
                    if track.playing { "playing" } else { "paused" },
                    track.title.encode_utf16().count(),
                    track.artist.encode_utf16().count(),
                    u8::from(track.controls[PREVIOUS]),
                    u8::from(track.controls[PLAY_PAUSE]),
                    u8::from(track.controls[NEXT]),
                    (track.rate * 1000.0) as i64,
                    track.position(now_ticks()) / 10_000_000,
                    track.timeline.end / 10_000_000,
                );
            }
            View::NoPlayer => "no player",
            View::Permission => "permission",
            View::Error => "error",
        };
        std::eprintln!(
            "Media: {name} session={generation} wakes={wakes} callback_on_host={on_host}"
        );
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        self.end_session();
        if let Some(request) = self.request.take() {
            request.cancel();
        }
        std::eprintln!("Media: stopped wakes={}", self.wakes);
    }
}

/// Logs host-thread GSMTC work that took long enough to delay input or frames.
fn slow(work: &str, started: Instant) {
    let elapsed = started.elapsed();
    if elapsed > Duration::from_millis(8) {
        std::eprintln!("Media: slow {work} {elapsed:?}");
    }
}

/// A completed request's outcome, if it has arrived.
fn received<T: RuntimeType + 'static>(request: &Option<Request<T>>) -> Option<Result<T>> {
    request.as_ref()?.result.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::{Receiver, channel as sync_channel};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    use windows::{
        Storage::Streams::{InMemoryRandomAccessStream, RandomAccessStreamReference},
        Win32::Foundation::E_FAIL,
    };

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
            status: Status::Playing,
            observed: Instant::now(),
            controls: [false, true, true],
            seekable: true,
            rate: 1.0,
            timeline: Timeline::default(),
        });
        let View::Track(track) = &state.view else {
            panic!("expected a track");
        };
        assert_eq!(
            (track.title.as_str(), track.playing, track.seekable),
            ("One", true, true)
        );
        assert_eq!(track.controls, [false, true, true]);
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
            status: Status::Playing,
            observed: Instant::now(),
            controls: [true; 3],
            seekable: true,
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
    fn progress_shows_whole_seconds_and_floored_pixels_and_needs_a_duration() {
        // Started 20 s in, stamped at 1000 s; 2:00 long on a 120 px bar.
        let playing = track(true, 1.0, 0, 120 * SECOND, 20 * SECOND);
        let at = |seconds: f64| 1000 * SECOND + (seconds * SECOND as f64) as i64;
        let shown = playing.progress(at(10.5), 120).unwrap();
        assert_eq!(
            shown,
            Progress {
                elapsed: 30,
                total: 120,
                filled: 30
            }
        );
        assert_eq!(shown.times(), ["0:30", "2:00"]);
        // Clamped at both ends of the timeline.
        assert_eq!(playing.progress(at(500.0), 120).unwrap().filled, 120);
        let rewound = track(true, -4.0, 0, 120 * SECOND, 20 * SECOND);
        assert_eq!(rewound.progress(at(10.0), 120).unwrap().elapsed, 0);
        // A late StartTime shifts both the time and the bar.
        let offset = track(false, 1.0, 60 * SECOND, 180 * SECOND, 90 * SECOND);
        let shown = offset.progress(0, 240).unwrap();
        assert_eq!((shown.elapsed, shown.filled), (30, 60));
        assert_eq!(
            Progress {
                elapsed: 3725,
                total: 59,
                filled: 0
            }
            .times(),
            ["62:05", "0:59"]
        );
        // No EndTime, or one not after StartTime: no bar, no deadline and no seek.
        for (start, end) in [(0, 0), (60 * SECOND, 30 * SECOND), (5, 5)] {
            let unbounded = track(true, 1.0, start, end, 20 * SECOND);
            assert_eq!(unbounded.progress(at(1.0), 120), None);
            assert_eq!(unbounded.next_tick(at(1.0), 120, Duration::ZERO), None);
            assert_eq!(unbounded.seek_target(0.5), None);
        }
    }

    #[test]
    fn without_a_floor_ticks_land_on_the_next_second_or_a_pixel_well_before_it() {
        let at = |seconds: f64| 1000 * SECOND + (seconds * SECOND as f64) as i64;
        let millis = |duration: Option<Duration>| duration.map(|d| d.as_millis());
        // 2:00 on 120 px: one pixel per second, so both change together on the second.
        let even = track(true, 1.0, 0, 120 * SECOND, 20 * SECOND);
        assert_eq!(
            millis(even.next_tick(at(0.25), 120, Duration::ZERO)),
            Some(750)
        );
        // 10:00 on 120 px: a pixel per 5 s, so the second comes first.
        let long = track(true, 1.0, 0, 600 * SECOND, 20 * SECOND);
        assert_eq!(
            millis(long.next_tick(at(0.25), 120, Duration::ZERO)),
            Some(750)
        );
        // 0:30 on 120 px: four pixels a second, each under a second before the next whole second,
        // so they wait for it.
        let short = track(true, 1.0, 0, 30 * SECOND, 0);
        assert_eq!(
            millis(short.next_tick(at(0.1), 120, Duration::ZERO)),
            Some(900)
        );
        assert_eq!(
            millis(short.next_tick(at(0.25), 120, Duration::ZERO)),
            Some(750)
        );
        // At half speed a second lasts 2 s, so a pixel 0.4 s away still leaves a full second.
        let half = track(true, 0.5, 0, 30 * SECOND, 0);
        assert_eq!(
            millis(half.next_tick(at(0.1), 120, Duration::ZERO)),
            Some(400)
        );
        // Exactly on a boundary, the next one is a full step away.
        assert_eq!(
            millis(even.next_tick(at(0.0), 120, Duration::ZERO)),
            Some(1000)
        );
        // Twice the rate halves the wait; a slow rate stretches it.
        let fast = track(true, 2.0, 0, 120 * SECOND, 20 * SECOND);
        assert_eq!(
            millis(fast.next_tick(at(0.25), 120, Duration::ZERO)),
            Some(250)
        );
        let slow = track(true, 0.5, 0, 120 * SECOND, 20 * SECOND);
        assert_eq!(
            millis(slow.next_tick(at(0.0), 120, Duration::ZERO)),
            Some(2000)
        );
        // The end is the last change; after it nothing moves.
        let ending = track(true, 1.0, 0, 20 * SECOND + SECOND / 2, 20 * SECOND);
        assert_eq!(
            millis(ending.next_tick(at(0.0), 1, Duration::ZERO)),
            Some(500)
        );
        assert_eq!(ending.next_tick(at(0.5), 1, Duration::ZERO), None);
        assert_eq!(ending.next_tick(at(30.0), 120, Duration::ZERO), None);
        // Only playback forward with a LastUpdatedTime advances.
        assert_eq!(
            track(false, 1.0, 0, 120 * SECOND, 0).next_tick(at(0.0), 120, Duration::ZERO),
            None
        );
        assert_eq!(
            track(true, 0.0, 0, 120 * SECOND, 0).next_tick(at(0.0), 120, Duration::ZERO),
            None
        );
        assert_eq!(
            track(true, -1.0, 0, 120 * SECOND, 60).next_tick(at(0.0), 120, Duration::ZERO),
            None
        );
        let mut unstamped = even.clone();
        unstamped.timeline.updated = 0;
        assert_eq!(unstamped.next_tick(at(0.0), 120, Duration::ZERO), None);
        // A zero-width bar still changes on each second.
        assert_eq!(
            millis(long.next_tick(at(0.25), 0, Duration::ZERO)),
            Some(750)
        );
    }

    #[test]
    fn the_floor_defers_pixels_and_fast_rates_but_never_a_second() {
        let at = |seconds: f64| 1000 * SECOND + (seconds * SECOND as f64) as i64;
        let millis = |duration: Option<Duration>| duration.map(|d| d.as_millis());
        let floor = Duration::from_secs(1);
        // 0:30 on 120 px: the second at 1 s never waits for the floor.
        let short = track(true, 1.0, 0, 30 * SECOND, 0);
        assert_eq!(millis(short.next_tick(at(0.1), 120, floor)), Some(900));
        // At half speed the pixel waits for the floor, and then the second is too close: it wins.
        let half = track(true, 0.5, 0, 30 * SECOND, 0);
        assert_eq!(millis(half.next_tick(at(0.1), 120, floor / 2)), Some(500));
        assert_eq!(millis(half.next_tick(at(0.1), 120, floor)), Some(1900));
        // At twice real time every change waits for the floor.
        let fast = track(true, 2.0, 0, 120 * SECOND, 20 * SECOND);
        assert_eq!(millis(fast.next_tick(at(0.25), 120, floor)), Some(1000));
    }

    #[test]
    fn late_wakes_never_skip_or_delay_a_shown_second_and_ticks_stay_a_second_apart() {
        // Each tick wakes 15 ms late and anchors the next floor at its scheduled time, as the host
        // does. Slow and fast pixels, at real time and below it.
        let late = 150_000;
        for (length, width, rate) in [
            (1800, 164, 1.0),
            (60, 164, 1.0),
            (60, 328, 1.0),
            (30, 120, 0.9),
            (240, 164, 0.9),
            (60, 164, 0.75),
            (60, 164, 0.5),
        ] {
            let case = format!("{length} s, {width} px, {rate}x");
            let song = track(true, rate, 0, length * SECOND, 0);
            let mut now = 1000 * SECOND + 3 * SECOND / 10;
            let mut anchor = None::<i64>;
            let mut shown = song.progress(now, width).unwrap().elapsed;
            let mut ticks = 0;
            while let Some(wait) = {
                let last = anchor.unwrap_or(now - SECOND);
                let floor = Duration::from_nanos(((last + SECOND - now).max(0) * 100) as u64);
                song.next_tick(now, width, floor)
            } {
                let due = now + (wait.as_nanos() / 100) as i64;
                if let Some(last) = anchor {
                    assert!(due - last >= SECOND, "{case}: ticks {} apart", due - last);
                }
                (anchor, now) = (Some(due), due + late);
                let next = song.progress(now, width).unwrap().elapsed;
                assert!(next - shown <= 1, "{case}: {shown} -> {next}");
                // A whole second is drawn within the wake latency of its boundary.
                let played = ((now - 1000 * SECOND) as f64 * rate) as i64;
                assert!(next == shown || played % SECOND <= late, "{case}");
                (shown, ticks) = (next, ticks + 1);
            }
            assert_eq!(shown, length, "{case}");
            assert!(
                ticks as f64 <= length as f64 / rate + 2.0,
                "{case}: {ticks}"
            );
        }
    }

    #[test]
    fn absurd_lengths_count_as_none() {
        for start in [0, 5 * SECOND, i64::MIN] {
            let live = track(true, 1.0, start, i64::MAX, 20 * SECOND);
            assert_eq!(live.progress(1000 * SECOND, 164), None);
            assert_eq!(live.next_tick(1000 * SECOND, 164, Duration::ZERO), None);
            assert_eq!(live.seek_target(1.0), None);
        }
        // The longest length allowed, a second before its end, on a bar wider than any real one.
        let longest = track(true, 1.0, 0, MAX_LENGTH, MAX_LENGTH - SECOND);
        let wide = 4096;
        assert_eq!(
            longest.progress(1000 * SECOND, wide).unwrap().filled,
            wide - 1
        );
        let wait = longest.next_tick(1000 * SECOND, wide, Duration::ZERO);
        assert!(wait.is_some_and(|wait| wait <= Duration::from_secs(1)));
        assert_eq!(longest.seek_target(1.0), Some(MAX_LENGTH));
    }

    #[test]
    fn an_accepted_seek_shows_its_target_until_the_player_reports_a_newer_timeline() {
        // The read at completion still predates the request: show the target from now.
        let mut stale = track(true, 1.0, 0, 120 * SECOND, 20 * SECOND);
        stale.sought(90 * SECOND, 1001 * SECOND, 1002 * SECOND);
        assert_eq!(
            (stale.timeline.position, stale.timeline.updated),
            (90 * SECOND, 1002 * SECOND)
        );
        assert_eq!(stale.progress(1003 * SECOND, 120).unwrap().elapsed, 91);
        // The player already reported its new position: keep it.
        let mut fresh = track(true, 1.0, 0, 120 * SECOND, 88 * SECOND);
        fresh.timeline.updated = 1001 * SECOND + 1;
        fresh.sought(90 * SECOND, 1001 * SECOND, 1002 * SECOND);
        assert_eq!(fresh.timeline.position, 88 * SECOND);
    }

    #[test]
    fn seek_targets_clamp_to_the_timeline() {
        let offset = track(true, 1.0, 60 * SECOND, 180 * SECOND, 90 * SECOND);
        assert_eq!(offset.seek_target(0.0), Some(60 * SECOND));
        assert_eq!(offset.seek_target(0.25), Some(90 * SECOND));
        assert_eq!(offset.seek_target(1.0), Some(180 * SECOND));
        assert_eq!(offset.seek_target(-0.5), Some(60 * SECOND));
        assert_eq!(offset.seek_target(7.0), Some(180 * SECOND));
    }

    #[test]
    fn seeks_need_a_session_a_shown_track_and_a_player_that_accepts_them() {
        let mut media = Media::create();
        media.state.view = View::Track(Track {
            title: "Song".into(),
            playing: true,
            seekable: true,
            ..track(true, 1.0, 0, 120 * SECOND, 0)
        });
        // No session object: nothing is sent and the command budget stays free.
        media.seek(0.5);
        assert!(media.command.is_none());
        // Opening with no session reads nothing and keeps the timeline.
        media.set_expanded(true);
        assert!(media.expanded);
        assert!(matches!(&media.state.view, View::Track(t) if t.timeline.end == 120 * SECOND));
        media.set_expanded(false);
        assert!(!media.expanded);
    }

    #[test]
    fn only_non_timeline_changes_are_logged() {
        let a = View::Track(track(true, 1.0, 0, 300 * SECOND, 20 * SECOND));
        let b = View::Track(track(true, 1.0, 0, 300 * SECOND, 21 * SECOND));
        assert!(!differs(&a, &b));
        assert!(differs(&a, &View::Track(track(false, 1.0, 0, 0, 0))));
        let mut enabled = a.clone();
        if let View::Track(track) = &mut enabled {
            track.controls[NEXT] = true;
        }
        assert!(differs(&a, &enabled));
        assert!(differs(&a, &View::NoPlayer));
    }

    #[test]
    fn the_notch_repaints_only_when_the_drawn_view_changes() {
        let playing = View::Track(Track {
            title: "Song".into(),
            artist: "Artist".into(),
            ..track(true, 1.0, 0, 300 * SECOND, 20 * SECOND)
        });
        let with = |change: fn(&mut Track)| {
            let mut view = playing.clone();
            if let View::Track(track) = &mut view {
                change(track);
            }
            view
        };
        // Timeline ticks and rate changes are not drawn.
        assert!(!redraw(&playing, &with(|t| t.timeline.position += SECOND)));
        assert!(!redraw(&playing, &with(|t| t.rate = 2.0)));
        assert!(!redraw(&playing, &playing.clone()));
        // Title, artist, play state and session presence are.
        assert!(redraw(&playing, &with(|t| t.title = "Next".into())));
        assert!(redraw(&playing, &with(|t| t.artist.clear())));
        assert!(redraw(&playing, &with(|t| t.playing = false)));
        // So is each control's enabled state.
        assert!(redraw(&playing, &with(|t| t.controls = [true; 3])));
        for control in [PREVIOUS, PLAY_PAUSE, NEXT] {
            let enabled = with(|t| t.controls = [true; 3]);
            let mut disabled = enabled.clone();
            if let View::Track(track) = &mut disabled {
                track.controls[control] = false;
            }
            assert!(redraw(&enabled, &disabled));
        }
        assert!(!redraw(&View::NoPlayer, &View::Track(Track::default())));
        assert!(redraw(&playing, &View::NoPlayer));
        // The no-media views all draw the plain clock notch.
        assert!(!redraw(&View::NoPlayer, &View::Permission));
        assert!(!redraw(&View::Error, &View::NoPlayer));
        assert_eq!(View::Permission.shown(), None);
        assert!(playing.shown().is_some_and(|t| t.title == "Song"));
    }

    #[test]
    fn media_requires_playback_and_metadata_and_transitions_only_repaint_drawn_changes() {
        let empty = View::Track(Track {
            playing: true,
            ..Track::default()
        });
        let paused = View::Track(Track {
            title: "Song".into(),
            ..Track::default()
        });
        let playing = View::Track(Track {
            title: "Song".into(),
            playing: true,
            ..Track::default()
        });
        let artist_only = View::Track(Track {
            artist: "Artist".into(),
            playing: true,
            ..Track::default()
        });
        for view in [&empty, &paused] {
            assert!(view.shown().is_none());
            assert!(!redraw(&View::NoPlayer, view));
        }
        for view in [&playing, &artist_only] {
            assert!(view.shown().is_some());
            assert!(redraw(&paused, view));
            assert!(redraw(view, &paused));
            assert!(redraw(&empty, view));
        }
        assert!(!redraw(&empty, &paused));
    }

    fn playback(status: Status, observed: Instant) -> Change {
        Change::Playback {
            status,
            observed,
            controls: [true; 3],
            seekable: false,
            rate: 1.0,
            timeline: Timeline::default(),
        }
    }

    #[test]
    fn changing_keeps_the_shown_state_and_the_grace_deadline() {
        let now = Instant::now();
        let mut media = Media::create();
        media.state.apply(Change::Session(true));
        media.state.apply(properties(media.generation(), "Song"));
        media.state.apply(playback(Status::Playing, now));
        let before = media.view().clone();
        // Between tracks while playing: no flicker, not even its controls.
        let mut changing = playback(Status::Changing, now);
        if let Change::Playback { controls, .. } = &mut changing {
            *controls = [false; 3];
        }
        media.state.apply(changing);
        assert_eq!(media.view(), &before);
        // While paused within the grace: the single deadline survives.
        media.state.apply(playback(Status::Paused, now));
        let due = Some(now + PAUSED_GRACE);
        assert_eq!(media.deadline(), due);
        media
            .state
            .apply(playback(Status::Changing, now + PAUSED_GRACE / 2));
        assert_eq!(media.deadline(), due);
        assert!(media.view().shown().is_some());
        // Any other non-paused status still ends it.
        media.state.apply(playback(Status::Closed, now));
        assert_eq!((media.deadline(), media.view().shown()), (None, None));
    }

    #[test]
    fn a_resume_long_after_the_grace_expires_it_once_and_reconnect_rereads_everything() {
        let now = Instant::now();
        let mut media = Media::create();
        media.state.apply(Change::Session(true));
        media.state.apply(properties(media.generation(), "Song"));
        media.state.apply(playback(Status::Playing, now));
        media.state.apply(playback(Status::Paused, now));
        // Hours asleep: one expiry and repaint, then nothing more.
        let resumed = now + Duration::from_secs(8 * 3600);
        assert!(media.state.expire(resumed));
        assert!(!media.state.expire(resumed));
        assert_eq!(media.deadline(), None);
        // Reconnect queues the session, playback and properties reads, picked up by the next pass.
        assert!(!media.pending());
        media.reconnect();
        assert!(media.pending());
        assert_eq!(
            media.wake.dirty.load(Ordering::Relaxed),
            SESSION | PROPERTIES | PLAYBACK
        );
        // Without a manager or session, the pass reads nothing and settles.
        assert!(!media.refresh(72));
        assert!(!media.pending());
    }

    #[test]
    fn recent_pause_or_stop_keeps_controls_until_one_deadline_and_resume_clears_it() {
        let now = Instant::now();
        for status in [Status::Paused, Status::Stopped] {
            let mut media = Media::create();
            media.state.apply(Change::Session(true));
            media.state.apply(properties(media.generation(), "Song"));
            media.state.apply(playback(status, now));
            assert!(media.view().shown().is_none()); // Never observed Playing.
            assert_eq!(media.deadline(), None);
            media.state.apply(playback(Status::Playing, now));
            assert!(media.view().shown().is_some());
            assert_eq!(media.deadline(), None);
            media.state.apply(playback(status, now));
            let due = now + PAUSED_GRACE;
            assert_eq!(media.deadline(), Some(due));
            assert!(
                media
                    .view()
                    .shown()
                    .is_some_and(|track| track.controls[PLAY_PAUSE])
            );
            assert!(!media.state.expire(due - Duration::from_nanos(1)));
            media
                .state
                .apply(playback(status, now + Duration::from_secs(1)));
            assert_eq!(media.deadline(), Some(due)); // Repeated reports never postpone expiry.
            media.state.apply(playback(Status::Playing, now));
            assert_eq!(media.deadline(), None);
            assert!(!media.state.expire(due));
            assert!(media.view().shown().is_some());
            media.state.apply(playback(status, now));
            assert!(media.state.expire(due));
            assert_eq!(media.deadline(), None);
            assert!(media.view().shown().is_none());
            assert!(!media.state.expire(due)); // Expiry repaints only once.
            media.state.apply(playback(status, due));
            assert_eq!(media.deadline(), None); // A late pause cannot resurrect the pill.
        }
    }

    #[test]
    fn grace_never_shows_empty_metadata_and_does_not_survive_session_or_status_changes() {
        let now = Instant::now();
        let mut media = Media::create();
        media.state.apply(Change::Session(true));
        media.state.apply(playback(Status::Playing, now));
        assert!(media.view().shown().is_none());
        media.state.apply(playback(Status::Paused, now));
        assert!(media.view().shown().is_none());
        assert_eq!(media.deadline(), None); // No expiry wake for an empty pill.
        media.state.apply(properties(media.generation(), "Song"));
        assert!(media.view().shown().is_some());
        assert_eq!(media.deadline(), Some(now + PAUSED_GRACE));
        media.state.apply(playback(Status::Closed, now));
        assert!(media.view().shown().is_none());
        assert_eq!(media.deadline(), None);
        media.state.apply(playback(Status::Playing, now));
        media.state.apply(playback(Status::Paused, now));
        media.state.apply(Change::Session(false));
        assert_eq!(media.deadline(), None);
        media.state.apply(Change::Session(true));
        media.state.apply(properties(media.generation(), "New"));
        media.state.apply(playback(Status::Paused, now));
        assert!(media.view().shown().is_none());
        assert_eq!(media.deadline(), None);
        // The gate itself rejects elapsed grace before the host consumes its wake.
        media.state.view = View::Track(Track {
            title: "Song".into(),
            paused_until: Some(now - Duration::from_secs(1)),
            ..Track::default()
        });
        assert!(media.view().shown().is_none());
        assert!(media.refresh(72));
        assert_eq!(media.deadline(), None);
        assert!(!media.refresh(72));
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

    /// A command that finishes only when `release` is sent, and ignores `Cancel`.
    fn hung(admitted: &Cell<u32>) -> (std::sync::mpsc::Sender<()>, IAsyncOperation<bool>) {
        admitted.set(admitted.get() + 1);
        let (release, held) = std::sync::mpsc::channel::<()>();
        let operation = IAsyncOperation::<bool>::spawn(move || {
            let _ = held.recv();
            Ok(true)
        });
        (release, operation)
    }

    #[test]
    fn a_hung_command_blocks_its_own_session_only_and_orphans_are_bounded() {
        let mut media = Media::create();
        let admitted = Cell::new(0);
        let mut releases = Vec::new();
        // One press at a time within a session, however often it is repeated.
        let (release, operation) = hung(&admitted);
        releases.push(release);
        let mut spare = Some(operation);
        for _ in 0..8 {
            let start = || {
                Ok(spare
                    .take()
                    .unwrap_or_else(|| IAsyncOperation::ready(Ok(true))))
            };
            let generation = media.generation();
            admit_command(
                &mut media.command,
                &mut media.orphans,
                generation,
                &media.wake,
                start,
            )
            .unwrap();
        }
        assert!(media.command.is_some() && media.orphans.is_empty());
        // The player hangs and dies; its session is replaced: later sessions still get commands.
        for round in 1..ORPHANS {
            media.end_session();
            media.state.apply(Change::Session(true));
            assert_eq!(media.orphans.len(), round);
            let (release, operation) = hung(&admitted);
            releases.push(release);
            let generation = media.generation();
            admit_command(
                &mut media.command,
                &mut media.orphans,
                generation,
                &media.wake,
                || Ok(operation),
            )
            .unwrap();
            assert!(media.command.is_some(), "round {round}");
        }
        // At the bound, the next replacement leaves new commands waiting rather than piling up.
        media.end_session();
        assert_eq!(media.orphans.len(), ORPHANS);
        admit_command(
            &mut media.command,
            &mut media.orphans,
            9,
            &media.wake,
            || {
                admitted.set(admitted.get() + 1);
                Ok(IAsyncOperation::ready(Ok(true)))
            },
        )
        .unwrap();
        assert!(media.command.is_none());
        assert_eq!(admitted.get(), ORPHANS as u32);
        // One orphan finishing frees one place.
        let first = media.orphans[0].clone();
        releases.remove(0).send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while running(&first) {
            assert!(
                std::time::Instant::now() < deadline,
                "the orphan never completed"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        admit_command(
            &mut media.command,
            &mut media.orphans,
            9,
            &media.wake,
            || {
                admitted.set(admitted.get() + 1);
                Ok(IAsyncOperation::ready(Ok(true)))
            },
        )
        .unwrap();
        assert!(media.command.is_some());
        assert_eq!(media.orphans.len(), ORPHANS - 1);
        // A seek that belonged to the ended session never carries over.
        media.seek = Some((1, 2));
        media.end_session();
        assert_eq!(media.seek, None);
    }

    #[test]
    fn media_access_failures_show_one_status_line_and_nothing_else_does() {
        assert_eq!(
            View::Permission.status(),
            Some("Windows blocked media access")
        );
        assert_eq!(View::Error.status(), Some("Media controls unavailable"));
        assert_eq!(View::NoPlayer.status(), None);
        assert_eq!(View::Track(Track::default()).status(), None);
    }

    #[test]
    fn album_art_skipped_without_a_notch_is_read_when_one_returns() {
        let mut media = Media::create();
        media.art_skipped = true;
        // Still no notch: the flag stays and nothing is queued.
        media.refresh(0);
        assert!(media.art_skipped);
        assert!(!media.pending());
        // A notch exists: the properties are read again, once.
        media.refresh(72);
        assert!(!media.art_skipped);
        assert!(!media.pending());
    }

    #[test]
    fn host_passes_without_a_media_event_are_not_refreshes() {
        let mut media = Media::create();
        for _ in 0..1000 {
            assert!(!media.refresh(72));
        }
        assert_eq!((media.refreshes, media.wakes), (0, 0));
        // A wake is one refresh, and the next idle passes add none.
        media.reconnect();
        media.refresh(72);
        for _ in 0..1000 {
            media.refresh(72);
        }
        assert_eq!((media.refreshes, media.wakes), (1, 1));
    }

    fn stream() -> IRandomAccessStream {
        InMemoryRandomAccessStream::new().unwrap().into()
    }
    fn thumbnail() -> Result<IRandomAccessStreamReference> {
        RandomAccessStreamReference::CreateFromStream(&stream())?.cast()
    }
    /// The next art decode handed to the worker queue, with a reference that resolves.
    fn art_job(incoming: &Receiver<Job>) -> (u32, Box<dyn FnOnce(IconPixels) + Send>) {
        match incoming.recv_timeout(Duration::from_secs(10)) {
            Ok(Job::Art { stream, side, done }) => {
                assert!(stream.resolve().is_ok());
                (side, done)
            }
            _ => panic!("no album art was handed off"),
        }
    }
    fn pixels() -> IconPixels {
        IconPixels {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
        }
    }

    #[test]
    fn album_art_is_one_entry_per_track_and_a_failed_track_is_never_read_again() {
        let (jobs, incoming) = sync_channel(4);
        let mut media = Media::create();
        media.jobs = Some(jobs);
        media.state.apply(Change::Session(true));
        let generation = media.generation();
        media.state.apply(properties(generation, "One"));
        media.state.apply(playback(Status::Playing, Instant::now()));
        let key = |title: &str| (generation, title.to_string(), "Artist".to_string());
        let reads = Cell::new(0);
        let missing = || {
            reads.set(reads.get() + 1);
            Err(E_FAIL.into())
        };
        // No thumbnail: the no-art layout, and the same track is not read again.
        media.read_art(key("One"), missing, 72);
        media.read_art(key("One"), missing, 72);
        assert_eq!(reads.get(), 1);
        assert!(media.art().is_none());
        // A new track's stream reaches the worker for a decode at the drawn side.
        media.read_art(key("Two"), thumbnail, 108);
        let (side, stale) = art_job(&incoming);
        assert_eq!(side, 108);
        // The track changes before that decode lands: its result is discarded.
        media.read_art(key("Three"), thumbnail, 108);
        let (_, current) = art_job(&incoming);
        stale(pixels());
        assert!(!media.refresh(72));
        assert!(media.art().is_none());
        // The finished read, which holds the opened stream, is released on the next wake.
        assert!(media.art.read.is_none());
        // The current track's art repaints once when it arrives.
        current(pixels());
        assert!(media.refresh(72));
        let entry = media.art().map(|(entry, _)| entry);
        assert!(entry.is_some());
        assert!(!media.refresh(72));
        media.read_art(key("Three"), missing, 72);
        assert_eq!(reads.get(), 1);
        // The same entry keeps its number, so its uploaded bitmap is reused.
        assert_eq!(media.art().map(|(entry, _)| entry), entry);
        // Ending the session drops the entry.
        media.end_session();
        assert!(media.art().is_none());
    }

    #[test]
    fn album_art_is_skipped_without_a_reference_or_queue_room() {
        let (jobs, incoming) = sync_channel(1);
        let hand = |opened| hand_off(opened, &jobs, 72, Box::new(|_| {}));
        hand(Err(E_FAIL.into()));
        assert!(incoming.try_recv().is_err());
        let stream = stream();
        hand(AgileReference::new(&stream));
        hand(AgileReference::new(&stream));
        assert_eq!(incoming.try_iter().count(), 1);
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
