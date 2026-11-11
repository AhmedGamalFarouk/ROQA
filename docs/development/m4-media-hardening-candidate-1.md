# M4 media hardening, candidate 1 (slice 8: hardening pass, fixes only)

Status: **CANDIDATE. REVIEW NOT RUN (paused by user).** Nothing here is approved or accepted. The author approves nothing.

- **Author:** Claude Opus 5.5 High (`claude-opus-5-5`), a ROQA worker thread launched by the ROQA Chief Thread at the user's request ("start working on the next slice"). Sole source writer in worktree `feat-m4-visualizer-29fbb01fac4ab1bb`.
- **Base:** local commit `28a17b5b4b64cc2d62c0238666977b18de0736c9` on `feat/m4-visualizer`, which is "wip(m4): visualizer and unmute-on-set (slice 7, review not run)". It sits on local `main` `8443a1e` (slice 6) and holds exactly the 6 slice-7 paths. Before that commit, every entry of the slice-7 manifest `artifacts/m4-visualizer-1/round-1/inputs.sha256` (SHA-256 `E3AD8894CB50B8B936C8CBE8034B181F145A3C6F8C1690447D1D990AEDE1880E`) matched the working tree. Slice 7's own review is also NOT RUN.
- **Branch:** `feat/m4-hardening`, created from `28a17b5`. The slice 8 work is uncommitted. Nothing was pushed, and `main` was not touched.
- **Scope:** fixes only, no new features (prep doc slice 8). Written under `/ponytail:ponytail`, with performance and footprint first. No new thread, timer, wait handle, poll, dependency or `windows` feature.

## What changed

| File | + / − | Change |
|---|---|---|
| `src/graphics.rs` | +85 / −22 | Per-canvas album-art bitmap cache (`Canvas::art`, pure `cached`). `NotchScene::art` carries the entry number. `fill_art` takes the cached bitmap. The slice-5 `shortcut:` comment is removed. 1 test |
| `src/media.rs` | +87 / −6 | `Changing` keeps the shown state. Art entry number `arts` and `art()` returning `(entry, pixels)`. `pending` and `reconnect`. A failed playback read re-checks the session once. 2 new tests and 1 extended |
| `src/audio.rs` | +11 / −0 | `pending` and `reconnect` (one `REBIND` bit, only after `start`) |
| `src/native_window.rs` | +22 / −21 | `repositions` returns `Some(reconnect)`. Resume and `TaskbarCreated` post `REPOSITION` with wParam 1, other changes with 0. The test is updated |
| `src/host.rs` | +18 / −1 | On `REPOSITION` with wParam 1, `media.reconnect()` and `audio.reconnect()`. The loop waits 0 ms while media or audio bits are pending. The meter test is extended with an hour-long gap |
| `docs/development/m4-media-hardening-candidate-1.md` | new | This packet |

No change to `Cargo.toml`, `Cargo.lock`, scripts, UIA code, the visual spec or `.gitignore`.

## Fixes

### 1. Album-art re-upload (performance): DONE

- **Before.** Every notch paint created a new `ID2D1Bitmap1` from the art pixels. While open and playing, that meant up to 15 visualizer repaints a second, plus progress ticks, clock minutes and open/close frames.
- **Now.** Each `Canvas` holds one `RefCell<Option<(u64, ID2D1Bitmap1)>>`. `cached` returns the held bitmap when the art entry number is unchanged. It uploads only for a new entry, and frees the old bitmap first.
  - **No art releases it.** A paint with no art (clock notch, no-art track, ended session) releases the held bitmap, so no GPU memory is kept for a track that is gone.
  - **Failed uploads retry.** A failed upload leaves the slot empty, so the next paint tries again.
- **Key.** `Media::arts` counts art entries; `read_art` bumps it whenever it replaces the entry. A pixel-pointer key was rejected because of ABA: the old entry's buffer is freed when it is replaced, and the next decode allocates the same size on the worker, so the address is likely to be reused.
- **Device loss.** The bitmap is a device resource on the canvas. `Graphics::lost` and `removed` lead to `lose_graphics` and `refresh_graphics`, which drop every `Surface`, and with it every canvas and cached bitmap. A text-scale rebuild also replaces the notch canvases. No other invalidation is needed.
- **Scope.** The cache is per surface, as briefed. One bitmap per notch monitor, at most the cover side squared × 4 bytes of GPU memory.
- **Test.** `graphics::media_tests::art_uploads_once_per_entry_and_no_art_or_a_failed_upload_releases_it` checks that 15 repaints of unchanged art give 1 upload, that a new entry gives exactly 1 more, that no art releases the bitmap and re-uploads on return, and that a failed upload holds nothing. The media art test also checks that the entry number is stable for an unchanged entry.

### 2. Hung or closed player mid-session: DONE in part, the rest DEFERRED

- **Exit or disappearance: verified, no change needed.** `SessionsChanged` and `CurrentSessionChanged` lead to `replace_session` and then `end_session`. That cancels the properties read and the command, clears `again`, drops the art entry (cancelling its read and orphaning its slot), and removes the session tokens. `Change::Session(false)` then gives `View::NoPlayer`, which drops the grace deadline.
  - Each pass, the host derives the progress deadline (`refresh_progress`) and the meter gate (`meter_due`, with `playing` false giving `audio.stop()`) from `shown()`, so no deadline survives.
  - `Status::Closed` already hides the track at once.
  - The host never blocks on a GSMTC async operation. Shutdown cancels requests without waiting, and the icon worker keeps its 1 s detach bound.
- **New: a failed playback read re-checks the session once.** A session whose `GetPlaybackInfo` or `GetTimelineProperties` fails may be gone without its event having arrived. The failure raises `SESSION`, so the next pass re-reads the current session. An unchanged session queues no further playback read, so this cannot loop.
- **New: the art bitmap is freed** on the next paint after the session ends (fix 1).
- **Kept by design:** a hung command keeps the single command budget until it completes, even after its session ends. This is the reviewed slice-3 decision. Whether a dead player's operation completes is UNVERIFIED; it is native item 2.
- **DEFERRED: bounding the synchronous getters** (`GetPlaybackInfo`, `GetTimelineProperties`, `GetCurrentSession`). They run on the host STA. There is no per-call timeout without moving them to another thread, and that costs a thread, against the footprint priority. Whether they can block on a hung player is still UNVERIFIED (prep UNVERIFIED 3). The existing `Media: slow ...` log (> 8 ms) is the detector. Native item 2 decides whether a later slice moves them.
- **DEFERRED: a hung thumbnail stream on the shared icon worker** (slice-5 MIN-4).
  - **Why not a cancellable read.** `Size()` and the WIC reads through `CreateStreamOverRandomAccessStream` are synchronous on `roqa-icons`, and nothing in the code can interrupt them. A bounded fix means reading the stream asynchronously on the media side (`ReadAsync` + `when`, cancelled with the art entry) and handing bytes, not a stream, to the worker.
  - **Why not a replacement worker.** The alternative is replacing a stuck worker.
  - **Why deferred.** Either one rewrites the reviewed slice-5 hand-off and its tests, and adds a second cancellable operation per track. That is too large for a fixes-only pass, and whether real players' streams block is UNVERIFIED (prep UNVERIFIED 4).
  - **What already bounds it.** The existing 1 s shutdown detach still bounds quit. Native item 3.

### 3. Sleep and resume: DONE

- **Before.** `WM_POWERBROADCAST`/`PBT_APMRESUMEAUTOMATIC` posted `REPOSITION`, which re-read topology and the clock only.
- **Now.** It posts `REPOSITION` with wParam 1, and the host calls `media.reconnect()` and `audio.reconnect()`.
  - **Media.** Sets `SESSION | PROPERTIES | PLAYBACK`. The next pass re-reads the current session (resubscribing only if it changed) and the playback state and timeline. It also starts one properties read, and that read asks for new art only when the track key changed, so the never-retry rule for a failed track stays.
  - **Audio.** Sets `REBIND` once started. The endpoint, its volume callback and its meter are unregistered and bound again, so a meter dropped by a failed `GetPeakValue` during sleep comes back.
  - **Coalescing.** Every surface posts its own `REPOSITION`, but the bits coalesce into one re-read.
- **Stale deadlines: verified, no burst by construction.** `Instant` is QPC here (`host.rs` notes it counts suspend), so every deadline from before sleep is simply past on resume. Each is single-shot and re-armed from the current time:
  - The meter's next sample counts from the last actual sample.
  - The progress floor `saturating_duration_since` gives one tick.
  - The grace deadline is consumed once by `expire`.
  - The clock, topology, full-screen poll and rebuild each run once.
- **Tests.** The new tests cover an hour-long gap in the meter test (one sample, then the 67 ms period again), an 8-hour gap on the grace deadline (one expiry, then none), `reconnect` setting exactly the three bits that the next pass consumes, and `repositions` returning reconnect only for resume and `TaskbarCreated`.

### 4. Explorer restart (`TaskbarCreated`): DONE

- **Reconnect.** `TaskbarCreated` takes the same wParam-1 path as resume, so media re-reads and audio rebinds once.
- **No duplicate subscriptions: verified.** No path on `REPOSITION` subscribes anew.
  - Media resubscribes a session only when `GetCurrentSession` returns a different COM identity, after `end_session` has removed the old tokens.
  - The manager subscription is never repeated.
  - Audio's rebind drops `Endpoint`, which unregisters the volume callback, before binding again. The device client is registered once, in `start`.
  - UIA and the window catalog are unchanged.
- **DEFERRED: recreating the GSMTC manager** if it dies with Explorer. Whether it does is UNVERIFIED. If `GetCurrentSession` then fails, the error is logged and the shown state kept, as before. Restarting `RequestAsync` needs a bounded retry rule and native evidence first. Native item 4.

### 5. Lost wakes: DONE

- **Before.** `Wake::raise` posts only on the 0-to-nonzero edge. A post that fails or is consumed, for example by the COM modal loop that a synchronous cross-apartment call runs on the host STA mid-pass, leaves the bits set and suppresses every later post. Recovery then waited for the next finite deadline: the clock's, up to a minute.
- **Now.** Before each wait, `event_loop` checks `media.pending()` and `audio.pending()` (one relaxed atomic load each) and uses a 0 ms timeout if bits are set, so the next pass applies them at once.
  - **No busy loop.** `refresh` swaps the bits to 0 every pass.
  - **Bits set during the wait.** A post that fails while the host is already waiting still falls back to the next finite deadline, as documented in `Wake::raise`. That case needs a failed `PostThreadMessageW` (a full queue), not a consumed one.
- **Not polling.** There is no timer: the check rides the existing pass.
- **Icons.** Each icon completion posts its own wake, so one lost post is recovered by any later message, and `collect` runs every pass. No change.

### 6. Transient `Changing` status: DONE

- **Before.** `Status::Changing` counted as "not playing and not paused". It cleared `playing` and the grace deadline, which flickered a playing notch to the clock pill and ended a paused grace early (council R4-MIN-1).
- **Now.** `State::apply` ignores a `Playback` change whose status is `Changing`: playing, grace, controls, seekability, rate and timeline all keep their shown values. The next non-`Changing` status applies normally. `Closed`, `Opened` and the rest are unchanged.
- **Test.** `media::tests::changing_keeps_the_shown_state_and_the_grace_deadline` checks that a playing view is unchanged even when `Changing` reports disabled controls, that the grace deadline survives `Changing`, and that `Closed` still ends it.
- **Known limit.** A player stuck in `Changing` keeps its last shown state, including an extrapolating position, until its next status.

## Footprint

| Item | Value |
|---|---|
| Base release `roqa.exe` (slice 7, `28a17b5`) | **800,768 B** (the slice-7 packet's figure; not rebuilt here) |
| Candidate release `roqa.exe` | **802,304 B**, SHA-256 `41412BA244AF12840DB04C09BF56DE71C06C7611A3F5AF79C1D619941BFEA05B` |
| Delta | **+1,536 B (+0.19 %)** |
| New crates, `windows` features, threads, timers, wait handles | None |
| Runtime GPU and CPU | One art bitmap per notch canvas while art is shown, released without art. Art uploads go from one per notch paint to one per art entry per canvas. **NOT MEASURED** |

## Verification (author-run; not a review)

- `scripts/check.ps1`: PASS. fmt, clippy `--all-targets -D warnings`, **102 tests passed**, 0 failed (99 at slice 7, plus 3 new; 3 more extended).
- `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-hardening-1/round-1/verification`: PASS. checks exit 0, `build --locked --release` exit 0. The logs are in that folder.
- A rerun of verify must use a different evidence folder, or the frozen log entries change.

## Native evidence plan: all NOT RUN

No native or GUI run was made. The system volume, audio settings and display settings were never changed. Each run needs fresh, explicit, desktop-free confirmation from the user via the Chief, and a foreground-window check first, because scripts inject input and move the cursor.

- **Separate authorization:** Explorer restart, sleep, a forced TDR and a remote-session switch each need their own explicit authorization, as does any audio or display setting change.
- **Stopping processes:** only the owned test process may be stopped, preferably with `roqa --quit`.

| # | Scenario | Pass looks like |
|---|---|---|
| 1 | **Art upload count** (counting debug build or ETW/PIX). Art shown, panel expanded and playing for 60 s, then a track change | 1 bitmap creation per canvas per art entry; 0 during visualizer, progress and clock repaints. After a device-loss probe (`Probe::DeviceLoss`), exactly 1 re-upload per rebuilt canvas |
| 2 | **Hung or killed player.** Reuse the hung-provider harness: hang, then kill, the player mid-playback while expanded | The host stays responsive. Any `Media: slow ...` lines are recorded (decides whether the getters move off-thread). The session clears to the clock and the meter and progress stop. Whether a pending command completes, and whether later presses work, is recorded |
| 3 | **Hung thumbnail stream** (a test player whose artwork stream never returns data) | Record whether `roqa-icons` blocks and whether dock icons still load. Quit stays within the 1 s detach. Decides the deferred fix |
| 4 | **Explorer restart** while playing and expanded (separately authorized) | One `Audio: rebound` line. The media state is current, with no duplicate events: the wake count rises once per real change. Record whether `GetCurrentSession` fails afterwards (manager dead), which decides the deferred manager recreation |
| 5 | **Sleep and resume**, paused within grace and playing (separately authorized) | One `Audio: rebound` line. The session and art are current. Grace expires at most once. There is no burst of paints or meter samples after resume (counts per second) |
| 6 | **`Changing`.** Skip tracks in a browser and in a desktop player | No collapse or flicker during the change, and a paused grace survives a skip |
| 7 | **Lost wake.** A player event during a long synchronous call (hard to force; best effort with the hung harness) | State converges at the next pass, not at the next clock minute |
| 8 | **Regression.** The five suites (`smoke`, `topology-check`, `picker-smoke`, `catalog-smoke`, `packaged-hung-check`) and a 30 s idle sample against this hash, each separately confirmed | As at slice 7 |

## UNVERIFIED

1. Whether the GSMTC getters or the thumbnail stream can block on a hung player (prep UNVERIFIED 3 and 4).
2. Whether the GSMTC manager survives an Explorer restart.
3. Whether a pending GSMTC command completes when its player dies.
4. That a D2D bitmap created from one `BeginDraw` context is valid in later `BeginDraw` contexts of the same surface. This is documented D2D device-resource behaviour (resources belong to the `ID2D1Device`), but it was not checked natively.
5. Every runtime footprint figure.

## Deferred and non-goals

- Bounding the synchronous GSMTC getters, the hung thumbnail stream on the shared worker, and recreating a dead GSMTC manager. Reasons are given above.
- The dock's per-paint icon upload (`graphics.rs` `shortcut:`) is outside this media slice.
- **Review: REVIEW NOT RUN (paused by user).** No clean-code-guard review, no council review and no subagent was started. This candidate is not accepted.

## Manifest

Frozen at `artifacts/m4-hardening-1/round-1/inputs.sha256` after this packet was written. Its SHA-256 is reported to the Chief outside this file, since the manifest includes this packet.
