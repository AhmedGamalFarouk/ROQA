# ROQA: dock and notch feasibility decision

**Status: DONE_WITH_CONCERNS. Date: 2026-11-09.** Two independent reviews by GPT-6.1 Sol High and Claude Opus 5.5 High, followed by chair synthesis and evidence qualifications. This is a feasibility study, not an implemented architecture or measured performance result. The models did not review each other's reports; disagreements below are resolved by the chair, not represented as peer agreement.

## Decision

**Yes, a substantially more efficient Windows dock plus notch is feasible enough to justify a prototype.** Nectar provides concrete evidence that the broad feature set can be built. Its source also identifies removable work: recurring polls, repeated window enumeration by multiple dock views, delayed capture threads, and weak cache bounds. Removing those costs and an always-resident browser runtime offers plausible savings. The size of the savings is unproven.

**A promise to outperform Nusa is premature.** Its public materials describe a native, event-driven notch and advertise roughly 30 MB typical memory. We found website source and installer releases, not inspectable application source. Test ROQA's notch-only mode against the installed matching Nusa edition. Test ROQA's combined dock/notch against Nectar. Broader scope is not proof of greater efficiency.

**Exact Windows taskbar parity is a separate, high-risk requirement.** Pin order, tray icons, Jump Lists, attention/progress state and reserved shortcuts are not all exposed through complete supported replacement APIs. Rendering technology does not solve that. Deliver reliable supported functionality first; describe compatibility features honestly.

Recommended next investment: **a 3–5 week vertical prototype**, for an engineer already comfortable with Windows development. It should settle the renderer, input, accessibility and lifecycle risks before a broad feature build. No application implementation is authorized or performed by this report.

## What the evidence actually supports

Both reviews examined Nectar at **`ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a`**, version 1.9.5 in its manifest.

| Evidence | Implication | Limit |
| --- | --- | --- |
| Tauri/WebView2 frontend with Rust host; additional notch/dock views on enabled monitors | Native small surfaces can avoid a resident web runtime and repeated document state | WebView2 process sharing varies; do not assume a process per window |
| System worker waits around 200 ms normally and 64 ms after activity; brightness and fullscreen/overlap workers have recurring waits | Use supported change events and demand-driven work to remove unnecessary wakes | Wait frequency is not measured CPU cost; some fallbacks remain necessary |
| Window-change broadcasts can cause each dock to request a fresh scan | One shared indexed window model can avoid duplicate scans | Nectar already coalesces events and discards stale frontend results |
| Native DWM previews, parked mouse worker, demand-gated audio capture, suspended settings views | Preserve these useful optimizations | A rewrite can regress them |
| Capture paths create delayed threads; some caches lack hard byte limits | Bounded workers, generation cancellation and byte budgets give predictable resource use | These are risk mechanisms, not demonstrated leaks |
| Project memory document reports 217 MB private commit on five monitors | Useful reproduction lead | Not a universal floor or our measurement; cannot compare to a one-monitor target |

Primary evidence: [worker scheduling](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L842-L930), [dock refresh](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/Dock.tsx#L847-L905), [native previews](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/live_preview.rs#L70-L85), [memory record](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/docs/memory.md). The Sol report contains 45 pinned source citations.

## Architecture to test

Start with **one unelevated resident Rust process**, `windows` bindings, separate small dock/notch HWNDs per enabled monitor, Windows composition for transforms/opacity, Direct2D for content and DirectWrite for text. This is a prototype candidate, not a language-based performance guarantee.

Use internal modules with explicit owners:

1. **Windows integration:** initial snapshot plus narrowly filtered WinEvents; media session and audio endpoint subscriptions; display, power, device and permission events. Callbacks only enqueue bounded work. COM objects remain on their owning apartment threads.
2. **Shared state:** one window index, media model, configuration store and feature registry across all monitors. Version snapshots; reject stale asynchronous results; reconcile after event loss rather than scanning on every event.
3. **Scheduler/workers:** replaceable deadlines for dwell, hide and timers; bounded background decoding and foreign calls. No thread per event, permanent high-resolution timer, blocking driver call on the UI thread, or busy idle loop.
4. **Views:** per-monitor layout and small retained surfaces. Redraw changed content; animate suitable properties in the compositor. Hidden means suspended demand, not merely opacity zero. Handle device removal, mixed DPI, negative coordinates and topology changes.
5. **Resource ownership:** byte-limited decoded-image/GPU caches, bounded previews, event registration cleanup and typed handle guards. File shelf stores references and metadata by default, rather than eagerly loading file contents.

Settings may use ordinary native controls initially. An on-demand WinUI/WebView settings process is acceptable if it materially reduces work, but it must exit on close and its open/close costs must be counted. Do not build a second full UI framework just for a preference screen.

A compatibility helper is justified only for integrations that can block or hang in foreign code. A recovery guardian is justified only when the user explicitly enables taskbar modification. It must handshake before modification, survive UI death, wait on process exit, detect hangs through a lease, and restore journaled original state conservatively. Default coexistence needs neither a guardian heartbeat nor permanent compatibility workers.

**Competing candidate: C#/WinUI 3, with NativeAOT where compatible.** Prefer it if it meets the budgets and supplies keyboard, Narrator, IME, Arabic/RTL and text scaling much sooner. Framework controls help but do not automatically make an application accessible. If both candidates pass, engineering and maintenance cost should decide. A disciplined Tauri design remains viable if measurements pass, although removing resident WebViews is a clearer route to a smaller runtime footprint.

## Council disagreements and chair resolutions

| Topic | Opus | Sol | Resolution |
| --- | --- | --- | --- |
| Idle memory | Approximately 40 MB commit goal | 80 MiB private bytes core budget | Use 80 MiB initial budget, about 40 MiB stretch. These are proposed targets on named hardware; do not silently change them after testing |
| Relative Nectar improvement | Suggests targeting a fivefold commit reduction | Targets at least 30% lower CPU and private bytes in matched scenarios | Use 30% as the initial practical-improvement gate; stronger claims require measurements. No ratio from different monitor configurations |
| Settings | Separate process by default | Avoid a second UI stack unless justified | Start simple; separate only when development benefit warrants it |
| Polling | Strong preference for eliminating all recurring polls | Explicit adaptive fallbacks for incomplete event coverage | Events first, bounded demand-driven reconciliation where needed |
| Store eligibility | Strong warning against taskbar manipulation | Scope by supported contracts | Policy risk attaches to implementation; no categorical ban inferred from the product name |

Additional qualifications are in [assessment-notes.md](assessment-notes.md). They supersede overstatements in either original review, including claims about a memory floor, free compositor work, and source inspection automatically creating GPL obligations.

## Prototype and stop/go criteria

**Working planning assumption:** current Windows 11 first, integrated-GPU laptop plus a mixed-DPI multi-monitor desktop. Windows 10, ARM64 and broad RDP coverage are additional commitments to evaluate, not silently assumed support.

The prototype must implement a small real vertical slice: grouped running apps and owned pins, activation/context menu, a DWM hover preview, media subscriptions with art/playback, one editable field, keyboard navigation/UIA, and animated expand/collapse on two mixed-DPI monitors. Include Arabic/RTL, IME, reduced motion and enlarged text. Probe edge reveal without blocking the global input path. Probe notification capability/permission with the actual chosen package model.

Test suspend/resume, lock/unlock, hot-plug, Explorer restart, source-app hang and D3D device loss. If replacement is essential, prototype independent recovery before hiding the native taskbar during daily use.

**Initial proposed budgets, not results:**

| Gate | Target |
| --- | --- |
| Static dock + notch, one monitor, optional modules off | ≤80 MiB app/helper private bytes; ≤60 MiB private working set; ~40 MiB private bytes stretch |
| Same core, three monitors | ≤120 MiB private bytes; shared services and bounded per-monitor resources |
| Static idle CPU | ≤1 ms CPU/s across the app/helper tree over 15 minutes; no perpetual app-owned render loop |
| Collapsed music, static metadata | ≤2 ms CPU/s; no hidden visualization, scrolling lyrics or animated progress |
| Input response | p95 ≤50 ms, p99 ≤100 ms; native-refresh frame pacing tested independently |
| Matched Nectar improvement | Seek ≥30% lower CPU time and private bytes, without a material GPU/DWM/power/latency regression |
| Resource churn | After 1,000 cycles and cooldown: within 5 MiB of warmed private-commit baseline; no monotonic handle growth |
| Replacement recovery, if included | Native taskbar accessible ≤2 seconds after UI exit signal; hang/Explorer cases separately tested; count guardian costs |
| Accessibility | Keyboard/Narrator/IME/RTL/text-scaling core flows pass; cannot trade this for a memory win |

If baseline CPU is below the reliable measurement floor, classify relative CPU improvement inconclusive and use absolute budgets. Compositor animation still costs DWM/GPU work. Measure whole-system power and graphics allocations rather than hiding them behind an improved app CPU number.

**Pivot to WinUI** if custom text/input/UIA or Rust composition interop consumes the timebox without a correct vertical slice, or if WinUI meets the same targets substantially sooner. **Rescope** if exact tray/taskbar behavior depends on unstable internals the product cannot afford to maintain. A failed 40 MiB stretch goal alone is not a reason to discard an otherwise good product.

## Benchmark protocol

Run each app alone, never two taskbar customizers together. Pin binary hashes, source versions where available, Windows/driver/WebView2 versions, monitor DPI/refresh/topology, enabled features and power mode. Use release builds, warm-up plus repeated randomized paired runs and a no-app baseline. Match visibility, theme/blur, media, pin/window counts and preview demand.

Record process-group CPU time, private bytes and private working set, GPU allocations, handles, threads, timer wakeups, first interaction after idle, frame pacing and power. Include associated WebView processes and optional helpers. Summed working sets can double-count shared pages. DWM is shared; measure its paired increment, not fictitious exact per-app ownership. Do not use periodic `EmptyWorkingSet` calls to pass budgets. Record settings-open, post-close, 200-window stress and long-running churn separately.

Nusa comparison is notch-only, including media and shelf scenarios; its site screenshot is not the benchmark baseline. Nectar comparison includes dock-only and matched combined scenarios on one and three monitors. Publish unsupported differences and uncertainty alongside results.

## Feature order and mistakes to avoid

Ship the notch's media/status/timer/file shelf and the dock's owned pins/window groups/activation/previews/keyboard access first. Multi-monitor correctness and recovery are core quality, not later polish. Add a capability-controlled local progress API, lyrics, notifications, calendar and richer control center after the core passes. Extra features should pay resource costs only when enabled and demanded.

The highest-cost mistakes are:

- Building all the widgets before proving renderer, input, accessibility and mixed-DPI lifecycle.
- Promising exact tray/pin/Jump List parity without supported APIs and a tested fallback.
- Fighting Explorer in a re-hide loop or relying on a crash handler to survive forced termination.
- Duplicating media/index/metrics services per monitor; spawning threads per event; unbounded decoded images and capture queues.
- Keeping offscreen blur, mascot, visualizer or progress animations running.
- Choosing Rust, a tiny installer or rounded Task Manager readings as evidence of efficiency.
- Adding arbitrary third-party plugins, network accounts and persistent history before stable module lifetimes and permission boundaries.
- Copying GPL code while assuming a proprietary release remains unaffected. Choose reuse/license strategy first. Reading source alone does not settle derivative-work questions.

Notification access requires capability, permission and a verified app-model route; Store distribution is not by itself the underlying permission mechanism. Sparse/external-location identity remains a prototype question. [Microsoft notification guidance](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener). Unsupported shell manipulation also raises [Store policy 10.2.8](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#102-security) concerns; certification is unverified.

## Scope and effort

The council's broad ranges overlap: a serious prototype takes weeks; a core beta takes months; polished broad parity plus additional integrations can take **6–12+ months**, depending heavily on Windows expertise, team size, design/QA support and compatibility scope. Opus suggests 6–11 months solo under experienced-engineer assumptions; Sol suggests 10–18 weeks for a narrower core beta with two experienced engineers and QA/design support. These are different deliverables, not competing deadlines. A solo engineer learning COM/graphics should expect longer.

The remaining product choices are the necessity of taskbar replacement, first feature shortlist, Windows/hardware support, licensing/distribution and team expertise. None prevents this feasibility conclusion. They do determine the implementation plan.

## Review artifacts

- [Sol independent source/engineering review](sol-review.md)
- [Opus independent feasibility/architecture review](opus-review.md)
- [Chair's evidence qualifications](assessment-notes.md)

Durable findings are preserved in these project documents. No runtime benchmarks, app builds or device tests were performed; no performance advantage is claimed as established.
