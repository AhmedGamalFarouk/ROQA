# M4 media controls candidate 1: independent Opus review 1 (slice 3)

- **Reviewer.** Claude Opus 5.5 High (`claude-opus-5-5`), a fresh delegated review task. It did not write any candidate source. It is the same underlying model as the author, which `PLAN.md` permits. It is read-only except for this report and one new evidence directory.
- **Date.** 2026-11-11.
- **Scope.** The whole slice-3 diff against `30cf00b`: `src/{media,notch_uia (new, untracked),dock_uia,layout,graphics,host,motion,native_window,dock,picker,main}.rs`, plus the controls changes to `docs/design/roqa-visual-spec.md`, the packet `docs/development/m4-media-controls-candidate-1.md` and the ledger tail. Context read: `AGENTS.md`, `PLAN.md`, `PRODUCT.md`, `progress.md` tail, `m4-media-preparation.md` (all of it), and the slice 1 and 2 packets.
- **Disposition (scoped to the manifest below).** **MATERIAL FIX REQUIRED before commit:** 1 major, 4 minor, 6 informational. This is not a council ACCEPT, and it accepts no part of M4. Required runtime evidence is missing (see the last section).

## Inputs and staleness

| Check | Start | End |
|---|---|---|
| `git rev-parse HEAD` | `30cf00b0521604a9aa272c04838c5dabcb582a5b` | same |
| SHA-256 of `docs/development/reviews/m4-media-controls-candidate-1.inputs.sha256` | `3E8136D1EDA17CF612D90B6E3739D82344A9F0B3D5C2001CB315ED2E9ED8E325` | same |
| Manifest entries (`sha256sum -c`) | 45 entries, **45/45 OK** | **45/45 OK** |

`src/notch_uia.rs` is untracked, so `git diff` does not show it. I read it in full from the working tree; its manifest entry matches. Older manifests describe earlier source and were not used as current.

## Checks performed

- `scripts/verify.ps1 -EvidenceDirectory artifacts/verification/m4-media-controls-opus-1-review`, a **new** directory. The frozen `artifacts/verification/m4-media-controls-1` was not touched.
  - checks PASS: fmt, clippy `-D warnings`, and `cargo test --locked` with **65 passed, 0 failed**;
  - release build PASS. `target/release/roqa.exe` is 713,216 B, SHA-256 `22B6BDBA42A344082ECA21D3406C0A47EB8720A0BC113A8DE0EF994E0EE8EBD0`, byte-identical to the packet's figure.
  - The base size (666,112 B) and the author's probe builds were not reproduced.
- **Static review.** The full diff plus its callers and lifecycle: `drain_messages` routing, `animate`, the fullscreen hide path, `close`, `lose_graphics`/`refresh_graphics`, `rescale_text`, topology removal, `Accessibility` drop, `Request`/`received`/`end_session`, and the `WM_ACTIVATE` window procedure. I compared every UIA provider with `dock_uia`.
- **Binding checks** in the pinned `windows-0.62.2` source: `ProviderOptions_ProviderOwnsSetFocus` (`Win32/UI/Accessibility/mod.rs:19625`) and `BitOr for ProviderOptions` (`:19593`) exist; `UIA_E_ELEMENTNOTENABLED` is at `:20362`.
- **Microsoft documentation**, fetched on 2026-11-11 (next section).
- **NOT RUN:** any native or GUI launch, player, Narrator or keyboard run, and any idle, memory or CPU measurement. No desktop-free confirmation was given, so no native run was attempted.

## The packet's four memory-only behaviours, checked against primary docs

| # | Packet claim | Primary source (fetched 2026-11-11) | Verdict |
|---|---|---|---|
| 1 | `SetForegroundWindow` can activate a `WS_EX_NOACTIVATE` window | *Extended Window Styles*, `WS_EX_NOACTIVATE`: "To activate the window, use the SetActiveWindow or SetForegroundWindow function." The same entry says: "The window should not be activated through programmatic access or via keyboard navigation by accessible technology, such as Narrator." *SetForegroundWindow* adds its foreground-rights list and: "It is possible for a process to be denied the right … even if it meets these conditions." | **Verified as permitted**, but not guaranteed. Whether ROQA, a background process acting on a posted UIA request, gets the right is a **runtime unknown**. |
| 2 | `DefWindowProc` gives keyboard focus to a window it activates | *WM_ACTIVATE*: "If the window is being activated and is not minimized, the DefWindowProc function sets the keyboard focus to the window." | **Verified.** |
| 3 | `Invoke` and `SetFocus` on a disabled element should fail with `UIA_E_ELEMENTNOTENABLED` | *UI Automation Error Codes*: "a method that requires an enabled element, such as Select or Expand, was called on an element that was disabled." The *IInvokeProvider::Invoke* and *IRawElementProviderFragment::SetFocus* pages do not name an error for disabled elements. | **Consistent with the documented code meaning**; not stated per method. The choice of code is acceptable. |
| 4 | `WM_ACTIVATE`'s low word is `WA_INACTIVE` on deactivation | *WM_ACTIVATE*: "WA_INACTIVE 0: Deactivated." It is **sent**, synchronously or asynchronously depending on the input queue. | **Verified.** Handling it in the window procedure is correct. |

The same fetch also established the following:

- **`IRawElementProviderFragment::SetFocus`** remarks: "The Microsoft UI Automation framework will ensure that the part of the interface that hosts this fragment is already focused before calling this method. Your implementation should update only its internal focus state … If you prefer that UI Automation not focus the parent window, set ProviderOptions_ProviderOwnsSetFocus in IRawElementProviderSimple::ProviderOptions for the fragment root." This contradicts the candidate's design; see M4C-OPUS-MAJ-1.
- **`IInvokeProvider::Invoke`**: "is an asynchronous call and must return immediately without blocking." Posting to the host loop satisfies this. **Verified.**
- **`IAsyncInfo.Cancel`**: only "Cancels the asynchronous operation." Nothing says whether a delivered GSMTC command is withdrawn. Still **UNVERIFIED**; see MIN-4.

## Findings

### M4C-OPUS-MAJ-1: UIA focus entry breaks the documented SetFocus contract, so focus return can silently fail. Major, P1, confidence medium-high

- **Where.** `src/notch_uia.rs:153-155` (root `ProviderOptions`, `ServerSideProvider` only), `src/notch_uia.rs:193-196` and `:336-338` (`SetFocus` posts `FOCUS`), and `src/host.rs:965-988` (`enter_focus`, which reads `dock::foreground_key()` at `:978` and then calls `take_foreground`).
- **Static evidence.** Per the `SetFocus` remarks quoted above, UIA itself focuses the window that hosts the fragment *before* it calls the provider's `SetFocus`, unless the fragment root sets `ProviderOptions_ProviderOwnsSetFocus`. The candidate does not set it, and its `SetFocus` does not "update only its internal focus state": it posts a request so the host can take the foreground itself.
- **Failure scenario A** (UIA activates the notch as documented):
  1. By the time the posted `FOCUS` is handled, `GetForegroundWindow()` is the notch.
  2. `foreground_key()` at `host.rs:978` records ROQA's own HWND as `return_to`.
  3. On Escape, `restore_foreground` (`dock.rs:145`) rejects it, because `valid_target` excludes ROQA's own process.
  4. Focus is taken and **never returned**. That breaks a required M4 behaviour (`m4-media-preparation.md`: "A keyboard or UIA entry that takes focus must return it to the previous foreground window on close or Escape").
  The packet calls this outcome "benign"; it is not.
- **Failure scenario B** (UIA cannot focus the `WS_EX_NOACTIVATE` host): UIA may fail the client's `SetFocus` without calling the provider at all. Keyboard entry would then be unreachable, independently of ROQA's own `SetForegroundWindow`.
- **Static or runtime.** The contract mismatch is static and doc-backed. Which of A or B real Windows does is a runtime unknown. Either way, the code relies on behaviour the docs say it should opt out of.
- **Minimal remedy (one line).** Return `ProviderOptions_ServerSideProvider | ProviderOptions_ProviderOwnsSetFocus` from the notch root's `ProviderOptions` (`notch_uia.rs:154`). The docs say to set it on the fragment root, and the binding is verified above. ROQA then owns activation: `enter_focus` reads the real previous foreground and only then asks for the foreground, which is the code's existing intent. Leave the dock root unchanged; it never takes focus.
- **Still unknown after the fix.** Whether Windows grants ROQA the foreground for a request that comes through UIA. A refusal is logged and focus is not taken, which is benign.
- **Native evidence.** Narrator or Inspect `SetFocus` on a control, then Escape, with the foreground window recorded before and after.

### M4C-OPUS-MIN-1: Keyboard focus can stay on a control that has become disabled. Minor, P2, confidence high

- **Where.** `src/host.rs:1179-1183` (after `media.refresh()`, focus is handled only when the session disappears); `focus_control` and `notch_key`.
- **Scenario.** The notch has focus on Next. Then either:
  - the player disables Next at the end of the queue, and the `Controls` bits arrive through `PlaybackInfoChanged`; or
  - the session is replaced and `Change::Session` resets `controls` to `[false; 3]` until playback is read.

  `Focus.control` keeps pointing at the disabled control. UIA then reports `HasKeyboardFocus = true` on an element whose `IsKeyboardFocusable` and `IsEnabled` are false. `GetFocus` returns it, and the ring is drawn around a 30 % chip. Enter and Space are no-ops, because `Media::command` checks the enabled state again. Tab and Escape still work, so nothing gets stuck.
- **Remedy.** In the same `media.refresh()` branch, for a focused notch whose control is now disabled, move to `step(enabled, Some(control), true)` and announce it. If no control is enabled, keep focus but do not report it, or call `release_focus`. That is a few lines reusing `step`, `focus_control` and `announce_focus`.

### M4C-OPUS-MIN-2: A notch destroyed while it has keyboard focus never returns the foreground. Minor, P3, confidence high

- **Where.** `Desktop::close` (`host.rs:775-777`, the hide command), `rescale_text` (`:1250`), topology removal (`:1144`), and `lose_graphics`/`refresh_graphics` (`:1209`, `:1219`, `:1230`).
- **Scenario.** A hide command, a text-size change or a monitor change while the notch has focus drops the `Surface`, and `Focus.return_to` with it. Windows then picks the next foreground window, often but not certainly the one that had it. The packet records this as a known limit. The "close" case is the user-visible one, because the preparation requires focus return on close.
- **Remedy.** Before the notch-dropping `retain`/`clear` in `close` and `rescale_text`, call the existing `release_focus` loop (blur, then `restore_foreground`). Leave device loss and topology as the recorded limit if preferred.

### M4C-OPUS-MIN-3: Stale notch UIA elements outlive their window and stay "available". Minor, P3, confidence medium

- **Where.** `src/notch_uia.rs:236-245` (`Item::current` checks only `version`) and `:259-265` (`act` posts to `state.hwnd`); `Accessibility::drop` (`dock_uia.rs`) disconnects only the root provider.
- **Scenario.**
  1. The notch is destroyed (hide, rescale or rebuild), and the shared `State` is never cleared.
  2. Child `Item`s still held by a UIA client keep the `Arc<State>`, with its old `hwnd`, enabled controls and matching `version`.
  3. `Invoke` or `SetFocus` on such an element passes `current()` and posts `WM_APP+4` or `WM_APP+5` to the dead HWND value. That post fails harmlessly, unless the value has been reused by another window, which would then receive a ROQA private message.
  4. `GetPropertyValue` also keeps answering instead of reporting `UIA_E_ELEMENTNOTAVAILABLE`, which the docs define as an element "that no longer exists".

  The dock has the same latent pattern from before this candidate; the notch copies it. Likelihood is low.
- **Remedy.** When the notch role is dropped, clear the shared state (`controls` emptied and `hwnd = 0`) before the `Accessibility` drop. Then `current()` returns `ELEMENTNOTAVAILABLE`. A small `Drop` on a notch-UIA owner struct, or an explicit clear in the teardown paths, does this.

### M4C-OPUS-MIN-4: Cancel-and-replace of commands may drop a rapid earlier press. Minor, P3, runtime unknown

- **Where.** `src/media.rs:448-451`.
- **What is known.** `IAsyncInfo.Cancel` makes no documented promise either way. A fast Next, Next, or Play then Pause, cancels the first operation. If the media service honours `Cancel` before forwarding, the earlier press is lost. If it does not, `Cancel` buys nothing beyond the client-side bound, which is already limited by the user's own presses.
- **Static checks that pass.** A cancelled operation's completion fills only its own orphaned slot and raises one wake (proven by `a_replaced_commands_late_result_stays_in_its_own_slot`). The `when` closure cannot panic. Session end and `Drop` cancel it, so it is bounded.
- **Remedy now.** None required. Add a rapid double-press case to the native evidence. If presses are lost, drop the `cancel()` call: let the earlier operation run, and keep only the latest `Request` for logging.

### Informational

- **M4C-OPUS-INFO-1 (runtime unknown, existing design).** `DEACTIVATED`, `FOCUS` and `INVOKE` are posted messages handled only in `drain_messages`. If a nested modal loop (for example COM's STA outgoing-call loop during a synchronous cross-process GSMTC getter) dispatches one, it reaches `DefWindowProc` and is lost. Losing `DEACTIVATED` would leave a focused panel held open, with its ring, until Escape or session end, because pointer leave no longer closes a focused panel (`host.rs:340`). The dock's `INVOKE` has the same exposure. If native runs show it, a cheap recovery is to blur when a notch with `focus` is found not to be foreground at the next notch input or paint.
- **M4C-OPUS-INFO-2.** If focus moves from notch A to notch B (two monitors), B records `return_to = None`, because the foreground is ROQA's A. The original app is not restored on Escape. This is an edge case; carrying A's `return_to` across would fix it.
- **M4C-OPUS-INFO-3.** Pointer commands fire on `WM_LBUTTONUP` alone. There is no matching button-down on the same control, so a press that starts elsewhere and is released over a control fires it. Hits also count from the first frame `reveal > 0`, when the chip is almost transparent. The dock uses the same release-only rule, so this is consistent. Noted for the native pointer pass.
- **M4C-OPUS-INFO-4 (footprint).** `publish_notch` (`host.rs:274-288`) rebuilds the `Vec` and three `String`s, plus a join `Vec` and a `format!`, on every notch paint, including about 30 animation frames per open or close. That is negligible next to the DirectWrite layouts. It is optional to rebuild only when the generation or the drawn view changed. Not required.
- **M4C-OPUS-INFO-5.** `Root::SetFocus` (`notch_uia.rs:193`) returns S_OK even when no control is enabled, and `enter_focus` then does nothing. It could return `UIA_E_ELEMENTNOTENABLED` when the snapshot has no enabled control.
- **M4C-OPUS-INFO-6.** The UIA `version` is republished only on paint. If a session is replaced while the drawn view stays identical (both `Track::default()`), `media.refresh()` reports no redraw. New elements then carry the old generation, and the host ignores their actions until the next paint (properties arriving, or the minute clock). This is transient and safe, because the host validates.

## Areas checked with no finding

- **Generation and security boundary.** `wParam` control indexes are bounds-checked (`get`) in the provider, in `enter_focus` and in `Media::command`. `Media::command`'s `_ => TrySkipNextAsync` arm is reachable only after `controls.get(control) == Some(true)`, so only indexes 0 to 2 reach it. `lParam` versions are compared with the live generation; truncating to `u32` is harmless. Any same-integrity process can post these private messages, as it already can for the dock. The worst effect is a revalidated media command or a refused foreground request.
- **Disabled controls.** They stay in the tree. `IsEnabled` and `IsKeyboardFocusable` are false, and `Invoke` and `SetFocus` return `UIA_E_ELEMENTNOTENABLED`. They are drawn at 30 % and never hidden. The host checks the enabled state again before acting.
- **Geometry agreement.** Drawing (`graphics::media_chips`) and pointer hits (`layout::media_control_at`) use the same current-progress `notch_body` and the same `media_controls(dpi)`. The notch DPI includes the text-scale factor, and the y range is clipped to the body height while opening, as `PushAxisAlignedClip` does. UIA `BoundingRectangle` and `ElementProviderFromPoint` both use the open-panel body (`progress = 1.0`) and agree with each other. The tests cover 96, 144, 192 and 240 DPI. Chip x (100 to 228 DIP) always lies inside the narrowest body (262 DIP).
- **Threading and FFI.** Providers run on the UI thread and only lock, copy and post. The host never pumps messages while holding the mutex, and `focus_changed` raises its event without the lock. The `when` closures only set a `OnceLock` and post. No new indexing can panic in a callback. The `WM_ACTIVATE` branch still calls `DefWindowProcW`.
- **Ownership.** `Role` is the first field of `Surface`, so the notch's `Accessibility` drops before the HWND. `end_session` cancels the command before the session's tokens go.
- **AR1-Q1.** `media::redraw` and `differs` both compare `controls`, and the tests cover each control. The UIA names derive only from title, artist and `playing`, which `redraw` already compares. The focus ring repaints through `live`. `Tween::retarget` is a no-op for an unchanged target, so a focus move is one frame, not an open animation.
- **Refactors keep behaviour.**
  - `restore_foreground` is the old `Picker::dismiss` body.
  - `runtime_id` is moved code.
  - The `fill_glyph` cover glyph has the same triangle, the same bars (2.0 to 4.2 and 5.8 to 8.0), and the same playing-to-triangle mapping.
  - `pixel_square` is the old closure.
- **Scope.** There is no seek, progress, art, volume, visualizer, timer, deadline, thread, hook, IPC or picker route, and no `Cargo` change. Taskbar, work area and system settings are untouched. The only foreground request is on ROQA's own HWND, after an explicit UIA focus request, and it is never forced. ICG1-1/2/3 are untouched.

## Footprint assessment

- **Wakes and blocking.** Nothing new runs at idle. A command adds one completion wake. Enabled-state changes ride the existing playback event and repaint only on a drawn change. A focus move paints one frame. There is no new timer, poll or wait handle.
- **Queues and retained callbacks.** At most one live command `Request`. A cancelled one keeps only its small `Arc` slot and closure until COM completes it. The UIA snapshot is three short strings.
- **Binary.** +47,104 B (+7.07 %) is measured: I reproduced the candidate binary byte for byte, but not the base or the probe attribution. About 22.5 KB, by the author's probe, is the notch providers that mirror `dock_uia`.
  - A shared, data-driven provider would recover most of that, but it would rewrite accepted dock UIA code. **I do not require it in this slice**; a later separate candidate is reasonable.
  - Dropping command tracking (about 5 KB) is worth doing only if MIN-4's native run shows that cancel loses presses.
  - No other smaller bounded change is justified now.
- **Native.** No native performance figure exists or is claimed.

## clean-code-guard (independent, review mode)

I read `SKILL.md`, `references/ai-failure-modes.md` (first) and `review-checklist.md`.

- **Plausible-but-wrong (failure mode 13).** MAJ-1 is this pattern: provider `SetFocus` was written from memory of how focus "should" work. The `IRawElementProviderFragment::SetFocus` contract says otherwise.
- **Swallowed errors.** The `let _ = UiaRaiseAutomationEvent` and `PostMessageW` results for `DEACTIVATED` are best-effort notifications, consistent with `dock_uia`. Command failures and refusals are logged. Nothing returns false success.
- **Boundary validation.** It is present where untrusted cross-process input enters: UIA calls and posted `wParam`/`lParam`. There are no redundant interior guards.
- **Duplication.** `notch_uia` Root and Item intentionally mirror `dock_uia`. The packet discloses this as a footprint trade-off and defers it with a reason. Not a finding for this slice.
- **Function size.**
  - `notch_key` is about 30 lines, but it is a flat key `match`.
  - `media_plan` was already long; this candidate only adds a call.
  - `enter_focus` and `notch_input` are about 22 lines each.
  - Complexity is low, and none of these warrants a split.
- **Clean.** There is no dead code or speculative option: `FIRST`, `generation()`, `Tween::target`, `Accessibility::root` and the `pub(crate)` helpers all have callers. Names are clear, and the comments explain why. Refactors keep behaviour, as checked above.
- **Result.** 1 material (MAJ-1) and 4 minor findings flagged for the author. The reviewer fixed nothing.

## Code gate

**Material fix required; not safe to commit as is.**

- **MAJ-1** needs the one-line `ProviderOwnsSetFocus` change (or an equivalent the author proves), a new manifest and a fresh independent recheck.
- **MIN-1** and **MIN-3** are cheap and recommended in the same batch.
- **MIN-2** and **MIN-4** can be deferred with a reason.
- A reviewer who authors a fix may not approve it, and this reviewer made none.

## Unresolved items and required runtime evidence (NOT RUN, missing)

- **Foreground grant.** Whether Windows grants the notch the foreground on a UIA focus request, both with `ProviderOwnsSetFocus` and without it as the candidate stands.
- **Foreground recorded before and after** a UIA focus followed by Escape (MAJ-1).
- **Cancel semantics.** Whether GSMTC `Cancel` withdraws a delivered command: rapid double Next, Previous, and Play then Pause.
- **Real players' `Is*Enabled` values,** and toggle-only players (the `shortcut:` in `read_playback`).
- **Lost posted messages.** Whether a nested modal loop ever dispatches the posted notch messages (INFO-1).
- **Real player runs.** Play, pause and skip from pointer, UIA Invoke and keyboard; Narrator naming and navigation; disabled exposure; focus entry and return; destroy and rebuild under focus.
- **Footprint.** Idle wake and paint counts while collapsed and expanded; private bytes, handles and threads.
- **Wider M4.** The always-animate check with effects off, and slice 2's still-missing native evidence and council review.

Keyboard and accessibility validation is **deferred by the user**. Nothing here claims it passed, and the implementation defects above are not deferred testing. **No milestone acceptance.**
