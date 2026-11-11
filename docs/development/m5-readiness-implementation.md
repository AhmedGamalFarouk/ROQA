# M5 readiness implementation candidate

Author: implementation sub-agent (sole writer). Base: root main `238c2c8` plus preserved WIP.
Date: 2026-11-11 Africa/Cairo. Status: FIX SUBMITTED, NOT INDEPENDENTLY REVIEWED / NOT ACCEPTED.
Companion manifest: `m5-readiness-implementation.sha256`; it lists only changed candidate files and packets.
The mutable progress ledger and user-owned `.gitignore` are excluded. Manifest SHA is in the handoff.

## Submitted behavior

| Group / finding | Implementation | Verification / remaining gate |
|---|---|---|
| 1 / B1 | Accessibility root held in Option; idempotent disconnect removes registry entry, releases host UIA references and disconnects before notch state invalidation and HWND destruction. Existing HRESULT stderr report remains; no runtime-ID workaround. | Required checks pass. Zero E_INVALIDARG / 110 cycles and resource plateau NOT RUN. |
| 1 / B4 | GUI-subsystem errors retain stderr and write fixed context only to a 16 KiB maximum error.log under LOCALAPPDATA/ROQA, truncated on host start. Dynamic titles/paths/error descriptions never enter fallback. Ordinary control invocations do not truncate it; explicit diagnostics keeps its existing stderr/report path. Informational media/audio traces remain stderr-only. One fixed startup-failure message includes HRESULT; no popup for runtime failure, diagnostics/control calls or debug probes. | No idle I/O; no new background writer. Real failed-start UX and failure-log retention NOT RUN. |
| 1 / P3 | Failure/panic command branches, injected host faults and panic-window-message handler are debug-only. Smoke keeps release lifecycle/render checks and uses a separately built, hashed debug binary for fault probes. | PowerShell parse and release/debug binary string checks pass; native fault probes NOT RUN. With -SkipBuild, first build release and debug using pinned cargo.ps1. |
| 1 / P2 | One atomic admission permit for network launch, no pending queue; held by the closure and released on completion, unwind or failed spawn. Each worker owns balanced STA COM initialization; failure logs fixed context. | Admission/unwind unit test passes; real hung share/launch failure NOT RUN. |
| 2 / B3 shelf | Media and shelf share the existing notch focus/foreground-return model. Empty shelf has an Add plus; all 12 references are reachable through two file slots plus More (ellipsis), wrapping pages. Up/Down/Left/Right/Tab navigate, Enter/Space open, Ctrl+Enter reveal, Delete remove, Ctrl+O add, Escape return focus. UIA names identify page and action; focusability/focus and stale page versions are exposed. | Pagination, mixed focus and stale UIA property tests pass. Native/Narrator/RTL/IME/scaling evidence NOT RUN. |
| 2 / B3 pins | Standard settings combo lists owned pins; native Move pin earlier/later buttons provide tab/mnemonic/Enter access and update the existing persisted pin model. | Existing pin reorder/persistence tests pass; native keyboard/Narrator NOT RUN. |
| 2 / P1 | One on-demand worker STA creates/releases file dialog and results on that STA, with one result slot and one completion message; host receives only copied paths. No cross-apartment COM object or cross-thread modal owner. Close/cancel/unwind releases owned COM; best-effort owned-dialog cancellation at host shutdown, no blocking join. No permanent idle thread, deadline timer or channel polling in idle host passes. | Debug/release compilation passes; timer/store deadlines with picker open, cancel/reopen and quit with picker open NOT RUN. Windows shell dialog calls can themselves hang; admission remains bounded to one. |
| 2 / P4 | Settled topology events refresh only native monitor checkboxes in an open settings window, retaining edits/settings and moving focus to a live control if necessary. | Existing topology tests pass; physical topology change while settings open NOT RUN. |
| 3 / B2 | SPI_GETHIGHCONTRAST and GetSysColor supply opaque system window/text/button colors with no glass rim/shadow; secondary ink is full contrast. WM_SETTINGCHANGE/WM_SYSCOLORCHANGE coalesce a re-read and rebuild only when palette changes. Stored theme and accepted always-animate choice preserved. | Pure palette/notification tests pass; high contrast and user system-color changes NOT RUN. No system setting changed by implementation. |
| 3 / P5 | UIA reuses shelf labels by shelf revision and media labels by event revision/audio level, updates timer text only when changed and formats progress only when changed. No duplicate retained track snapshot or DirectWrite cache. | Required checks pass. Benefit is avoided label construction on unchanged animation frames, not measured CPU/memory savings. |
| 4 / B5 | `release-readiness.md` defines concrete license, third-party notices, signing, distribution, uninstall/data, updates, review/native/performance and publication gates. | License/signing/distribution/update choices OPEN for user. Nothing bought, selected, installed or published. |
| 4 / B6 | Freeze source/docs with exact-path manifest and check evidence; preserve prior packets and smoke fix. | Independent Opus 5.5 High via Chief NEXT; M4/M5 council/CCG/native/performance acceptance still OPEN. |

## Checks and evidence

Sequential pinned verification: group1 (175 tests), group2 (176), group3 (177), final (180).
Each passing verify.ps1 runs fmt --check, clippy --locked --all-targets -- -D warnings, cargo test --locked
and cargo build --locked --release. Initial lint failures were corrected before each passing run.
Final additionally builds the debug host with `scripts/cargo.ps1 build --locked`.
Artifacts: `artifacts/m5-readiness/{group1,group2,group3,final}/`; final has verification.json,
checks stdout/stderr, build stdout/stderr and targeted.json. PowerShell smoke parse and binary probe
string checks are static checks only. `git diff --check` passes. No native application was launched.

Guard rules from clean-code-guard were used during implementation to correct error handling,
COM lifetime/reentrancy and stale UIA semantics. This author did not perform or grant the independent
CCG/council pass required by PLAN; the Chief must obtain it against the frozen inputs.

Release binary: 1,059,840 bytes; SHA-256 `968551D6E7EF115C973022413FCC2F51488745AD28B8E2AE5DAC188688A465F7`.
Debug binary (built, not run) SHA-256 `917A821144CFFA3C538C6CC070AB53EF1D024640C028130038D7A9FD91D20C12`.
Compared with inherited 999,424-byte binary: +60,416 bytes; not a resident-memory or performance measurement.
Final evidence file hashes (artifacts remain untracked/ignored):

- `artifacts/m5-readiness/final/verification.json`: `A0F87CC034556EAED6934BBD91AE35D0239A1DCE3C910F0F05AFD2196BFC9A6E`
- `artifacts/m5-readiness/final/checks.stdout.log`: `AF58D5FB1082577C366E60B7D8DF913EB8811C1EC1FBC9E173F79D8C5749B755`
- `artifacts/m5-readiness/final/checks.stderr.log`: `CBA397F87D34B05AEC9413798071DF2C3429A4D5E0D7E1EF50B17424EED0530C`
- `artifacts/m5-readiness/final/build.stdout.log`: `E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855`
- `artifacts/m5-readiness/final/build.stderr.log`: `646D43E391273F686E5AD30B861A813120D9E3C6E6C7C086D292A762F8F9A8A0`
- `artifacts/m5-readiness/final/targeted.json`: `A693237C683BF30D22CE6BD8760D760A61856A7BC96CAE105CDE1308BAD16C6C`


## Preservation and handoff

Preserved the user's `.gitignore`, existing smoke envelope fix, smoke/native packets/manifests,
readiness assessment, native artifacts and prior progress WIP. No broad staging, reset, commit, push,
land.ps1, global PATH modification, native GUI run, measurement, settings/display/audio/sleep mutation,
Explorer restart, TDR, hooks/hotkeys or taskbar mutation. The rejected roqa-m2b2/target deletion remains
pending and was not retried. Existing packet hashes remain historical, not acceptance of this candidate.

Exact candidate files are in the companion manifest; the source diff is `git diff 238c2c8 -- src scripts/smoke.ps1`.
The new untracked error_log.rs, release checklist and this packet must also be read by the reviewer.
Review relevant preserved WIP before eventual exact-path integration; exclude .gitignore and all artifacts.
The latest user authorizes eventual commit/push only AFTER Chief's fresh Opus 5.5 High review and resolution
of material findings. Mechanical landing then uses Sol-low, fresh verify, land.ps1 -DryRun and reviewed
actual land with required coauthor trailer; this implementation worker does not commit or push.

Native gates: fresh desktop-free confirmation via Chief + foreground check; smoke/UIA churn, picker,
catalog/packaged-hung, Narrator/RTL/IME/text scale/mixed DPI, theme/high contrast, timer/store deadlines,
resume/Explorer/device-loss and M5 performance/comparator matrix remain NOT RUN on this candidate.
Product backlog and open user decisions are enumerated in release-readiness.md. No acceptance claim.


## Round 2 fixes (review `m5-readiness-review.md`, 2026-11-11)

Static fixes plus unit tests; no native/GUI run. `scripts/verify.ps1` PASS: 184 tests (180 + 4), release 1,072,128 B (+12,288 B over round 1).

- **RR-H1 done.** `error_log::initialize()` now runs in `host::run` after `Controls::create`, so a second launch never opens or truncates `error.log`. Already-running is a distinct HRESULT (`ERROR_ALREADY_EXISTS`, 0x800700B7): no dialog, stderr line only, as before. Tests: `dialog_wanted`, `wants_log`. The ordering itself is structural, not unit-testable.
- **RR-M1 done.** `Palette.high_contrast` is `Option<(track, gray)>`; `Palette::track` draws bar tracks in the system face colour (grey text when it equals the window colour), `Palette::ink_dim` draws disabled chips, muted/absent volume fill and missing shelf placeholders in `COLOR_GRAYTEXT`. Test: `high_contrast_keeps_track_and_disabled_distinct_from_ink`. Risk: not seen on a real high-contrast theme (NOT RUN).
- **RR-M2 done.** `change_pins` refreshes the Settings pin combo (keeping the selected row) whenever an edit applies; `clamp_selection` is tested.
- **RR-M3 done.** The monitor-checkbox refresh runs after surface placement; an error is logged and closes Settings, never the host.
- **RR-L1 done.** Pin controls are created before Reset/Close; recreated monitor checkboxes are inserted after the previous sibling in z-order.
- **RR-L2 done.** A failed high-contrast query is logged and falls back to the theme palette (`system_palette` is infallible).
- **RR-L3 done.** `FilePicker::start` collects a finished result whose wake was lost; the host adds those paths (`start_file_picker`).
- **RR-L4 done.** `change_shelf` re-announces only a focused shelf slot.
- **RR-L5 done.** `generation` / `uia_revision` doc comments and the Settings row-height comment corrected.
- **RR-L6 done.** Startup failure also logs the fixed HRESULT (`ROQA startup failure 0x........`).
- Deferred: none. Gates listed in the review stay NOT RUN. Manifest `m5-readiness-implementation.sha256` regenerated (now also covers `ipc.rs` and the review packet files).

## Round 3 fixes (review `m5-readiness-recheck.md`, 2026-11-11)

`scripts/verify.ps1` PASS: 184 tests (the contrast test is extended), release 1,081,856 B, SHA-256 `1E704AB5503A8BF7B6D446F155555214090DA7FF219D2E15742C839BE68A665E`. No native run.

- **RC-M1 done.** The cover tile and the dock hover chip fill with `tint(p.track(..))`; the test asserts they differ from the glyph ink.
- **RC-L1 done.** When the track colour would equal the dim grey, bars draw a 1 px ink outline (`Palette::track_outline`, `draw_track`); the test asserts outline vs grey fill.
- **RC-L2 done.** Refreshed monitor checkboxes trim `\.\` like `build`.
- **RC-N1 done.** A Settings Move selects the target row (`select_pin`) before `change_pins`, which refreshes once.
- **RC-N3 done.** `E_ACCESSDENIED` from `CreateMutexW` is reported as `ALREADY_RUNNING`.
- **RC-N2 done.** `reconcile_focus` takes a predicate over the focused control instead of two flags.
