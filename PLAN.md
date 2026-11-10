# ROQA implementation plan

Revision: M0 iteration 2. Author: primary implementation agent (`/root`). Date: 2026-11-09. Current independent dispositions and acceptance state are recorded in `docs/development/progress.md`; changing this file requires a new candidate manifest.

## Objective

Build a native Windows dock and notch with Nectar-class capabilities, a Nusa-inspired compact notch, and demonstrably efficient resource use. The feasibility decision and its qualifications live in `docs/council/decision.md`. No speedup has been measured yet.

The first implementation target is a buildable, runnable native shell, followed by a usable event-driven dock and media notch. This plan describes the broader project; it does not label the first milestone full feature parity.

## Working defaults

- Windows 11 x64 first; existing MSVC 2022 Build Tools and Windows SDK are installed. Windows 10/ARM64 support remains open.
- Rust + Win32 + Windows composition is the first prototype candidate. Rust is not installed/on PATH. Create `.gitignore` entries for `.tools/` and `target/` before bootstrap. Obtain `rustup-init` from the official Rust host, verify its published SHA-256, verify supported flags, and run with `--no-modify-path`. Set `RUSTUP_HOME` and `CARGO_HOME` only within project wrappers, both under `.tools/`; do not modify global PATH. Pin an exact compiler in `rust-toolchain.toml`, with rustfmt/clippy and the `x86_64-pc-windows-msvc` target. Pin dependencies/lockfile, use `--locked` for build/test/clippy, and record provenance. Reuse installed MSVC 14.44 (Build Tools 17.14) and Windows SDK 10.0.26100.0; do not install a new Visual Studio distribution.
- Native taskbar stays available. No taskbar hiding, registry pin-blob parsing, Explorer UIA automation, admin elevation, startup registration, update installation or global input hooks in the first milestone.
- Do not reuse Nectar code or either project's artwork. New implementation against platform contracts; repository license remains an explicit user decision before release.
- Small opaque/solid native surfaces first. Custom composition animation and visuals are evaluated with lifecycle/accessibility costs included. No browser runtime in the always-on core.
- Reference hardware/Windows build and real measurements are recorded when tests run. Targets are not results.

## Independent hill-climbing protocol

The user requires council review at every milestone and no agent reviewing its own work. The council is GPT-6.1 Sol High and Claude Opus 5.5 High, using the evidence-based gstack approach already applied to feasibility.

1. Define one bounded candidate, acceptance conditions and baseline. Identify its author and changed files.
2. Implement that candidate. Author may build/debug/run tests; these are verification, not an independent review or self-approval.
3. Freeze the candidate with a SHA-256 manifest of relevant source/config/docs. Request fresh independent Sol and Opus reviews against that manifest. Each review is a new delegated task, read-only except for its own report. Neither reviewer may edit the candidate or review a candidate it authored.
4. Review correctness, performance mechanisms, Windows lifecycle, accessibility, scope and maintainability. Use evidence and severity: blocker/major/minor; no invented quality score. Rank findings by user impact and reversibility.
5. The implementer fixes material findings in a small batch, records verification and a new manifest, and requests a fresh independent recheck of the changed candidate. A reviewer who implements a fix becomes an author and is ineligible to approve that fix.
6. Accept only when the final candidate has no open blocker/major finding, required checks pass, and required empirical evidence exists. Unknown/manual-only gates remain explicitly open; a static review cannot certify them.
7. Stop iterating when gates pass or a round produces no justified improvement. If a non-negotiable gate remains unmet, record a blocked/pivot decision and do not call it accepted. Do not spend review cycles on speculative scope or cosmetic churn.
8. Update `docs/development/progress.md` with author, reviewer task links, input hash, findings, changes, check results, remaining limitations and next candidate. Start the next milestone only after its predecessor's required gate is satisfied.

Convergence rules:

- Findings get stable IDs, such as `M0-OPUS-MAJ-1`, with status open/fix-submitted/fixed/disputed/withdrawn/deferred in the ledger. Only an independent review can mark a submitted material fix resolved. Minors never block acceptance and can be deferred with a reason.
- Fresh rechecks cover prior findings, the candidate diff and regressions caused by it. New findings on unchanged content require blocker/major severity and concrete evidence; no new cosmetic audit on every recheck.
- The author may accept the stricter recommendation and fix it without adjudicating a disagreement. If the author disputes a blocker/major, record the evidence and obtain a fresh reviewer recheck. An unresolved substantive conflict goes to the user; the author cannot waive it or grant acceptance.
- Allow at most three fix/recheck rounds after the initial review. If required gates or a material disagreement remain unresolved, report the evidence and request a user scope/pivot decision; do not continue an unbounded review loop or silently accept. Passing gates ends iteration immediately.
- Acceptance requires explicit ACCEPT from both independent council reviewers on the **identical final manifest SHA-256**. Earlier acceptances on different inputs cannot be combined. Reports and ledger updates are not part of their own input manifests.

Review independence means a different agent/task instance that did not author the artifact. Using the same underlying model in a separate independent reviewer is permitted; actual author identity is recorded. The primary agent does not grant itself a council pass.

## M0 — reviewed plan and workflow

Candidate: this plan, project rules and initial product context. Both council models challenge scope, toolchain setup, review independence and feasibility. Amend material findings and obtain a new review before production code starts. Environment inspection is allowed during this gate.

Exit: both reviewers explicitly ACCEPT the identical final manifest SHA-256; no blocker/major open in the plan; clear M1 acceptance contract. This gate reviews the plan's evidence requirements, not future runtime results.

## M1 — native shell and reproducible build

Deliver:

- Cargo project with pinned lockfile/toolchain, focused modules and local bootstrap/build/test/run commands.
- One resident process with a blocking Win32 message loop and two small native windows: top notch and bottom companion dock on the primary monitor. Place both within the work area, clear of the native taskbar, with no AppBar registration or work-area reservation. Use nonactivating tool windows for these passive surfaces. Test independent open/close, DPI awareness and display geometry. Keep the scope primary-monitor-first; all-monitor rendering follows in M3. Closing one surface leaves the other alive; closing the last surface gracefully exits. A documented control path can reopen either surface while the host remains alive.
- Native GPU composition proof with solid content. The interactive state is a pointer-enter composition color/opacity change that reverts on pointer-leave, without activation. Verify the chosen Windows API bindings before coding. Bound graphics exploration to **two implementation candidates**; if the second cannot satisfy the graphics gate, submit a pivot/rescope candidate for council review. Do not silently substitute a CPU renderer and call the graphics proof passed. A WinUI pivot is a reviewed plan amendment with its own pinned local .NET SDK bootstrap, since no .NET SDK is installed.
- Provide a focus-independent graceful exit through a second invocation, `roqa --quit`, using per-user/session IPC. Failure to signal is reported, not treated as successful shutdown. The run wrapper exposes this to keyboard/terminal users; keyboard focus entry into the surfaces follows in M2. Launching does not steal focus from the current app. No invisible full-monitor input surface. Initial custom controls must expose correct semantics or be clearly unavailable until M2; no claim of accessibility completion from a blank shape.
- Explicit error propagation, RAII/resource cleanup, safe FFI boundaries, COM/thread ownership, graceful shutdown. No panic unwinds through Win32 callbacks.
- Unit tests for geometry/DPI boundary behavior actually used by the host, and a process smoke/lifecycle check. A native smoke run is authorized as part of development; it must leave taskbar/settings unchanged and stop the instance it launched.

Acceptance checklist:

- Clean setup/build succeeds; `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked` pass. Use `cargo build --locked --release` for runtime evidence.
- Native smoke test records unchanged foreground HWND before/after launch (with a stable foreground test app), expected two window rectangles against primary-monitor work-area/DPI geometry, bounded HWND sizes, no AppBar/work-area changes, independent close/reopen of each surface and last-surface shutdown. `roqa --quit` must work without activating a surface and exit the launched host with code 0. Forced kill is a failed graceful-exit test, never a pass.
- **Rendered evidence is mandatory:** capture actual screen regions corresponding to both visible window rectangles, and assert expected solid-content pixels plus the pointer-enter/leave color or opacity transition. Keep images and assertion results with the packet. Window existence or an offscreen renderer test alone is insufficient. If no permitted capture method succeeds, the graphics gate remains OPEN and M1 cannot be accepted; a user scope waiver or reviewed pivot is required.
- In one live process, after 10 warm-up cycles, perform 100 window/composition create/destroy cycles in measured blocks, retaining a control/IPC host as needed. Record private bytes, kernel handles, USER and GDI objects before/after each block and after cooldown. Investigate monotonic growth; initial plateau tolerances are ≤5 MiB private bytes, ≤10 kernel handles and ≤5 each USER/GDI objects relative to the warmed baseline. Also inject failure after the first HWND/composition allocation and verify cleanup and explicit failure. Process-exit cleanup alone does not prove lifetime correctness.
- Record chosen COM apartment, callback panic strategy and composition teardown ordering against actual APIs. Prevent unwinding through callbacks; exercise the failure strategy in an isolated test process. Embed per-monitor-v2 DPI awareness and exercise `WM_DPICHANGED` geometry even before multi-monitor rendering.
- M1's performance observation uses a release build, 5-second settle then 30-second sample via PowerShell `System.Diagnostics.Process` CPU-time delta/private bytes/handle counters, with start/end samples and OS build, CPU/GPU/display configuration recorded. Include sampling overhead/limits and short-run variance. Label it **observation only**, not an M5 budget verdict or Nectar/Nusa comparison.

## M2 — useful dock and accessible interaction

Deliver: initial window enumeration plus narrow event subscriptions, one shared window model, real title/icon identity, grouping, user-triggered activation, keyboard navigation and UIA semantics, explicit unsupported/inaccessible-window behavior. Own pins and persistence follow once identity is stable. Bounded reconciliation after lost events; no scan/thread per callback. Event registration unhooks on exit. UI actions must do real work, not mock app listings.

Acceptance: real create/close/minimize/focus changes converge; normal/elevated/packaged windows behave as documented; no focus stealing on passive changes; Narrator/keyboard core flow and failure cases tested. Unit-test reducer/generation/coalescing cases and run event-storm/lifetime checks. Before expanding custom controls, prove one Arabic/RTL title with the chosen text path, system text-scale changes, and one IME-capable editable route (a standard OS edit control is acceptable). Record actual text/input evidence, not a shaping assumption. End M2 with a **council-reviewed stack go/no-go** using these results, the M1 renderer evidence and council pivot criteria; M3 cannot start before that decision. Both council reviews pass the final candidate manifest.

## M3 — displays and previews

Deliver: shared state across multiple dock/notch HWNDs; mixed DPI/refresh, negative coordinates, topology changes and per-monitor fullscreen suppression; DWM previews only on demand; bounded icon/art caches; resume and Explorer restart resilience in coexistence mode.

Acceptance: reference hardware matrix and lifecycle scenarios pass; no stranded work areas/hidden input windows; per-monitor cost measured. Exercise lock/unlock and inject graphics-device-loss/recreation, validating retained-state recovery or a clearly reported safe failure. Exercise a hung source app and ensure title/icon/preview work cannot hang the dock or its shutdown. No unsupported taskbar mutation introduced incidentally.

## M4 — media notch vertical slice

Deliver: real GSMTC subscriptions, participating-player controls, bounded album-art decoding, volume/device events and clear no-player/permission/error states. Compact/expanded panel with focus return and accessible actions. Hidden progress/visualizer work stops. Proper event-token teardown and media-session replacement.

Acceptance: tracks/player changes, seek/play/pause, app disappearance/hang, sleep/resume and audio-device changes tested. By user decision (2026-11-10) the notch and media panel always animate, even when Windows animation effects are turned off for reduced motion; focus return and accessible actions remain usable. Static collapsed playback has no continuous app render loop. Real performance trace and independent reviews pass.

## M5 — core product and performance proof

Deliver: owned pins/reorder/persistence, safe file shelf references, timers and practical settings, complete keyboard/Narrator/RTL/IME/text-scale flows, diagnostics without sensitive content, bounded resources. Build comparison harness for matched Nectar combined and Nusa notch-only scenarios.

Proposed gates from council: one-monitor core ≤80 MiB private bytes (about 40 MiB stretch), three monitors ≤120 MiB; static app/helper CPU ≤1 ms/s over 15 minutes; input p95 ≤50 ms/p99 ≤100 ms; bounded churn and no material compositor/power regression. Seek ≥30% lower CPU/private bytes than Nectar in matched scenarios. Below reliable CPU measurement floors report inconclusive. Revisit thresholds only through a documented evidence-backed plan amendment reviewed by the council.

Acceptance: feature-matched repetitions with pinned releases/hashes/settings/OS/GPU/runtime; app/helper and WebView descendants accounted; DWM increment, GPU memory, power, latency, warm/cold/idle/churn reported separately. No comparison to screenshot-only marketing figures. No working-set trimming used to pass gates.

## M6 — expansion and optional compatibility

After the core gate: notification packaging/permission prototype, control center, lyrics/calendar/local progress integrations and themes. One feature per independently reviewed candidate with measured incremental costs. Plugin execution is a separate design project. Taskbar replacement requires its own supported-contract inventory, user-visible limits, independently surviving restore guardian and fault-injection/uninstall proof before daily use. License, signing, distribution and updates need concrete reviewed release plans.

## Review packets

Store reports under `docs/development/reviews/`. Each packet includes milestone/iteration, author, manifest hash/file list, commands and results, evidence paths, changed behavior and known gaps. Hash normative referenced context, including `docs/council/decision.md` and `docs/council/assessment-notes.md`, alongside candidate files. `progress.md` is intentionally excluded: it is a mutable administrative ledger updated after reviews, never an authority that changes a frozen candidate's acceptance conditions.

M1+ packets require an **independent clean-code-guard pass**, performed by a non-author council reviewer, with its findings/fixes/disposition identified. Reviewers report stable finding IDs, concrete file/line evidence, input manifest SHA-256 and a disposition scoped to those inputs. A stale hash invalidates approval. Preserve superseded reports so a claimed improvement can be audited.
