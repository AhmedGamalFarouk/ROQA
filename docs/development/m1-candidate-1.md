# M1 candidate 1: native composition shell

Author: `/root` (primary GPT-6.1 Sol High agent). Status: implementation and author verification complete; independent council dispositions pending. This packet is not author approval.

## Scope and real behavior

ROQA now builds as one unelevated native Windows process. It draws a small notch and a companion dock as solid rounded surfaces, with reversible hover color feedback. These are graphics/lifecycle proof surfaces, **not yet app launchers, media controls, or accessible custom controls**. M2 owns real dock interactions and accessibility/text input.

The primary monitor's work area determines placement. Nonactivating tool windows use tight bounds and rounded input regions. No AppBar registration, taskbar hiding, global hook, startup entry, registry mutation, networking, telemetry or browser runtime is involved in the resident application. Exiting destroys both surfaces. Closing one leaves the other alive; closing the last exits. While another surface stays alive, a CLI command can recreate the closed surface.

Graphics exploration attempt: **1 of 2** under the accepted plan's bound. DirectComposition with a Direct2D device/context supplied by a hardware BGRA D3D11 device is working here. The lower-level API was selected to keep this small proof synchronous and UI-thread-owned without a WinRT dispatcher queue. This is not a final stack go/no-go; M2 must still prove text/IME/RTL/UIA before M3.

## Build and provenance

- Rust 1.91.1, rustfmt/clippy, MSVC target; `windows` 0.62.2 with committed Cargo.lock. MSVC 14.44 / SDK 10.0.26100.0 reused.
- Toolchain/cache homes are ignored project-local `.tools/rustup` and `.tools/cargo`; wrappers restore their process environment and PATH on exit.
- Official archived rustup-init 1.28.2 verified against both published and pinned SHA-256 `88d8258dcf6ae4f7a80c7d1088e1f36fa7025a1cfd1343731b4ee6f385121fc0`; `--no-modify-path` verified via installer help. The initial setup auto-updated rustup to 1.29.3 during compiler installation and failed its final verification invocation. Self-updates are now disabled before toolchain installation; bootstrap subsequently completed successfully. The compiler itself remains pinned at 1.91.1. No persistent/global PATH change was requested.
- A package-local build script embeds the application manifest in the ROQA binary only. Initial global linker arguments incorrectly affected dependency build scripts; that failed build was fixed, not counted as a pass.
- Windows release subsystem is used. `scripts/run.ps1` captures startup errors locally and starts hidden before surfaces explicitly show without activation. `--quit` control delivery is distinct from verified host exit.

## Ownership and shutdown

Main thread initializes an STA, registers the window class, creates session/user control objects, then owns the desktop/composition objects. Drops occur in reverse scope order: desktop surfaces and graphics, control handles, registered class, COM apartment. A surface owns its canvas before its HWND so the composition tree detaches before HWND destruction. Composition objects and device stay on that thread.

The window procedure retains no Rust pointers. It does minimal native handling and posts private notifications to the normal UI queue; state changes and drawing occur in the owning event loop. `catch_unwind` catches intentional/unexpected Rust callback panics; it posts a failing quit message rather than allowing Rust unwinding through the FFI boundary. The main loop returns failure after normal ownership teardown. `--probe-panic` exercises this in a separate process; `--probe-failure` returns an explicit error after first surface construction to exercise partial-initialization cleanup.

The host waits indefinitely on auto-reset control events plus its Windows message queue when idle. It has no periodic render tick or sampler. Security object names contain the current token's SID and use the Local session namespace and default token DACL. CLI commands are an allowlist of event names, not arbitrary executable/IPC payloads. The instance mutex prevents a second resident instance. Token handles and SID allocation are released.

## Verification contract and evidence

Author commands executed successfully before candidate freeze:

```powershell
./scripts/bootstrap.ps1
./scripts/check.ps1
./scripts/cargo.ps1 build --locked --release
./scripts/smoke.ps1 -SkipBuild
./scripts/measure-idle.ps1 -SkipBuild
```

`check.ps1` runs formatter check, Clippy over all targets with warnings denied and locked unit tests. Three tests cover negative-coordinate work areas/reserved taskbar space, tiny-area clamping at several scales and 200% notch dimensions. Runtime evidence under `artifacts/m1/` is frozen alongside candidate source; ignored output is still included in the review manifest when normative evidence.

Smoke checks:

- Dedicated foreground fixture, unchanged foreground after launch/hover, actual DPI-scaled sizes/centered positions, work-area preservation.
- Actual on-screen captures of both surfaces in idle, hover and pointer-leave states. Center pixels assert RGB `22,27,34` idle/leave and `42,51,62` hover.
- An existing unrelated notch overlay expanded over the first top-edge hover capture, so the original assertion correctly failed. The harness now verifies intended geometry first, moves only the test-owned ROQA window into the work area's interior for isolated capture, then restores it. Other applications are not stopped. This validates real composed pixels but is not evidence ROQA can visually coexist at the same edge with every other customizer.
- Synthetic WM_DPICHANGED with a valid suggested RECT exercises the message/geometry path; a subsequent display-change reconciliation restores actual scale. An earlier invalid test supplied a null RECT and did not exercise delivery correctly. This is a synthetic path test, not a physical mixed-DPI hardware test.
- Independent surface close/reopen, focus-independent `--quit` and exit code zero, native WM_CLOSE of either surface, last-surface zero exit, explicit initialization/callback failures and no remaining windows after failure.

### In-process resource gate

To resolve M0-OPUS-R2-MIN-1, the **dock remains alive while the notch is repeatedly destroyed/recreated**; no test-only permanently resident mode changes shipped last-surface semantics. A separate native-close test checks last-surface exit.

After 10 warm-up cycles, five blocks of 20 cycles collect kernel handles, USER/GDI counts and private bytes after a cooldown. Limits are **pass/fail**: ≤5 MiB private-byte increase, ≤10 kernel handles and ≤5 each USER/GDI over warm baseline. Sustained strictly increasing values across all measured blocks fail even within those absolute limits. Each block is checked; process-exit cleanup is not the resource proof. This resolves M0-OPUS-R2-MIN-2. `smoke.json` contains the actual counts and every assertion; no leak freedom beyond this workload is claimed.

### Short idle observation

`idle.json` records the binary hash, machine, exact elapsed interval and endpoint counters: release build, five-second settle, thirty-second wait. On the recorded Windows 11 Pro 26200 / Ryzen 5 PRO 4650G / Radeon integrated GPU desktop, the sample shows approximately 11 MiB process private bytes and no measurable CPU-time increase at that counter's resolution. The raw integer bytes and sampling limitations are authoritative. This is **not zero CPU in all circumstances**, full system memory, or a product/competitor benchmark; DWM/GPU/power/shared allocations are not attributed. Full performance gates remain M5.

## Independent review instructions

Freeze source, configuration, test harness, this packet, normative plan/council context and author evidence in `reviews/m1-iteration-1.manifest`. Reports and progress ledger are excluded because they are administrative outputs. Both reviewers must explicitly accept that same manifest hash; any changed input needs a new candidate/recheck.

Sol: inspect correctness, FFI/COM/ownership, event/IPC behavior, reproducible setup and evidence/test validity; may independently run formatting/Clippy/unit/build checks but do not launch GUI tests while Opus owns that test slot.

Opus: inspect native Windows graphics/lifecycle and evidence gaps; independently exercise the smoke test with `-EvidenceDirectory artifacts/m1-opus-review` so frozen author evidence is not overwritten. Inspect captured images. Record whether actual runtime gates pass, rather than relying on the author's assertions. No other reviewer runs live ROQA concurrently.

At least one non-author council reviewer applies `C:/Users/Ahmed/.agents/skills/clean-code-guard/SKILL.md` (review mode) and records its disposition as part of the code review. Authors do not self-approve their own code. Reviewers may generate ignored build/runtime evidence and write only their own reports; they may not modify implementation/config/tests or overwrite frozen evidence.

Known limits: primary-monitor technical shell only; no functional dock/media/settings/UIA controls yet; no physical topology/DPI, lock/device-loss/hung-source testing yet; graphics hardware required, no silently accepted software-rendering fallback. Those later-milestone limits must not be advertised as completed. No source/assets copied from reference projects. The full product visual design remains undecided; the proof uses solid native capsules to test the renderer without a decorative workload.
