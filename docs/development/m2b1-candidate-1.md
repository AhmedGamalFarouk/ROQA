# M2b1 candidate 1: modeless window picker on the live catalog

Author: `/root` (this session took over from thread `53d0a5f2-b6cf-4975-ac6e-b7fa1fead022`, which stopped on a usage limit; it authored the picker source and harness, this session fixed, re-verified and packaged them). Date: 2026-11-09.
Status: author verification complete; independent initial reviews required. This candidate accepts no milestone by itself.

## Scope

M2a is accepted. M2b1 is the first bounded interaction candidate: a **modeless** picker built from the live shared catalog, with standard Win32 controls only.

- Opens on explicit user action only: click on the dock capsule or `--show-picker`. No passive-event focus steal. If Windows denies foreground the picker says so in its status line and does not force it.
- Native filter edit (label `&Filter windows`), list box, status static, Open and Close buttons. Tab/Enter/Escape routing through `IsDialogMessageW` in the existing bounded host loop. **No modal loop**: catalog deadline servicing and publication continue while the picker is open (addresses the M2a Opus carry-forward). Live catalog generations refresh the list and keep the selection by window identity.
- Rows are real catalog windows (`WindowKey` HWND+PID, real title). No mock rows. Enter/double-click/Open revalidates the target (same PID, still a visible window, not ROQA), restores a minimized window asynchronously and calls `SetForegroundWindow`. A failure is reported in the status line.
- Escape/Close/`WM_CLOSE` dismiss and return foreground to the previously foreground window only if the picker still owns foreground and the target revalidates.
- Text: Unicode/Arabic titles through standard controls and a message font taken from `SystemParametersInfoForDpi`, scaled by per-window DPI and `UISettings.TextScaleFactor` (clamped 1.0–2.25). A `TextScaleFactorChanged` subscription relayouts through a scalar thread-message wake. Subscription and RO apartment are released on drop.
- Accessibility: standard edit/list/button controls expose UIA natively; the harness reads them through a project-built native UIA probe (`tests/support/uia_probe.rs`, built as `roqa-uia-probe.exe`).
- FFI: the dialog procedure only posts scalar actions, `catch_unwind` guards it, failure posts `WM_QUIT(71)`; no Rust state is touched reentrantly.

Not in scope, still owed to M2b2 and the full M2 gate: real icons, packaged identity/grouping, elevated/packaged/hung-source activation matrix, own pins, Narrator speech, OS text-scale setting changes, real IME composition, stack go/no-go. M3 cannot start.

## Changes made in this session (author fixes on the inherited WIP)

- `src/picker.rs`: collapsed a nested `if` that failed warnings-denied Clippy (`clippy::collapsible_if`); removed a redundant `EnableWindow` in `refilter` (the unconditional one after it covers all cases).
- `src/host.rs`: single `Desktop::open_picker` replaces two duplicated create paths.
- `scripts/picker-smoke.ps1`: failure-evidence capture is now guarded, so a failed capture no longer masks the real failure and leaves the JSON in `RUNNING`; foreground loss is classified (foreground owned by the resident = FAIL, otherwise INTERFERENCE with HWND/PID/title) and never counts as a pass.
- `README.md`: picker section.

## Author verification (final binary)

Release `roqa.exe` SHA-256 `9D9F0A52DB71A18E306BDFA53AB5343F7B69405327E5932684A853048AEAC3A7`; `roqa-uia-probe.exe` `595A0211434FEE2928255FCE965FD8FF7281C488E2591F71C01E503E09B6EC6E`. All evidence is under `artifacts/m2b1-candidate-1/` and carries the roqa hash above.

- `verification/`: `scripts/verify.ps1`: fmt, locked all-target Clippy with warnings denied, **11 tests**, locked release build; both subprocesses **exit 0** with stdout/stderr retained.
  - The first run of this session **failed** on the Clippy lint above (exit 1); its logs were overwritten by the passing rerun, so only this note preserves it. Fixed in source, not waived.
- `picker-run7/`, `picker-run8/`: `scripts/picker-smoke.ps1 -SkipBuild`, final harness, each **PASS 217 checks**, binary hash matches. Covers: no pre-existing surface; Arabic-titled real fixture; passive launch keeps foreground; explicit open creates the dialog and takes foreground; focus starts in the filter edit; UIA role (50004 Edit), name `Filter windows`, focusable; Arabic filter set through UIA ValuePattern retains the real Arabic title and code points; Tab moves to the list; **Enter activates and the fixture is actually foreground**; live title rename reaches the open picker without stealing focus; no-match filter gives empty list, disabled Open and `0 windows.` status; Escape returns focus to the prior app; 5 warm-up plus 3×10 open/close cycles; quit while the picker is open exits 0 and the picker HWND is gone.
  - Run 8 samples (private bytes / handles / USER / GDI): warm 12,234,752 / 347 / 26 / 17; block1 12,242,944 / 347 / 26 / 17; block2 12,341,248 / 350 / 26 / 17; block3 12,369,920 / 350 / 26 / 17. Run 7: 12,271,616 / 347 / 26 / 17 -> 12,406,784 / 350 / 26 / 17. USER and GDI counts are flat; handles +3, private bytes about +135 KB over 30 cycles. These are workload samples, not a leak-freedom proof; limits in the script are 8 MB / 5 handles / 3 USER / 3 GDI per block.
  - Runs 3 and 4 also PASS 217 on the same binary under an intermediate harness (foreground diagnostic only); they are supporting, not final.
- `catalog/`: native catalog smoke **PASS 67**, 636 generations, hooks_live 0 and stopped true. `shell/`: **PASS 305** (M1 shell regression with the picker-capable host loop). `idle/`: release, 30.0078 s, CPU 0 ms at endpoint-counter resolution, private bytes 11,173,888 -> 11,362,304, handles 234 -> 237. Observation only; DWM/GPU/power not attributed; no budget verdict or competitor comparison.

### Failed, inconclusive and masked runs (kept, not hidden)

| Evidence | Result | Notes |
| --- | --- | --- |
| `picker/` | status stuck `RUNNING`, 93 checks | First run this session. The failure handler's own screenshot threw (`Invalid capture rectangle`) and masked the original error; the real cause is **unknown**. Harness fixed afterwards. |
| `picker-failed-run1/` | INCONCLUSIVE | Windows denied initial fixture foreground (run launched from a different shell). |
| `picker-failed-run2/` | FAIL, 182 checks | Picker did not take foreground within 10 s during the cycle loop (old harness, no owner recorded). |
| `picker-run5/` | FAIL, 4 checks | `Passive launch keeps foreground fixture`; the foreground owner was not recorded (old harness). |
| `picker-run6/` | INCONCLUSIVE | Windows denied initial fixture foreground. |

**Open unknown:** the foreground losses in `picker-failed-run2` and `picker-run5` are not attributed. Four later runs passed with the identical binary, and the current harness will now record the foreground owner, but I did not reproduce a failure under it, so I cannot say whether it was desktop interference (the machine was in use) or a rare product focus race. The independent Opus runtime run is asked to look at this specifically. Do not treat these flakes as proven interference.

## Known risks and limits

- Narrator speech, OS text-scale setting changes and real IME composition are untested; `HasImeContext` is not composition evidence. Text scale is verified only as API integration plus font height.
- `Picker::create` failure currently propagates and exits the host (fail-fast, orderly shutdown); it is not downgraded to a status message.
- Activation covers a same-session, normal-integrity fixture. Elevated, packaged and hung targets are not tested (M2b2/M3).
- No icons, no grouping. Executable/PID grouping remains the provisional M2a identity.
- Inherited M2a minors still open: constructor decomposition, diagnostics transient-lock policy, source-side native fault injection. M3 carry-forwards: DPI/topology message authority, physical changed-scale test.

## Independent review packet

Manifest `reviews/m2b1-iteration-1.manifest`; snapshot `artifacts/review-snapshots/m2b1-iteration-1.zip`. Progress, new review reports and reviewer evidence are excluded. No candidate mutation while either review runs.

Fresh **Sol High**: offline; scope M2b1 diff against the accepted M2a manifest, the packet claims, and the **independent clean-code-guard** on changed production code (`src/picker.rs`, `src/picker_font.rs`, `src/text_settings.rs`, `src/host.rs`, `tests/support/uia_probe.rs`); **no GUI launches**. Fresh **Opus High**: exclusive native slot; runs the picker, catalog and shell scripts from unmodified frozen inputs into its own evidence directory, and investigates the unattributed foreground failures. Neither authored any candidate input. Reports use stable IDs (`M2B1-SOL-*`, `M2B1-OPUS-*`), the exact manifest SHA-256 and a disposition scoped to those inputs.
