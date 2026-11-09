# M1 candidate 2: current display DPI and reliable evidence

Author: `/root` (primary GPT-6.1 Sol High agent). Review iteration 2, fix/recheck round 1 of 3. Status: author verification complete; independent rechecks required. This is the same native renderer exploration attempt, with corrections to candidate 1, not a new stack or a completed product.

## Scope and changes

The M0 plan and product contract are unchanged. M1 remains a primary-monitor Rust/Win32/DirectComposition technical shell with two solid hoverable surfaces, blocking event dispatch and graceful control/lifecycle paths. Dock app listing, accessible controls, text/IME and media remain later gates.

Candidate 1 received Sol ACCEPT and Opus REVISE on manifest `76B1A9EC66BF2033B8DD095B51993A4120B398F8ECF229A25FE87914536EFA8B`. The stricter DPI finding is accepted for correction; no finding is waived or disputed. Those reports and manifest are retained. Candidate 2's changes are in `src/native_window.rs`, `src/host.rs`, `tests/NativeProbe.cs`, `scripts/smoke.ps1`, `scripts/measure-idle.ps1`, this packet and new author evidence. Superseded evidence under `artifacts/m1` is untouched.

| Finding | Submitted response (independent resolution pending) |
| --- | --- |
| M1-OPUS-MAJ-1 | Removed GetDpiForSystem. At startup and settings/display reconciliation, create a hidden nonactivating 1x1 HWND inside the primary work area, query GetDpiForWindow under the embedded PerMonitorV2 context, then destroy it through RAII. A zero DPI or failed HWND creation is explicit failure. Both surfaces use this current primary DPI. The smoke reference HWND explicitly sets PMv2 on its own thread. Synthetic DPI followed by settings/display messages checks both dimensions of both surfaces against that independent HWND's DPI. |
| M1-OPUS-MIN-1 | Store current work area and DPI; unchanged broadcasts return without destroying either surface. A settings-message burst verifies unchanged surface HWNDs. A temporary invisible DPI query HWND is still created per reconciliation; notification coalescing beyond the geometry guard is deferred until measured need. |
| M1-OPUS-MIN-2 | Pixel checks reposition and sample the cursor over 200 ms plus after capture. Retry observed interference at most 15 times. A stable wrong-color capture fails immediately. Exhausted cursor interference or unavailable foreground fixture yields INCONCLUSIVE, throws and cannot pass. If ROQA takes foreground it fails; an external foreground change is inconclusive. |
| M1-OPUS-MIN-3 | Six actual-screen captures check center and all interior pixels outside the corner squares/border, not only center. Both injected failures must exit exactly 1. Post-exit absence is explicitly labeled an orphan check where OS cleanup also applies; the actual resource proof remains in-process churn. No claim of direct pre-exit teardown instrumentation is made. |
| M1-OPUS-MIN-4 | Idle sampling now accepts EvidenceDirectory just like smoke. Independent reviewers can preserve frozen evidence. |
| M1-OPUS-MIN-5 | Explicit limit: topmost surfaces currently overlap fullscreen apps. Per-monitor fullscreen suppression remains M3. |
| M1-SOL-MIN-1 | Recorded active manager version/hash separately from archived installer and pinned compiler in tooling.json. Observed uninstall DisplayVersion is documented with unknown attribution and no pre-bootstrap baseline; no registry repair performed. Next authorized fresh setup must capture registry/PATH baselines and tool identities. |

Microsoft API references checked during this fix: GetDpiForWindow documentation and GetDpiForMonitor documentation. The latter explicitly cautions against use from a per-monitor-aware calling thread, so the fix uses an owned primary-monitor HWND rather than that API. References: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow and https://learn.microsoft.com/en-us/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor .

## Author verification

Commands:

```powershell
./scripts/check.ps1
./scripts/cargo.ps1 build --locked --release
./scripts/smoke.ps1 -SkipBuild -EvidenceDirectory artifacts/m1-candidate-2
./scripts/measure-idle.ps1 -SkipBuild -EvidenceDirectory artifacts/m1-candidate-2
```

Formatter, warnings-denied all-target Clippy, three geometry unit tests and release build passed. `verification.log` records them. An initial attempt to log native stderr through a PowerShell pipeline raised NativeCommandError on normal Cargo progress; switching to a transcript captured the successful checks. This was not counted as a passing command. No Rust source changed after the successful build/check run.

Final native smoke passed **305 assertions** after explicitly setting the reference-window thread to PMv2. Its earlier run also passed; the final run supersedes it. Actual screen captures cover notch and dock idle/hover/leave, with center and interior fill checks. Foreground/work area preservation, intended DPI placement, synthetic DPI and both settings/display reconciliation, unchanged-settings HWND preservation, independent close/reopen, zero-exit CLI/native last-close, and exact failure exits passed.

Resource gate: 10 warm-up plus 100 in-process notch recreation cycles while dock stays alive. Warm and all five measured blocks are identical: **11,055,104 private bytes, 245 kernel handles, 7 USER, 0 GDI**. Limits remain <=5 MiB / <=10 handles / <=5 USER/GDI above warm baseline, plus rejection of sustained strictly increasing samples. This supports only this workload, not general leak freedom.

Idle observation: release, 5-second settle, 30.006-second endpoint sample on the recorded Windows 11/Ryzen/Radeon machine. Private bytes **10,915,840 to 11,227,136**, handles 225 to 228, CPU delta 0 at counter resolution. DWM/GPU/power/shared allocations and repeated-run variance are excluded; this is not a zero-CPU guarantee, M5 budget verdict or competitor comparison.

Final release binary SHA-256: `4C5BC51B0EE71720A157997C6999235A5EDAD8F5D7787595B661BD10D2D4236B`, matching both final smoke and idle metadata. Evidence: `artifacts/m1-candidate-2/{smoke.json,idle.json,verification.log,tooling.json}` and six PNGs. All are included in the new manifest despite being ignored build artifacts. No ROQA test process was left running.

Tool provenance: archived installer 1.28.2 remains checksummed as before; active manager is rustup 1.29.3, SHA-256 `38027D51D69A5D05493627A0634DBEA52D6CD793C8F072D2373D3F4163EEC7DB`; compiler is pinned 1.91.1. The read-only uninstall entry observation does not establish who created it. No reinstall, global PATH edit or registry mutation was performed in this fix batch.

## Boundaries and independent recheck contract

No real display scaling was changed on the user's desktop. Physical changed-since-sign-in/mixed-DPI/topology coverage remains unperformed. Synthetic messages test dispatch and geometry; API selection/ownership require independent source review. The reference thread's DPI awareness is explicit so it does not silently return 96 DPI on scaled hardware. Screen checks omit rounded-corner squares and the one-pixel border; fullscreen/edge-overlay coexistence is not proved. A different running notch can cover the edge, so only the test-owned windows are temporarily moved for actual-screen captures and restored. No unrelated app is stopped.

Functional UIA/keyboard, text scaling/Arabic/IME, multimonitor, lock/unlock, device loss, hung-source handling and media remain scoped to M2+. M1 cannot be accepted as a finished dock/notch or as proof of performance superiority.

Freeze current source/config, normative M0/council context, this packet, initial findings and new author evidence in `reviews/m1-iteration-2.manifest`. Progress ledger and new review reports are intentionally excluded administrative outputs. Both fresh reviewers must verify all entries and accept the identical manifest hash. Review only prior findings, changed implementation/evidence and regressions; new findings on unchanged content must be material and evidenced. Minor findings do not block.

Fresh Sol: independently recheck current-DPI query, ownership and geometry guard, harness/provenance corrections, plus changed-code clean-code-guard. Build/check permitted; no GUI launch while Opus owns the native slot. Write `reviews/m1-iteration-2-sol.md`.

Fresh Opus: independently recheck original major and changed native behavior; run the actual frozen smoke and optionally idle with separate evidence directories (`artifacts/m1-opus-recheck-2`), inspect captures, and leave test-owned instances stopped. No real system scale changes or unrelated-app termination. Write `reviews/m1-iteration-2-opus.md`.

Reviewers must not edit fixes or frozen inputs and cannot overwrite author evidence. M1 remains unaccepted until both independent dispositions pass; M2 cannot start before that gate. No author self-approval.
