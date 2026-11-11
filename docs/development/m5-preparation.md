# M5 preparation: core product and performance proof

Prepared 2026-11-11 by a Claude Sonnet 5.5 worker (Tier 2) for the ROQA Chief Thread, on branch `docs/m5-prep` (base local `main` `ff8372a`). Doc-only: no code, script or Cargo edits, no build, no native or GUI run. Every size, time or count below is an **UNVERIFIED** estimate unless a ledger entry is cited. Nothing here is a result or an accepted gate.

Inputs: `PLAN.md` (M5, M6), `PRODUCT.md`, `AGENTS.md` (tiered review model), the tail of `progress.md`, `m4-media-preparation.md`, and file headers or targeted greps of `src/`. No source file was read in full.

## 1. What exists today

| Capability | File(s) | State |
|---|---|---|
| Window catalog, WinEvent ingest, coalescing, 30 s repair | `window_catalog.rs`, `window_source.rs`, `window_model.rs`, `catalog_timing.rs` | Landed (M2a). Static accepted; native matrix partly run. |
| Dock: one slot per catalog group plus a leading picker slot, sorted by `Identity`; activate, cycle, minimize | `dock.rs`, `host.rs`, `graphics.rs` | Landed. Order is derived, not owned. **No pins, reorder or persistence.** |
| Identity: `Application(AUMID)`, `Executable(path)`, `Unavailable(pid)` | `window_model.rs` | Landed. Usable as a pin key; unpackaged and elevated matrix still open. |
| Real icons, bounded cache, STA icon worker (`IShellItemImageFactory`) | `icons.rs` | Landed; also carries album-art decode (M4). Reusable for shelf and pin icons. |
| Window picker (modeless standard-control window with filter edit) | `picker.rs`, `picker_font.rs` | Landed (M2b1). Only existing IME/RTL-capable edit route; the model for a settings window. |
| Dock UIA (ToolBar, Button per slot, Invoke) | `dock_uia.rs` | Landed. Narrator run **NOT RUN** (deferred by user 2026-11-10). |
| Notch with clock, media panel (controls, seek, art, volume, visualizer), UIA, keyboard route | `host.rs`, `graphics.rs`, `layout.rs`, `notch_uia.rs`, `media.rs`, `audio.rs` | M4 slices 1-8 on `main`. Slice 7-8 REVIEW NOT RUN (paused 2026-11-11). All native evidence NOT RUN. |
| Text settings (system text-scale change event) | `text_settings.rs` | Landed. Live relayout on a scale change **NOT RUN** natively. |
| IPC control commands (`--quit`, `--show/hide-notch`, `--show/hide-dock`, `--show-picker`) | `ipc.rs`, `main.rs` | Landed. No settings or shelf command. |
| Diagnostics: `--diagnostics <path>` writes atomic JSON counters; window title/path content is already excluded (lengths only) | `window_catalog.rs` | Landed, opt-in. Media log writes title/artist lengths only. No log rotation or unified report. |
| Multi-monitor topology, fullscreen suppression, resume/Explorer-restart, device loss | `topology.rs`, `native_window.rs`, `host.rs` | M3 accepted statically; some native runs done. |
| Idle measurement script (`-SampleSeconds`, cycles, handles, USER/GDI) | `scripts/measure-idle.ps1` | Exists, ROQA-only. No scenarios, descendants accounting or competitor path. |
| Native probes and suites | `scripts/smoke.ps1`, `catalog-smoke.ps1`, `picker-smoke.ps1`, `topology-check.ps1`, `tests/*.cs` | Exist; each native run needs user confirmation. |
| Pins, reorder, persistence | none | **Missing** |
| Timers | none (only the clock deadline) | **Missing** |
| File shelf | none (no IDropTarget; `Win32_System_Ole` feature already on) | **Missing** |
| Settings model and UI | none | **Missing** |
| Comparison harness for Nectar and Nusa | none | **Missing** |
| M2 council stack go/no-go (RTL/IME/text-scale/Narrator proof) | `progress.md` | **Open**; PLAN says M3 could not start before it. Carried as an open risk. |

Release binary at last landing: 801,792 B (ledger). Idle CPU was INCONCLUSIVE in the M3 measurement; no budget verdict exists.

## 2. Slice sequence

Order: S1 first (it can run beside everything because it touches no `src/`), then the persistence foundation, then the features that consume it, then cross-cutting accessibility, then the proof run. Slices marked **∥** have disjoint file sets and can run together. `host.rs`, `graphics.rs` and `layout.rs` are the contention hotspots; any two slices that both touch them must be serialized.

### S1. Comparison harness and M5 baseline (scripts and docs only) ∥ everything

- **Scope.** Turn `measure-idle.ps1` into a scenario runner: idle, warm/cold start, churn (window open/close, track change), expanded media; repetitions with a recorded OS build, CPU/GPU, displays and DPI; descendant processes (WebView2, helpers) summed; private bytes, handles, USER/GDI, threads, CPU-time delta over a configurable sample (default 15 min for the static gate); JSON output with an explicit INCONCLUSIVE verdict below the counter floor. ROQA-only baseline first, so the gate numbers exist before any feature. Competitor mode takes a pinned executable path and a SHA-256, and fails without them. No working-set trimming anywhere.
- **Files.** New `scripts/measure-scenario.ps1` (or extend `measure-idle.ps1`), new `scripts/compare-report.ps1`, new `docs/development/m5-harness.md`. No `src/`.
- **Unit-testable.** Verdict logic (floor, ratio, repetition count) as a PowerShell function with a fixed-input self-test. DWM, GPU and power counters stay manual.
- **Native evidence (NOT RUN, needs the user).** A ROQA-only baseline per scenario; each run needs the user to keep the desktop idle and confirm; competitor runs need S10's pre-conditions.
- **Footprint risk.** None to the binary; harness sampling overhead must be recorded. Low.

### S2. Persistence store

- **Scope.** One versioned file under `%LOCALAPPDATA%\ROQA\`. Pure model plus a small loader and writer. Atomic temp-and-rename write (the pattern already in `write_diagnostics`), a size cap, a schema version, unknown fields ignored, and a corrupt or oversized file loaded as defaults with the bad file renamed aside. Writes are dirty-flagged and flushed on a host-loop deadline, plus once on quit.
- **Files.** New `src/store.rs`; one `mod` line in `src/main.rs`. Host wiring happens in S3, S4 and S6, so S2 has no `host.rs` edit.
- **Unit-testable.** Round trip, version skew, truncated, empty and oversized file, concurrent writer leftover temp file, coalescing (N changes inside the delay give one write), and the flush-on-quit decision. All pure or temp-directory tests.
- **Native evidence.** None required. Optionally one crash-during-write run (NOT RUN; needs the user).
- **Footprint risk.** Low: one small file, zero reads after startup, and writes only on change. **Rule in section 3.**

### S3. Pins model and dock merge

- **Scope.** Pure `Pin { identity, display_name, launch }` list owned by ROQA, keyed by `Identity::Application` (AUMID) or `Identity::Executable` (path). `dock::items` merges pins with running groups: pinned running items sit in pin order, unpinned running items follow in the existing stable order, pinned-not-running items are launchers. Launch target is validated when the user activates it (never scanned at startup). The model supports add, remove and move-to-index with bounds. It reads no taskbar pin blob, per PLAN.
- **Files.** New `src/pins.rs`, `src/dock.rs` (items, names, `Action::Launch`), `src/window_model.rs` only if `Identity` needs an accessor; `src/store.rs` section via S2. No `host.rs`, `graphics.rs` or `dock_uia.rs` yet.
- **Unit-testable.** Merge order and stability, duplicate pin, identity moves from `Executable` to `Application`, unavailable identity not pinnable, reorder bounds, max-pins cap, names for pinned-not-running items.
- **Native evidence.** None for the model.
- **Footprint risk.** Low (a `Vec` of a few dozen entries; no new thread).

### S4. Pins interaction: pin, unpin, reorder, launch

- **Scope.** Wire S3 into the host. Pin or unpin from a dock context action and from the picker. Reorder by pointer drag (past a movement threshold) and by keyboard (a modifier plus arrow while a dock slot is focused), only inside the focused window's message handling. Launch of a not-running pin through the shell, user-triggered only. Running-state indicator and a missing-target state. UIA: Button names include "pinned" and "not running", plus actions that expose reorder; focus is returned as in the notch route.
- **Files.** `src/host.rs`, `src/graphics.rs`, `src/layout.rs`, `src/dock_uia.rs`, `src/icons.rs` (icon lookup for a not-running pin by path or AUMID), `src/picker.rs`. Contended; serialize with S5 and S6.
- **Unit-testable.** Hit-test and drag-threshold math, keyboard reorder mapping, drop-slot index, name strings, and that a reorder produces exactly one store dirty mark.
- **Native evidence (NOT RUN).** Drag reorder, keyboard reorder, launch of a packaged and an unpackaged app, an unplugged target, Narrator reading pinned state, RTL slot order, 150% scale and mixed DPI. Needs the user.
- **Footprint risk.** Medium: icon requests for pins that are not running. Bounded by the existing icon cache cap; no request until first paint; no eager load.

### S5. Timers

- **Scope.** A small pure timer set: countdown timers (default max 3), each with an absolute deadline in monotonic ticks. Collapsed pill: shows remaining time only at minute granularity, so no new collapsed repaint rate beyond the clock's. Expanded and visible: a 1 s deadline for the seconds display. Completion: the notch expands once and plays a system sound if enabled; no toast, because notification packaging is M6. Timers persist as absolute wall-clock expiry so they survive a restart and `WM_TIMECHANGE`. Pause is out of scope for the first cut.
- **Files.** New `src/timer.rs`, `src/host.rs` (one more deadline in the existing wait minimum), `src/graphics.rs`, `src/layout.rs`, `src/notch_uia.rs`, `src/ipc.rs` (optional `--timer <seconds>`). Contended; serialize with S4 and S6.
- **Unit-testable.** Deadline math, next-deadline selection, the visible-and-expanded gate (no tick when hidden, collapsed or fullscreen-suppressed), wall-clock change handling, completion fires once, cap enforcement, persistence round trip.
- **Native evidence (NOT RUN).** A timer running while collapsed (paint and wake counts across a minute boundary), completion while hidden by fullscreen, sleep and resume across an expiry, and a clock change.
- **Footprint risk.** Medium if done wrong: a 1 s tick must not run collapsed. **Rule in section 3.** Adds zero wait handles.

### S6. File shelf (references only)

- **Scope.** A list of up to N references (default 12) shown in the expanded notch. Entries store the path and display name only; ROQA never copies or reads file content. Add by `IDropTarget` on the notch (OLE is already initialized on the STA) and by a keyboard route using the standard file dialog. Open and reveal only on user activation, through the shell. Existence is checked lazily at activation, never polled. Reject device (`\\.\`, `\\?\GLOBALROOT`) and relative paths, deduplicate, and mark UNC paths as "not checked" instead of touching the network on the host thread. Icons use the existing icon worker with a fixed small size; no thumbnails by default. Persisted through S2.
- **Files.** New `src/shelf.rs`, `src/host.rs`, `src/graphics.rs`, `src/layout.rs`, `src/notch_uia.rs`, `src/icons.rs`. Contended; serialize with S4 and S5.
- **Unit-testable.** Path validation and canonical key, dedupe, cap and eviction rule, reject list, persistence round trip, missing-target state transitions, and layout of the shelf row (hit-test).
- **Native evidence (NOT RUN).** Drag from Explorer, a locked or deleted file, a UNC path on an unreachable host (host stays responsive), a long path, and an RTL file name. Needs the user.
- **Footprint risk.** Medium-high: thumbnail temptation and drag registration. **Rule in section 3.**

### S7. Settings surface

- **Scope.** A modeless standard-control window in the `picker.rs` style, created on demand and destroyed on close. Practical settings only: which surfaces start (dock, notch), per-monitor enable, timer completion sound, shelf size cap within a hard ceiling, diagnostics opt-in path, and a reset to defaults. No launch-at-login (PLAN forbids startup registration until a reviewed milestone calls for it). Persisted through S2 and applied live.
- **Files.** New `src/settings.rs` (pure model, defaults, clamping) and `src/settings_window.rs`; `src/host.rs` (apply on change), `src/ipc.rs` and `src/main.rs` (`--show-settings`). Overlaps `host.rs` and `ipc.rs` with S5, so serialize.
- **Unit-testable.** Clamping, defaults, migration from older store versions, applying a change to the model, and a diff-of-settings to the surfaces to create or destroy.
- **Native evidence (NOT RUN).** Keyboard-only traversal, Narrator, RTL layout, IME typing in the diagnostics path edit, text scale change while open, and idle state after close (no resident window). Needs the user.
- **Footprint risk.** Low: zero resident cost when closed. Window and font handles are released on close; verified by the S1 churn handle count.

### S8. Accessibility and text completion pass

- **Scope.** Close the gaps the earlier slices expose: a full keyboard map for dock, notch, shelf and settings; UIA names and states for every new control; RTL mirroring of the dock, the media row and the shelf row; live relayout on a text-scale change; and the three M2 proofs still owed (Narrator flow, OS text-scale change, IME typing). Fixes only; no new features.
- **Files.** `src/layout.rs`, `src/graphics.rs`, `src/dock_uia.rs`, `src/notch_uia.rs`, `src/picker.rs`, `src/settings_window.rs`. Serialize after S4 to S7.
- **Unit-testable.** Mirrored hit-testing, tab order tables, accessible-name strings for every state, and text-scale relayout metrics.
- **Native evidence (NOT RUN).** Narrator core flows, keyboard-only use of every surface, Arabic RTL title, Arabic IME composition, 100/150/200% scale, high contrast. Needs the user at the desktop.
- **Footprint risk.** Low.

### S9. Diagnostics report and resource budgets

- **Scope.** One redacted diagnostics report: version, build hash, uptime, counters (wakes by source, paints, scans, store writes, icon-cache and art-cache sizes, timer and shelf counts, handle and thread counts) and error codes. Not included: titles, paths, track data, file names, pin targets or shelf entries. An allowlist of fields is enforced by a test that fails when a new string field is added. The same slice puts every cache and list cap in one `budgets` table with tests that each cap is enforced.
- **Files.** New `src/diagnostics.rs`; small edits to `src/window_catalog.rs` (move the dump), `src/media.rs` (counter), `src/main.rs`, `src/store.rs` (write count). Only light overlap with `host.rs`, so it can run beside S4 to S7 if it avoids that file.
- **Unit-testable.** The allowlist test, redaction of the shelf, pin and track data, cap enforcement, and the counter arithmetic.
- **Native evidence (NOT RUN).** A report from a real session reviewed by hand for sensitive content; churn runs showing caps hold.
- **Footprint risk.** Low: counters are integers incremented where the work already happens; the report is written only on request, never on a timer.

### S10. Performance proof run and milestone gate (no new code)

- **Scope.** Run S1's scenarios on ROQA and, after the user supplies them, pinned Nectar (combined) and Nusa (notch-only) builds: idle (15 min), warm and cold start, churn, expanded media, one, two and (if available) three monitors. Report the council gates from PLAN: one-monitor private bytes, three-monitor private bytes, static CPU, input p95/p99, bounded churn, and the 30% comparison; INCONCLUSIVE where a counter is below its floor.
- **Files.** `docs/development/m5-results.md` and raw evidence under `artifacts/`; no `src/`.
- **Unit-testable.** Only the report arithmetic.
- **Native evidence.** All of it is NOT RUN and needs the user (idle desktop, installs, hardware). Competitor installs are user-scope actions and need an explicit go-ahead.
- **Footprint risk.** This slice is the measurement; the risk is a failed gate and a documented amendment, not code.

**Suggested parallel groups.** (S1 ∥ S2 ∥ S3) then (S4, S5, S6 one at a time; S9 beside them if it avoids `host.rs`) then S7, then S8, then S10. S2 should land before S3 so S3 can include the pin section in its tests.

## 3. Footprint and idle budget notes

| Feature | How it could break the gates | Design rule |
|---|---|---|
| Persistence writes | A flush on every drag step, timer tick or settings edit wakes the disk and the process. | Write only on a real change; one deadline-coalesced write per burst (default delay about 1-2 s, UNVERIFIED); no periodic flush; one write at quit; reads only at startup. Count writes in S9 diagnostics. |
| Timers | A 1 s tick while collapsed breaks the zero-collapsed-repaint rule. | Absolute deadlines in the existing host-loop minimum; no `WM_TIMER`, no thread, no wait handle. Seconds shown only while visible and expanded; collapsed shows minutes. Clear the deadline when hidden, suppressed or no timers. |
| Shelf thumbnails | Decoding previews costs memory, handles and decode CPU, and an unreachable UNC target can hang. | No thumbnails by default; system icons only at a fixed size through the existing bounded worker; no stat or open of UNC or network paths on the host thread; no icon request until the entry is first painted. |
| Shelf and drag registration | An extra OLE drop target is resident per HWND. | Register on the notch only, revoke on destroy; fixed entry cap; nothing happens until a drop. |
| Pin icons | Not-running pins load icons eagerly at start. | Lazy: request on first paint; the existing icon cache cap applies. |
| Keyboard | A global hook or `RegisterHotKey` adds a resident cost and breaks the no-global-hooks rule. | **No global hooks and no global hotkeys.** Keys are handled only in the focused ROQA window's own message handling and through UIA. |
| Settings window | Left resident. | Create on demand, destroy on close; no shared handle outlives it. |
| Diagnostics | A timer-driven log or a growing file. | Opt-in, written only on request, atomic, bounded size, no sensitive content (allowlist test). |
| Harness | Sampling distorts what it measures. | Record sampling overhead; run it from a separate process; never trim the working set. |
| Binary size | Each feature adds code and crate features. | Add `windows` features only in the slice that needs them (as M4 did); record the release-size delta per slice; no new crates. UNVERIFIED budget for the whole milestone until the user sets one. |

## 4. Open questions for the user

1. **Competitor versions and installation.** Which Nectar and Nusa versions, and may the harness install them in user scope? *Default:* the user supplies pinned installers and SHA-256; install is user-triggered; the harness changes no system settings. ROQA-only baselines run first.
2. **Reference hardware.** How many monitors, which OS build and GPU for the three-monitor gate? *Default:* report one and two monitors; mark three-monitor INCONCLUSIVE or NOT RUN unless a third display exists.
3. **Shelf placement.** Where does the shelf live? *Default:* a row in the expanded notch panel, shown when the user toggles it, never in the dock; up to 12 references.
4. **Timer scope and completion.** Countdown timers only, or also stopwatch and alarm? Sound on completion? *Default:* up to 3 countdown timers, expand the notch and play the system sound by default (toggle in settings), no toast.
5. **Settings content.** Is the list in S7 right, and is it correct to defer launch-at-login to a reviewed M6 item? *Default:* yes to both.
6. **Review pause and the older gates.** Reviews are paused (2026-11-11); the M2 stack go/no-go and the M4 gate are unreviewed. Proceed with M5 anyway? *Default:* proceed, label S2 to S9 `REVIEW NOT RUN`, and run one milestone gate when the user lifts the pause, covering M4 and M5 deltas.
7. **Which model runs the milestone gate.** *Default:* Opus 5.5 High only; Sol only if the user grants one round, as in the budget memory.
8. **Pin identity risk.** The M2 packaged and elevated matrix is unrun, so packaged pins may group differently than expected. Accept the risk for M5? *Default:* yes, pin by AUMID when present and otherwise by executable path, and state the limit in the S4 packet.

The data directory (`%LOCALAPPDATA%\ROQA`), the single-file store, the no-startup-registration rule and no thumbnails are plan decisions, not questions.

## 5. Review plan under the AGENTS.md tiers

- **Gate for S1 to S9: `scripts/verify.ps1` only** (fmt, clippy `-D warnings`, tests, release build), run by the orchestrator at Tier 3. When it passes, commit and continue. S1 and S10 are script and doc slices: the PowerShell self-test and a read-through are the gate.
- **Author tier.** Sonnet 5.5 for S1, S2, S3, S7 and S9, which are small and mostly pure logic. Opus 5.5 High for S4, S5, S6 and S8, which touch the hot files and accessibility (per the worker-model memory).
- **One milestone-gate review, after S10 (Tier 1).** A single Opus 5.5 High clean-code-guard pass, plus Sol High only if the user grants a round, over the frozen M5 manifest (all `src/` deltas since `ff8372a`, the harness, the results and the packets). Checklist:
  1. Code: the footprint rules above, no global hooks, no `WM_TIMER` or new thread, cap tests, the diagnostics allowlist, COM ownership and teardown order with the new drop target, persistence atomicity.
  2. Evidence: every gate number comes from S10 raw output with hashes; INCONCLUSIVE is reported as such; no working-set trimming; competitor builds pinned.
  3. Accessibility: the S8 evidence or an explicit NOT RUN list.
  4. Carried items: M2 stack go/no-go, M4 slice 7 and 8 review, and M3 native gaps, listed as open or closed. They are not silently absorbed.
- **Not reviewed individually.** S2 to S9 intermediate commits get no formal review. If the pause is lifted earlier, an optional single check of S4 to S6 together is cheaper than three reviews.
- **Native evidence stays NOT RUN** until the user confirms a desktop-free window, per the standing rule. No milestone acceptance on stale inputs, failed checks or missing runtime evidence.
