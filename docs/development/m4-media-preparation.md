# M4 preparation: media notch vertical slice

Prepared 2026-11-10 by a Claude Opus 5.5 High worker thread launched by the ROQA Chief Thread. Revision 2 is based on `402941c`. It records the user's decisions of 2026-11-10: always animate, a visualizer only while expanded and visible, and a static playing glyph when collapsed. This is preparation only: no production, script or Cargo edits, no build, no native or GUI run, and no runtime results. The council, accessibility checks, native suites and idle measurement remain **NOT RUN / deferred**, as recorded in `progress.md`. None of them is a passed gate.

API facts below were checked against the pinned crate source (`windows` 0.62.2 and `windows-future` 0.3.2 under `.tools/cargo/registry`) and against Microsoft Learn pages fetched on 2026-11-10. Anything not checked is marked **UNVERIFIED**.

## Current coupling

- **Notch paint.** `Surface::paint_notch(time, date)` (`src/host.rs`) builds a `NotchScene { time, date, progress, dpi }` and paints the shadow, then the body (`Canvas::paint_notch`, `src/graphics.rs`). It has four callers:
  - `Desktop::refresh_clock`, for visible notches. Since `402941c` it locale-formats only once `clock_due` (the next-minute deadline) passes, and paints only when the text changed. `REPOSITION`, which `WM_TIMECHANGE` now also posts, resets `clock_due`;
  - `Desktop::animate`, only while `surface.live`;
  - `Desktop::refresh_fullscreen`, before a hidden surface is restored;
  - `Desktop::place`, for the first paint before show.

  Media content plugs in here as one more borrowed field on `NotchScene`. A `refresh_media` that mirrors `refresh_clock` keeps its own `media_due` deadline, does work only once that deadline passes or on a media wake, and repaints visible notches only when the *drawn* media view changes. All four call sites must pass the media view.
- **Animation.** `Tween` (`src/motion.rs`) drives open and close. `live` is cleared when motion settles. `animate()` returns true while any surface runs, and only then does the loop use `next_frame` (compositor clock, ≤33 ms, falling back to ≤16 ms). Media must never set `live` by itself. A state change is one paint, not a frame loop.
- **Wait handles and timeouts.** `event_loop` waits on the six IPC command events (`Controls::handles`) plus `QS_ALLINPUT`. Its timeout is the minimum of the catalog, `clock_due`, topology and fullscreen-poll deadlines, all of which round up via `catalog_timing::milliseconds_until`. Media adds one `media_wait()` deadline (expanded progress tick, visualizer sample or retry) to that minimum, and `INFINITE` when no tick is due. Media adds no new wait handle.
- **Cross-thread delivery.** There are three existing patterns:
  - `text_settings.rs`: a WinRT `TypedEventHandler` sets an `AtomicBool` and calls `PostThreadMessageW(ui_thread, WM_APP+30)`. The loop wakes, and `take_changed()` is read on the host thread.
  - `icons.rs`: an STA worker, a bounded `sync_channel`, and `ICON_READY` wakes.
  - `window_catalog.rs`: an MTA worker.

  Media reuses the first pattern: handlers only set dirty bits and post one scalar wake (for example `WM_APP+50`). All GSMTC reads and mutations happen on the host thread.

  A lost thread message is recovered at the next finite deadline, which the clock bounds to 60 s or less. That is the same accepted limit as text settings.
- **Apartment.** The host thread is an STA (`CoInitializeEx(COINIT_APARTMENTTHREADED)`), and `TextSettings` also holds `RoInitialize(RO_INIT_SINGLETHREADED)`. Its `Runtime` guard is private to `text_settings.rs`. Media must be dropped before that guard and before `Apartment`. One fix is to make `Runtime` `pub(crate)` and give media its own balanced guard. Another is to order media ahead of `text` in `Desktop`.
- **Visible state.** `Surface::visible` (fullscreen suppression) and `motion.value()` (0 collapsed to 1 expanded) already exist. Gating media work needs only these two values plus "is playing".

## Verified platform contracts

| Area | Contract | Source |
|---|---|---|
| Manager | `GlobalSystemMediaTransportControlsSessionManager::RequestAsync() -> IAsyncOperation<Manager>`; `GetCurrentSession`, `GetSessions` (IVectorView, `windows_collections`, no extra feature gate); `CurrentSessionChanged` and `SessionsChanged` return an `i64` token, with matching `Remove*` | crate `Windows/Media/Control/mod.rs` |
| Manager docs | Windows 10 1809 (10.0.17763), UniversalApiContract v7. The class is `MarshalingBehavior(Agile)` and `ThreadingModel.Both`. The page lists the **`globalMediaControl` app capability** | Microsoft Learn, fetched 2026-11-10 |
| Session | `TryGetMediaPropertiesAsync` (Title, Artist, `Thumbnail`); `GetPlaybackInfo` (`PlaybackStatus`, `Controls.Is{Play,Pause,Next,Previous,…}Enabled`, `PlaybackRate`); `GetTimelineProperties` (`StartTime`, `EndTime`, `Position`, `LastUpdatedTime`); `SourceAppUserModelId`; `{MediaProperties,PlaybackInfo,TimelineProperties}Changed` + `Remove*(i64)` | crate source |
| Commands | `TryPlayAsync`, `TryPauseAsync`, `TryTogglePlayPauseAsync`, `TrySkipNextAsync`, `TrySkipPreviousAsync`, `TryChangePlaybackPositionAsync`, each returning `IAsyncOperation<bool>` | crate source |
| Timeline | "Position: the playback position, current as of LastUpdatedTime." The page gives no extrapolation guidance; ROQA derives the shown position itself | Microsoft Learn |
| Thumbnail | `MediaProperties::Thumbnail -> IRandomAccessStreamReference`, gated on `Storage_Streams`; `OpenReadAsync`; `IRandomAccessStream::Size() -> u64`. The docs state no format or size | crate source + Learn |
| Async | `IAsyncOperation::when(FnOnce(Result<T>) + Send + 'static)` and `.join()` are inherent methods. `join` blocks, so it is forbidden on the host thread. `IAsyncInfo::Cancel` exists | `windows-future` 0.3.2 `when.rs`, `join.rs`, `bindings.rs` |
| Agility | Every GSMTC type has `unsafe impl Send + Sync` in the crate | crate source |
| WIC | `CLSID_WICImagingFactory`, `CreateDecoderFromStream`, `CreateBitmapScaler`, `CreateFormatConverter`, `GUID_WICPixelFormat32bppPBGRA`; `CreateStreamOverRandomAccessStream` (`Win32_System_WinRT`, already enabled) bridges the WinRT stream to `IStream` | crate source |
| Core Audio | `IMMDeviceEnumerator::{GetDefaultAudioEndpoint, Register/UnregisterEndpointNotificationCallback}`; `IMMNotificationClient_Impl` (device added/removed/state/default/property); `IMMDevice::Activate<T>` (needs `Win32_System_Com_StructuredStorage` + `Win32_System_Variant`, both enabled); `IAudioEndpointVolume::{GetMasterVolumeLevelScalar, GetMute, Register/UnregisterControlChangeNotify}`; `IAudioEndpointVolumeCallback_Impl::OnNotify` | crate source |
| Level meter | `IAudioMeterInformation::{GetPeakValue, GetMeteringChannelCount, GetChannelsPeakValues}` (`Win32_Media_Audio_Endpoints`), obtained via `IMMDevice::Activate`. Per the docs it meters the endpoint's render stream before endpoint volume, peaks are "recorded over one device period", 0.0–1.0, and a software meter reports 0.0 in exclusive mode. It has no event or callback; the Microsoft sample polls it on a 125 ms timer | crate source + Learn (`IAudioMeterInformation`, `GetPeakValue`, "Peak Meters") |
| Core Audio rules | IMMNotificationClient methods "must be nonblocking" and never wait. Never call Register/Unregister inside a callback. Never release the final MMDevice reference in a callback. Keep the client alive until Unregister, then release | Microsoft Learn |

**Features to add in the slice that first needs them, never ahead of it:**

- `Media_Control` (implies `Media`) and `Storage_Streams` (implies `Storage`), in slice 1;
- `Win32_Graphics_Imaging`, in slice 5;
- `Win32_Media_Audio` and `Win32_Media_Audio_Endpoints`, in slice 6. The visualizer in slice 7 needs nothing more.

No new crates. `windows-future` and `windows-collections` are already locked.

### Threading, marshalling and teardown

- **Event handlers and `when` completions.** These may run on any thread, because the objects are agile; which thread a given one uses is UNVERIFIED. Their bodies:
  - only set atomic dirty bits, or `try_send` a result into a capacity-1 channel tagged with the session generation;
  - then call `PostThreadMessageW`;
  - never touch `Desktop`, HWNDs or Direct2D, never block, never call `join`, and never panic. `windows-core` 0.62.2 has no `catch_unwind` in delegate glue, so a panic would reach the FFI boundary.
- **Host-thread work.** The host thread reads snapshots (`GetPlaybackInfo`, `GetTimelineProperties`) on wake and issues `Try*Async` + `when`. Command results are only logged. State comes from the change events.
- **Session replacement.** On `CurrentSessionChanged`, the host thread:
  1. calls the three `Remove*` with the old session's tokens;
  2. calls `Cancel()` on any pending `TryGetMediaPropertiesAsync` or `OpenReadAsync`;
  3. bumps the generation, so late completions are discarded;
  4. drops the old session;
  5. subscribes to the new one.

  Never do this inside the handler.
- **Shutdown order.** Session tokens, then manager tokens, then the Core Audio `Unregister*` calls, then release the callbacks. All of it happens before the RoInitialize and Apartment guards drop. A failure to remove a token is logged, as `TextSettings::drop` does, and never panics.
- **Threads.** ROQA creates no media thread. Album-art decode reuses the existing `roqa-icons` STA worker as a second request kind.
- **Stream handoff.** The stream that `OpenReadAsync` returns (`IRandomAccessStreamWithContentType`) is not `Send` in the pinned crate. Only the GSMTC classes and `RandomAccessStreamReference` are, and GSMTC agility proves nothing about the stream. So the `when` completion wraps the stream in `windows_core::AgileReference::new`. That type is `Send + Sync` in windows-core 0.62.2 (`agile_reference.rs`). The completion then `try_send`s it to the icon worker, and the worker calls `resolve()` in its own STA before WIC decode.
  - If creating or resolving the reference fails, or the queue is full, the art is skipped: the no-art layout shows, and that key is not retried.
  - The worker drops the resolved stream and the reference before replying. Late results are discarded by generation.
  - Whether this marshalled read works for real players is UNVERIFIED. WinRT, RPC and Core Audio may use their own system pool threads; that count is UNVERIFIED and must be measured.

## Footprint-first design

1. **No continuous render loop.** Media state changes cause one paint per visible notch, and only when the drawn view changes (same pattern as `refresh_clock`). No media work ever sets `live` or uses the compositor-clock frame path.
2. **Collapsed look: static playing glyph plus static title (user decision, 2026-11-10; option A, title added 2026-11-10).** Media gives the collapsed notch no tick and no deadline, and adds zero collapsed repaints of its own. Play, pause and track changes can trigger a repaint, but only when the drawn glyph or title actually changes.

   The accepted pill still draws the clock. The existing `refresh_clock` (`src/host.rs`) repaints every visible notch once per minute when the time text changes; this is pre-existing, accepted behaviour, and the clock really is drawn while collapsed. Static collapsed playback therefore paints exactly once per minute, for the clock, and never for media. That is not a continuous render loop.

   If the user wants literally zero collapsed paints, the clock must leave the pill. That would be a design change, not part of this plan. The progress ring (option B) is dropped. The collapsed notch shows the track title (user decision, 2026-11-10), which needs a wider pill than the accepted 150 × 30. The width and truncation rule go to the slice-2 media design addendum for separate review. Default for that addendum: static end-ellipsis, no marquee or scrolling, because a marquee needs a continuous repaint and breaks the zero-collapsed-repaint rule. The collapsed title repaints only when the track or the play state changes.
3. **Expanded progress gate.** Progress ticks run only while the notch is `visible && expanded && Playing && rate > 0`. Hidden, fullscreen-suppressed, collapsed, paused or no-player states clear the deadline. While animating, the existing frame loop already repaints, and no extra tick is added.
4. **Coarse cadence.** The position is `Position + (now − LastUpdatedTime) × PlaybackRate`, clamped to `[StartTime, EndTime]`. The next deadline is the moment the shown value changes: the next whole second of `m:ss`, or the next physical pixel of the bar, whichever is earlier. It is bounded to at least 1 s and rounded up with `milliseconds_until`. The clock already formats only on minute deadlines (`402941c`), so media ticks add no locale formatting.
5. **Album-art cache.** One decoded entry, for the current track only, as premultiplied BGRA (`IconPixels`), keyed by session generation plus title and artist. Decode is capped at the drawn size in physical pixels, with a hard ceiling of 256 × 256 (≤256 KiB). Streams over 4 MiB (`Size()`) are rejected before decoding. A failure falls back to a no-art layout and is never retried for the same key.
6. **Visualizer: only while expanded and visible (user decision, 2026-11-10).**
   - **Gate.** It runs only while `visible && expanded && Playing`. When the notch collapses, is hidden, is fullscreen-suppressed or pauses, or when there is no player or device, the deadline is cleared and no meter call is made.
   - **Data source.** `IAudioMeterInformation` on the default render endpoint, already bound by slice 6. This measures the **whole endpoint**, all apps mixed, before volume. It is not a per-player level and not a spectrum; there is no FFT and no audio capture. It reads 0.0 for exclusive-mode streams without a hardware meter.
   - **Rate.** A host-loop deadline of at most 15 Hz (67 ms), not the compositor clock. That is more often than the Microsoft sample's 125 ms poll. Each sample is one `GetPeakValue` call, which returns the peak across all of the stream's channels. That makes it valid for mono, stereo and multichannel endpoints alike. `GetChannelsPeakValues` is not used: its count must equal the endpoint's metering channel count, and the pinned binding passes the slice length as that count. The bars come from a fixed array of the last few samples, with no allocation. It paints only when a bar's height, quantized to whole physical pixels, changes. So silence or steady levels cost samples but no paints.
   - **Bound.** At most 15 meter calls and at most 15 paints per second, only while expanded. That is about 900 per minute of continuous hover, and zero otherwise. The per-call CPU cost is UNVERIFIED.
7. **Volume.** Event-driven only, with no polling. It re-reads the level on `OnNotify` and rebinds the endpoint on `OnDefaultDeviceChanged` (eRender/eConsole), outside the callback. Volume is user-settable (user decision, 2026-11-10): setting it uses `IAudioEndpointVolume::SetMasterVolumeLevelScalar` on the default render endpoint, UNVERIFIED until slice 6 verifies it against the pinned crate. Where and how it appears goes to the design addendum; default: in the expanded panel.

**Added footprint: estimates to be measured, not results.**

| Item | Estimate |
|---|---|
| Private bytes, GSMTC (WinRT proxies, `Windows.Media.Control` load) | about 1–4 MiB |
| Private bytes, WIC (when first loaded) | about 0.5–2 MiB |
| Private bytes, art buffer | ≤0.25 MiB |
| Private bytes, Core Audio | about 0.5–2 MiB |
| Kernel handles | about 10–40 |
| System pool threads | about 1–4, not created by ROQA |
| Idle CPU, collapsed playback | 0 media paints by design; the existing clock paint, once per minute, remains |
| CPU while expanded and playing | ≤15 meter samples/s plus progress ticks; unmeasured |

None of these numbers has been measured. They are a planning guess for M5 comparison only.

## Bounded implementation sequence

Each slice is one candidate with its own SHA-256 manifest, `check.ps1`/`verify.ps1` run, and a **fresh independent non-author clean-code-guard review**. Code follows `/ponytail:ponytail`: the smallest complete change, with no player abstraction, trait or plugin layer.

1. **Media state and subscription, no UI.**
   - **Build.** A pure `media::View` reducer from snapshots, plus a host-thread GSMTC owner with tokens, generation, wake and teardown. `RequestAsync` runs via `when`. The states are no-player, permission (access denied or capability), error and playing/paused.
   - **Tests.** The reducer, covering: session replacement, stale-generation discard, no-player, error and permission, the timeline extrapolation clamp, paused and rate 0, a missing `EndTime`, and token removal ordering through a fake owner.
   - **Native evidence.** The diagnostics log shows real session and track changes, and whether `RequestAsync` succeeds unpackaged.
   - **Non-goals.** Drawing, art, volume and controls.
2. **Media design addendum, then a static paint.**
   - **Build.** A short design addendum to `roqa-visual-spec.md` for media content only, reviewed separately. It does not redo the accepted notch. Then the play-state glyph in the collapsed scene, and title, artist and play state in the expanded scene, repainted only on view change.
   - **Tests.** Text layout overflow and RTL titles via the existing DirectWrite path, and a scene snapshot from the view.
   - **Collapsed look.** The static playing glyph only, inside the accepted 150 × 30 pill geometry, with no title.
   - **Native evidence.** Screen capture of collapsed and expanded playback. Paint counts while collapsed and static must span at least one minute boundary, and show only the clock's single minute repaint and none from media.
   - **Non-goals.** Progress, art and controls.
3. **Controls and accessible actions.**
   - **Build.** Play/pause, previous and next chips, disabled per `Is*Enabled`. Pointer actions on the existing nonactivating notch. A notch UIA provider with Invoke, following the `dock_uia` pattern. Keyboard route: Tab and arrow keys into the focused notch's controls through UIA (user decision, 2026-11-10).
   - **Tests.** Hit-testing and the enabled-state mapping.
   - **Native evidence.** Real player play/pause/skip, Narrator invoke and focus return.
   - **Non-goals.** Seek and volume.
4. **Progress and seek.**
   - **Build.** The gated, coarse progress deadline, for the expanded panel only. `TryChangePlaybackPositionAsync` from a click or tap on the bar (user decision, 2026-11-10).
   - **Tests.** The deadline math, gating (hidden, collapsed, paused, fullscreen) and clamping.
   - **Native evidence.** Wake counts while expanded and playing, compared with collapsed. Seek against a real player.
   - **Non-goals.** A visualizer.
5. **Album art.**
   - **Build.** `OpenReadAsync` + `when`, then the stream wrapped in `AgileReference` and handed to the icon worker, which resolves it, then a WIC decode with scaler and PBGRA conversion under the caps, then the single cache entry.
   - **Tests.** Cap and reject logic, the cache key, stale-result discard, and the skip path when the reference cannot be created or resolved.
   - **Native evidence.** Art for two players, a huge or invalid thumbnail, and private bytes before and after track churn.
   - **Non-goals.** Multi-entry or disk caching, and blur or colour extraction.
6. **Volume and device events.**
   - **Build.** One `IMMNotificationClient` and one `IAudioEndpointVolumeCallback` via `#[implement]`, rebinding on the host thread, plus a display per the design addendum.
   - **Tests.** The rebind decision logic.
   - **Native evidence.** Default-device switch, unplug, mute and volume change.
   - **Non-goals.** Setting the volume, unless the user asks; per-app sessions.
7. **Visualizer.**
   - **Build.** Activate `IAudioMeterInformation` with the slice 6 endpoint and rebind it with that endpoint. A ≤15 Hz deadline gated on visible, expanded and playing. A fixed-size peak array, and quantized bar heights.
   - **Tests.** Gating: no deadline when collapsed, hidden, fullscreen, paused or without a device. The quantize-and-skip-unchanged logic, and the rate bound.
   - **Device coverage.** `GetPeakValue` on mono, stereo and multichannel endpoints in the native run.
   - **Native evidence.** Meter-call and paint counts expanded versus collapsed (collapsed must be 0), CPU while expanded and playing, and behaviour on a device switch.
   - **Non-goals.** FFT or spectrum, loopback capture, per-app meters, and any collapsed visualizer.
8. **Hardening pass.** App exit or hang mid-session, sleep and resume, Explorer restart, and lost-wake recovery. Fixes only, with no new features.

## Accessibility and UX constraints

- **Actions.** Every control is reachable by Narrator (UIA Invoke, with names that include state, such as "Pause, Song – Artist") and by keyboard. The notch stays nonactivating for pointer use. A keyboard or UIA entry that takes focus must return it to the previous foreground window on close or Escape.
- **Text.** Text scale (already multiplied into the notch DPI), RTL and bidi titles via DirectWrite, and contrast tokens per the visual spec. Disabled controls are exposed as disabled, not hidden.
- **States.** No-player, permission and error states are text, not blank.
- **Motion: always animate (user decision, 2026-11-10).** The notch and media panel animate even when Windows animation effects are turned off for reduced motion, matching the 2026-11-09 notch decision. PLAN.md's M4 acceptance line is amended in this candidate to record this. Its remaining intent stays: focus return and accessible actions must work while animating. Actions must never wait for an animation to finish, and UIA must expose the target state at once. This is still a known accessibility regression against the original brief.

## Evidence contract

These are the native runs needed from PLAN M4. Every one is **NOT RUN**. Each needs fresh, desktop-free user confirmation and must leave the taskbar and settings unchanged.

- Track change and player change, including session replacement with two players.
- Seek, play and pause, from both pointer and keyboard or Narrator.
- App disappearance, and a hung app (the host must stay responsive, and shutdown must not hang).
- Sleep and resume.
- Audio-device change: default switch and unplug.
- With Windows animation effects off: the panel still animates, and focus return and accessible actions still work mid-animation.
- Focus return and accessible actions.
- No continuous render loop during static collapsed playback: wake and paint counts must span at least one minute boundary, and show only the existing per-minute clock repaint and zero media paints while collapsed and static.
- The visualizer does zero meter calls and zero paints while collapsed, hidden or fullscreen-suppressed, and stays within ≤15 Hz while expanded.
- A real performance trace (CPU, private bytes, handles and threads), then independent council review.

Deterministic tests do not prove any of these. Council, accessibility checks, native suites and idle measurement stay deferred.

## UNVERIFIED

1. Whether an **unpackaged** desktop app can use GSMTC without the `globalMediaControl` capability that the docs list. Slice 1 must prove this and show a permission state if it fails.
2. Which thread runs GSMTC event handlers and `when` completions when the objects are created on an STA.
3. Whether the synchronous getters (`GetPlaybackInfo`, `GetTimelineProperties`) can block on a hung source app. If they can, move reads off the host thread.
4. Whether reading a thumbnail stream can block, the real thumbnail formats and sizes, and whether the `AgileReference` handoff of the stream to the icon worker resolves and reads correctly for real players.
5. Which threads Core Audio callbacks use, and how many system pool threads GSMTC, RPC and Core Audio add.
6. Whether `PlaybackRate` and `LastUpdatedTime` are populated by common players.
7. That Rust 1.91 aborts, rather than unwinds, on a panic out of an `extern "system"` callback. This is known language behaviour, but it was not re-checked here.
8. The per-call CPU cost of `IAudioMeterInformation::GetPeakValue`, and whether a per-session meter (for a per-player level) is obtainable. The docs fetched say nothing about either.
9. Every footprint estimate above.

## Open questions for the user

1. **Collapsed title or wider pill.** DECIDED (user decision, 2026-11-10): the collapsed notch shows the track title, which needs a wider pill. Width and truncation rule go to the slice-2 media design addendum (default: static end-ellipsis, no marquee). Media controls are confirmed in scope (slice 3, unchanged).
2. **Visualizer source.** DECIDED (user decision, 2026-11-10): the endpoint-wide level, which includes all apps' audio, is acceptable. It still runs only while the panel is expanded, visible and playing. A per-player level is UNVERIFIED and is not planned.
3. **Panel size.** DECIDED (user decision, 2026-11-10): the panel gets taller, about 360 × 120, with a title row, a controls row and a volume row. The exact size and its effect on the fixed 400 × 124 HWND go to the slice-2 media design addendum for separate review. This may grow the HWND, a small footprint cost that the addendum must state.
4. **Keyboard route.** DECIDED (user decision, 2026-11-10): Tab and arrow keys move into the focused notch's controls, exposed through UIA. There is no global hotkey, no `roqa --media` IPC command and no picker entries.
5. **Volume.** DECIDED (user decision, 2026-11-10): user-settable, not display-only. Where and how it appears goes to the design addendum (default: expanded panel).
6. **Seek UI.** DECIDED (user decision, 2026-11-10): click or tap the progress bar to seek. Arrow-key adjustment through RangeValue is not planned.
