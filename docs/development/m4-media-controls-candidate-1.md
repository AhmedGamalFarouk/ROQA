# M4 media controls candidate 1: controls and accessible actions (slice 3)

- **Author.** Claude Opus 5.5 High (`claude-opus-5-5`), the delegated implementation sub-agent and sole source writer in worktree `t3-6fed8e4c`. It approves nothing: not this code, not the slice, not the milestone.
- **Base.** `30cf00b0521604a9aa272c04838c5dabcb582a5b` on `t3/improve-codebase-architecture`. The brief states its source matches integrated root main `d0f5626` (root source `0e1105c`); this author did not re-check the root. Uncommitted, by instruction.
- **Baseline quirk, preserved.** `docs/development/icon-cache-integration-1.md` shows as modified only because its CRLF bytes were restored to match the root; `git diff` shows no content change. It was not edited.
- **Authorization.** The user explicitly authorized starting this slice after the architecture review, fix, commit and integration. **Slice 2's native evidence and council review are still missing and are not passed by this candidate.** This is feature progression, not acceptance.
- **Plan item.** `docs/development/m4-media-preparation.md`, "Bounded implementation sequence" item 3, with decided open question 4 (keyboard route) and the user's always-animate decision.

## Changed scope

| File | Change |
|---|---|
| `src/media.rs` | `Track::controls` (previous, play/pause, next enabled) read from `GetPlaybackInfo().Controls()`; `redraw` and the log comparison include it (AR1-Q1); `Media::command` sends one GSMTC command through the existing `Request` + `when` slot, with at most one in flight; `end_session` cancels it; `generation()` accessor; 2 tests extended, 1 new async test |
| `src/notch_uia.rs` (new) | Notch UIA provider: ToolBar root, three Button items with Invoke, version (session generation) checks, disabled exposure, focus requests, focus-changed event; 1 test |
| `src/dock_uia.rs` | `DockAccessibility` became `Accessibility` with `register`, so the notch reuses the same per-HWND `WM_GETOBJECT` registry and teardown; runtime-ID and VARIANT helpers made crate-visible (the runtime-ID code moved unchanged) |
| `src/layout.rs` | `media_controls` (direction-B control boxes) and `media_control_at` (pointer and UIA hit-testing); 1 test |
| `src/graphics.rs` | `media_chips` and `focus_ring` plan the three controls, their disabled state and the focus ring in the panel's controls row; one `fill_glyph` helper now draws the cover glyph too, and `pixel_square` replaces the cover's local closure (same geometry); `NotchScene::focus`; 2 new tests, 1 extended |
| `src/host.rs` | Notch role holds its UIA registration, shared state and keyboard `Focus`; `publish_notch` (called by `paint_notch`) shares UIA state; `notch_input` dispatches to `click_control`, `enter_focus` and `notch_key`, plus UIA Invoke and deactivation; `release_focus` on session end; `step` focus order; 1 test |
| `src/motion.rs` | `reveal` (the date/artist fade, now shared with the controls) and `Tween::target` |
| `src/native_window.rs` | `WM_ACTIVATE` (inactive) posts `DEACTIVATED`; still passes to `DefWindowProcW` |
| `src/dock.rs`, `src/picker.rs` | `restore_foreground`, extracted unchanged from `Picker::dismiss` and shared with the notch's Escape |
| `src/main.rs` | `mod notch_uia;` |
| `docs/design/roqa-visual-spec.md` | Controls row, disabled and focus-ring look, repaint rule, notch UIA and keyboard route (PROVISIONAL, like the rest of direction B) |

No `Cargo.toml`, `Cargo.lock`, feature, script or toolchain change. No new crate, thread, timer, deadline, wait handle, hook, IPC command or picker route.

## What it does

- **Drawing.** While the panel is open or opening, previous, play/pause and next sit at the direction-B mockup's places (100, 148 and 200 DIP from the body's left, 66–98 from its top). They fade in on the artist's schedule and stay put while the body clip reveals them. Play/pause shows its action: pause bars while playing, the play triangle otherwise, black on a white circle. A control the player does not enable is drawn at 30 % opacity, never hidden. The pill draws no controls.
- **Enabled state.** `IsPreviousEnabled`, `IsNextEnabled`, and `IsPauseEnabled` while playing or `IsPlayEnabled` otherwise, read with the existing playback snapshot on `PlaybackInfoChanged`. No new event subscription.
- **Commands.** `TrySkipPreviousAsync`, `TryPauseAsync`/`TryPlayAsync`, `TrySkipNextAsync`, issued on the host thread only when the current track enables that control. The returned `IAsyncOperation<bool>` goes through the existing `Request` slot: its `when` completion only fills its own slot and posts the existing media wake. A newer command cancels the previous one; session replacement and shutdown cancel it too. The result is only logged; the notch changes when the player's own events report the new state. Nothing waits on animation.
- **Pointer.** `WM_LBUTTONUP` on the notch hit-tests the controls at the current motion progress, through the same `media_control_at` the UIA tree uses. The surface stays nonactivating (`MA_NOACTIVATE` unchanged): a click never takes focus.
- **UIA.** A ToolBar root ("ROQA Notch") registered through the dock's existing `WM_GETOBJECT` registry, with three Button children while a session exists: "Previous track", "Pause, Title – Artist" or "Play, …", and "Next track". Providers read a mutex snapshot and post `INVOKE` or `FOCUS` (wParam control, lParam session generation) to the notch HWND; the host loop revalidates the generation and the enabled state before acting. A disabled control reports `IsEnabled` and `IsKeyboardFocusable` false and refuses Invoke and focus with `UIA_E_ELEMENTNOTENABLED`. An element from an older session reports `UIA_E_ELEMENTNOTAVAILABLE`. Bounds are the open panel's while the panel is open or opening, and offscreen otherwise; Invoke works either way. Every notch paint publishes the snapshot; focus changes publish at once, before the focus event is raised.
- **Keyboard (user decision).** Entry only through UIA focus (`SetFocus` on the root or a control). The notch then asks for the foreground (`SetForegroundWindow` on its own HWND, never forced; a refusal is logged and focus is not taken), records the previous foreground window, and opens with the accepted spring. Tab/Shift+Tab and the arrows move among enabled controls, wrapping; Enter and Space press the focused control (key repeat ignored); Escape closes the panel and gives the foreground back to the recorded window (`restore_foreground`, the picker's existing rule). Losing the foreground ends focus without restoring; a session ending under focus restores it. Pointer leave does not close a focused panel. The focus ring is a drawn state, so moving focus paints one frame.
- **Motion.** Always animated, as decided; no reduced-motion path was added.

## Footprint

- **Wakes and work.** No timer, deadline, poll, wait handle or thread. A command adds one completion wake. Enabled-state changes ride the existing playback event and paint only when a drawn control state changes. Focus moves paint one frame. Idle collapsed playback adds nothing.
- **Memory.** At most one command request (a COM operation, a slot `Arc` and a closure). The notch UIA snapshot holds three short names (a title and artist are capped at 512 UTF-16 units each upstream). The snapshot names are rebuilt on every notch paint, including animation frames; that is three small allocations per frame, next to the per-frame DirectWrite layouts that already exist.
- **Binary size, measured.** Release `roqa.exe` built by the pinned toolchain: base `30cf00b` (scratch build) 666,112 B; this candidate **713,216 B, +47,104 B (+7.07 %)**, SHA-256 `22B6BDBA42A344082ECA21D3406C0A47EB8720A0BC113A8DE0EF994E0EE8EBD0`. Two scratch-only probe builds of the pre-guard-pass source (713,728 B) attribute it approximately (LTO makes the parts non-additive):
  - without constructing the notch UIA providers: 691,200 B, so the providers cost about 22.5 KB;
  - without tracking the command result (no `Request<bool>`/`when`): 708,608 B, so about 5.1 KB;
  - the remaining ~20 KB is drawing, focus and key routing, and the GSMTC control bindings.

  The providers repeat the dock's pattern rather than share one data-driven provider type with it. Sharing could save most of the 22.5 KB, but it rewrites accepted dock UIA code; it is offered as a later, separately reviewed option, not done here.
- **Source size.** About 900 production lines added (about 360 in the new `notch_uia.rs`, about 270 in `host.rs`, about 180 in `graphics.rs`) plus about 250 test lines. Smallest complete design considered: UIA Invoke and focus for three buttons need the root/item provider pair; keyboard entry, focus ring, focus return and deactivation need the focus state and routing in `host.rs`. Nothing generic was added: no player abstraction, trait or plugin layer.

## Verified API receipts

Checked in the project-local pinned sources under `.tools/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (path `W` = `windows-0.62.2/src/Windows`). Signatures were used exactly as generated; the build compiles against them.

| API | Receipt |
|---|---|
| `GlobalSystemMediaTransportControlsSession::{TryPlayAsync, TryPauseAsync, TrySkipNextAsync, TrySkipPreviousAsync} -> Result<IAsyncOperation<bool>>` | `W/Media/Control/mod.rs` 51, 58, 93, 100 (inside `impl GlobalSystemMediaTransportControlsSession`, from 22) |
| `…PlaybackInfo::Controls() -> Result<…PlaybackControls>` | same file 498 (impl from 497) |
| `…PlaybackControls::{IsPlayEnabled, IsPauseEnabled, IsNextEnabled, IsPreviousEnabled} -> Result<bool>` | same file 375, 382, 417, 424 (impl from 374) |
| `IAsyncOperation<T>::when(FnOnce(Result<T>) + Send + 'static)`; `bool: RuntimeType` | `windows-future-0.3.2/src/when.rs` 35–39; `windows-core-0.62.2/src/runtime_type.rs` 19 |
| Tests only: `IAsyncOperation::ready`, `IAsyncOperation::spawn` (its `Cancel` is a no-op) | `windows-future-0.3.2/src/async_ready.rs` 211; `async_spawn.rs` 274, 146 |
| `UIA_E_ELEMENTNOTENABLED`, `UIA_E_ELEMENTNOTAVAILABLE` | `W/Win32/UI/Accessibility/mod.rs` 20362, 20361 |
| `UIA_HasKeyboardFocusPropertyId`, `UIA_IsKeyboardFocusablePropertyId`, `UIA_IsOffscreenPropertyId`, `UIA_IsEnabledPropertyId` | same file 20395, 20432, 20436, 20424 |
| `UIA_AutomationFocusChangedEventId`, `UIA_Invoke_InvokedEventId`, `UIA_ButtonControlTypeId`, `UIA_ToolBarControlTypeId` | same file 20307, 20413, 20314, 20637 |
| `UiaClientsAreListening`, `UiaRaiseAutomationEvent` | same file 389, 595 (already used by `dock_uia`) |
| `GetKeyState(i32) -> i16`; `VK_TAB/ESCAPE/RETURN/SPACE/LEFT/RIGHT/UP/DOWN/SHIFT` | `W/Win32/UI/Input/KeyboardAndMouse/mod.rs` 63; 1087, 889, 1072, 1084, 989, 1073, 1227, 885, 1081 |
| `WM_ACTIVATE`, `WA_INACTIVE`, `SetForegroundWindow`, `GetForegroundWindow` | `W/Win32/UI/WindowsAndMessaging/mod.rs` 6893, 6691, 2142, 866 |

**Behaviour taken from Microsoft documentation from memory, not re-fetched for this candidate (UNVERIFIED here):** that `SetForegroundWindow` can activate a `WS_EX_NOACTIVATE` window; that `DefWindowProc` gives keyboard focus to a window it activates; that `IInvokeProvider::Invoke` and `SetFocus` on a disabled element should fail with `UIA_E_ELEMENTNOTENABLED`; and that `WM_ACTIVATE`'s low word is `WA_INACTIVE` on deactivation. Each failure mode is benign: no keyboard focus, or a different error code.

## Threading, ownership and FFI

- GSMTC calls stay on the host STA. `when` completions and UIA provider methods never touch `Desktop`, HWND state beyond `PostMessageW`, or Direct2D; they cannot block. UIA providers lock a mutex that the host holds only briefly, with no calls inside, and the host raises the focus event without holding it.
- No new panic path in a callback: indexes from UIA are bounds-checked (`get`), never indexed directly.
- The notch's `Accessibility` registration is the first field of its role and is dropped before the notch HWND, through the existing teardown (`UiaReturnRawElementProvider(…, None)`, then `UiaDisconnectProvider`).
- The command request is cancelled on session end and in `Media::drop` (via `end_session`), before the session's tokens are removed, as the fetch request already was.

## Requirements mapping

| Brief requirement | Where |
|---|---|
| Previous, play/pause, next chips in the reserved controls row | `layout::media_controls`, `graphics::media_plan`/`media_text`; spec "Controls row" |
| Enabled/disabled per GSMTC `Is*Enabled` | `media::read_playback`, `Track::controls`; drawn at 30 %; UIA `IsEnabled` |
| Existing Media/Track/`when` owner; bounded in flight; generation and stale completion; cancel and cleanup | `Media::command` (one `Request<bool>`, cancel-and-replace), `refresh` logs and drops it, `end_session`; UIA version = generation, rechecked in `notch_input`; test `a_replaced_commands_late_result_stays_in_its_own_slot` |
| Pointer controls on the nonactivating notch | `notch_input` `WM_LBUTTONUP` + `layout::media_control_at`; `MA_NOACTIVATE` unchanged |
| Notch UIA Invoke following `dock_uia` ownership, marshalling and version checks | `notch_uia.rs`, shared `Accessibility` registry |
| Disabled controls exposed as disabled | `IsEnabled`/`IsKeyboardFocusable` false, Invoke and focus refused, still in the tree and drawn |
| Tab/arrows within the focused notch via UIA entry; focus return on close/Escape | `notch_input` `FOCUS`/`WM_KEYDOWN`, `step`, `restore_foreground`, `release_focus` |
| No global hotkey, hook, IPC or picker route | none added |
| Always animate; actions never wait for animation; target UIA state at once | springs unchanged; commands and Invoke ignore progress; `open` publishes the tween target, focus publishes immediately |
| Pure tests: hit-testing, enable-state mapping, generation/async safety | layout, graphics, media, notch_uia and host tests listed below |
| Extend the redraw contract for newly drawn state (AR1-Q1) | `media::redraw` compares `controls`; test extended; focus ring repaints through `live` |
| Non-goals: seek, progress ticks, art, volume/device, visualizer, generic layer, ICG1-1/2/3 | untouched |

## Author guard pass

The author read `clean-code-guard` only after the first verify, then applied it to its own diff before freezing; this is not an independent review.

- `src/host.rs` — split the ~100-line `notch_input` into a dispatcher plus `click_control`, `enter_focus` and `notch_key` (same behaviour).
- `src/host.rs` — `publish_notch` extracted from `paint_notch`, so painting no longer also builds UIA state inline.
- `src/graphics.rs` — `media_chips`, `focus_ring` and `pixel_square` extracted from `media_plan`; its stale doc line about empty control rows fixed.
- Flagged for the reviewer: `notch_key` and `media_plan` remain longer than 20 lines (a key `match`, and the accepted pill-to-panel plan).

clean-code-guard: 3 fixed, 1 flagged for author (self-applied; independent review still required).

## Checks and evidence

- **Pinned `scripts/verify.ps1 -EvidenceDirectory artifacts/verification/m4-media-controls-1`:** checks PASS (`cargo fmt --check`, `clippy --locked --all-targets -D warnings`, `cargo test --locked`: **65 passed, 0 failed**; base had 59) and release build PASS. Logs and `verification.json` in that directory (ignored by git, not committed).
- **New tests:** `layout::media_controls_hit_only_their_drawn_rows_once_revealed`; `graphics::expanded_panel_draws_previous_play_pause_and_next_in_the_controls_row`; `graphics::keyboard_focus_rings_the_focused_control_at_any_scale`; `media::a_replaced_commands_late_result_stays_in_its_own_slot`; `notch_uia::names_say_the_action_and_the_track_and_follow_the_enabled_controls`; `host::keyboard_focus_steps_over_disabled_controls_and_wraps`. Extended: the redraw, log-comparison, session-replacement and collapsed-pill tests.
- **Base size build and probes:** scratch copies outside the worktree, pinned toolchain via this worktree's `scripts/cargo.ps1`; not part of the frozen inputs.

## Not run, unverified and limits

- **NOT RUN, by rule:** any GUI or native launch. That covers real player play/pause/skip, enabled-state mapping with real players, pointer hits on screen, appearance of the controls and ring, Narrator invoke and navigation, keyboard entry, focus return, idle wake and paint counts, memory and CPU, and the five native suites. **Keyboard and accessibility validation is DEFERRED by the user; nothing here claims it passed.** Council review is NOT RUN.
- **UNVERIFIED:**
  - whether Windows grants the notch the foreground on a UIA focus request (Narrator or another client); if not, keyboard entry is refused and logged;
  - whether UIA itself activates the notch before calling `SetFocus`; if so, the recorded "previous foreground" is the notch and Escape cannot return focus;
  - whether `Cancel` on a GSMTC command withdraws a command the player already received (rapid repeated presses cancel the earlier request);
  - real players' `Is*Enabled` values, and players that offer only the play/pause toggle (they would show a disabled play/pause; recorded as a `shortcut:` in `read_playback`);
  - the documentation-derived behaviours listed under the API receipts.
- **Known limits:**
  - If the notch is destroyed while it has keyboard focus (hide command, topology or text-scale rebuild, device-loss rebuild), Windows picks the next foreground window; ROQA does not restore it. A full-screen app taking the foreground ends focus normally.
  - No UIA property-changed events: clients see enabled-state and name changes on their next read. Focus changes do raise the focus event.
  - No pointer hover or press feedback on the controls.
  - The ring's top edge (63 DIP) overlaps the reserved progress row (50–64) by 1 DIP; slice 4 may adjust.
- **Not fixed, by instruction:** ICG1-1, ICG1-2, ICG1-3; slice 2's open native and council items.

## Frozen inputs

`docs/development/reviews/m4-media-controls-candidate-1.inputs.sha256` lists raw-byte SHA-256 of the source, config, normative docs, this packet and the verify logs, written after the final checks. Because the manifest covers this packet, its own hash is recorded in `progress.md` and the hand-off, not here. `progress.md` is excluded so a reviewer can append. Earlier frozen reports and manifests are kept unchanged as historical evidence; their hashes describe earlier source.

## Next gate

A **fresh, independent, non-author** clean-code-guard review (and the council reviews the project rules require) against the identical manifest, read-only except for its own report, before this code is called reviewed or committed. Then the native evidence above, which needs a fresh desktop-free confirmation for each run.
