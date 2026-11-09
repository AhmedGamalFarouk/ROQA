# M2a candidate 1: real event-driven window catalog

Author: `/root` (primary GPT-6.1 Sol High agent). Status: implementation/author verification candidate; independent Sol and Opus dispositions required. This is the **foundation step within M2**, not acceptance of full M2 or a stack go/no-go. M1 was independently accepted on manifest `3080840DA84AE480AF07B382ABD4DF42D9E2E1956706EEAACB9693E3606E0D7C` before any M2 implementation began.

## Bounded contract

Deliver one catalog for the future dock/notch, with real top-level window metadata, provisional process grouping, narrow event subscriptions, bounded scheduling and explicit cleanup. The visible surfaces remain M1's native solid capsules. This candidate adds no fake launcher content or user-facing app actions.

Full M2 still requires real icons and stable packaged-app identity, user-triggered activation, keyboard navigation, UIA/Narrator, actual Arabic/RTL rendering, system text scaling and an IME-capable editable route. M2b will deliver and independently verify those, then submit the cumulative stack go/no-go. M3 cannot start before full M2 passes both reviewers.

Changed production: `src/window_model.rs`, `src/window_source.rs`, `src/window_catalog.rs`, integration in `src/host.rs` and `src/main.rs`, and Windows feature selections in `Cargo.toml`. No new dependency version, compiler or installation. Changed verification/developer files: `tests/NativeProbe.cs`, `scripts/smoke.ps1`, new `scripts/catalog-smoke.ps1` and `scripts/verify.ps1`, `README.md`, this packet and the new ledger/evidence. M0 normative plan/product/rules are unchanged.

## Architecture and limits

- Six out-of-context WinEvent subscriptions: foreground, minimize start/end, create/destroy/show/hide, title, state and cloak/uncloak. They skip ROQA's own process. Callbacks perform scalar atomic writes into a fixed 256-slot handle buffer and post a coalesced UI-thread wake. They do not read window text, spawn work, acquire application locks or intercept input. This is the window-state subscription work authorized by M2, not an input hook.
- The owning UI thread deduplicates pending HWNDs (bounded at 512), batches for 50 ms and permits one worker request in flight. Request/result queues each have capacity one. Every follow-up uses the same timer. A full buffer/pending-set overflow requests full repair. The main message pump yields after 128 messages or 4 ms so control handles are reconsidered under continuous events.
- One worker owns native source state. Ordinary events inspect affected HWNDs, not the whole desktop. Foreground flags update across its retained records after one scalar foreground query. Startup, overflow, incomplete previous work and a 30-second repair timer use full enumeration. No scan/thread is created per callback. Versioned complete snapshots publish to one UI-thread model for both future surfaces; a complete in-order snapshot can publish while another refresh is pending, avoiding starvation. Out-of-order/duplicate generations are rejected. Incomplete snapshots retain the last complete published records and mark uncertainty.
- Enumeration is capped at 4096 raw HWNDs, 512 eligible records and a checked 250 ms metadata-processing interval. Pending/queued work is bounded. This is a cooperative bound between native calls, not a hard OS-call deadline. Partial scans do not prove disappearance. Shutdown cancels work, drops bounded channels to release blocked sends, joins the worker, kills timers and unhooks on the registering thread. Explicit cleanup errors make normal shutdown fail; Drop remains a cleanup fallback. An unexpectedly disconnected worker result channel is an explicit error when collected, including at the timed repair, rather than being treated as an empty result.
- Source filtering: top-level, visible, titled windows, excluding ROQA's process, tool windows, owned windows without APPWINDOW and cloaked/unknown-cloaking windows. Empty/unreadable captions are omitted. A record is an HWND/PID pair, title, process identity, minimized and foreground state. Own-process windows are excluded before GetWindowText to avoid same-process cross-thread caption messages. No SendMessage title/icon query is made to foreign apps. Process identity uses limited-query rights; denied process identity falls back to a separate PID group without elevation. Identity handles close per batch.
- Grouping by lowercased executable path is **provisional**. Packaged apps/shared hosts, aliases, app IDs, icon identity and inaccessible-window UX need M2b work. Window captions are bounded to 1023 UTF-16 code units; truncated/unsupported windows are not proof of full application parity. No activation can target a stale record in M2a because it exposes no activation action. M2b must revalidate target identity before acting.
- The normal launch keeps data in memory. Explicit `--diagnostics <file>` writes numeric generations, counts, HWND/PID state and title lengths, not titles/executable paths. Parent directory must exist; the file is atomically replaced from a sibling temporary file. Consumers must permit delete-sharing. Generation zero means no snapshot has yet published; readiness requires a published generation and actual expected records. This mode has disk I/O and is not the idle benchmark configuration.

API contracts checked against primary Microsoft documentation on November 9, 2026: SetWinEventHook's registering-thread message loop/out-of-context delivery, UnhookWinEvent's registering-thread restriction, GetWindowTextW's cross-process caption behavior, PostThreadMessageW's queue/message constraints, and QueryFullProcessImageNameW's limited-query rights. Binding signatures were checked against installed windows 0.62.2. References:

- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-unhookwinevent
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postthreadmessagew
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew

## Author verification and known failures

Commands for the final candidate:

```powershell
./scripts/verify.ps1 -EvidenceDirectory artifacts/m2a-candidate-1/verification
./scripts/catalog-smoke.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-candidate-1
./scripts/smoke.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-candidate-1/shell
./scripts/measure-idle.ps1 -SkipBuild -EvidenceDirectory artifacts/m2a-candidate-1/idle
```

`verify.ps1` starts hidden child PowerShell processes, drains stdout/stderr separately, records exact commands and exit codes, and throws on failure/timeout. This addresses M1-SOL-R2-MIN-1's transcript omission. Seven tests include the three M1 geometry cases and four state cases: stale generations/HWND reuse, incomplete snapshots, fallback grouping and 10,000 coalesced invalidations with continued publication. Native results and exact final binary identity are summarized in the ledger and frozen raw evidence, not inferred from unit tests.

The catalog harness launches its own real native fixture in another process relative to ROQA. It exercises initial enumeration, changed Arabic title **metadata length only**, minimize/restore, foreground, hide/show, create/destroy, 501 real title changes, 15 additional fixture lifetimes, actual 30-second timer reconciliation, continued publication during a title storm and graceful shutdown while that storm still runs. It checks fewer than 50 worker batches for the 501-change burst and at most one full repair; raw counters distinguish full scans, targeted batches and inspections. Three lifecycle blocks enforce <=8 MiB private-byte growth, <=10 kernel handles and <=3 USER/GDI each over warm baseline. Explicit post-shutdown diagnostics are written after successful unhook and worker join, adding in-process cleanup evidence beyond OS process-exit cleanup.

The M1 shell smoke runs on the new binary and still requires real captured pixels, focus/geometry/CLI/close/failure behavior and 100 in-process surface cycles. The final run is under `shell/`. The same fixed 5-second-settle/30-second endpoint idle observation is under `idle/`, without diagnostic file writes; its legacy label still says M1 observation, but its binary hash identifies this candidate. This is not a full M2/M5 budget result or a competitor comparison. DWM/GPU/power/shared allocations and repeated-run variance are excluded.

Earlier failed development runs are not counted as passes: PowerShell singleton-array enumeration in the new fixture, an initially bypassed follow-up timer (106 scans for 501 changes), and process ExitCode access before retaining the process handle. The final source fixes those. Ordinary events were then tightened from full census to targeted source updates, and publication was made live during continuous invalidations. The fixture now resets readiness/state, joins its thread and unregisters the class between lifetimes, addressing M1-OPUS-R2-MIN-2 structurally. An earlier two-full-smoke same-PowerShell experiment passed its first run but was INCONCLUSIVE on the second when Windows declined foreground activation; it is not repeated-pass evidence. The final shell harness can establish a stable pre-launch foreground baseline from an existing window when Windows denies the fixture foreground rights; taking focus into ROQA still fails, and outside foreground movement is inconclusive. Two final complete shell runs in the same PowerShell process then both passed. Final authoritative evidence is only the explicitly frozen files, not other development folders under artifacts.

Not verified here: elevated/packaged/secure or UIAccess apps, a genuinely hung foreign app, injected dropped WinEvents/failed PostThreadMessage, forced buffer overflow, hard native-call failure/cancellation fault injection, real display-scale/topology change, Narrator, keyboard activation, RTL shaping, text-scale changes or IME composition. The periodic repair path was exercised, but that is not an injected lost-event test. Physical DPI and trusted DPI-message ordering remain recorded M1/M3 follow-ups. Fullscreen suppression remains M3. These limits keep full M2 open.

## Final author results

- Final binary SHA-256: `78A4C04E562977EE53482E11FA30ACF927AA828184BFD69641FFAA549665224F`. It matches catalog, both shell runs, idle and the executable at freeze.
- Captured check/build exit codes: both 0. Formatter, all-target Clippy with warnings denied, seven unit tests and release build passed. stdout/stderr and commands are in `verification/`.
- Final catalog harness: **PASS 61 assertions**. Burst: **501 changes, 4 targeted batches, 4 affected-HWND inspections, 0 full enumerations**, 280.2704 ms to observed convergence. This is one own-fixture observation, not a product latency guarantee or competitor benchmark.
- All four catalog lifecycle samples: **11,116,544 private bytes, 248 handles, 15 USER, 0 GDI**, with no growth. Final successful unhook/worker-join record has zero live hooks. The repair timer fired once; source counters report 2 full enumerations (startup plus repair) and 47 targeted batches over the whole run. Shutdown while title events continued: **169.1823 ms** for this run.
- Final shell compatibility and same-PowerShell re-entry: **PASS 305 assertions each**, including actual screenshots and 100 in-process surface cycles per run. The shell-reentry fixture is the same loaded C# type; the second result is not from a fresh PowerShell host.
- Idle: **30.0114 seconds**, **15.625 ms** CPU delta (about 0.521 ms/s at counter resolution), private bytes **11,005,952 to 11,337,728**, handles 231 to 234. One short uninstrumented-resident sample; no M5 acceptance, zero-CPU assertion or total-system-memory result.
- All owned ROQA and fixture instances stopped before review. No system scale, taskbar, startup or registry setting changed. The manual scale-change check remains open.

## Independent gate

Freeze source/config, normative plan/council context, accepted M1 baseline reports/manifest, this packet and final evidence in `reviews/m2a-iteration-1.manifest`. Reports for this review and the mutable progress ledger remain excluded. Preserve a local source/evidence zip for later comparison. Both fresh non-author reviewers must accept the same manifest before M2b starts; M2 as a whole remains unaccepted.

Sol: full correctness/architecture/ownership/race review of the new foundation and integration; independently apply clean-code-guard. Check bounded source caches/queues, callback FFI, shutdown and incomplete/stale handling. May run offline checks/build, but no GUI while Opus owns native testing.

Opus: full independent native behavior/lifecycle review. Run the frozen catalog harness and shell smoke sequentially with separate evidence paths; inspect captures. Native test slot is exclusive. Do not change system settings, terminate unrelated apps or overwrite frozen evidence. Report missing gates honestly; M2b features are outside this foundation acceptance, not silently waived from M2.

Use stable `M2A-SOL/OPUS-MAJ/MIN-N` findings, blocker/major/minor severities, exact manifest hash, evidence/limits and ACCEPT/REVISE. No invented scores and no blocking on minor style. Reviewers edit only their reports/ignored independent evidence, never fixes. Fresh tasks recheck any author corrections, capped by the existing plan protocol. Author verification is not self-approval.
