# Code review: ROQA M4 slice 3 controls, candidate 1

## Summary

**Code gate: MATERIAL FIX REQUIRED; not safe to commit this candidate as reviewed.** Three major findings affect the required bounds and focus-return contract. This is a fresh independent Sol High correctness and clean-code-guard review, not milestone acceptance; native/player/keyboard/accessibility/performance evidence remains DEFERRED / NOT RUN.

Counts: **3 critical, 3 important, 0 nits** in the clean-code-guard template. Project severity mapping: the three critical findings are **major**, the three important findings are **minor**; **0 blocker** findings. All six findings below are open. Informational assessments and evidence limitations below are not additional counted findings.

Reviewer: GPT-6.1 Sol, High, fresh non-author review task. Candidate author: a different Claude Opus 5.5 High implementation task. I authored no candidate source or fixes, launched no app, made no delegates, and wrote only this new report. Review date: 2026-11-11 Africa/Cairo (review-end input check 2026-11-10T22:56:19Z).

## Critical findings

### M4C1-SOL-1 — Cancel-and-replace does not bound outstanding commands or retained completions

- **Severity / priority / confidence:** major / P1 / high.
- **Exact location:** `src/media.rs:442`; cancellation/replacement at `src/media.rs:448`, completion retention at `src/media.rs:307`, best-effort cancellation at `src/media.rs:321`, session teardown at `src/media.rs:576`.
- **Evidence:** `let operation = match control { ... Try*Async() ... };` starts the next operation before `previous.cancel()`. Cancellation is only `let _ = self.operation.Cancel();`; it neither waits for terminal completion nor reserves any outstanding-work capacity. Each `Request::start` installs a `when` closure owning a separate `Arc<OnceLock<_>>` and `Arc<Wake>`. Dropping the stored request does not remove that closure from an operation still running.
- **Reachable failure:** while an enabled player's command is slow, ignores cancellation, or returns a cancellation error, N pointer/UIA/key presses can start N unfinished operations and retain N completion closures/slots. There is only one entry in `Media.command`, not necessarily one operation in flight. Session replacement discards the same accounting, so a per-session-only fix would still allow accumulation through player churn. A late completion cannot corrupt the newer slot, but it can still retain resources and raise another wake. The new late-result test explicitly uses an operation whose `Cancel` is a no-op; it proves isolation, not a concurrency or retention bound.
- **Evidence distinction:** this is a statically established missing bound under permitted asynchronous behavior, **not a measured GSMTC leak**. Microsoft documents `Cancel` as a request; it does not promise that return means all work and completion state have been retired. The packet's claim that cancellation “bounds a hung player to one request” is unsupported.
- **Minimal remedy:** enforce a small outstanding-command budget **before** issuing `Try*Async`, retaining accounting until completion, including cancelled commands from replaced sessions. The smallest policy is one outstanding command globally, rejecting additional commands until it terminates; if product behavior needs replacement, retain at most one scalar pending intent and revalidate its generation/capability when admitted. A hung operation must consume the budget rather than cause another allocation per press. Use existing completion wakes, without a timer, poll, queue framework, or new worker. Add a deterministic cancel-ignoring test that asserts admitted operation/callback counts remain capped during repeated requests and session churn. Correct the bounded-work claims in the next packet.
- **clean-code-guard:** AI failure mode 13 (plausible but wrong semantics), boundary contract verification, resource-lifetime correctness.

### M4C1-SOL-2 — Default UIA focus handling loses the previous foreground before ROQA records it

- **Severity / priority / confidence:** major / P1 / high.
- **Exact location:** `src/notch_uia.rs:153`; foreground capture at `src/host.rs:978`; return filtering at `src/dock.rs:137`.
- **Evidence:** the root returns only `Ok(ProviderOptions_ServerSideProvider)`. Microsoft explicitly states that UIA focuses the hosting part before calling `IRawElementProviderFragment::SetFocus`, unless the fragment root supplies `ProviderOptions_ProviderOwnsSetFocus`. The provider then posts `FOCUS`; the later host handler reads `dock::foreground_key()`.
- **Reachable failure:** through the documented UIA client focus route, the saved previous window can already be the notch. Escape/session-end calls `restore_foreground`, whose `valid_target` rejects ROQA's own process. The actual previous app is lost. This concerns the sole planned keyboard-entry route, not an optional alternate route. The packet's preactivation uncertainty is a documented platform behavior that must be handled; lack of a native run does not justify classifying the implementation gap as benign.
- **Minimal remedy:** make the notch root own focus with `ProviderOptions_ServerSideProvider | ProviderOptions_ProviderOwnsSetFocus` (the flag exists in pinned windows 0.62.2), and capture the foreign foreground before the host activates the notch. Preserve one target through internal control moves. Verify the ordering in a desktop-free test/fake, then later prove actual foreground grant and return natively. Keep the dock provider behavior unchanged.
- **Evidence distinction:** the mismatch with UIA's documented default is static/documentary evidence. Actual Narrator behavior, foreground permission and focus restoration on this Windows build are **NOT RUN**.
- **clean-code-guard:** API behavioral-contract verification; do not infer runtime semantics from successful compilation or signatures.

### M4C1-SOL-3 — Removing a focused notch discards its return target

- **Severity / priority / confidence:** major / P1 / high.
- **Exact location:** `src/host.rs:777`; topology removal at `src/host.rs:1136`, device-loss removal at `src/host.rs:1209`, text-scale removal at `src/host.rs:1250`.
- **Evidence:** these paths `retain`/`clear` surfaces without releasing their `Focus` through the restoration path. `Surface` has no focus-restoring destructor. `release_focus` is called only when media becomes absent (`src/host.rs:1180`). In-place hiding also clears `focus` directly at `src/host.rs:231`, after hiding the HWND.
- **Reachable failure:** enter through UIA, then issue the existing hide/close command, change text scale or topology, or encounter graphics-device loss. The still-valid saved foreground is dropped with the notch; Windows chooses a replacement window. Rebuilding the notch does not recover the saved target. This is explicitly admitted by the packet, but contradicts preparation's required focus return on close. The user deferred **validation**, not this behavior.
- **Minimal remedy:** provide one bounded focus-release operation used before ROQA-initiated removal/hiding of a focused HWND, while that HWND can still be checked as foreground. Restore only if it still owns foreground and the saved foreign key remains valid; do not take focus back from an app that has already become foreground. For rebuilds, deliberately restore before destroying or transfer focus state to the replacement, with the former being smaller. Cover hide/close, topology, text-scale, device-loss and cleanup paths. No hook, foreground polling, or shared-dock rewrite is needed.
- **Evidence distinction:** loss of the saved state is proven from the lifecycle callers. The resulting native foreground choice was not observed and is not claimed as a tested failure.
- **clean-code-guard:** required cleanup and observable behavior are not optional simplifications.

## Important findings

### M4C1-SOL-4 — UIA names are rebuilt on every animation frame, with more allocation than reported

- **Severity / priority / confidence:** minor / P3 / high.
- **Exact location:** `src/host.rs:280`; construction at `src/notch_uia.rs:47`.
- **Evidence:** every `paint_notch` calls `publish_notch`, which calls `controls(track)` while holding the shared mutex. For a title and artist present, this constructs a temporary `Vec<&str>`, joined string, formatted play/pause name, two fixed-name strings, and the final controls vector: **at least six allocating constructions**, rather than the packet's three allocations. These occur on every open/close frame on each notch, although the names and enabled flags usually stay identical. `GetPropertyValue` also clones the name for non-name properties (`src/notch_uia.rs:280`).
- **Impact:** bounded transient allocation and allocator work, plus a longer mutex critical section; no retained-memory growth or measured CPU regression is established. This is a small, relevant footprint improvement, not grounds to redesign accepted rendering or UIA infrastructure.
- **Minimal remedy:** update the semantic controls snapshot only when generation/title/artist/play state/enabled flags change; update focus/open/dpi separately. Build changed strings outside the mutex and avoid cloning a name for unrelated properties. A bounded fixed three-element representation or direct string construction can also remove the temporary vectors without a generic provider abstraction. Correct the allocation statement. Defer if minimizing the material-fix batch, but retain the limitation explicitly.
- **clean-code-guard:** KISS/YAGNI and accurate cost claims; no new dependency or cache framework warranted.

### M4C1-SOL-5 — Toggle-only GSMTC players cannot use the play/pause control

- **Severity / priority / confidence:** minor / P2 / high for conditional behavior; real-player prevalence unknown.
- **Exact location:** `src/media.rs:344`; command selection at `src/media.rs:444`.
- **Evidence:** enablement reads only `IsPauseEnabled` while playing or `IsPlayEnabled` otherwise, and command dispatch has no toggle branch. The pinned API also supplies `IsPlayPauseToggleEnabled` and `TryTogglePlayPauseAsync`; a player advertising only that supported command receives a permanently disabled center control.
- **Assessment:** this follows the provisional spec's narrower mapping and is honestly disclosed, so it is not independently elevated to a major all-player compatibility requirement. It does matter to the participating-player experience: “player supports toggling” and “player has no play/pause action” are different states. No evidence here establishes which installed/common players exhibit it, and no native result should be invented.
- **Minimal remedy:** a small state/capability selector that prefers the enabled directional action and falls back to toggle when the directional action is unavailable and toggle is enabled; dispatch the selected action, and extend redraw and mapping tests. Otherwise explicitly retain this supported-capability exclusion as a scoped limitation. Do not create a player abstraction or media layer to handle it.

### M4C1-SOL-6 — Escape does not close a keyboard-focused panel while the pointer is over it

- **Severity / priority / confidence:** minor / P2 / high.
- **Exact location:** `src/host.rs:1010`; conditional closing at `src/host.rs:384`.
- **Evidence:** Escape calls `surface.blur()`, but `blur` retargets closed only when `!engaged`. With the pointer in the notch, Escape clears the ring and attempts foreground return while leaving the motion target at 1.0 and UIA `open` true. The packet/spec say Escape closes the panel; this branch implements loss of keyboard focus with hover retention instead.
- **Minimal remedy:** give explicit Escape dismissal its own close operation that clears hover intent/engagement and retargets closed, while keeping ordinary deactivation's hover behavior. Verify the pointer-over case and the mid-animation case without changing always-animate behavior. This is a limited dismissal mismatch; working foreground return would still let the user continue in the previous app.

## What's good

- `media::redraw` includes all three newly drawn enablement flags, with per-control regression assertions: the slice satisfies AR1-Q1's controls carry-forward contract. Focus movement independently requests a paint, and `Tween::retarget` avoids restarting an already targeted open animation.
- Pointer and UIA commands use the existing host owner. UIA revalidates control indexes/capabilities and posted session generation; disabled children remain exposed, return `UIA_E_ELEMENTNOTENABLED` from action/focus requests, and do not disappear from the tree. Old async result slots cannot overwrite a new request's slot.
- No new dependency/feature, timer, deadline, polling loop, global hook, thread or wait handle was added. The dock/picker foreground-helper extraction preserves the prior implementation; the registry/helper changes have both existing dock and new notch callers. The accepted infrastructure does not need a generic rewrite for this slice.

## Coverage

Applied [clean-code-guard](C:/Users/Ahmed/.agents/skills/clean-code-guard/SKILL.md) independently in **review mode**. Read `references/ai-failure-modes.md` first, then the review checklist, DRY/KISS/YAGNI and relevant naming/functions guidance. This is a correctness review, not an author refactor or self-approval.

- Section A (naming & functions): no actionable finding after the requested behavioral/footprint filter. Reviewed the author's long `notch_key` and `media_plan` flags; cohesive key dispatch and the existing geometry plan do not justify style-only extractions.
- Section B (comments & formatting): misleading bounded-work and allocation claims are covered by SOL-1/SOL-4; no separate cosmetic finding.
- Section C (SOLID): clean for this bounded scope. Concrete owners and the root/item COM pair serve present requirements; no strategy/player layer needed.
- Section D (DRY/KISS/YAGNI): SOL-4; reject speculative unification of the two providers solely for estimated binary savings.
- Section E (AI failure modes): SOL-1/SOL-2/SOL-3, with SOL-5/SOL-6 as explicit conditional behavior gaps. Reviewed callback indexes, scalar message marshalling, COM field/drop ordering, failure logging and callers. No fixture-success production path or newly introduced explicit panic path found.

**clean-code-guard: 0 fixed, 6 flagged for author. Outcome: material fixes required.** I did not approve any edits of my own.

## Exact inputs, staleness and review method

- Baseline and observed HEAD at start/end: `30cf00b0521604a9aa272c04838c5dabcb582a5b`, unchanged.
- Manifest: `docs/development/reviews/m4-media-controls-candidate-1.inputs.sha256`.
- Exact raw-byte manifest SHA-256 at start/end: **`3E8136D1EDA17CF612D90B6E3739D82344A9F0B3D5C2001CB315ED2E9ED8E325`**.
- **START: 45/45 raw-byte entries matched. END: 45/45 raw-byte entries matched.** The end verification includes the five frozen artifacts: four stdout/stderr logs and `verification.json`. No author evidence was overwritten. Final report is outside the input manifest.
- Read AGENTS.md, PLAN.md, PRODUCT.md, progress tail, full preparation and controls packets, prior state/paint packets, visual spec including provisional controls changes, and relevant council/architecture disposition context. Reviewed the entire tracked `git diff -- src` and **explicitly read untracked `src/notch_uia.rs`**, not merely the tracked diff. Traced shared dock/picker/native dispatch, creation/failure paths, paint/update/animation, hiding, topology/text-scale rebuild, device-loss recovery and teardown callers. Read pinned dependency implementations for `when`, completion retention and the test operation's no-op `Cancel`.
- Historical state/paint/icon manifests describe earlier source. I did not compare them to current source or infer approval from them. Progress is an excluded mutable administrative ledger, not a way to waive frozen requirements.
- No source, manifest, packet, ledger, root file, `.gitignore` or other review was edited. No commit/push or delegation. The only reviewer-authored artifact is this report.

## Checks actually performed

1. Independent start/end SHA-256 verification of the manifest and all 45 entries; independent HEAD comparison; source/diff/caller/lifecycle inspection; pinned `windows =0.62.2`, `windows-future =0.3.2`, lockfile and Rust `1.91.1` inspection; primary-document checks below.
2. Read all five frozen verification artifacts. They report checks and release build exit 0, **65 tests passed / 0 failed**, fmt and clippy `-D warnings`. These are **author-run checks**, not an independent rerun. Inspected the new/extended tests: they cover geometry, paint planning, stepping, name construction, enabled-state invalidation and stale-slot isolation; they do not test the material focus/budget failure paths above.
3. Independently hashed the existing, unlaunched `target/release/roqa.exe`: **713,216 bytes**, SHA-256 **`22B6BDBA42A344082ECA21D3406C0A47EB8720A0BC113A8DE0EF994E0EE8EBD0`**, matching the packet. The executable itself is not one of the 45 manifest entries. I did not rebuild the base or repeat the scratch attribution probes.
4. **Reviewer cargo/build/test reruns: NOT RUN. Native/GUI/player/keyboard/Narrator/probe/smoke/performance launches: NOT RUN.** No new evidence directory was created. Only existing evidence was read; nothing under `artifacts/verification/m4-media-controls-1` was changed.

## Primary Microsoft behavior checks

Documentation was fetched during this review; bindings alone were not treated as proof of behavior.

| Packet's memory-only assertion | Independent disposition and primary source |
|---|---|
| `SetForegroundWindow` can activate a `WS_EX_NOACTIVATE` window | **Documented.** [Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles) explicitly names `SetActiveWindow`/`SetForegroundWindow` as the activation route. This does not prove ROQA receives foreground in a particular run; Windows foreground restrictions remain relevant. |
| `DefWindowProc` gives keyboard focus to an activated window | **Documented, conditional on activation and not being minimized.** [WM_ACTIVATE](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-activate). Native keyboard delivery remains untested. |
| Disabled `Invoke` and `SetFocus` should fail with `UIA_E_ELEMENTNOTENABLED` | **Invoke verified; exact native SetFocus error prescription remains unresolved.** Microsoft [IInvokeProvider.Invoke](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.provider.iinvokeprovider.invoke?view=windowsdesktop-10.0) specifies `ElementNotEnabledException`; [custom automation peers](https://learn.microsoft.com/windows/apps/design/accessibility/custom-automation-peers) maps it to this HRESULT. [UIA error codes](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-error-codes) supports rejecting enabled-only operations on disabled elements. The native [fragment SetFocus](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nf-uiautomationcore-irawelementproviderfragment-setfocus) page does not prescribe that exact disabled-element HRESULT. The code nevertheless implements the requested rejection for both actions; no change to its chosen error is required by this review. |
| `WM_ACTIVATE` low word is `WA_INACTIVE` on deactivation | **Documented.** [WM_ACTIVATE](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-activate) defines that low word and `WA_INACTIVE = 0`; the new mask matches. |

Additional decisive contracts:

- [Fragment SetFocus](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nf-uiautomationcore-irawelementproviderfragment-setfocus) documents hosting focus before provider entry and the root opt-out; [ProviderOptions](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/ne-uiautomationcore-provideroptions) defines `ProviderOwnsSetFocus`. This underpins SOL-2 rather than leaving preactivation as a generic runtime unknown.
- [IAsyncInfo::Cancel](https://learn.microsoft.com/en-us/windows/win32/api/asyncinfo/nf-asyncinfo-iasyncinfo-cancel) requests cancellation. Microsoft's [advanced concurrency guidance](https://learn.microsoft.com/en-us/windows/apps/develop/cpp-winrt/concurrency-2) describes delayed cancellation observation and cancellation not automatically propagating to nested asynchronous objects. Neither supplies a GSMTC retirement/withdrawal guarantee. This underpins SOL-1; player receipt/withdrawal remains unknown.
- [IInvokeProvider::Invoke](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nf-uiautomationcore-iinvokeprovider-invoke) requires an immediate asynchronous return and permits raising the invoked event before servicing an action where waiting is impractical. The notch's posted dispatch and early event fit that design; the safety comment that the call is always on the UI thread is not itself a threading guarantee.
- [UiaReturnRawElementProvider](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcoreapi/nf-uiautomationcoreapi-uiareturnrawelementprovider) documents the null-provider raised-event-map cleanup. [UiaDisconnectProvider](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcoreapi/nf-uiautomationcoreapi-uiadisconnectprovider) documents disconnection and unavailable errors. The shared owner unregisters before outbound cleanup, and its field is dropped before the HWND. I have not established that every retained child/interface/in-flight posted message is invalidated on native HWND destruction/reuse; no runtime lifetime pass is claimed.

## Footprint, idle and growth assessment

The measured packet delta is **666,112 -> 713,216 bytes: +47,104 bytes (+7.07%)**. The base number is author scratch-build evidence, not independently rebuilt here. Approximate 22.5 KB provider and 5.1 KB tracking attribution came from different pre-guard scratch variants; LTO makes those non-additive. They are not reliable predictions of a shared-provider rewrite's savings.

For three actual controls, COM providers and focus ownership explain real code cost. Fix command admission first (SOL-1), then consider semantic snapshot reuse (SOL-4). Neither needs a new worker, player abstraction, global cache or accepted-dock/provider rewrite. Binary bytes alone do not establish runtime private bytes or CPU. The two UIA provider families repeat mechanisms, but merging them now could spread focus/lifecycle changes into accepted dock behavior for unmeasured savings; leave that to a separately justified candidate.

Source inspection supports **no new autonomous idle wake source**. Media events/completions retain coalesced dirty-bit wake delivery; focus navigation paints one frame after settling, ordinary animations remain bounded, and hidden notches are skipped for paint. Existing clock/catalog/fullscreen scheduling remains; “no new polling” must not be restated as “the whole app never polls.” Enabled-state changes may paint a collapsed visible notch even though the controls are not yet visible; with unchanged drawn state there is no added media frame loop. Actual event frequency, completion frequency, idle paints and resources remain unmeasured, and SOL-1 prevents accepting the stronger retained-work bound.

The new UIA state uses `Arc<Mutex<_>>`, checked child indexes and scalar posts. It does not mutate `Desktop` or D2D from provider methods. Brief synchronization is present, so “cannot block” is too absolute; SOL-4 reduces the avoidable work under that lock. Session generation checks prevent old-session actions once state is published and the host revalidates them. Version truncation to u32 would need billions of session changes to wrap; that is not a material reachable finding here. Publication is tied to paints, so offscreen/hidden snapshots and native provider disposal still need evidence; do not claim fresh UIA state independently of those paths.

## Explicit disposition of unverified items and known limits

| Item from controls/preparation/prior packets | This review's assessment |
|---|---|
| Foreground grant for UIA entry | Still unknown per run. Code requests activation without forcing it and logs refusal. No native pass. Required focus-return implementation gaps are SOL-2/SOL-3, not waived by this uncertainty. |
| UIA activation before provider `SetFocus` | Documented default; SOL-2. Exact Narrator/NOACTIVATE interaction remains NOT RUN. |
| Cancellation withdraws a delivered command / limits a hung player | Withdrawal remains unknown; the claimed bound is contradicted by admission/retention logic, SOL-1. Old-slot isolation is sound but insufficient. |
| Real players' `Is*Enabled`; toggle-only players | Directional mapping is inspected and disabled rejection is implemented. No real-player evidence. Toggle-only exclusion is SOL-5, not a tested absence of such players. |
| Hide/close/topology/text-size/device-loss under focus | Lost target is a source-level required-behavior gap, SOL-3. Actual Windows fallback foreground is untested. Fullscreen app taking foreground should not trigger focus theft back to the old app. |
| Escape while hovering | Source-level mismatch, SOL-6. Foreground return is separately subject to SOL-2/SOL-3 and native permission. |
| No UIA name/enabled property-change events | Known implementation limitation. A fresh provider read returns the snapshot; cached/listening clients are not proactively notified of those changes. Focus events are raised after state publication and outside the lock. Narrator freshness must be tested; no complete accessibility claim. A later targeted event implementation is preferable to polling. |
| No control hover/press feedback | Matches the provisional design; no additional finding. Action outcome is driven by player events. |
| Ring overlaps future progress row | Known provisional design conflict; slice 4 must resolve it with its own geometry review. No progress UI exists in this slice, so no present overlap failure claimed. |
| Unpackaged access / off-host callback evidence | Prior user evidence on slice-1 `478d9e0` established access and off-host callbacks there. It is historical, not a fresh runtime pass for these controls or this binary. |
| Synchronous getters, command initiation, `Cancel`, token removals / hung source / shutdown latency | Still unknown native latency. All remain on host STA; an async return type does not bound call-entry or cancellation/removal latency. No worker rewrite justified without evidence, and no responsiveness claim. |
| Session COM identity stability | Existing identity comparison inspected; real-player stability across play/pause and unrelated sessions remains unverified. |
| `PlaybackRate` / `LastUpdatedTime`; “Changing”/“Opened” mapped paused | Existing limits retained. Rate/timeline are not newly drawn; not blockers for this slice. Progress must verify these before relying on them. |
| Event rate, OS pool threads, startup, private bytes, handles, DComp/GPU/power | Unmeasured. No numerical runtime saving or plateau claim. Existing footprint estimates remain guesses; SOL-1 is an additional bound defect. |
| FFI/panic behavior | Existing native WndProc catches unwind and posts failure; new provider action paths use checked indexes and fallible lock access, and command completions write their own slots and atomics. No new explicit panic source found. No callback panic probe or independent proof of the earlier general Rust abort assertion was run. |
| Appearance, glyphs, RTL ellipsis, text scale, media region hits, animation, fullscreen/rebuild on screen | All NOT RUN. Existing DirectWrite/text-scaling paths are preserved; the new controls use the effective notch DPI. Prior extreme text-scale clipping limit and unproven RTL ellipsis side remain. Always-animate is the user's accepted decision and was not reverted. |
| No-player/permission/error text | Existing plain-clock fallback is retained from the accepted prior slice; clear state text remains a later M4 requirement, not supplied by controls. No claim this candidate completes all M4 states. |
| Art/stream marshalling, Core Audio, endpoint meter cost, seek/volume/progress/visualizer | Outside slice 3 and still unverified as recorded. This candidate does not implement or validate them. |
| ICG1-1/2/3 | Untouched/deferred as instructed; no new review churn or closure claim. |
| Prior slice-2 native/council gaps, five regression suites, Windows 10, long idle trace | Still missing. This review provides no inherited acceptance credit. |

## Required evidence and disposition

**Required runtime evidence remains absent:** real player previous/play-pause/next and capability mapping, session/player replacement and disappearance/hang, pointer rendering/hits, keyboard entry/disabled-state navigation/focus return (including Escape, hide and rebuild), Narrator/UIA invoke and disposal, mid-animation behavior with Windows effects off, static paint/wake counts spanning a minute boundary, idle/churn memory/handles/threads/CPU and five native regression suites. Relevant broader M4 sleep/resume/device/seek/volume cases also remain open for their slices. Keyboard/accessibility validation is user-deferred; that is not an implementation waiver and supplies no passed evidence.

Parent author should fix SOL-1/SOL-2/SOL-3 in a bounded candidate, choose/document dispositions for the three minors, rerun required checks into a **new distinct** evidence directory, freeze new raw-byte inputs and request fresh independent reviews. Do not combine this review with an Opus review of different bytes. This Sol report is a **CHANGES REQUIRED** council input; the other required independent Opus High review and runtime gates cannot be replaced by the author's guard pass or this report. **No full milestone acceptance.**
