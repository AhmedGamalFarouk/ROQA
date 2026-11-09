# M1 iteration 2 - independent Sol recheck

Date: 2026-11-09. Reviewer: fresh GPT-6.1 Sol High review sub-agent. Fix/recheck round 1 of 3. Candidate author: `/root`, including all implementation, fixes, tests and candidate documentation. I authored none of those inputs, edited none, spawned no agents, and did not read the iteration-2 Opus report before forming these findings.

## Disposition and exact inputs

**ACCEPT** for `docs/development/reviews/m1-iteration-2.manifest`, SHA-256 **`3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C`**.

Open findings: **0 blocker, 0 major, 1 new nonblocking minor**. Independent changed-production-code clean-code-guard: **PASS**. This is only Sol's disposition. Both council models must explicitly accept this identical manifest before M2; I do not infer the other disposition.

Read project rules, PLAN, PRODUCT, progress, candidate-2 packet, normative council decision/assessment notes, both initial M1 reports and manifest, relevant implementation/test/tooling paths, and frozen evidence. Before review and after report creation/checks, the manifest and **all 41 input hashes match**, with zero missing/mismatching inputs. Final expected/actual values are in `artifacts/m1-sol-recheck-2/input-integrity.json`; the initial complete 41/41 check is in the tool transcript. Progress and this report are intentionally not manifest inputs.

Comparing shared paths against the initial manifest identifies exactly five changed implementation/harness files: `src/host.rs`, `src/native_window.rs`, `tests/NativeProbe.cs`, `scripts/smoke.ps1`, `scripts/measure-idle.ps1`. Other additions are the new packet, evidence and retained initial review records. PLAN, PRODUCT, graphics, layout, IPC, main, app manifest and dependency/toolchain definitions are unchanged. This is not a new cosmetic audit of unchanged code.

## Prior finding dispositions

| Stable ID | Independent disposition | Evidence and boundary |
| --- | --- | --- |
| M1-OPUS-MAJ-1 | **RESOLVED for M1** | Startup and settings/display reconciliation query a new primary-monitor PMv2 HWND with GetDpiForWindow, not sign-in system DPI. Synthetic reconciliation compares against a separate explicitly PMv2 primary HWND. Source/API reasoning and synthetic coverage support the correction; physical changed-since-sign-in scaling remains unperformed. |
| M1-OPUS-MIN-1 | **RESOLVED for the submitted M1 remedy** | `src/host.rs:168` returns before destruction when work bounds and DPI match. Frozen smoke records unchanged-settings HWND preservation. Further coalescing/in-place resizing are explicitly deferred; one temporary query HWND per reconciliation remains. |
| M1-OPUS-MIN-2 | **RESOLVED for M1** | Cursor checks/retries are bounded; stable wrong pixels fail. External foreground loss/exhausted cursor interference throws with INCONCLUSIVE status; foreground owned by ROQA remains FAIL. This classification was source-reviewed, not live-interference-tested by me. |
| M1-OPUS-MIN-3 | **RESOLVED for M1** | Interior pixels, not just centers, are checked; both probes require exit exactly 1. Post-exit absence is an orphan check with OS cleanup acknowledged. Independent image decoding corroborates the interior claims. Corner/border and direct pre-exit teardown instrumentation are not claimed. |
| M1-OPUS-MIN-4 | **RESOLVED** | `scripts/measure-idle.ps1:1` accepts EvidenceDirectory and resolves it separately from frozen outputs. |
| M1-OPUS-MIN-5 | **RESOLVED by documentation** | Packet explicitly states topmost overlap with fullscreen apps. Suppression remains M3, not certified in M1. |
| M1-SOL-MIN-1 | **RESOLVED by provenance documentation** | Tooling JSON/packet distinguish installer 1.28.2, manager 1.29.3 and compiler 1.91.1. I independently verified manager hash and read-only uninstall DisplayVersion. Attribution remains unknown without a baseline; fresh-setup baseline collection is deferred until authorized. No registry repair/install requested. |

## New nonblocking finding

### M1-SOL-R2-MIN-1 - Frozen verification transcript omits command results

Severity: **minor**, open/nonblocking; evidence capture, not a production-code guard defect.

`docs/development/m1-candidate-2.md:34` says verification.log records successful checks. But `artifacts/m1-candidate-2/verification.log:1` contains only 31 lines: PowerShell session header, command text, blank output and footer. A complete read and searches for Finished, test result, passed and exit results find no actual formatter/clippy/build/test result or decisive exit status. A transcript footer alone does not establish success.

I independently reran all required checks successfully and retained actual stdout/stderr plus exit 0, so this omission does **not** leave a required check unknown or block M1. It does not invalidate separate smoke/idle JSON and PNGs.

Minimal action: on the next authorized evidence refresh, capture native stdout/stderr explicitly with decisive exit status; describe the old transcript as session/command metadata rather than a results log. Preserve frozen artifacts. This minor may be deferred without a new implementation/freeze cycle solely to improve logging.

## DPI correctness, ownership and regression review

- **Current primary placement:** `src/native_window.rs:125` gets the current primary work rectangle with MonitorFromPoint/GetMonitorInfoW. `src/native_window.rs:147` places a 1x1 probe at that rectangle's origin, inside the primary work area. Its owned popup HWND has no WS_VISIBLE, is never shown and never composed. `app.manifest:8` declares PerMonitorV2; no production thread-awareness override was added. GetDpiForWindow on that HWND obtains its monitor DPI, not the stale system-DPI value. This is the supported API for this context; mechanically substituting GetDpiForMonitor would ignore Microsoft's explicit per-monitor-aware-thread caution.
- **Temporary ownership:** `src/native_window.rs:64` returns the existing sole-owner NativeWindow type. The probe is a local owner through the query and drops before primary_dpi returns, including the zero-DPI error path. Creation failure returns without an owned HWND. `src/native_window.rs:114` destroys it on its creating/UI thread. It is never added to Desktop surfaces, forgotten, detached, moved off-thread or composed. Destruction does not post quit: dispatch has no unconditional WM_DESTROY-to-quit branch. This is source-based ownership proof, not runtime instrumentation of every temporary HWND.
- **Errors:** allocation failure and zero DPI propagate explicitly; no 96/system-DPI fallback masks failure. The query runs while the HWND is live. Destruction retains the existing diagnostic Drop implementation; no new unchecked ownership transfer was found.
- **No-change guard:** `src/host.rs:101` initializes work/current DPI before showing surfaces. `src/host.rs:161` recomputes work/current DPI for settings/display notifications and compares both before clearing anything. Equal state preserves HWNDs, composition and hover. Changed bounds at equal DPI, or changed DPI at equal bounds, rebuild only the currently open kinds using updated shared state. Closed surfaces are not reopened. Synthetic WM_DPICHANGED still uses scalar DPI; later reconciliation queries the actual primary display rather than treating synthetic state as persistent truth.
- **COM/teardown:** Surface's canvas-before-window, Desktop's surfaces-before-graphics and run's desktop/controls/class/apartment teardown remain intact. The temporary probe has no COM state. Ordinary failures unwind owners before main exits 1. No new production background thread or retaining closure was added.
- **FFI/reentrancy/panic:** `src/native_window.rs:170` retains catch_unwind and nonzero WM_QUIT. `src/native_window.rs:200` posts scalar work without retaining WM_DPICHANGED's borrowed RECT. Stateful changes remain in the owner loop; create/destroy callbacks neither borrow Desktop nor recursively execute reposition. No new unwind-through-FFI or off-thread COM/HWND path was found.
- **Scope:** no hooks, AppBar/work-area mutation, elevation, startup registration, taskbar hiding, new dependency, polling or system configuration changes were introduced.

Microsoft primary contracts checked directly on 2026-11-09: GetDpiForWindow (per-monitor HWND returns monitor DPI; invalid HWND returns zero), GetDpiForMonitor (calling-context caution), CreateWindowExW (styles/nonactivation), DestroyWindow (creating-thread restriction), and High-DPI improvements (created HWND's awareness context). References:

- `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow`
- `https://learn.microsoft.com/en-us/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor`
- `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowexw`
- `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroywindow`
- `https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-improvements-for-desktop-applications`

## Independent checks and evidence

No resident ROQA, GUI, smoke, idle or control-client invocation was launched by me. Only the non-GUI geometry unit-test harness ran. Opus retained the exclusive native test slot. Frozen inputs were not overwritten; generated reviewer evidence/build outputs are under ignored `artifacts/m1-sol-recheck-2/`.

Used existing wrappers with `CARGO_NET_OFFLINE=true` and separate `CARGO_TARGET_DIR=artifacts/m1-sol-recheck-2/target`:

| Check | Independently observed result |
| --- | --- |
| cargo fmt --check | PASS |
| cargo clippy --locked --all-targets -- -D warnings | PASS, fresh-target compilation |
| cargo test --locked | PASS, 3 passed / 0 failed |
| cargo build --locked --release | PASS, fresh-target compilation |
| Repeated captured wrapper run | PASS, decisive exit 0; actual test/build output retained |
| Manifest and all frozen inputs, before/after | MATCH, 41/41 |
| Runtime executable hash | `4C5BC51B0EE71720A157997C6999235A5EDAD8F5D7787595B661BD10D2D4236B`, matches smoke and idle |

Evidence: `verified-checks.json`, `verified-checks.stdout.log`, `verified-checks.stderr.log` (also preserves PowerShell CLIXML progress), supplemental `checks.log`, `binary-comparison.json`, `image-checks.json`, `input-integrity.json`. Initial fresh compilation succeeded in the tool transcript; separate-stream capture repeated checks successfully against that target. Two Node-based capture-wrapper attempts failed in the tooling environment, not as candidate checks. The successful capture uses a hidden .NET child process and verified exit status.

**Source-to-binary corroboration:** separate-target release SHA-256 is `20F1AE7CB8261425D860621D33DC1E16E9F7EB89FAEBCE83848C806FF34E4054`. Both binaries are 176,640 bytes. Exactly 24 bytes differ: PE TimeDateStamp at offsets 264-265; three parsed debug-directory timestamps at 147444-147445, 147472-147473, 147500-147501; 16-byte RSDS PDB GUID at 147832-147847. Debug directory starts at 147440; RSDS payload at 147828. Code and non-debug data/resources match. This corroborates frozen source/binary identity, not byte-reproducible linking. Neither executable was launched by me.

**Frozen runtime evidence:** smoke JSON is PASS with 305 logged passing checks, not 305 independent scenarios. All six warm/block samples are exactly 11,055,104 private bytes / 245 handles / 7 USER / 0 GDI. The harness keeps dock/host alive across 10 warm-up plus 100 measured notch recreation cycles. This supports that workload's plateau; those cycles do not query primary_dpi on every iteration and do not prove universal leak freedom.

Independently decoded all six PNGs without the candidate capture helper: notch 208x36, dock 300x64, interior RGB 22,27,34 for idle/leave and 42,51,62 for hover. Each notch has 5,848 checked interior pixels and dock 17,320; zero mismatches in every image. Visually inspected dock-hover/notch-idle. This corroborates saved images, not a new live transition. Border/corner exclusions are explicit; fullscreen/edge-overlay coexistence is unproved.

Idle JSON is one 30.0061109-second endpoint observation after five-second settle: private bytes 10,915,840 to 11,227,136; handles 225 to 228; CPU-time delta zero at counter resolution. Recorded hardware: Windows 11 build 26200 / Ryzen 5 PRO 4650G / Radeon / 1920x1080 144 Hz. I did not repeat it. No zero-CPU guarantee, DWM/GPU/power attribution, variance result, M5 verdict or competitor comparison follows.

Manager file independently hashes to `38027D51D69A5D05493627A0634DBEA52D6CD793C8F072D2373D3F4163EEC7DB`. Read-only `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Rustup` returned DisplayVersion 1.29.3 and no DisplayName/UninstallString, consistent with the packet. It does not establish who created that state. I made no install, global PATH edit, registry mutation, commit or branch.

## Code review: changed M1 production code

### Summary

Independent **clean-code-guard review mode: ship for the bounded M1 fix**. **PASS**; counts: **0 critical, 0 important, 0 nits** in changed production code. The evidence-log minor is separate. No implementation edits made.

Applied `C:/Users/Ahmed/.agents/skills/clean-code-guard/SKILL.md`, `references/ai-failure-modes.md` and `references/review-checklist.md` to changed host/native-window code, with layout/graphics/main and test callers as neighbors. Walked the imperatives against the actual fix, prioritizing behavior/lifetime risk over new style feedback on unchanged code.

- Section A (naming/functions): **clean**. primary_dpi/create_hidden have concrete responsibilities; query ownership is local and short. Reposition owns one transition. Multiline Win32 argument lists do not require unrelated extraction.
- Section B (comments/formatting): **clean**. Awareness/ownership comments explain FFI rationale. Existing safety/error style is preserved; independent formatting passes.
- Section C (SOLID): **clean**. NativeWindow owns Win32 lifetime, Desktop owns surface policy, Graphics owns composition. No speculative interface/factory, actor mixing or cross-thread state added.
- Section D (DRY/KISS/YAGNI): **clean**. Surfaces/probe share one hidden allocation/RAII path; no copied create/destroy knowledge or dependency. Geometry equality is a justified guard, not speculative scheduling machinery.
- Section E (AI failure modes): **clean**. Verified API context rather than plausible naming; zero DPI/allocation failures propagate. No fake success, stale fallback, lost ordinary-error cleanup, duplicate owner, dead caller, speculative option or removed boundary validation found. The existing FFI panic catch produces orderly failure, not successful error swallowing.

**clean-code-guard: clean.**

## Acceptance boundary

The material stale-sign-in-DPI defect is corrected for this M1 primary-monitor technical-shell scope, with no identified material regression. Physical scale changes and primary-switch/mixed-DPI/topology hardware coverage remain unperformed; API/source review plus synthetic messages are the actual DPI coverage. Fullscreen suppression, functional keyboard/UIA, Arabic/RTL/text scaling/IME, lock/device-loss, hung-source and media remain later reviewed gates. No M2+ gate was imposed on M1.

**ACCEPT the exact iteration-2 manifest above.** The new logging minor may remain open/deferred and does not block. This does not authorize editing frozen inputs, combine an earlier different-hash acceptance, or start M2 without independent Opus ACCEPT on the identical final manifest.
