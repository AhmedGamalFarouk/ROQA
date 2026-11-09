# M1 iteration 1 - independent Sol review

Date: 2026-11-09. Reviewer: fresh GPT-6.1 Sol High review sub-agent. Candidate author: `/root` (all submitted implementation/config/tests/docs). I did not author or change those inputs, did not delegate, and did not read the other M1 review before forming this disposition.

## Disposition and integrity

**ACCEPT**, scoped exactly to `docs/development/reviews/m1-iteration-1.manifest`, SHA-256 **`76B1A9EC66BF2033B8DD095B51993A4120B398F8ECF229A25FE87914536EFA8B`**.

Open findings: **0 blocker, 0 major, 1 minor**. Independent clean-code-guard: **PASS for this bounded M1 production-code scope**. This is the Sol disposition, not a declaration that both required acceptances exist. Opus must accept this identical manifest and provide its independent live-runtime assessment; I do not infer that outcome.

Read `AGENTS.md`, `PLAN.md`, `PRODUCT.md`, the progress ledger, M1 packet, normative council decision/assessment notes, relevant M0 iteration-2 reports, and all candidate source/tooling/test files. Mutable progress/reports are administrative context, not frozen implementation inputs.

Verified the manifest hash and **all 36 entries individually**, before review and again after checks: **36 matches, zero mismatches, zero missing files**. The complete expected/actual per-entry record is `artifacts/m1-sol-review-20261109-153849/input-integrity.json`. This includes all six Rust modules, seven PowerShell scripts, six PNGs, both JSON evidence files, the C# probe, lock/config/build/manifest/ignore files and normative documents. No candidate input was changed.

The existing `target/release/roqa.exe` independently hashes to **`9411F5548BF9A5E6370F87B0AB4AD090142B3C33948EDA61842BA5C765AE6CA7`**, matching both frozen author JSON records. I did not rebuild it in place, launch it, signal it or modify it.

## Findings

### M1-SOL-MIN-1 - Complete tool-manager isolation provenance

Severity: **minor**, nonblocking. Status: **open**. Category: tooling/evidence, not resident-runtime safety.

`scripts/bootstrap.ps1:25` skips the archived installer when local `rustup.exe` exists; `scripts/bootstrap.ps1:30` correctly disables future self-updates before compiler installation. `docs/development/m1-candidate-1.md:17` honestly records the earlier automatic update from the checksummed 1.28.2 installer to manager 1.29.3. My read-only query confirms manager 1.29.3 and compiler/Cargo 1.91.1. The installer checksum is therefore not the identity of the manager currently executing wrappers.

A read-only query found `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Rustup` with `DisplayVersion=1.29.3`, but no returned `DisplayName` or `UninstallString`. Upstream rustup's Windows updater includes a separate Programs DisplayVersion write, so PATH suppression alone does not prove a self-update has no registry effects. **No pre-bootstrap registry baseline exists: causal attribution to ROQA is unknown. I do not claim the final script created this entry.** The final script disables the admitted update path before compiler installation on fresh setup. No global PATH change was performed in this review.

Minimal action: record the active manager version/hash separately from installer/compiler provenance; note the registry observation and missing baseline. At the next authorized fresh-bootstrap verification, capture relevant registry/PATH baselines and tool identities. Do not silently repair/remove user registry state; any proposed repair needs separate authorization. No new installation is requested during review, and this does not invalidate the pinned compiler or block M1.

Read-only observations/manager hash: `artifacts/m1-sol-review-20261109-153849/tooling-observation.json`. Primary source inspected: rust-lang/rustup tag `1.28.2`, `src/cli/self_update.rs` (`do_install`/`no_modify_path`) and `src/cli/self_update/windows.rs` (`run_update`, `do_update_programs_display_version`, `rustup_uninstall_reg_key`). This distinguishes PATH suppression from earlier updater behavior, without asserting a new frozen-script failure.

## Correctness and safety assessment

- **COM ownership:** `src/host.rs:29` balances successful STA initialization with `CoUninitialize`. Scopes at `src/host.rs:173` destroy desktop resources before controls, class and apartment. Graphics/HWND operations stay on the owning thread, without retained Rust callback pointers or background workers.
- **Composition lifetime:** `src/host.rs:48` declares canvas before HWND, and desktop surfaces before graphics. `src/graphics.rs:126` detaches the root and commits before field release and HWND destruction. Partial construction owns its locals and propagates errors. COM references retain device dependencies.
- **Drawing:** `src/graphics.rs:46` supplies Direct2D to DirectComposition. `src/graphics.rs:88` uses the supplied context and offset, covers the full surface, calls surface `EndDraw` even after a returned brush error, and commits on success. Correctly avoids Direct2D `BeginDraw`/`EndDraw` on that supplied context. No CPU-rendering fallback is labelled GPU success.
- **Win32/FFI lifetime:** `src/native_window.rs:60` creates tight nonactivating tool windows. `src/native_window.rs:87` transfers the region only on successful `SetWindowRgn`, deleting it on failure. Shared cursor/module handles are not incorrectly freed. Class unregister follows surface destruction.
- **Panic/errors:** `src/native_window.rs:149` catches callback unwinding and posts nonzero quit; `src/host.rs:217` propagates failure after normal teardown. Stateful work is deferred, avoiding mutable desktop borrows across reentrant callbacks. Probes have actual harness callers, not fake product success paths. No concrete callback-unwind, double-ownership or ordinary-error cleanup defect was found.
- **IPC/single instance:** `src/ipc.rs:76` checks mutex `ERROR_ALREADY_EXISTS` immediately; partial event-creation errors release owned handles. `src/ipc.rs:114` combines the token SID with Local session namespacing. The aligned token buffer lives through SID conversion; SID text/token allocations are freed. Clients request only `EVENT_MODIFY_STATE`; failed open/signal propagates failure.
- **Security limits:** null security attributes select token-derived default security, not an audited explicit owner-only ACL. SID names are namespacing, not secret capabilities. I checked API semantics/call sites, not live DACLs, cross-account/session behavior or hostile object squatting. The packet accurately states default token DACL, without stronger adversarial-isolation claims. Hardening/tests remain appropriate before expanding this technical-shell IPC.
- **Loop/lifecycle:** `src/host.rs:192` blocks on five live events plus the Windows queue, detects failed waits and exits after the last surface disappears. Native close defers mutation to the owner loop. Ordinary repeated show cannot duplicate a surface. CLI success means signaling, not completed state change. Separate auto-reset events are not FIFO; no overlapping-client/storm ordering guarantee is inferred.
- **Geometry:** the host uses tested `src/layout.rs:15` work-area placement, negative-coordinate handling and small-area clamping. `app.manifest:8` embeds PerMonitorV2. `src/native_window.rs:179` defers DPI/display/settings work without retaining a borrowed RECT. Synthetic scaling and system-DPI reconciliation do not replace M3's physical DPI/topology matrix.

Microsoft primary contracts checked: `CreateEventW` default security/reset/inheritance; `MsgWaitForMultipleObjectsEx` live-handle/queue semantics; `IDCompositionSurface::BeginDraw` context/device/offset/completion rules; `GetDpiForSystem` versus `GetDpiForWindow`. Installed `windows` 0.62.2 bindings also compiled independently. No undocumented shell API, global hook, elevation, startup registration or taskbar/work-area mutation was found in resident source.

## Independent checks

No bootstrap/install, ROQA launch, smoke/performance test, foreground manipulation or control signal was run by me. Opus retained the exclusive live-GUI slot. No candidate/lockfile or frozen `artifacts/m1` evidence was overwritten.

Ran existing scripts with `CARGO_NET_OFFLINE=true` and a fresh ignored target, `artifacts/m1-sol-review-20261109-153849/target`:

| Check | Result |
| --- | --- |
| `scripts/check.ps1`: `cargo fmt --check` | PASS |
| `cargo clippy --locked --all-targets -- -D warnings` | PASS |
| `cargo test --locked` | PASS: 3 passed, 0 failed |
| `scripts/cargo.ps1 build --locked --release` | PASS |
| Decisive runner exit status | 0 |
| Final manifest and all 36 entries | MATCH |
| Original runtime binary identity | MATCH to frozen records |

Decisive logs: `verified-checks.json`, `verified-checks.stdout.log`, `verified-checks.stderr.log` in that evidence directory. The first reviewer capture wrapper encountered PowerShell native-stderr/redirection behavior on normal compile output; a second outer wrapper could not retrieve its completed child's exit code. Neither wrapper failure was counted as a candidate test result. I reran permitted checks with a .NET process wrapper, separate streams and verified exit 0. Earlier capture logs remain available; author scripts were unchanged.

The separate-target release build hashes to `EF51CFF1D9AA675C6EA0CA2F727F6033E26D2A3BB80E7F2A96F9FB9E3D2A8014`. It was never launched and is not substituted for the frozen runtime binary. Byte-for-byte deterministic builds were not demonstrated or claimed. I inspected bootstrap safety/provenance but did not independently reproduce a fresh installation.

## Runtime evidence validity and limits

- The frozen smoke record contains **282** passing assertions, not 282 independent scenarios: many repeat control-delivery assertions during churn. The harness throws on failure; timeout/forced cleanup is not converted to successful graceful exit.
- `scripts/smoke.ps1:125` runs 10 warm-up plus 100 measured notch HWND/composition cycles in one process with dock alive. Blocks enforce the specified tolerances and reject a strictly-increasing sequence. Actual warm sample: 11,030,528 bytes / 242 handles / 0 GDI / 7 USER. Block 1: 11,194,368 / 245 / 0 / 7, flat through blocks 2-5. This supports that workload's plateau, not general leak freedom or a long-running trend guarantee.
- Native-close, last-surface-zero-exit, initialization/callback failure and focus-independent quit tests are present separately from churn. Keeping the dock alive and enforcing plateau limits addresses M0-OPUS-R2-MIN-1/2 within the actual M1 packet/evidence; M0 plan approval is not implementation approval.
- Independently decoded all six PNGs and checked dimensions/center RGB: dock 300x64, notch 208x36; idle/leave `22,27,34`, hover `42,51,62`. Results: `image-checks.json` in my evidence directory. Also visually inspected `dock-hover.png` and `notch-idle.png`; these are scoped solid rounded proof surfaces, not finished UI.
- `tests/NativeProbe.cs:109` captures screen pixels using `CopyFromScreen`, not an offscreen surrogate. Intended geometry is asserted before moving only test-owned HWNDs for isolation, then restored. Overlay interference is disclosed. Center pixels/captures do not prove every corner, adapter/HDR case or coexistence with arbitrary edge overlays.
- The synchronous synthetic-DPI helper allocates/frees a valid RECT. Production defers scalar DPI work without retaining that pointer. This tests the message/scaling/reconciliation path, not a physical mixed-DPI transition.
- The idle record contains a 30.017-second endpoint observation, approximately 11 MiB private memory, no CPU-time delta at counter resolution and a three-handle endpoint increase. Limits exclude DWM/GPU/power attribution and repeated-run variance. No zero-CPU, full-system-memory or competitor-superiority result is justified or claimed.
- I did not reproduce the original foreground desktop, live workload or idle run. Frozen hashes, source inspection, decoded images and original binary identity make author evidence internally coherent. Opus's separate live review remains required independent runtime corroboration. Functional keyboard/UIA, Arabic/IME/text-scale, physical topology, lock/device-loss, media and hung-source gates remain M2+; none is silently accepted here.

# Code review: M1 native shell production code

## Summary

Independent clean-code-guard review mode: **ship for the bounded M1 technical-shell scope**. Read `SKILL.md`, `references/ai-failure-modes.md` and `references/review-checklist.md`; walked all six Rust modules. No production-code guard finding requires revision. The tooling-provenance minor is outside this production-code guard count.

Counts: **0 critical, 0 important, 0 nits**.

## What's good

- Concrete ownership boundaries encode teardown dependencies instead of relying only on process exit.
- Actual Windows graphics/control APIs propagate errors; no invented product data or success-only substitute appears in production.
- The pointer-free callback defers stateful work, keeping reentrancy/panic reasoning tractable without speculative machinery.

## Coverage

- Section A (naming & functions): clean for scope. Fixed Win32 signatures and necessary construction sequences judged by responsibility/risk, not arbitrary parameter/line limits.
- Section B (comments & formatting): clean. Safety/ownership comments explain FFI contracts, not comment pollution. Formatter passed independently.
- Section C (SOLID): clean. Focused modules, no speculative framework/factory or broad shared mutable service.
- Section D (DRY/KISS/YAGNI): clean. Both surfaces share lifetime/render logic. Error/panic probes have present callers; no unused future configurability demanded.
- Section E (AI failure modes): clean within M1. Checked swallowed errors, fake success, API plausibility, arithmetic boundaries, cleanup, dead code, duplicate ownership and speculative dependencies. Destructor diagnostics and documented signal delivery are not disguised completed runtime success.

## Final acceptance boundary

**ACCEPT M1 iteration 1 for manifest SHA-256 `76B1A9EC66BF2033B8DD095B51993A4120B398F8ECF229A25FE87914536EFA8B`.** No blocker/major finding was identified. M1-SOL-MIN-1 can be documented/deferred without blocking. Changed candidate inputs require a new manifest and fresh rechecks; this acceptance cannot be combined with a different-input review. M2 must not start before both independent dispositions and the milestone gate are recorded.
