# Code review: ROQA M4 controls candidate 2, independent Sol recheck

## Summary

**CODE GATE: SAFE FOR AUTHORIZED LOCAL COMMIT from this independent Sol High recheck, with the recorded minor deferrals.** The prior major command-admission and focus-lifetime defects are resolved in the reviewed code; no material corrective regression was found. This is a code-only verdict, not council ACCEPT or slice/milestone/runtime acceptance; the separate fresh Opus 5.5 High review must cover the identical manifest before the joint code-review gate is complete.

Counts: **0 critical, 0 important, 0 nits newly raised**. Project severity: **0 new blocker, major or minor findings**. Two prior Sol minor findings remain explicitly deferred, as detailed below; they are not claimed fixed or counted as new findings.

Reviewer: GPT-6.1 Sol, High, fresh independent non-author task, 2026-11-11 Africa/Cairo. Original implementation author: a different Opus 5.5 High task. Corrective author: parent Sol 6.1 Medium task, not this reviewer. I authored no implementation or fix and approve none of my own work. No delegation, commit, push, GUI/native application launch or desktop interaction was performed. Reviewer-authored document: only this report. The authorized fresh verification also generated ignored build outputs and logs in its own distinct folder.

## What's good

- Command admission now covers the lifetime of the actual outstanding operation and its completion subscription across session churn, rather than merely retaining the latest tracked request.
- Focus release is attached to the complete surface lifetime, before provider/canvas/HWND teardown; explicit Escape has a separate dismissal path.
- The corrective batch changes only three production files and keeps the existing host, provider and wake mechanisms. No new worker, queue framework, dependency, timer or poll is introduced.

## Coverage

Applied [clean-code-guard](C:/Users/Ahmed/.agents/skills/clean-code-guard/SKILL.md) independently in review mode. Read the AI failure modes and full review checklist. This is a correctness recheck of prior findings and correction regressions, not a refactor request or a fresh cosmetic audit of unchanged content.

- Section A (naming & functions): clean within the corrective scope. The small admission, dismissal and reconciliation operations have clear purposes and present callers; no style-only extraction is justified.
- Section B (comments & formatting): clean within scope. The cancellation, hung-budget and busy-press comments now describe the actual policy. Candidate 2 corrects the historical allocation and cancellation claims without rewriting the historical evidence.
- Section C (SOLID): clean within scope. Existing concrete COM owners remain appropriate; no generic player/provider architecture is needed.
- Section D (DRY/KISS/YAGNI): clean within scope. One production admission helper is shared with its regression. Surface destruction centralizes required cleanup. Existing per-paint semantic allocations remain the explicitly deferred SOL-4, not a new finding.
- Section E (AI failure modes): clean within scope. Traced operation creation, registration errors, cancellation and collection separately; checked pinned bindings and callback implementation; checked stale provider rejection, focus order and teardown callers. No fixture-success implementation, new unbounded retry/intent queue, unchecked external control index or new explicit callback panic path was found. Busy `Ok(())` means deliberate ignored admission, not a fabricated player result; command success still comes from the operation/player events.

**clean-code-guard: clean for this corrective recheck; 0 fixed, 0 newly flagged for author.** Prior minor deferrals remain visible below. No architecture rewrite is requested.

## Exact inputs and staleness

| Check | Start | End of source review |
|---|---|---|
| Local timestamp | 2026-11-11T01:09:37+02:00 | 2026-11-11T01:12:14+02:00 |
| HEAD | `30cf00b0521604a9aa272c04838c5dabcb582a5b` | unchanged |
| Candidate-2 raw manifest SHA-256 | `7474C26D70DF0AB8B97ADB65004859D6FF752BB427189893CCDD8975970505DF` | identical |
| Every raw-byte entry | **49/49 matched, 0 mismatches** | **49/49 matched, 0 mismatches** |

Manifest: `docs/development/reviews/m4-media-controls-candidate-2.inputs.sha256`. Every entry was hashed from bytes with SHA-256, without newline/text normalization, including the five frozen candidate-2 verification artifacts and both prior reports. HEAD remained the stated base; existing candidate changes remain uncommitted. The report is outside its own reviewed inputs. The pre-report end status matched the initial status.

Independently verified the historical archive at `artifacts/candidates/m4-media-controls-1/`: **45/45 entries match the original candidate-1 hashes**. Comparing archived/current source hashes confirms that only `src/host.rs`, `src/media.rs` and `src/notch_uia.rs` changed after candidate 1. Read the correction-only `artifacts/candidates/m4-media-controls-2.delta.patch` and current implementations/callers. The archive matches the original bytes; I did not repeat the parent's reverse-patch recovery procedure. Candidate-1 manifest SHA-256 `3E8136D1EDA17CF612D90B6E3739D82344A9F0B3D5C2001CB315ED2E9ED8E325` is historical and is not a description of current source.

Read project AGENTS/PLAN/PRODUCT and progress tail, full candidate-2 and candidate-1 packets, preparation, accepted visual specification including the provisional controls addendum, and both prior Sol/Opus reviews. Explicitly read the entire new/untracked `src/notch_uia.rs`; tracked `git diff` alone omits it. Inspected supporting dock/picker/registry/native dispatch changes against HEAD and the current host creation, input, hide, topology, text-scale, graphics-loss, failure and shutdown paths. Scope stayed on prior findings and corrective regressions; no new cosmetic audit of unchanged rendering or accepted infrastructure.

## Prior finding dispositions

All resolutions below are **source-level dispositions on the exact candidate-2 manifest**. They do not certify native focus rights, player behavior or accessibility.

| Stable ID | Prior severity / priority | Disposition and evidence |
|---|---|---|
| M4C1-SOL-1 | major / P1 | **RESOLVED (code).** `media.rs:328-350` guards the global slot before invoking the operation factory. `end_session` retains it at `:598-600`; only completed-result collection clears it at `:506-507`. Registration failure reserves it until shutdown. Detailed accounting below. |
| M4C1-SOL-2 / M4C-OPUS-MAJ-1 | major / P1 | **RESOLVED (code).** Notch root `ProviderOptions` at `notch_uia.rs:169` includes `ProviderOwnsSetFocus`. `host.rs:1004-1011` captures foreground before activation and preserves the initial return target across internal focus moves. Dock root behavior is unchanged. Actual grant/return NOT RUN. |
| M4C1-SOL-3 / M4C-OPUS-MIN-2 | major / P1 (Sol); minor / P3 (Opus) | **RESOLVED (code).** `Surface::drop`, `host.rs:118-124`, releases focus while all fields/HWND still exist. In-place hide releases before `hide`, `:211-227`. Existing restoration requires current foreground ownership and a still-valid foreign HWND/PID, `dock.rs:148-158`. All complete-surface removal paths use this destructor. |
| M4C1-SOL-4 / M4C-OPUS-INFO-4 | minor / P3; informational | **DEFERRED, not fixed.** `publish_notch` still constructs semantic names under the snapshot lock every paint (`host.rs:285-297`, `notch_uia.rs:57-76`), at least six allocating constructions with title/artist present, plus property-name clones. Bounded transient work; no measured runtime regression. Keep the small semantic-update optimization for a separate justified change; no cache/provider framework required. |
| M4C1-SOL-5 | minor / P2 | **DEFERRED, not fixed.** `media.rs:370-375` still maps directional Play/Pause only; toggle-only players remain excluded. Real-player prevalence NOT RUN. A later small directional-first/toggle fallback is the remedy if pursued. This follows the narrower provisional slice contract and is not elevated to major without new evidence. |
| M4C1-SOL-6 | minor / P2 | **RESOLVED (code).** Escape calls `dismiss_focus` (`host.rs:1035-1037`), clearing engagement/hover intent before `release_focus` (`:416-423`). With focus present, `blur` now necessarily retargets closed and publishes `open=false`, even with a stationary pointer over the panel. Native pointer/mid-animation behavior NOT RUN. |
| M4C-OPUS-MIN-1 | minor / P2 | **RESOLVED (code).** `reconcile_focus`, `host.rs:1048-1068`, publishes current controls/version, keeps an enabled focused control, moves to the next enabled control and announces it, or releases when none remain. Called on redraw or generation change (`:1219-1223`). |
| M4C-OPUS-MIN-3 | minor / P3 | **RESOLVED (code).** Surface Drop invalidates shared HWND/controls/focus/open before Accessibility drops. Stateful `read` rejects HWND zero; item `current` also validates generation/index. Properties, actions, pattern queries, navigation, runtime ID, bounds and fragment-root access use these checks. Constant identity/empty-result methods need not consult state. Local retained-child regression passes; actual UIA disposal/HWND reuse NOT RUN. |
| M4C-OPUS-MIN-4 | minor / P3 | **RESOLVED BY POLICY REMOVAL.** No cancel-and-replace on a new press. Busy presses are intentionally ignored; cancel on session end does not reopen admission. Rapid-press usability and command withdrawal remain unmeasured, not fixed by assumption. |
| M4C-OPUS-INFO-5 | informational | **RESOLVED (code).** Root SetFocus reads current state and rejects no-enabled-control snapshots with ELEMENTNOTENABLED (`notch_uia.rs:210-223`); invalidated state rejects earlier. Live all-disabled focus was not invoked in this review. |
| M4C-OPUS-INFO-6 | informational | **RESOLVED (code).** Host compares session generation around refresh and republishes/reconciles even without pixel redraw (`host.rs:1219-1223`). Equal drawn pixels no longer postpone the version update until a later paint. |
| M4C-OPUS-INFO-1/2/3 | informational | **DEFERRED, unchanged.** Possible nested-loop message loss, cross-monitor return-target chain, and release-only pointer activation remain limitations. No new concrete blocker/major evidence was found in this recheck. Do not represent them as repaired. |

No regression IDs are raised: no new finding met the requested concrete behavioral/materiality threshold. Missing property-change events, hover/press feedback and ICG1-1/2/3 remain deferred; always-animate remains the explicit user decision, including effects-off operation.

## Global command and callback accounting

The production `Media::command` validates session/control capability, constructs a lazy `start` closure and calls the same `admit_command` exercised by the regression. Constructing that closure does not call `Try*Async`. A nonempty slot returns before `start`, so repeated pointer, keyboard or UIA requests cannot create another operation or register another completion while an old command is unfinished.

For an admitted command, `Request::start` installs one `when` completion owning one result-slot Arc and Wake Arc. The pinned `windows-future 0.3.2` `async.rs:47-63,96-100` uses one completion registration for a started operation, or executes the completion inline for an already-terminal one. It consumes its FnOnce payload. Cancellation is requested by reference; `end_session` does not take/drop/reset the command. New sessions cannot reset the global budget. The outstanding operation and any still-pending completion captures therefore stay accounted for until result collection, including cancel-ignoring commands. Completion changes only the command's own result slot; it does not write a newer session's state.

Registration/status failure is handled conservatively: after operation creation, an error from `Request::start` leaves an owned operation and a fresh empty result slot in `command`. Even if the failing provider retained the attempted callback, repeated presses cannot add more registrations because admission remains occupied. That empty fallback slot is never completed by the attempted callback, so unrelated wakes/cancellation/session changes cannot accidentally reopen admission. The error is logged; shutdown cancels/releases the owned operation. This branch was inspected statically, **not fault-injected** by the current tests.

The regression `cancel_ignoring_command_keeps_its_budget_across_presses_and_sessions` (`media.rs:859-903`) holds the real helper's admitted `IAsyncOperation::spawn` behind a channel. The pinned test implementation's `Cancel` is a no-op. It calls real `end_session`, changes generation and retries admission 128 times; factory invocations remain exactly one and no result is available. After release, real `Media::refresh` collects completion; only then does another factory run. It counts operation-factory admissions, not an independently instrumented COM callback tally; because production registration occurs only once per admitted factory result, it exercises the actual retention mechanism rather than a parallel test-only budget model.

A never-completing command or failed subscription may disable subsequent controls globally until completion/shutdown. This deliberate availability tradeoff is disclosed and bounds retained command work without timers, polling, queued intent or replacement callbacks. It does **not** prove server-side command withdrawal, bounded COM call-entry/Cancel/token-removal latency, OS pool-thread behavior, or all preexisting media read retention under churn. No broader media-latency or memory-plateau claim is granted.

## Focus lifetime, failure cleanup and Escape

Rust runs `Surface::drop` before its fields. The destructor releases focus and invalidates the shared notch snapshot; then `role` (including Accessibility), canvas and finally NativeWindow drop in declaration order. Accessibility unregisters the root and performs UIA cleanup while the HWND remains alive. Poisoned snapshot locks make later reads fail; they do not permit stale actions.

Traced complete-surface close (`host.rs:803`), topology removal (`:1176-1177`), placement rollback (`:1184`), graphics loss/rebuild/failure clears (`:1251`, `:1261`, `:1272`), text-scale removal (`:1292`) and normal/error Desktop destruction. They all reach Surface Drop. A local Surface whose first clock/paint fails before insertion also drops through it. Before a complete Surface exists, fallible construction has no saved focus or published controls; local role/registration/canvas/window owners still unwind through RAII, with registration released before HWND destruction. No fabricated foreground cleanup test is claimed for these paths.

Restoration reads current foreground while the HWND exists and validates the foreign target's PID, existence and visibility. If another app already owns foreground it makes no restoration request. The existing helper ignores SetForegroundWindow's return, so source ordering proves the opportunity and guarded request, **not success**. Real foreground entitlement and return are NOT RUN.

Explicit Escape clears engagement and pending hover intent before blur, so the old hover-dependent branch cannot keep the open target. Ordinary deactivation still blurs without restoring and retains its existing hover semantics. Enabled-state reconciliation publishes before focus notifications, and generation-only changes are published without forcing a media paint. No new continuous idle work follows from these corrections.

## Checks actually performed

- Fresh independent `scripts/verify.ps1 -EvidenceDirectory artifacts/verification/m4-media-controls-sol-2-recheck` completed **PASS**: fmt check, locked all-target clippy with `-D warnings`, **66 passed / 0 failed / 0 ignored**, and locked release build. Checks/build exit codes are both 0; verification timestamps are 2026-11-10T23:09:47Z and 23:09:48Z. This folder did not exist before the run. Neither controls-1 nor controls-2 evidence was overwritten.
- Read the frozen candidate-2 verification JSON and stdout/stderr logs; they independently record the parent's PASS, 66 tests and release build. Their raw bytes still match the frozen manifest after the reviewer run.
- Independently hashed the resulting, unlaunched release binary: **717,312 B**, SHA-256 **`D7E3D685A56CFA2A315B086D04BCC31E9C4DE5B316BC0ED233D0C833E3B14117`**, matching the packet. It is not a manifest input. Base build and prior attribution probes were not repeated; binary size is not runtime memory/performance evidence.
- The two new/replacement regressions passed: cancel-ignoring global admission, and local COM provider focus-option/invalidation checks. The latter uses sentinel HWND 1 as snapshot data; the exercised calls do not activate or query a real window. Existing focus-step tests cover enabled/none/wrap choices. They do not execute real Surface destruction or foreground restoration.
- Verified pinned Rust 1.91.1, windows 0.62.2 and windows-future 0.3.2; inspected actual `ProviderOwnsSetFocus` constant/BitOr binding and async callback/no-op test cancellation implementation. Used the primary Microsoft documentation receipts in both prior frozen reviews for UIA focus, cancellation, activation and teardown contracts; no fresh web fetch was performed or claimed.
- No source, candidate packet, manifest, ledger, other review, root `.gitignore` or system configuration was edited. No hooks, elevation, startup registration, shell/taskbar/work-area mutation, new dependency/feature, timer, thread or wait handle is introduced by the corrections. No native taskbar availability test was run; the existing coexistence design is preserved by source inspection.

## Missing runtime and council gates

**Keyboard/accessibility validation: explicitly DEFERRED / NOT RUN. Native performance: NOT MEASURED.** No desktop-free authorization was supplied, and no native app, probe, smoke suite or GUI was launched. Compilation/unit checks and this report supply no runtime or milestone acceptance.

Still required: actual UIA focus entry/grant and foreground return on Escape, hide/close, disabled-state changes, session end, topology/text-scale/device-loss and shutdown; retained-client disposal; Escape with pointer/motion on screen; real previous/play/pause/next, capability mapping, rapid busy presses, session churn/disappearance/hung players and shutdown latency. Native wake/paint counts spanning a minute boundary, CPU/private bytes/handles/threads, effects-off animation, visual/RTL/text-scale behavior and applicable regression suites remain unpassed. Slice 2's missing native/council evidence and wider M4 seek/art/volume/device/sleep-resume requirements are not supplied by controls.

The separate fresh Opus 5.5 High recheck on **this identical 49-entry manifest hash** remains required for the joint code gate; no result from another task is presumed here. PLAN's two-reviewer council ACCEPT and required empirical evidence remain separate and incomplete. This reviewer recommends no further material corrective batch on current evidence, permits the authorized local-commit code disposition from the Sol side, and grants **no milestone acceptance**.
