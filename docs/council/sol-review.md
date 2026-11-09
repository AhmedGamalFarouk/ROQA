# ROQA council: independent Sol engineering review

**Reviewer:** GPT-6.1 Sol, high reasoning effort. **Research date:** 2026-11-09. **Status:** DONE_WITH_CONCERNS.

Read-only research and planning, using the requested gstack direct-analysis route. The parent supplied the router decision; I also read `C:/Users/Ahmed/.claude/skills/gstack/SKILL.md`. No unavailable subskills, model wrappers, onboarding, installs, implementation, commits, global configuration changes, external messages, or nested agents were used. This report is the only project artifact authored by this review. Public source files were downloaded to a temporary research directory for inspection. No application was built or run, and no performance result was measured.

## Verdict and decision boundary

**Proceed to a bounded technical proof, not a full feature build.** A fast, accessible dock plus notch is feasible. Exact Windows taskbar replacement, universal native tray behavior, arbitrary monitor brightness, and identical integration across Windows updates are not safe promises. The largest engineering risk is shell compatibility and recovery; the largest product risk is adding background features faster than they can be budgeted and tested.

My recommendation is a Windows 11 native host in Rust, using `windows` bindings, HWNDs, Windows composition, Direct2D and DirectWrite. Use small retained surfaces and event subscriptions, with explicit ownership and a bounded work scheduler. Choose between Windows.UI.Composition desktop interop and lower-level DirectComposition in the first proof. Microsoft explicitly recommends the former for modern desktop apps; lower-level DirectComposition is a justified alternative only if its interop and lifecycle prove simpler for our needs. [Microsoft guidance](https://learn.microsoft.com/en-us/windows/win32/directcomp/how-to-use-directcomposition), [Win32 visual-layer interop](https://learn.microsoft.com/en-us/windows/uwp/composition/using-the-visual-layer-with-win32).

This recommendation is conditional on existing Windows graphics/COM expertise or time to acquire it. C#/WinUI 3 is a credible competing recommendation if accessibility and delivery speed matter more than the lowest resident footprint. Rust does not make a timer, blur shader, COM call, or enormous texture cheap. An optimized Tauri app could beat a poorly designed native app.

**User decisions still required:** whether replacing/hiding the native taskbar is essential; supported Windows builds and hardware; acceptable compatibility exceptions; distribution/license model; the first feature set. Council agreement would be advice, not authorization or a product decision.

## Evidence and reproducibility

Nectar `main` resolved during research to **`ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a`**, committed 2026-11-08 12:28:58 UTC. Its package identifies version 1.9.5, Tauri 2.11, and GPL-3.0-only. [Pinned Cargo manifest](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/Cargo.toml#L1-L24), [commit](https://github.com/kua8/Nectar/commit/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a). Every Nectar source link below uses this SHA. Findings concern this source snapshot, not every released binary. A fair benchmark must identify the tested release SHA and binary hash separately.

Evidence labels used here:

- **Observed:** source or public documentation inspected directly.
- **Claim:** publisher describes behavior/performance; we did not establish it experimentally.
- **Inference:** likely consequence of the observed mechanism, requiring measurement or reproduction.
- **Recommendation/target:** proposed design or acceptance criterion, not an implemented decision or result.

### Nusa: what is and is not known

The [public page](https://rama2555.github.io/nusa/) advertises Rust, GPU drawing, change-driven rendering and Windows events; roughly 30 MB typical memory, zero idle CPU, and a roughly 3 MB download are **publisher claims**, not measurements by us. Its own illustrated everyday-use sample displays 0.3% CPU and 30.6 MB, without a reproducible workload or metric definition. Download size is not resident memory.

Its scope is a notch: media/lyrics, focus, files, clipboard, tasks, reminders, status and overlays; some widgets and notification features depend on edition/distribution. It is not a dock baseline. More total ROQA features would establish broader scope, not better notch efficiency.

The public [repository tree](https://github.com/rama2555/nusa/tree/8b7f7920edf0e7eb064c931ca405942a41657432) at **`8b7f7920edf0e7eb064c931ca405942a41657432`** contains `README.md`, `index.html`, and `nusa-icon.png`; README is only a title. The [v1.2.0 release](https://github.com/rama2555/nusa/releases/tag/v1.2.0) exposes an installer ZIP. I found **website source and binaries, not application source or an application-source license** in that public repository. This does not prove private source or another repository does not exist. We cannot independently audit its architecture from these materials.

## Feature inventory and honest parity scope

Nectar already has both the dock and notch. Calling the proposal “Nectar plus a notch” would misunderstand the baseline. Its [pinned README](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/README.md) and code show:

| Area | Baseline to account for | Proposed release treatment |
| --- | --- | --- |
| Dock launchers | Pinned apps/order, running apps, app identities, real/custom icons, drag reorder and dropped launchers, hide selected apps | Core. ROQA owns its pins; offer explicit one-time import with a review screen. |
| Window switching | Grouped windows, indicators, activation, new instance, close/quit, attention indicators, previews with per-window titles | Core grouping/activation/close; previews only while requested. “Quit” and force termination must be distinct actions. |
| Dock utilities | Start, search, app launcher popup, Task View, drive shortcuts, Recycle Bin, show desktop, calendar/settings/Task Manager access | Deliver the commonly used subset first; retain Windows entry points. |
| Shell behavior | Fixed/Smart/Peek visibility; native taskbar hiding; native tray flyout access; Win+number interception in replacement mode | Coexistence first. Hiding/tray automation require separate compatibility gate. Reserved-key interception is optional. |
| Displays and appearance | Primary/specific/all monitors, per-monitor modes, scaling, solid/glass/theme choices, attention and preview settings | Multi-monitor and DPI correctness are core; visual variety is later. |
| Notch | Multiple media sources, art/controls/visualizer, configurable control center, battery/weather/status, calendar/timers/Pomodoro, shelf | Media/status/timer/shelf first. Visualizer and system metrics are opt-in workloads. |
| Integrations and operations | Audio devices/mixer, brightness, network/Bluetooth controls, calendar sync, agent status/API, overlays, hotkeys, update/recovery/diagnostics | Capability-by-capability support. Calendar accounts, agent APIs and network features are expansion work. |

Concrete dock evidence: [grouping and pin matching, Dock.tsx 1140–1200](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/Dock.tsx#L1140-L1200), [window actions and pin persistence, 999–1053](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/Dock.tsx#L999-L1053), [utilities and extras, 1600–1710](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/Dock.tsx#L1600-L1710), [keyboard slot handling, 1248–1285](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/Dock.tsx#L1248-L1285). Inventory is not a claim that every path works on every PC.

Recommended differentiators are correctness, keyboard access, predictable recovery, per-monitor policies and visible feature budgets. Do not define “more features” as loading every possible widget at startup. A published compatibility matrix is more useful than an unsupported “complete taskbar replacement” label.

## Nectar code review: mechanisms, strengths, costs and risks

### 1. Scheduling is hybrid, not uniformly polling or event-driven

**Observed:** window create/destroy/show/hide hooks feed a 200 ms debounce and `windows-changed` event. Each event schedules an async sleep; a shared last-time value suppresses older emits. Each dock subscribes and calls `get_active_windows`, with a ten-second fallback through `setVisibleInterval`. The backend command runs `EnumWindows` in `spawn_blocking` and reconstructs/group results. [services.rs 276–361](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L276-L361), [Dock.tsx 847–905](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/Dock.tsx#L847-L905), [commands.rs 791–820](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/commands.rs#L791-L820).

**Good:** sequence checks discard stale frontend responses; unchanged lists reuse React state; blocking enumeration is off the UI path. **Inference:** multiple docks can independently request the same full scan after a broadcast; discarding stale results does not cancel backend work. Event storms can create many pending debounce tasks even when one event eventually emits. A single indexed window model, one replaceable debounce deadline and versioned snapshots would reduce duplicated work.

**Observed:** `setVisibleInterval` still creates a `window.setInterval`; its callback returns early when `document.hidden`. It does not cancel the interval or prove that a transparent, collapsed or occluded notch is browser-hidden. [visibleInterval.ts 1–6](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/hooks/visibleInterval.ts#L1-L6). That distinction matters when counting timers, callbacks and visible-feature subscriptions.

### 2. Several persistent workers still wake on short deadlines

- **Observed:** the system worker waits 200 ms normally, 64 ms for two seconds following commands, checks the default audio device every two seconds, reads volume/mute on passes, and scans media every two seconds or on refresh. [services.rs 842–930](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L842-L930), [989–1018](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L989-L1018).
- **Observed:** brightness WMI polling uses 1.5 seconds when supported and 30 seconds otherwise; the foreground/overlap/fullscreen worker ends passes with 150 ms sleeps. [1143–1208](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L1143-L1208), [1538–1545](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L1538-L1545).
- **Observed:** a cursor/mixer fallback runs every 200 ms, including a cursor check before the mixer rectangle guard. A separate window-style guard wakes every second and checks relevant WebView HWNDs. [2034–2055](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L2034-L2055), [3758–3805](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L3758-L3805).
- **Observed:** each mounted notch initializes metrics and schedules a three-second metrics request, without a selected-widget guard in this effect. Wi-Fi refresh is better scoped to the open command center. [App.tsx 1498–1547](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src/App.tsx#L1498-L1547).

These are **wake frequencies in code, not measured CPU percentages or proof that they dominate**. Individual calls, browser throttling, enabled settings and display topology determine the real cost. They provide specific profiling hypotheses and removable work; they do not justify promising a fixed speedup.

### 3. Mouse and audio paths already contain meaningful optimizations

**Observed:** the mouse hook writes coordinates and unparks a worker; the worker normally parks indefinitely, uses a timeout only for pending edge dwell, and limits movement processing with a 32 ms sleep. The hook has its own message loop. [services.rs 1975–2014](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L1975-L2014), [2084–2102](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L2084-L2102). Do not describe it as a perpetual 32 ms cursor poll. The click path can still spawn a thread, and the independent 200 ms fallback remains.

**Observed:** audio visualization checks demand, idles for one second when unwanted, uses 32/200 ms processing waits while capturing, and exits its active capture after 15 seconds without media. FFT buffers/planning are reused within the capture session. [585–615](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L585-L615), [665–695](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L665-L695). A rewrite must preserve these demand gates, then improve waiting and teardown. It must not run a visualizer merely because audio is playing offscreen.

### 4. Multi-monitor cost is more than the number of logical widgets

**Observed:** all-monitors mode creates additional dock and notch WebView windows from the base configuration; it unregisters appbars and destroys removed monitor windows. [monitors.rs 127–158](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/monitors.rs#L127-L158), [services.rs 3498–3543](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L3498-L3543). The config includes a 1920×420 notch, a 1920×600 dock, and a 1920×1080 overlay before runtime positioning. [tauri.conf.json 13–76](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/tauri.conf.json#L13-L76).

**Inference:** extra document state, rendering resources, subscriptions and large transparent surface bounds can increase CPU/GPU memory and composition work. Allocated backing resources must be traced; configuration dimensions alone do not prove exact texture allocation.

Do not repeat the README's simplified “each window has its own process” as a Windows invariant. Microsoft's [WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model) has browser, renderer and helper processes, with sharing dependent on environments, user-data folders and site isolation. Measure the actual process group and per-monitor increment.

**Credit:** Nectar handles scale-factor changes and display/power messages rather than relying only on startup geometry. [services.rs 3426–3495](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L3426-L3495), [3682–3756](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L3682-L3756). The shown power handler repositions for power-status change and user-triggered resume; it does not handle automatic resume in this match. This is a test case, not a demonstrated failure.

### 5. Native previews are good; fallback capture needs tighter budgets

**Observed:** live previews register DWM thumbnail relationships, remove obsolete registrations, and unregister on clear. [live_preview.rs 70–85](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/live_preview.rs#L70-L85), [187–194](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/live_preview.rs#L187-L194). Keep this mechanism where it works instead of streaming screenshots through IPC.

**Observed:** foreground changes spawn delayed capture threads, skip fullscreen windows, throttle per HWND and use a global in-flight guard. Restore events have a separate delayed-thread path. Cached fallback images are encoded strings keyed by HWND; above 15 entries pruning removes dead windows, not the oldest live windows. Thus **15 is not a hard cache limit**. [services.rs 423–533](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L423-L533), [commands.rs 2985–3037](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/commands.rs#L2985-L3037).

**Inference:** focus storms can create waiting threads; many live windows can retain many images; HWND reuse can confuse an identity based only on its numeric handle. Use a bounded capture worker, cancellation/generation checks, byte-limited LRU, and `(HWND, PID, process-start/window-generation)` validation. Minimized/protected windows must be allowed to show an unavailable or stale preview; never restore windows just to obtain an image.

### 6. Cache ownership and reclamation need metric discipline

**Observed:** the icon map loads persisted JSON into memory and has explicit clear operations; process friendly names are kept in a process-wide map. No capacity limit is visible in these paths. [commands.rs 1553–1567](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/commands.rs#L1553-L1567), [1875–1886](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/commands.rs#L1875-L1886), [2343–2356](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/commands.rs#L2343-L2356). This is a capacity/lifetime concern, not proof of a leak. Settings/calendar windows also suspend and lower their memory level when closed, a useful optimization. [on_demand_windows.rs 104–112](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/on_demand_windows.rs#L104-L112).

**Observed:** optional RAM saver is off by default. Its worker wakes every second; when enabled and idle it enumerates the process tree and calls `EmptyWorkingSet`, with nominal two-second retrims, or 15 seconds during media. [memory_saver.rs 77–158](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/memory_saver.rs#L77-L158), [161–217](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/memory_saver.rs#L161-L217). Do not attribute enabled-saver behavior to the default configuration.

Microsoft defines [EmptyWorkingSet](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-emptyworkingset) as removing resident pages. It does not establish that allocations or private commit were freed. Potential refault/latency effects need measurement. Benchmark default and optional saver separately, including first interaction after idle. ROQA should reclaim owned objects and idle features rather than optimize a Task Manager screenshot.

### 7. Shell integration is the main maintenance trap

**Observed:** Nectar imports `.lnk` files from the user's pinned-taskbar directory, shells out to `reg query` for `Taskband\Favorites`, and searches that blob to recover order. The import has a persistent “synced” marker and then uses its own `pinned_apps.json`; the inspected path is not continuous synchronization. [taskbar_pins.rs 18–40](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/taskbar_pins.rs#L18-L40), [99–114](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/taskbar_pins.rs#L99-L114), [commands.rs 1837–1855](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/commands.rs#L1837-L1855). Native registry access would avoid this child process, but would not turn the undocumented blob into a supported API.

**Observed:** tray access walks Explorer's UI Automation descendants, recognizes automation IDs and English/Turkish names, invokes the native overflow, moves it and has shortcut fallbacks. An open-flyout loop can check every 150 ms for up to 2400 iterations. [tray_icons.rs 70–87](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/tray_icons.rs#L70-L87), [291–325](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/tray_icons.rs#L291-L325), [350–417](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/tray_icons.rs#L350-L417). This is native UI reuse via compatibility logic, not a documented rehostable tray service.

**Observed:** taskbar handling changes appbar state, finds primary/secondary Explorer taskbars, modifies visibility/position/styles, saves a crash marker, and restores on next launch. A WinEvent hook suppresses taskbar show/location changes while hidden. [utils.rs 124–235](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/utils.rs#L124-L235), [services.rs 536–554](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L536-L554), [main.rs 270–281](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/main.rs#L270-L281). This is valuable recovery work, but **next-launch recovery is not immediate recovery from process termination**.

The fixed dock also uses appbar registration/position calls and then forces its visible window to the physical bottom, overriding placement adjustments. [services.rs 3143–3177](https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/src-tauri/src/services.rs#L3143-L3177). That increases the need to test other appbars, taskbar auto-hide, accessibility tools and updates. Do not copy this behavior into the default architecture without a specific compatibility requirement.

## Windows constraints checked against Microsoft primary documentation

| Constraint | Verified contract and consequence |
| --- | --- |
| Screen space reservation | [Appbar API](https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars): register `ABM_NEW` with a callback message, negotiate `ABM_QUERYPOS`/`ABM_SETPOS`, handle position/state changes, and `ABM_REMOVE` before destruction. Reserving work area is different from hiding Explorer. Use monitor-specific rectangles; [auto-hide EX](https://learn.microsoft.com/en-us/windows/win32/shell/abm-setautohidebarex) is monitor-aware and registration can fail. |
| Fullscreen | [ABN_FULLSCREENAPP](https://learn.microsoft.com/en-us/windows/win32/shell/abn-fullscreenapp) reports full-screen transitions to registered appbars. Treat it as one signal; borderless games, video and multi-monitor focus still need per-monitor geometry/cloaking checks and manual tests. Do not claim an official API identifies every game mode perfectly. |
| Foreign-window events | [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook): out-of-context events arrive on the installing thread, which needs a message loop; callbacks can reenter. Copy event data into a bounded queue, filter own/child noise, and do no blocking shell or rendering work there. Unhook on the owning thread. |
| Low-level input | [LowLevelMouseProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc): a timed-out hook may be silently removed; Microsoft recommends a dedicated thread handing work off, or raw input where suitable. Prefer normal pointer handling, a small edge activation surface and registered shortcuts. If reliable edge activation needs a global hook, keep it minimal and optional; a hook-less edge strip can itself interfere with applications and must be tested. |
| Keyboard shortcuts and focus | [RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey) can fail on conflicts and reserves Windows-key shortcuts for the OS. [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow) is restricted and can fail even when conditions appear satisfied. Activation must be user initiated, checked and recoverable; do not promise universal focus stealing or Win+number ownership. |
| Native tray | [Shell_NotifyIcon](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw) is for adding/modifying/removing an application's own icon and callbacks. The reviewed public API does not offer a complete third-party icon enumeration/rehosting contract. Show the real Windows tray as an explicit compatibility feature, or reveal the native taskbar. Avoid process injection and Explorer memory scraping. |
| Explorer restart | Microsoft's [taskbar documentation](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar) documents the registered `TaskbarCreated` message. Recreate our tray icon, invalidate Explorer HWND/UIA caches and renegotiate appbars. Use a hidden top-level receiver for relevant broadcasts; do not assume a message-only window receives all broadcasts. |
| Taskbar parity | [ITaskbarList3](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-itaskbarlist3) lets apps publish their own taskbar UI; it is not an enumeration feed for all other apps' progress, overlays, Jump Lists or pin order. Current [TaskbarManager guidance](https://learn.microsoft.com/en-us/windows/apps/develop/windows-integration/pin-to-taskbar) permits requesting user-approved pinning of our own app, subject to support/policy checks and version-dependent limited-access rules. This qualifies the older ITaskbarList3 page's blanket self-pinning restriction; it does not establish enumeration, arbitrary other-app pinning or exact pin-list synchronization. |
| DPI | [Per-monitor-v2 guidance](https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows) and [WM_DPICHANGED](https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged): set awareness before windows exist, use the actual window DPI, update resources and honor the suggested rectangle. No global primary-monitor scale factor. |
| Composition | [DirectComposition concepts](https://learn.microsoft.com/en-us/windows/win32/directcomp/basic-concepts): composition combines bitmap content and transforms; Direct2D/DXGI supplies pixels and DirectWrite supplies text. Composition is not a complete control/accessibility toolkit. Offloading animation can shift work to DWM/GPU rather than eliminate it. |
| Media and audio | [GSMTC manager](https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionmanager?view=winrt-26100) exposes session changes only for participating players; [endpoint callbacks](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nn-endpointvolume-iaudioendpointvolumecallback) report volume/mute changes. Subscribe rather than repeatedly reading unchanged state. Media “any player” still means players integrating with the Windows session mechanism. |
| Notification access | [Notification listener](https://learn.microsoft.com/en-us/windows/uwp/design/shell/tiles-and-notifications/notification-listener): manifest capability, explicit permission requested on the UI thread, and revocation handling. Nusa's Store-only availability is its documented distribution behavior, not proof that Store distribution is the sole Windows permission rule. Prove a packaged desktop route before promising this feature. Reading notification text does not guarantee replaying every app-specific action/reply. |
| Brightness | [SetMonitorBrightness](https://learn.microsoft.com/en-us/windows/win32/api/highlevelmonitorconfigurationapi/nf-highlevelmonitorconfigurationapi-setmonitorbrightness): DDC/MCCS support varies, Microsoft warns about incomplete implementations, and a call takes roughly 50 ms according to the docs. Isolate calls, coalesce slider values and release physical-monitor handles. Built-in display and external-monitor control need different capability paths. |
| Suspend/resume | [WM_POWERBROADCAST](https://learn.microsoft.com/en-us/windows/win32/power/wm-powerbroadcast): automatic resume is distinct from user-triggered resume. Handle both, plus display/session transitions; rebuild device/session state and recompute deadlines rather than replaying missed timer ticks. |
| Virtual desktops | [IVirtualDesktopManager](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager) exposes window desktop ID/current-desktop checks and move. It is not a complete supported enumeration/switching API. “All desktops” behavior and private interfaces must be scoped and tested, not assumed. |
| Accessibility and UIA | [Provider overview](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-providersoverview): custom controls need UIA providers. [Client threading](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading): Explorer UIA calls belong on a separate windowless MTA thread; event registration/removal must have controlled ownership. This client thread is distinct from our own accessibility provider implementation. |

## Proposed architecture and operating budgets

### Stack tradeoffs

| Candidate | Why it can work | Main cost / rejection condition |
| --- | --- | --- |
| Native Rust + HWND + Windows composition + D2D/DirectWrite | Fine control of surface sizes, subscriptions, lifetime and concurrency; no always-loaded browser runtime; Windows API integration in one language | Highest custom UI/accessibility/text-input burden; unsafe FFI, COM/apartment and graphics recovery expertise needed. Reject if the proof cannot provide correct Narrator/IME/keyboard operation within the agreed timebox. |
| C# + WinUI 3 + Windows composition | Native controls, binding, localization, accessibility and tooling can cut delivery risk; same underlying Windows APIs and compositor | Managed/runtime/XAML footprint and interop packaging need measurement; custom translucent desktop overlays still require Win32 work. A GC is not evidence of unacceptable latency. Choose this if it meets budgets materially sooner. |
| Tauri + Rust + WebView2 | Fast iteration, established web layout/accessibility semantics, easiest Nectar-adjacent prototype; native work can stay in Rust | More process/resource accounting and transparent-webview costs; per-monitor documents and IPC fan-out need discipline. Viable if measured totals pass. Avoid resident WebViews for each decorative widget. |

Do not introduce two complete UI stacks at launch simply to make settings easier. Start with native standard controls or a small conventional settings surface. If a separate C#/WinUI or Tauri settings process demonstrably saves enough effort, launch it only on demand and close it completely afterward; its open and post-close costs remain benchmark scenarios.

### Processes, modules and ownership

**Default resident process:** one unelevated process in the signed-in user session. It owns a UI/composition thread, window/taskbar model, scheduler, shared caches and feature registry. Modules are internal boundaries first, not a microservice per widget. Separate dock and notch HWNDs per enabled monitor because they occupy different regions and visibility states; their data model and services remain shared. Prefer windows sized to actual content plus bounded shadow/animation extent, not full-monitor transparent canvases. A tiny separate appbar/activation HWND may be justified if work-area reservation must differ from visible content.

**Worker model:** one window/hook event pump with deterministic unregistration; one MTA integration owner where APIs permit; a small bounded blocking pool for filesystem/icon decoding; dedicated ownership for COM objects requiring a particular apartment; one graphics owner. Initialize/uninitialize COM on the owning threads. `Send` or an async function is not permission to move apartment-bound objects across arbitrary executor threads. No disk/network/DDC/UIA calls on the render, input-hook or window-procedure path.

**Optional compatibility process:** launch for Explorer UIA, slow monitor control, or capture integrations that can hang in foreign code. It exposes a small versioned IPC protocol, accepts bounded requests, and can be terminated without losing the dock model. A timeout on a future does not cancel a blocked COM/driver call; process isolation is the containment where necessary. Do not split benign, cheap features into permanent processes without evidence.

**Recovery guardian:** only if taskbar hiding is enabled. Start and handshake before changing Explorer state. Hold a handle to the UI process, a local recovery journal, session identity and a short progress lease. Process exit is waitable; hang detection requires a timed lease while replacement is active. No guardian heartbeat in the default coexistence mode. The guardian must survive UI termination rather than be placed in a kill-on-close job owned by the UI.

Record original taskbar/autohide state and intended mutation before hiding, validate journal writes, and restore conservatively on termination or lease expiry. Disable rehiding before recovery begins. On `TaskbarCreated`, rediscover shell handles instead of restoring stale HWNDs. If the guardian disappears, the UI should reveal the taskbar. An emergency “restore Windows taskbar” command and next-launch repair remain necessary. Restoration is best effort when Explorer is unavailable, both processes are killed, another shell customizer is active, or an OS change breaks undocumented behavior. Do not claim a helper makes unsupported shell mutation fully safe.

**IPC and release boundaries:** compatibility/settings/guardian IPC needs per-user/session access control, bounded payloads and an allowlisted command schema; no generic shell-execution endpoint. Keep the main process unelevated and treat higher-integrity windows as an explicit compatibility limit. Use signed release/update artifacts, verified update metadata, atomic install/rollback and recovery-before-uninstall ordering. A failed update must leave native taskbar access available. These are proposed requirements, not an assertion that signing prevents every reputation warning or that a packaged app can bypass permission rules.

### Event-driven state and scheduling

1. Subscribe, take one initial snapshot, and reconcile queued changes using a generation counter. Maintain a central window index and derive monitor-specific views. Track identity beyond an HWND; invalidate titles/icons/group membership on the appropriate events and validate before activation.
2. Use narrow WinEvent subscriptions for foreground, visibility, destruction, minimize and relevant location/title changes. Callbacks enqueue small records immediately. Coalesce high-frequency position updates by window. If a bounded queue overflows, record it and schedule one reconciliation snapshot, not a scan for every dropped event.
3. Use GSMTC session/property/playback/timeline events, endpoint/device callbacks, power subscriptions, device watchers, clipboard listeners, directory/registry change notifications where applicable. Registration tokens belong to a service owner and are released when the last consumer goes away. A missing API warrants a named adaptive fallback, not an invented event guarantee.
4. Maintain one scheduler of deadlines with cancellable, replaceable tasks. Dwell, hide delay and reminders are one-shot waits. Align visible clocks to the next displayed time unit. Never request global high timer resolution for ordinary UI; no perpetual `timeBeginPeriod` or spin loop.
5. Continuous quantities need honest sampling: CPU/network meters, an exposed seek bar and an active visualizer can justify bounded rates. Sample once centrally only while displayed or explicitly requested. Hidden, occluded, fullscreen-suppressed and locked states revoke demand. Diagnostic sampling is separately switchable.
6. Rendering becomes dirty only for changed content/layout. Cache glyph layouts, icons and artwork by size/DPI/theme; update only damaged surfaces. Animate transforms/opacity in the compositor where suitable. Any app-rendered animation requests frames only until completion. A static clock without seconds does not need 60 callbacks per second. Compositor animation still consumes GPU/power, so stop it offscreen and honor reduced motion.
7. Art/file thumbnail decoding gets cancellation, input-size limits and a byte-budgeted LRU; persist metadata/paths rather than eagerly loading file contents. Suggested starting limits: 24 MiB decoded CPU image cache, 32 MiB GPU cache per adapter, at most four visible live previews, at most two concurrent decodes and one capture. These are tuning targets, not demonstrated optimal values.
8. Long tasks use per-service queues and last-value semantics for sliders. Store configuration atomically with schema migrations; do not synchronously write on every drag tick. Bind network retries to exponential backoff/cancellation and discard stale responses by generation.

Full-screen, lock and suspend suppress surfaces and expensive services, not just their opacity. Release live thumbnails and capture handles on hide/source destruction; revoke WinRT/COM subscriptions on teardown; release HICON/HBITMAP/HDC/physical-monitor handles through typed guards. Handle D3D device removal by rebuilding resources from the retained model; degraded software rendering must not silently become a permanent busy loop.

### Multi-monitor and Windows 11 correctness

Maintain physical monitor rectangles, work areas, per-window DPI and stable display identity as different concepts. Persist a display-device path/config identity where possible, not only array index or transient `HMONITOR`; have a fallback for changed IDs and disconnected displays. Support negative desktop coordinates and rotated monitors. Keep layout in DIPs locally and convert exactly once at HWND/pointer boundaries.

Debounce topology changes with a generation, recreate only affected windows, and apply the final display snapshot. A removed monitor cannot leave an invisible input-intercepting window or reserved work area. Test 100/125/150/200% scaling, different refresh rates, primary-monitor changes, duplicate/extend switching, USB-C docking, RDP and returning locally. Monitor count must scale surfaces, not duplicate media/calendar/metrics services.

Fullscreen policy is per monitor. A game on monitor A must not suppress intentionally visible UI on B. Combine appbar notifications with foreground/window geometry and DWM cloaking; verify borderless, exclusive, maximized ordinary windows and fullscreen video. Avoid z-order fights and focus stealing. Provide a user suppression shortcut for exceptions.

On automatic/user resume, lock/unlock and Explorer restart: reacquire audio/media/device subscriptions, refresh display/DPI/work areas, invalidate stale handles, recalculate reminder state, and renegotiate appbar reservations. Timers need a documented product policy: elapsed deadlines can count sleep, while stopwatch/focus behavior may differ. Use appropriate elapsed-time and persisted wall-clock deadlines, and test time-zone/system-time changes. Do not produce a burst of overdue notifications after wake.

### Accessibility is part of the proof, not later polish

Expose dock groups, individual windows, media actions, sliders and menus through UIA names, roles, bounds, state and Invoke/Selection/RangeValue patterns. Support a user-configurable conflict-checked focus shortcut, arrows within the dock, Tab across groups, Enter/Space activation, Shift+F10 context menu and Escape to dismiss and restore prior focus. Hover-only affordances need keyboard equivalents. Ordinary passive alerts remain nonactivating, while explicitly opened interactive panels accept focus.

Test Narrator, Windows text scaling/high contrast, reduced motion, touch targets and focus visibility. Shelf/search/task text entry needs IME and clipboard support, RTL and mixed-script text; a DirectWrite draw call alone does not supply these controls. Use standard edit controls when possible. Do not globally record key streams to implement shortcuts. Clipboard and notification history are opt-in with clear/pause controls, bounded retention and no default content logging.

### Plugins and bloat control

No arbitrary in-process native plugins in v1. Begin with declarative themes and first-party feature modules with demand lifetimes. Every feature declares its triggers, permissions, network use, caches, sampling rate and resource owner, and has a disable path that actually releases resources.

If third-party execution becomes necessary, use an isolated host, capability-limited messages and CPU/memory/time budgets. A separate process alone is not a security sandbox; untrusted code needs a real isolation design. A WebView or embedded scripting engine per plugin can erase the footprint advantage. Freeze the extension ABI only after several first-party modules have exposed stable needs.

## Proposed benchmark protocol: no results yet

“Significantly faster” must mean a preregistered workload, metric and practical improvement. Track both absolute experience and relative baseline results. No comparison against Nusa's website numbers alone.

### Matched scenarios

| Scenario | Fair baseline and controlled workload |
| --- | --- |
| A. Minimal notch | ROQA notch only versus installed Nusa matching edition/features: one display, static collapsed UI, seconds/mascot/visualizer disabled where possible; then the same participating media player, art and playback controls. Report unmatched behavior. |
| B. Notch interaction | Same media changes, open/close cadence, timer and shelf containing 0/70/500 path references; visible previews and decoded thumbnails matched. Nusa's 70-file claim is not a guaranteed storage policy for ROQA. |
| C. Dock only | ROQA versus Nectar: same 12 pins, 20 windows, grouping/indicators, visibility mode, preview count and taskbar policy. Include 100/200-window stress. |
| D. Combined | ROQA versus Nectar with dock + notch, media, shelf and same enabled control/status widgets. Run all modules off, then each on, then realistic combined use. This is not a matched Nusa comparison. |
| E. Display scaling | Repeat A/C/D on one and three monitors, mixed DPI and 60/144 Hz; report the incremental cost per monitor. |
| F. Worst practical transitions | 100 focus changes, window create/close burst, four live previews, media/device changes, repeated settings open/close, 1,000 shelf adds/removes, suspend/dock/Explorer restart and first action after ten-minute idle. |

Pin versions, hashes, Windows build, GPU driver, WebView2 runtime, power mode, display layout, theme/transparency, feature settings and media/files. Build release artifacts without debuggers. Compare current stable binaries; source-head prototypes are a separate lane. Run baseline without any tested app, then each app alone in randomized paired order, with identical other processes. Never run two taskbar customizers simultaneously. Use at least five repeated runs after five-minute warmup and 15-minute steady-state recording; also measure cold launch and a two-hour resource churn run. Report variability/confidence and excluded runs. Long battery/power runs need longer paired repeats and thermal stabilization.

### Instrumentation and accounting

- **CPU:** ETW/WPR/WPA process and thread CPU time, context switches, timer wakeups and sampled stacks. Report milliseconds of CPU per second plus both one-core-equivalent and whole-machine percentage with the logical-core count stated. Task Manager's rounded 0% does not mean zero work. Count ROQA helpers and Nectar's associated WebView browser/renderer/GPU/utility processes. Identify WebView process groups by runtime environment/process information and traces, not merely names or a single parent-PID snapshot.
- **Composition and GPU:** record app rendering/presents plus DWM CPU and GPU-engine activity against the no-app baseline. DWM is shared: report a paired **increment**, not a fictional per-app ownership total. Record device/adapter, dedicated/shared GPU memory and system graphics allocations where tooling exposes them. Do not double-count shared system RAM or shared pages when adding process totals.
- **Memory:** separately report total working set, private working set, private bytes/commit, peak commit, handles, GDI/USER objects and cache bytes. Summed working sets double-count shared pages; label that sum and include private metrics. Record start, warm steady state, peak transition, post-churn plateau and after settings close. Download ZIP/exe size, installed size, shared/runtime prerequisites and resident memory are separate columns.
- **Latency:** use application ETW markers and presentation traces to measure input-to-visible-response p50/p95/p99, event-to-state latency, startup-to-interactive and frame pacing. Present intervals are not input latency. Verify click-to-photon with a suitable high-speed-camera/hardware method for a sample; do not call software timestamps photon latency. Capture dropped/deadline-missed frames at each monitor refresh rate and after idle/page trimming.
- **Power:** paired battery discharge/energy readings or external power meter, fixed brightness/network/power settings, stable temperature and long runs. Include DWM/audio helpers and dGPU wakeups. Report uncertainty and whole-system baseline; estimated process energy alone is not definitive. No high-frequency sampler should contaminate idle runs without a control run measuring its overhead.
- **Reliability:** deterministic fault-injection runs for UI crash/hang, compatibility-host death, guardian death, Explorer restart, source app hang and permission revocation. Archive traces, settings, resource graphs and reproduction steps locally; clipboard/notification contents stay out of logs.

### Acceptance gates, all proposed targets

These numbers are initial engineering budgets for a named reference Windows 11 laptop with integrated graphics, 16 GB RAM and a 60 Hz display. They are **targets, not measured results or promises across hardware**. Final thresholds should be ratified after baseline variance is known; do not move them after seeing a disappointing result without documenting the change.

| Gate | Proposed target |
| --- | --- |
| Quiescent core | Static no-seconds dock + notch, optional modules off: app tree ≤1 ms CPU/s averaged over 15 min; no app-owned animation/present loop after settle; ≤1 periodic app timer wake/s average, with remaining wake sources named. Report DWM increment separately. |
| Resident memory | Combined one-monitor core ≤80 MiB private bytes and ≤60 MiB private working set at steady state; three monitors ≤120 MiB private bytes. Initial incremental GPU allocation budget ≤64 MiB per enabled monitor, reported separately. No forced working-set trim needed to pass. |
| Useful idle media | Collapsed media metadata/control mode with no visualizer/seconds/lyrics scrolling: app tree ≤2 ms CPU/s average, no continuous app rendering. If a track-position display requires ticks, disclose the changed scenario. |
| Input and animation | First visible hover/key response p95 ≤50 ms, p99 ≤100 ms; end-to-end animation interval p95 ≤one refresh period and p99 ≤two periods during scripted transitions, with missed-deadline rate <1%. Measure high-refresh monitors separately. |
| State freshness | Window lifecycle/foreground changes reflected p95 ≤250 ms; volume notifications p95 ≤100 ms after OS callback. External hardware brightness completion is excluded from UI response and reported separately. |
| Scaling/concurrency | 200 windows and 100 focus changes: bounded pending work, no thread-per-event growth, no stale app resurrection, and the same input-response target. One shared media/metrics service irrespective of monitor count. |
| Resource lifetime | After 1,000 cycles and cooldown, private commit within 5 MiB of warmed baseline, with bounded named caches and no monotonic handle/GDI/USER-object growth; investigate variance before declaring leaks or success. |
| Recovery | With replacement explicitly enabled, UI process death restores accessible native taskbar within 2 s of process-exit signal; UI hang within 5 s of lease expiry; Explorer restart rehydrates within 5 s after the taskbar is usable. Pass 100/100 scripted repetitions on each supported build, including originally auto-hidden taskbar. |
| Power | No dGPU wake caused by a static UI; idle whole-system power within 0.3 W of the no-app paired baseline if measurement precision supports it. Otherwise classify inconclusive, not pass. |
| Relative improvement | In matched combined scenarios, seek ≥30% lower app-tree CPU time and private bytes than Nectar, with no material DWM/GPU/power/latency regression. If baseline CPU is near the measurement floor, judge the absolute gate and mark relative CPU improvement inconclusive. |
| Accessibility | Complete core flows keyboard-only and with Narrator on all supported DPI/text scales, including focus return, context menu and IME editing. This gate cannot be traded for a memory win. |

For Nusa, report measured notch-only deltas and quality/feature differences. Do not require an arbitrarily smaller footprint than an unaudited 30 MB claim, and do not say a larger combined app “beats Nusa” by having a dock. A broader app can have an efficient minimal mode and a transparent incremental cost for extra capability.

## Phased proof gates and effort risk

Estimates are planning ranges for experienced Windows engineers, not measured productivity or a delivery commitment. Existing team skills can move them substantially. Parallelism does not eliminate device/lifecycle integration work.

| Phase | Proof and exit decision | Indicative effort |
| --- | --- | --- |
| 0. Contract and baseline | Agree OS/build/hardware matrix, parity checklist, coexist/replacement policy, licensing direction and benchmark scripts/protocol. Establish Nectar/Nusa binary baselines. Stop if success cannot be defined. | 3–5 engineer-days |
| 1. Native vertical proof | Small dock + notch on mixed-DPI displays, one media subscription, one real app group, DWM preview, keyboard/UIA flow and on-demand rendering. Measure native surface/device lifecycle. Decide Rust versus C#/WinUI or Tauri using evidence. | 2–4 engineer-weeks |
| 2. Shell/recovery proof | Appbar coexistence, Explorer restart, fullscreen/suspend/topology matrix. If hiding is essential, guardian/journal and fault injection before any daily-driver use. Expose limits of tray/pins rather than adding private APIs by default. | 2–4 engineer-weeks, partly overlapping phase 1 |
| 3. Core product | Pins/groups/launcher, reliable interactions, media, timer, bounded shelf, native settings, update/uninstall recovery and diagnostics; regression tests for event model. Pilot with real hardware. | 6–10 engineer-weeks |
| 4. Feature expansion | Notification packaging/permissions, control center, clipboard, calendar accounts, lyrics, advanced metrics/themes. Each feature pays its measured idle/active cost. Plugin execution remains a separate project. | 8–16+ engineer-weeks |

A credible core beta is roughly **10–18 elapsed weeks with two experienced engineers plus QA/design support**, depending on overlap and scope. Broad polished parity plus additional integrations can take **6–12+ months**. A solo developer learning Win32/graphics should expect longer. Exact tray/taskbar parity may stay infeasible within supported APIs regardless of time spent. Use stop/go proofs to avoid spending months building controls on a renderer that misses accessibility or compatibility gates.

Nectar's GPL-3.0-only manifest is a concrete reuse constraint. Decide distribution/license strategy before copying code or linking derived components. A fresh architecture based on public API contracts and independently implemented behaviors is a different scope from a Nectar fork; do not silently mix the two. Nusa's visual inspiration does not establish permission to reuse its branding/assets/binaries or access to its application source.

## Highest-cost mistakes to avoid

1. Declaring “Rust is faster” or “0% CPU” without matched traces and compositor/descendant accounting.
2. Treating Nectar as an unoptimized toy and losing its parked workers, suspended settings and native DWM previews in a rewrite.
3. Calling a one-time pin import continuous taskbar synchronization, or calling UIA flyout automation a supported tray API.
4. Hiding the native taskbar before recovery survives termination, hang, Explorer restart and uninstall failure.
5. Moving work from application CPU to DWM, dGPU, GPU memory or page faults and advertising only the improved number.
6. A browser/native surface or independent sampler per widget/monitor, unbounded per-event threads, and caches bounded by object count assumptions rather than bytes.
7. Reimplementing a control toolkit and discovering accessibility, text scaling, RTL and IME late.
8. Expanding to calendar accounts, notification history, plugins and system dashboards before the core service lifecycle is proven.

## Strongest objections to my recommendation

**Custom native UI may cost more than it saves.** The proposed renderer takes responsibility for controls, focus, accessibility providers, text input, layout and device loss. A mature WinUI implementation could be cheaper to maintain and still meet every meaningful performance target. Phase 1 must prove a real accessible interaction, not just a pretty animated shape.

**The workload may be integration-bound.** Shell calls, icon decoding, DDC delays and media providers can dominate regardless of language or renderer. A careful Nectar/Tauri refactor might deliver most savings without a rewrite. A small instrumented comparison could invalidate the native recommendation.

**A large architecture can recreate the overhead it seeks to remove.** Three always-on helpers, polling health checks, GPU caches and a complex scheduler could consume more than a single simpler process. Separate only the risky foreign-call boundaries justified by failure tests. Default coexistence avoids guardian cost and unsupported mutation.

**The real product requirement may be exact Windows taskbar behavior.** Neither native graphics nor Rust repairs the absence of supported APIs for all Explorer internals. If exact native tray/pin/Jump List/progress parity is nonnegotiable, narrow the promise or revisit the product concept before selecting a renderer.

## Unknowns and next evidence needed

- User priority between coexistence and replacement, feature shortlist, Windows versions, hardware targets, ARM64/RDP expectations, commercial/open-source distribution, team expertise and schedule.
- Actual release performance of Nectar and Nusa on controlled hardware; no CPU, memory, power, latency or reliability measurements were made in this review.
- Nusa implementation and source availability beyond the inspected public website/release repository; its API and renderer claims cannot be code-audited here.
- Actual WebView2 environment/process topology and backing-surface allocation in a running Nectar release; dynamic configuration may differ from base dimensions.
- Window/session event coverage across participating/nonparticipating media apps, elevated windows, game modes, virtual desktops and shell customizers; which fallback checks remain necessary after event subscriptions.
- Notification listener behavior and identity/capability packaging for the selected distribution; arbitrary notification actions are not assumed.
- Scope of safe tray access across current supported Windows builds and languages, and the user's tolerance for explicitly best-effort compatibility.
- Whether proposed memory/power budgets are realistic for accessible UI at three 4K mixed-refresh monitors. Ratify them using traces, not estimates.

## Three questions for the other council model

1. Which specific supported Windows contract would you use for native tray access and pin parity, and which behaviors would you explicitly exclude? If hiding remains required, how does recovery work after both a UI hang and Explorer restart without restoring stale shell state?
2. What evidence would make you choose C#/WinUI or a disciplined Tauri refactor over native Rust, and what is the smallest prototype that fairly tests accessibility, mixed-DPI transparency and idle compositor cost rather than only animation appearance?
3. Which proposed acceptance targets or scope boundaries are least credible, and how would you preregister a fair Nectar combined-app comparison and a separate Nusa notch-only comparison, including DWM/WebView/GPU/power attribution?

**Recommended next action:** approve a feature/compatibility contract and a timeboxed vertical proof; do not authorize the full feature build from architecture prose alone. Durable research findings are preserved in this report; no global learning or telemetry files were written under the read-only scope.
