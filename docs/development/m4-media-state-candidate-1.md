# M4 media state candidate 1: media state and subscription, no UI

- **Author.** A Claude Opus 5.5 High worker, launched by the ROQA Chief Thread at the user's request ("start M4"). The author approves nothing.
- **Base.** `0b53246` (origin/main), branch `feat/m4-media-state`. Uncommitted.
- **Scope.** `docs/development/m4-media-preparation.md`, "Bounded implementation sequence", item 1. Drawing, art, volume, controls and progress ticks are out of scope. The notch is unchanged.
- **Changed files.**
  - `src/media.rs` (new, 753 lines; tests start at line 531): the reducer, the GSMTC owner and 9 tests.
  - `src/host.rs` (+9): the `Desktop.media` field (declared before `text`), `Media::create()` in the constructor, `media.start()` after the first shows, and `media.refresh()` in `update`.
  - `src/main.rs` (+1): `mod media;`.
  - `Cargo.toml` (+3): feature `Media_Control`; a direct dependency `windows-future = "=0.3.2"`, which was already locked.
  - `Cargo.lock` (+1): the roqa dependency list only; no new package.
- **Final manifest.** `artifacts/m4-media-state-1/round-2/inputs.sha256`, 11 entries, SHA-256 `5F92048240958E927FBBB8A4997AA5FD9279652CAD06E0C64A1ACD02B217F7BE`. The worktree re-checked 11/11 against it after the review.

## Design built

- **Reducer (pure, tested).**
  - `State { generation, view }` is updated by `apply(Change)`.
  - `View` is `NoPlayer | Permission | Error | Track(Track)`. `Track` holds `title`, `artist`, `playing`, `rate` and a `Timeline` (`start`, `end`, `position`, `updated`, all in WinRT 100 ns ticks).
  - `Change::Session(present)` bumps the generation and clears the track.
  - A `Change::Properties` tagged with any other generation is discarded.
  - `Change::Unavailable` gives `Permission` for `E_ACCESSDENIED`, otherwise `Error`.
  - `Track::position(now)` computes `Position + (now − LastUpdatedTime) × rate` only while playing and only with a stamp. Elapsed time is never negative, and the result is clamped to `[start, end]`. An `end` at or before `start` means there is no duration.
- **Owner (`Media`, host thread only).**
  - `create()` is a plain constructor. `start()` calls `RequestAsync` + `when`. `Desktop::create` calls it after both first shows, so the first paints never wait for it.
  - On success it subscribes `CurrentSessionChanged` and `SessionsChanged`. On the current session it subscribes `MediaPropertiesChanged`, `PlaybackInfoChanged` and `TimelinePropertiesChanged`.
  - `refresh()` does all GSMTC reads. With nothing pending it costs one atomic swap.
- **Callbacks.** `Wake::raise` does three things only:
  - stores the caller's thread ID (diagnostics);
  - `fetch_or(Release)`s a dirty bit;
  - posts `WM_APP+50` to the host thread, but only if no bit was already pending.

  Async completions also `OnceLock::set` their own request's slot. There is no `Desktop`, HWND or D2D access, no blocking and no panic path. The host takes the bits with `swap(Acquire)`.
- **Session replacement.** It compares the COM identity of `GetCurrentSession()` with the held session. On a change it:
  1. cancels the in-flight properties read;
  2. drops the old `Subscribed` (tokens removed newest first, then the object released);
  3. subscribes the new session;
  4. applies `Change::Session`, which bumps the generation;
  5. reads properties and playback.

  A null session comes back as the empty error (`code() == S_OK`, verified in windows-core `from_abi`) and means no player.
- **Properties.** Only one read is in flight, tagged with its generation. A change meanwhile sets `again`, which queues exactly one more read when the result lands. Title and artist are each capped at 512 UTF-16 units, and the cut never splits a surrogate pair.
- **Teardown.** `Media::drop` cancels pending reads. Fields then drop in declaration order: `session` (tokens, then release), then `manager` (tokens, then release). `Desktop.media` is declared before `text`, so all of this happens before the `RoUninitialize` in `TextSettings` and before the COM `Apartment` guard in `run`. A removal failure is logged and never panics.
- **Apartment.** It reuses the existing host STA (`CoInitializeEx(COINIT_APARTMENTTHREADED)`) and the `RoInitialize` held by `TextSettings`. No new guard.

## Deviations from the plan and brief

All were judged acceptable by both reviews.

1. **One-shot `Arc<OnceLock<Result<T>>>` per request, not a capacity-1 `sync_channel`.**
   - The semantics are the same: one write, the writer never waits, and a late result from a dropped request is discarded.
   - Measured saving: 10,752 bytes.
   - Because each request has its own slot, a stale result can never fill a shared channel and push out the newest one.
2. **`Storage_Streams` not added.** Only `Thumbnail` (slice 5) needs it.
3. **`windows-future` as a direct dependency.** It is needed to name `IAsyncOperation`, which the owner holds in order to call `Cancel`. It is already locked, and no new crate is added.
4. **`SessionsChanged` subscribed as well as `CurrentSessionChanged`.** Reports differ on whether `CurrentSessionChanged` alone fires when the last player closes. An unrelated session change costs one `GetCurrentSession` call and no replacement.
5. **A read failure on a live session is logged; the view stays as it was.** `Error` means only that the manager could not be obtained, for a reason other than access denied.
6. **Diagnostics print text lengths and integers only.**
   - Titles and app names are never logged.
   - Removing float and `Debug` formatting saved 27,136 bytes, measured.
7. **Status mapping and rate.** Only `Playing` counts as playing; every other status shows as paused while the session exists. A null `PlaybackRate` is taken as 1x (`shortcut:` comment).

## Verify

- `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-media-state-1/verify-r2`:
  - checks PASS: fmt, clippy `-D warnings`, and `cargo test` with **42 passed, 0 failed** (33 before, plus 9 media tests);
  - release build PASS.
- Release `roqa.exe`: **656,384 bytes**, SHA-256 `7520DE68B7622E283599B7B1009466056DF46BDD78F54AB2FB2D7CD2200EA445` (worktree build).
- No native, GUI or smoke run.

## Cost

| Item | Value | Basis |
|---|---|---|
| Release `roqa.exe` | 616,960 → 656,384 bytes, **+39,424 (+6.39%)** | Measured. The base was built at `0b53246` in this worktree, SHA-256 `1C0F132ED5A12C597BF3DB5E76031A56B3F93E0F885B79BFE483B1A20DC23F84`; a copy is in `artifacts/m4-media-state-1/baseline/`. |
| Idle, no player | Zero added timers, deadlines, polls, wait handles or ROQA threads. Only `SessionsChanged`/`CurrentSessionChanged` can wake the loop. | By construction. Not measured. |
| Playing | One wake per GSMTC event burst (the bits coalesce), plus one per completed read. No paint (there is no UI). | By construction. The event rate is unmeasured. |
| Startup | `RequestAsync` loads the GSMTC component on the host thread, after the first paints and before the loop starts. | **Estimate:** a few to tens of ms. Unmeasured. |
| Private bytes | GSMTC WinRT proxies and component load. | **Estimate** from the plan: about 1 to 4 MiB. Unmeasured. |
| System pool threads | ROQA creates none. WinRT/RPC may add some. | UNVERIFIED. |

## Review

Each round was a fresh, independent, read-only Claude Opus 5.5 High clean-code-guard task. No Sol was used, per the user's budget.

- **Round 1: ACCEPT WITH NITS.**
  - Manifest `artifacts/m4-media-state-1/round-1/inputs.sha256` = `4D18C359334DF36FB3E0E10653B4980D8BAD748BBCACEB8C7FD6EB1DDD35BE0E`, matched 9/9 at start and end.
  - Report: `artifacts/m4-media-state-1/round-1/review-opus-1.md` (`6047F362…80CD`).
  - Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Aa0434741-ab0b-4b11-9e51-23d828474967%3Adelegate-task%3A443bd854-1bfb-4df8-9269-a8f00c6440ed>.
- **Round 2: ACCEPT, 0 new findings.**
  - Manifest `artifacts/m4-media-state-1/round-2/inputs.sha256` = `5F92048240958E927FBBB8A4997AA5FD9279652CAD06E0C64A1ACD02B217F7BE`, matched 11/11 at start and end. `check.ps1` re-run by the reviewer: 42 passed.
  - Report: `artifacts/m4-media-state-1/round-2/review-opus-2.md` (`17330820…2D9D`).
  - Link: <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3Aa0434741-ab0b-4b11-9e51-23d828474967%3Adelegate-task%3Aa68926d4-3b73-461e-a929-0c1e657d7dcf>.

| ID | Finding | Disposition (round 2 status) |
|---|---|---|
| M4S1-OPUS-MIN-1 | The dirty bit was Relaxed, so the host could see it before the slot write | Fixed with Release/Acquire (resolved) |
| M4S1-OPUS-MIN-2 | `RequestAsync` ran before the first paint | Fixed: `start()` runs after the first shows (resolved) |
| M4S1-OPUS-MIN-3 | Cancelling on every change could starve the title | Fixed: one read in flight plus `again` (resolved) |
| M4S1-OPUS-NIT-1 | `callback_on_host` is ambiguous | Documented: only `false` is conclusive (resolved) |
| M4S1-OPUS-NIT-2 | A manager-events failure was logged as a `RequestAsync` failure | Fixed (resolved) |
| M4S1-OPUS-NIT-3 | The text cap could split a surrogate pair | Fixed, with a test (resolved) |
| M4S1-OPUS-NIT-4 | An identity `None == None` edge | Fixed: falls back to the raw pointer (resolved) |
| M4S1-OPUS-NIT-5 | The teardown test mirrors `Media`'s field order instead of testing it | Declined: a real test needs a GSMTC fake or trait, which the no-abstraction rule excludes (accepted) |
| M4S1-OPUS-NIT-6 | `bounded` was untested | Fixed: new test (resolved) |

**Known limit (documented).** A properties read that never completes blocks title updates for that session until it is replaced. Hung-player hardening belongs to slice 8.

## UNVERIFIED (updated)

1. **Unpackaged access.** Whether an unpackaged ROQA can use GSMTC without the `globalMediaControl` capability. Slice 1 logs `RequestAsync succeeded` or `failed, <HRESULT>` and shows the permission state on `E_ACCESSDENIED`. Native run needed.
2. **Callback threads.** Which threads run GSMTC handlers and `when` completions. The log's `callback_on_host` is conclusive only when `false`, because an already-complete `when` runs on the host thread.
3. **Blocking getters.** Whether `GetCurrentSession`, `GetPlaybackInfo` or `GetTimelineProperties` can block the host thread on a hung player. Also whether a properties read can never complete (the known limit above).
4. **Session identity.** Whether `GetCurrentSession` returns the same COM identity for an unchanged session. If not, every `SessionsChanged` blanks the title and bumps the generation. Native evidence must show the generation stays put across play/pause and across an unrelated player starting.
5. **Field population.** Whether common players populate `PlaybackRate` and `LastUpdatedTime`.
6. **Event rate.** How often `TimelinePropertiesChanged` and `PlaybackInfoChanged` fire per player, and therefore the number of wakes while playing.
7. **Pool threads.** System pool threads added by WinRT/RPC (and Core Audio, later).
8. **Startup and memory.** The startup time and private bytes `start()` adds. Every footprint figure except binary size is an estimate.
9. **Panic behaviour.** That Rust 1.91 aborts, rather than unwinds, on a panic out of an `extern "system"` callback. Carried from the plan. The callbacks have no panic path, but this was not re-checked.
10. **Later slices.** Thumbnail, `AgileReference`, the Core Audio items and the meter cost remain UNVERIFIED, as listed in the plan.

## Native evidence plan (all NOT RUN)

Every run needs fresh desktop-free confirmation from the user, relayed via the Chief Thread. Before each run:
- confirm no `roqa.exe` or `measure-idle` is running;
- launch the release build with stderr redirected, as `scripts/run.ps1` and the smoke scripts already do;
- stop it with `roqa --quit`, which must exit 0;
- leave the taskbar and settings unchanged.

**Pass criteria:**

1. **Unpackaged access** (UNVERIFIED 1).
   - Start ROQA with no player open.
   - PASS: the log shows `Media: RequestAsync succeeded`, then `Media: no player session=<n>`.
   - If instead it shows `RequestAsync failed, 0x80070005` and `Media: permission`, record that. Packaging is a separate, reviewed decision. Any other failure gives `Media: error`, and there must be no crash or retry loop: exactly one failure line.
2. **Play, pause, track change** (one player, for example a browser tab or the Media Player app).
   - Play, pause, resume, then skip to the next track.
   - PASS:
     - one `playing` line and one `paused` line per action;
     - a track change prints a new line, with `title_utf16`/`artist_utf16` updated once properties land;
     - `session=` stays the same throughout, which covers UNVERIFIED 4;
     - `end_s` and `position_s` are plausible;
     - `rate_milli` is recorded for UNVERIFIED 5.
3. **Player switch and close.**
   - Start a second player and make it current, then close both.
   - PASS:
     - `session=` increments on each real switch;
     - the title comes from the new player and never from a stale one;
     - after the last player closes, `Media: no player`.
     - Also start an unrelated second player without changing the current one: `session=` must not change.
4. **Idle with no player.**
   - Run `scripts/measure-idle.ps1`, 30 s, interleaved against the base build `artifacts/m4-media-state-1/baseline/roqa.exe`.
   - PASS: no added wakes beyond the noise of the base (the wake proxy, plus the cycles comparison within the base range); GDI/USER unchanged; thread count and handles recorded.
   - Private-bytes and thread delta versus base are recorded as measured cost (UNVERIFIED 7 and 8).
   - CPU at or below the measurement floor is reported as inconclusive, not as a pass.
5. **While playing (cost).**
   - A 60 s sample with a player playing and the notch collapsed.
   - Record the media `wakes=` delta per minute from the log, and the wake proxy (UNVERIFIED 6).
   - PASS: no paint caused by media, because no UI changed; wakes are bounded by GSMTC events.
6. **Shutdown.**
   - `roqa --quit` while playing.
   - PASS: exit 0, `Media: stopped wakes=<n>`, no `Media event cleanup:` error lines, and no hang.
7. **Regression.**
   - Re-run `smoke.ps1`, `topology-check.ps1`, `picker-smoke.ps1`, `catalog-smoke.ps1` and `packaged-hung-check.ps1` on this build.
   - PASS: the same counts as M3 native validation 1 (25, 329, 226, 67 and 16).

## Still NOT RUN or deferred (not passed)

- All native evidence above.
- The council review (Sol High and Opus High on an identical manifest).
- Accessibility, Windows 10, DWM/GPU/power, the native M3 cases and the 15-minute trace.
- Hung-player hardening (slice 8).
