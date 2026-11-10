# Code review: ROQA M4 slice 3 controls, candidate 2 (independent Opus recheck 2)

## Summary

**Code gate (this reviewer, these exact inputs): SAFE FOR AN AUTHORIZED LOCAL COMMIT. No blocker or major finding is open.** All three prior major findings (M4C1-SOL-1, M4C1-SOL-2 / M4C-OPUS-MAJ-1, M4C1-SOL-3) are resolved in code. The submitted minor and informational fixes are resolved too. This is one council input, not a council ACCEPT and not milestone acceptance. The Sol High recheck on the identical manifest, all native/runtime evidence and the slice-2 gaps are still missing.

Counts: 0 critical, 0 important, 1 nit (clean-code-guard template). Project mapping: 0 blocker, 0 major, 0 minor. There are 5 new informational items (M4C2-OPUS-INFO-1 to INFO-5) that are not counted findings.

- **Reviewer:** Claude Opus 5.5 High (`claude-opus-5-5`), a fresh delegated recheck task. It wrote none of the candidate source or the fixes. The original slice author was a different Opus 5.5 High task; the corrective author was the Sol 6.1 Medium parent. Using the same model as the original author is allowed by `PLAN.md`; this task authored nothing it reviews.
- **What I wrote:** only this report, plus one new evidence directory produced by the pinned verify script. I made no source, manifest, packet, ledger or `.gitignore` edits, no commit or push, no delegation and no GUI or native launch.
- **Date:** 2026-11-11 Africa/Cairo.

## Inputs and staleness

| Check | Start (2026-11-10T23:09Z) | End (2026-11-10T23:13Z, before this report was written) |
|---|---|---|
| `git rev-parse HEAD` | `30cf00b0521604a9aa272c04838c5dabcb582a5b` | same |
| SHA-256 of `docs/development/reviews/m4-media-controls-candidate-2.inputs.sha256` | `7474C26D70DF0AB8B97ADB65004859D6FF752BB427189893CCDD8975970505DF` | same |
| Raw-byte entries (`sha256sum -c`) | 49 entries, **49/49 OK** | **49/49 OK** |
| `git status --short` | 13 modified, 7 untracked | identical set |

- The manifest is LF ASCII. It covers the five frozen `artifacts/verification/m4-media-controls-2` logs, and none of them was touched.
- `src/notch_uia.rs` is untracked, so I read it in full from the working tree; its manifest entry matches.
- Candidate-1 manifest hashes describe historical source by design. I did not use them as current inputs. I used the delta patch `artifacts/candidates/m4-media-controls-2.delta.patch` only as a guide, and confirmed each hunk against the current source.

## Checks actually performed

1. **`scripts/verify.ps1 -EvidenceDirectory artifacts/verification/m4-media-controls-opus-2-review`**, a new, git-ignored directory. Both `-1` and `-2` evidence folders were left untouched.
   - Checks PASS: fmt, clippy `--locked --all-targets -D warnings`, and `cargo test --locked` with **66 passed, 0 failed**. The new tests `media::…cancel_ignoring_command_keeps_its_budget_across_presses_and_sessions` and `notch_uia::…root_owns_activation_and_destroyed_windows_invalidate_retained_children` pass.
   - Release build PASS. `target/release/roqa.exe` is **717,312 B**, SHA-256 **`D7E3D685A56CFA2A315B086D04BCC31E9C4DE5B316BC0ED233D0C833E3B14117`**, byte-identical to the packet.
   - The base build (666,112 B) was not rebuilt.
2. **Frozen logs:** read `verification.json` and the logs. Both steps exit 0, with 66 passed.
3. **Pinned bindings:** `ProviderOptions_ProviderOwnsSetFocus` = 16 (`windows-0.62.2 …/Accessibility/mod.rs:19625`), and `BitOr for ProviderOptions` exists (`:19593`).
4. **Completion registration:** `windows-future-0.3.2/src/async.rs:47-64` shows that `when` fails only at `status()?` or `set_completed`. In both cases the closure is never registered and is dropped, so it is not retained.
5. **Microsoft documentation receipts:** I relied on the primary-document receipts already recorded in both prior reports (fragment `SetFocus` remarks, `ProviderOptions`, `IAsyncInfo::Cancel`, `WM_ACTIVATE`, `SetForegroundWindow`). I did not re-fetch them.
6. **NOT RUN:** any GUI or native launch, real player, Narrator or keyboard run, foreground grant or restore, and any idle, wake, CPU, memory or handle measurement. No desktop-free authorization was given, and keyboard/accessibility validation is user-DEFERRED.

## Prior findings: dispositions on these inputs

| ID | Prior severity | Disposition | Evidence |
|---|---|---|---|
| M4C1-SOL-1 | major | **RESOLVED (code)** | See "Command bound" below |
| M4C1-SOL-2 / M4C-OPUS-MAJ-1 | major | **RESOLVED (code, doc-backed); the native grant is NOT RUN** | `notch_uia.rs:170`; capture before activation at `host.rs:1004` |
| M4C1-SOL-3 / M4C-OPUS-MIN-2 | major / minor | **RESOLVED (code)** | `host.rs:118-125`, `:216`, `:411-415` |
| M4C-OPUS-MIN-1 | minor | **RESOLVED** | `host.rs:1048-1069`, called at `:1219-1223` |
| M4C-OPUS-MIN-3 | minor | **RESOLVED for the notch.** The dock keeps its pre-existing pattern, which was out of scope. | `notch_uia.rs:46-53`, `:119`; `host.rs:121-123` |
| M4C1-SOL-6 | minor | **RESOLVED**, with an informational residual (INFO-1) | `host.rs:416-424`, Escape at `:1035-1037` |
| M4C-OPUS-INFO-5 | info | **RESOLVED** | `notch_uia.rs:211-221` |
| M4C-OPUS-INFO-6 | info | **RESOLVED** | `host.rs:1219-1223` |
| M4C-OPUS-MIN-4 | minor | **WITHDRAWN / SUPERSEDED.** Cancel-and-replace was removed, so the risk of losing a press no longer applies. Busy presses are now ignored, which needs native double-press evidence. | `media.rs:328-351` |
| M4C1-SOL-4 / M4C-OPUS-INFO-4 | minor / info | **DEFERRED (accepted).** It is bounded transient work, and the corrected count (at least six) is stated. See INFO-2 for one extra call site. | `host.rs:302`, `:1055` |
| M4C1-SOL-5 | minor | **DEFERRED (accepted).** It is a scoped, disclosed limit with no evidence about player prevalence. | `media.rs` `read_playback` |
| M4C-OPUS-INFO-1 / 2 / 3, property-change events, hover/press feedback | info | **DEFERRED, unchanged.** None is fixed, and none is claimed fixed. None is material for this gate. | — |

### Command bound (M4C1-SOL-1), and correction of the prior Opus claim

- **Prior assertion withdrawn.** My predecessor's report (`m4-media-controls-opus-1.md`, MIN-4) said: "Session end and `Drop` cancel it, so it is bounded." **That was wrong.** `Cancel` is a request (`IAsyncInfo::Cancel`). The old code started a new `Try*Async` before cancelling the previous one, and each `when` closure (holding an `Arc` slot and an `Arc<Wake>`) stayed registered on its operation until a terminal completion. So N presses against a player that ignores cancellation could hold N operations and N closures. Sol was right; this report supersedes that sentence.
- **What candidate 2 does.**
  - `admit_command` checks `slot.is_some()` (`media.rs:334`) **before** the factory `start()` runs (`:337`). A busy press creates no COM operation, closure, allocation or log line.
  - `end_session` now only borrows the command (`:598`), calls `Cancel` and keeps the slot.
  - The only place that clears it is `refresh`, after `received` shows a terminal result (`:506-507`). `Completed` also fires for `Canceled` and `Error` statuses, so a cancelled command still releases the budget.
  - Session churn cannot reset the budget. No other code writes `Media.command`; I checked for any `command.take` or reassignment.
- **Bound.** At most **one** ROQA-admitted command operation, **one** completion closure (an `Arc<OnceLock>` plus an `Arc<Wake>`) and **one** `Request` exist globally, until a terminal completion arrives or the process shuts down. Fetch requests are separate (see INFO-3).
- **Failure path.**
  - If `start()` itself fails, nothing is retained.
  - If `Request::start` fails, the closure was never registered (receipt 4), and only the operation proxy is kept, with an empty slot that never fires (`:341-347`). That is a deterministic fail-closed state: no further command can be admitted until shutdown. It is disclosed and bounded (INFO-4).
  - `Media::drop` still cancels it, and the field is released in the existing order.
- **Wakes.** There is no timer, poll, queue, retry or worker. A completion raises the existing coalesced `READY` bit once.
- **Test.** The new regression holds a cancel-ignoring `spawn` operation through 128 combinations of session end, generation bump and admission. It checks that the factory ran exactly once, that the budget reopens only after `refresh` collects the result, and that the next admission works. It calls `admit_command` directly, not `Media::command`, which needs a live session. That is adequate for the admission logic. It does not cover the registration-failure branch (INFO-4), and it says nothing about server-side withdrawal or player responsiveness, which are NOT RUN.

### ProviderOwnsSetFocus and capture ordering (SOL-2 / MAJ-1)

- **The fix.** The fragment root returns `ServerSideProvider | ProviderOwnsSetFocus` (`notch_uia.rs:170`), which is where Microsoft's `IRawElementProviderFragment::SetFocus` remarks say the opt-out belongs. Items keep `ServerSideProvider` only, and the dock root is unchanged.
- **Ordering.** Root and item `SetFocus` only read the snapshot and post `FOCUS`. Then `enter_focus` reads `dock::foreground_key()` (`host.rs:1004`) **before** `take_foreground`. With the opt-out, UIA no longer activates the host first, so the recorded target is the real previous foreground window.
- **Repeated entries.** A focus move within an already-focused notch keeps the first `return_to` (`focus_control`). A refused foreground request is logged, and no focus is taken.
- **The test** asserts the flag bit; it calls no foreground API.
- **NOT RUN.** Whether Windows grants ROQA the foreground for a UIA-originated request, and whether Narrator or Inspect then sees Escape return the foreground to the recorded window.

### Removal, hide and failure paths restore only while ROQA owns foreground (SOL-3 / MIN-2)

- **Drop order.** `Surface::drop` (`host.rs:118-125`) runs before any field drops:
  1. `release_focus` calls `blur`, then `dock::restore_foreground(self.window.0, return_to)` while the HWND is still alive.
  2. `notch_uia::invalidate`.
  3. The fields drop: `role` first (the notch `Accessibility` unregisters and disconnects), then the shadow, the canvas, and last the `NativeWindow` HWND.
- **Restore rule.** `restore_foreground` (`dock.rs:145-153`) acts only if `GetForegroundWindow() == from` and `valid_target` passes (same PID, visible, still a window, not ROQA). It never takes the foreground back from another app.
- **Removal callers.** Every removal path goes through `Drop`, so every one is covered:
  - `close`, including the IPC hide and Alt+F4 (`WM_CLOSE` posts `CLOSE_SURFACE`, then `close`);
  - topology release, and topology placement failure (`retain`);
  - `rescale_text`;
  - `lose_graphics`, and `refresh_graphics` success or failure (`clear`);
  - an error in a new surface's first paint (the local is dropped);
  - shutdown or a host error (`Desktop` fields drop).
  - `Surface::create` failure builds no `Surface`, so no `Drop` runs; no focus existed.
- **Hide.** `set_visible(false)` releases at `:216`, **before** `window.hide()`. On the fullscreen path, the fullscreen app normally owns the foreground already, so nothing is restored. That is correct.
- **Re-entrancy.** `SetForegroundWindow` inside `Drop` synchronously delivers `WM_ACTIVATE(WA_INACTIVE)` to the notch. `native_window::dispatch` only posts `DEACTIVATED` and touches no `Desktop` state. The later `DestroyWindow` flushes that post, and if it were delivered, `blur` on an unfocused surface does nothing. A `WM_GETOBJECT` arriving meanwhile still sees valid, un-invalidated state on a live HWND. I found no unwinding through FFI and no new panic path.
- **Rebuilds.** They restore before destroying rather than transferring focus to the replacement window, which is the smaller remedy Sol allowed.
- **NOT RUN.** The actual foreground transition on any path.

### Stale children after invalidation (MIN-3)

- **Clearing.** `invalidate` sets `hwnd = 0`, clears `controls`, and resets `focus` and `open`. It runs before the `Accessibility` disconnect.
- **Rejection.** `read` rejects `hwnd == 0` with `UIA_E_ELEMENTNOTAVAILABLE` (`notch_uia.rs:119`), and so does every `Item` path through `current`: properties, pattern, `Navigate`, `GetRuntimeId`, bounds, `act`/`Invoke`/`SetFocus` and `FragmentRoot`. Root `GetPropertyValue`, `HostRawElementProvider`, `Navigate`, `SetFocus`, `ElementProviderFromPoint` and `GetFocus` reject it as well.
- **Harmless leftovers.** The root's `GetRuntimeId`, `BoundingRectangle`, `GetEmbeddedFragmentRoots` and `FragmentRoot` return constants or self, with no HWND use and no post.
- **Result.** A retained child can no longer post to a dead or reused HWND value.
- **Poisoned lock.** If the mutex is poisoned, `invalidate` is skipped, but `read` then returns `E_FAIL`, so the element still fails closed.
- **Scope.** The dock keeps its pre-existing pattern. That is outside this slice, and I am not raising it as a finding.

### Disabled-focus reconciliation (MIN-1) and generation-only publication (INFO-6)

- **Trigger.** `update` captures the generation before `media.refresh()`. It runs `reconcile_focus` when the result is `redraw || generation changed` (`host.rs:1219-1223`). Enabled-flag changes are part of `redraw` (AR1-Q1), and a session change bumps the generation, so both MIN-1 triggers are caught.
- **Steps.** `reconcile_focus` does this for each notch, in order:
  1. Publish first, so the new version and controls are visible.
  2. Leave focus alone if the focused control is still enabled.
  3. Otherwise `step` to the next enabled control and announce it, keeping the original `return_to`.
  4. If no control is enabled, `release_focus`, which restores only while ROQA owns the foreground.
- **Ordering.** The publish-then-announce order means the focus event's element carries the new version.
- **Coverage.** The existing `keyboard_focus_steps_over_disabled_controls_and_wraps` test covers the next-enabled and none-enabled cases of `step`. The integration is checked statically.

### Escape under hover (SOL-6)

`dismiss_focus` clears `engaged` and `intent`, then `release_focus` retargets the panel closed (`blur` closes when `!engaged`) and restores the foreground under the same ownership rule. Ordinary `DEACTIVATED` still uses `blur` alone, keeping the hover semantics, with no restore. Always-animate is unchanged. For the residual, see INFO-1.

## New informational items (not counted findings; no fix required for this gate)

- **M4C2-OPUS-INFO-1: after Escape with the pointer still over the panel, any pointer move re-engages hover.**
  - **What happens.** `hovered` stays true, so the next `WM_MOUSEMOVE` calls `set_notch_hover(true)`. While the close animation is still above `NOTCH_SWELL`, the "re-entered while still closing" branch (`host.rs:356`) reopens the panel at once; after that, it reopens after `INTENT_TIME`.
  - **Assessment.** This follows the accepted hover contract and Sol's remedy, so it is not a defect. It needs a native check.
  - **Optional remedy if native use finds it intrusive.** Leave `engaged = true` in `dismiss_focus` and retarget closed directly, so the panel stays closed until the pointer leaves and re-enters (one line).
- **M4C2-OPUS-INFO-2 (footprint, part of the SOL-4 deferral).** `reconcile_focus` publishes every notch (`:1055`), and the following `paint_notches` publishes visible notches again (`:302`). So each media-change redraw now builds the controls snapshot twice. Animation frames are unchanged, and this is transient and bounded. Fold it into the deferred SOL-4 semantic-snapshot change. Do not fix it alone.
- **M4C2-OPUS-INFO-3 (pre-existing slice-1 code, outside this recheck).** The same "cancel alone does not bound" reasoning applies to `fetch`. `end_session` takes and cancels it (`media.rs:595`), and the next session starts a new fetch. So under rapid session churn, closures for fetches that ignore cancellation could pile up, one per churned session.
  - Churn is driven by the player and session events, and property reads normally complete quickly.
  - It is not counted here, because the code is accepted and unchanged and there is no evidence of a hang.
  - Record it for a later bounded follow-up. Holding `fetch` like the command slot would be a small change; this is not an architecture rewrite.
- **M4C2-OPUS-INFO-4.** The registration-failure branch fails closed until shutdown. That is safe and bounded, but no test covers it, and in that state the notch silently ignores every later press. The probability is very low: the operation would have to fail between `Try*Async` returning and `status()`/`SetCompleted`. Releasing the slot here would also be defensible, because no closure is retained (receipt 4); the deliberate choice is to keep it, and that is disclosed. Native evidence should note any `Media: command` error lines.
- **M4C2-OPUS-INFO-5.** When a session is replaced and the focused control is still enabled, focus is kept without a new focus event. The client's old focused element (old version) then reports unavailable until its next `GetFocus`. This is part of the deferred property-change/event limit, and it fails safely.

## Nit

- `host.rs:242`: in `set_visible(false)`, the tuple assignment `(*engaged, *intent, *focus) = (false, None, None)` and the following `state.focus = None` are now redundant for `focus`, because `release_focus` at `:216` already took it. The `open = false` reset is still needed. Fix: drop `*focus` from the tuple. Optional; it is not worth a new review cycle on its own.

## What's good

- The admission check sits before the factory call, so a busy press costs one `Option` check. That is the tiniest possible bound, and it needs no timer, queue or worker.
- One `Surface::drop` covers every removal path, including ones nobody listed, such as Alt+F4 and first-paint failure. Seven caller-specific edits were not needed.
- The fixes stayed within the three named files. There is no new dependency, feature, thread, timer, hook, IPC route, or taskbar or system mutation. The dock UIA was not rewritten, and there is no player or provider framework.

## Coverage (clean-code-guard, independent review mode)

I read `SKILL.md`, `references/ai-failure-modes.md` and `references/review-checklist.md`. The review was scoped to the correction delta and its callers.

- **Section A (naming & functions):** clean. `admit_command`, `release_focus`, `dismiss_focus` and `reconcile_focus` are intent-revealing, and each is 20 lines or fewer. `admit_command` takes four parameters, one of them a closure. There are no boolean flag arguments.
- **Section B (comments & formatting):** clean. The comments state the reasons: cancel is a request, the budget is global, the restore happens only while ROQA owns the foreground, and retained children are rejected. The `shortcut:` comment documents the busy-ignore limit. The packet's corrected bound and allocation claims now match the code.
- **Section C (SOLID):** clean. The concrete owners are unchanged, and no abstraction was added.
- **Section D (DRY/KISS/YAGNI):** 1 nit (the redundant focus clear). INFO-2's double publish is folded into the existing SOL-4 deferral.
- **Section E (AI failure modes):** clean.
  - Mode 13 (plausible but wrong): the earlier wrong claims, including the prior Opus "cancel bounds it", are corrected in both code and packet.
  - Mode 1 (swallowed errors): `let _ = Cancel()` and the ignored busy press are documented best-effort or policy choices. Command errors are logged.
  - Mode 2 (defensive guards): the guards sit at the UIA trust boundary, where they belong.
  - Mode 12 (fixture success): none. The sentinel HWND appears only in a test.
  - Mode 15 (deleted cleanup): the cleanup paths were strengthened, not deleted.

**clean-code-guard outcome:** 0 critical, 0 important, 1 nit. The reviewer fixed nothing.

## Footprint

- **Binary.** +4,096 B over candidate 1 (713,216 to 717,312 B), measured and reproduced byte-identically.
- **Idle.** No new idle wake source. `reconcile_focus` runs only when a media wake changed the view or the generation.
- **Retained work.** The retained command work is now capped globally at one operation and one closure.
- **Runtime.** Private bytes, CPU, wakes and paint counts were NOT measured, and no improvement is claimed.

## Required gates still missing (not passed by this report)

- **Council.** A fresh Sol High recheck on the identical manifest `7474C26D…0505DF` is required. Council acceptance requires explicit ACCEPT from both reviewers on one identical final manifest.
- **Native and runtime evidence (all NOT RUN):**
  - real-player previous, play/pause and next; busy and double-press behaviour; a hung player or cancel;
  - actual foreground grant and return through UIA focus and Escape, including under hover, and on hide, close, rebuild, device loss, text scale and topology;
  - disabled-state changes under focus;
  - Narrator naming, navigation and disposal;
  - pointer hits and appearance;
  - always-animate with Windows effects off;
  - idle wake and paint counts, CPU, private bytes and handles;
  - the five regression suites.
- **Deferred by the user.** Keyboard and accessibility validation is DEFERRED, and this report does not claim it passed.
- **Still missing.** Slice-2 native and council evidence, and full M4 acceptance.

**No milestone acceptance. No self-approval: this reviewer authored nothing it reviews.**
