# Council review (Opus 5.5, high): native dock + notch for Windows

Author: Opus 5.5 (independent council member). I did not read Sol's report.
Date: 2026-11-09. Scope: research and planning only. Nothing was implemented, installed, committed or sent.
Method: gstack direct-analysis path (evidence first, adversarial engineering review, product review). I cloned the Nectar source, read the Nusa site, inspected Nusa's release binary statically (I did not run it), and fetched the relevant Microsoft Learn pages as text.

---

## 0. Verdict in ten lines

1. **Feasible, but the hard part is not rendering.** About 80% of the risk is in Explorer and shell integration: hiding the taskbar and restoring it after a crash, appbar fights, fullscreen detection, tray access, pinned-item import, multi-monitor/DPI churn, AV/SmartScreen. Nectar's issue tracker is mostly about these.
2. **Beating Nectar on footprint is realistic and large.** Nectar is Tauri/WebView2. Its own doc reports **217 MB commit, 8 processes and 204 threads at idle** (5 monitors). A single native process can plausibly land at a small fraction of that. That is a 5× or bigger commit win I would be comfortable targeting. It is still a target, not a promise.
3. **Beating Nusa on footprint is not something to promise.** Nusa is already native (per its claims), notch-only and closed-source. Its "~30 MB / 0% idle CPU" figures are unverified vendor claims. A dock+notch app should aim to be *in the same class* (≤1.5× Nusa on a like-for-like protocol) and win on features. Do not market "lighter than Nusa".
4. **Recommended architecture:** one always-on native Rust process. Win32 HWNDs + Windows.UI.Composition visuals + Direct2D/DirectWrite content, drawn only on change. Animations run as **composition animations** (spring/keyframe), so the compositor animates without per-frame app CPU. The rarely-used Settings UI runs as a separate **on-demand** process that exits when closed. WebView2 or WinUI is acceptable *there*.
5. **Gate it with a 3–5-week kill-test**, not faith. If the native spike cannot hit the idle and robustness gates, or if text/a11y/DPI costs blow up, fall back to C#/WinUI 3 (NativeAOT) for surfaces. Keep the same event-driven Rust/C# core either way.
6. **Ship the notch first, as a "companion" dock second, and taskbar replacement last and opt-in.** Microsoft Store policy 10.2.8 makes taskbar hacking incompatible with a Store build. Notifications-in-notch, the best extra feature, needs package identity.
7. **Licensing:** Nectar is GPL-3.0-only. Do not read-and-port its code unless the product will be GPL-3.0. Nusa has no license and no source, so no code rights at all. Features and ideas are fair game. Expression, assets, names and mascots are not.

---

## 1. Evidence base and confidence

| Source | What | Immutable ref |
|---|---|---|
| Nectar repo | Full source, shallow clone | `kua8/Nectar@ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a` (2026-11-08, v1.9.5 in Cargo.toml) |
| Nectar issues | 80 most recent titles via `gh issue list` | live, read 2026-11-09 |
| Nusa site | https://rama2555.github.io/nusa/ (index.html, 749 KB) | repo `rama2555/nusa` tree `8b7f7920edf0e7eb064c931ca405942a41657432` |
| Nusa binary | `NusaSetup.zip` v1.2.0 → `NusaSetup.exe` SHA-256 `bc4133305b28b929eca42cc32833dc4fd34002ce35676d27761942419f4d4b02` | static string scan only, never executed |
| Microsoft Learn | 23 pages fetched as text (URLs in §12) | live, read 2026-11-09 |

Permalink base for Nectar citations: `https://github.com/kua8/Nectar/blob/ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a/`. A citation `services.rs:885` means `src-tauri/src/services.rs#L885` under that base unless another path is shown.

Confidence labels: **High** means read in source or official docs this session. **Medium** means strong inference, or a doc that implies but doesn't state it. **Low/Unverified** means from memory or a vendor claim, so verify it in the kill-test.

---

## 2. Nectar: what it actually is (source-verified)

**Stack (High):** Tauri 2.11 + React 19 + framer-motion. The Rust host is `windows` 0.61 with about 55 Win32/WinRT features (`src-tauri/Cargo.toml:21-100`). License `GPL-3.0-only` (`Cargo.toml:7`, `package.json`). It began as a fork of Bloom (GPL), per the README's Credits section, and `shelf.rs:1` says "adapted from Bloom (GPLv3)". Size: about 16.9k lines of Rust in `src-tauri/src` and about 17k lines of TS/TSX in `src`. Most of it sits in two god-files, `services.rs` (3,929 lines) and `commands.rs` (4,293 lines).

**Footprint per its own doc (High that the doc says it; Medium that the numbers mean what they seem):** `docs/memory.md:16-26`, measured on 5 monitors:
- After its optimisations: **private WS 14.7 MB, total WS 38.5 MB, commit 217 MB, 8 processes, 2 renderers, 204 threads, idle CPU 0.032 cores.**
- **Inconsistency:** line 9 says "default settings". Lines 103-130 attribute the low working set to `EmptyWorkingSet` trimming every 3 s. But line 128 says Memory Saver is "default off", and `memory_saver.rs:155-159,176` confirms it is gated on `nectar-ram-saver`, defaulting to false. So either the headline WS was measured with a non-default setting, or the trim isn't what produced it. **Treat 217 MB commit and 8 processes as the honest floor, and the 14.7 MB private WS as an optimistic, trim-gamed metric.** This matters for us: *Task Manager's "Memory" column is trimmable, and we must not chase it.*
- Doc drift: `memory.md:65,77,103` reference `lazy_windows.rs`, `webview_tuning.rs` and `mem.rs`, which don't exist. The real files are `on_demand_windows.rs`, `webview_args.rs` and `memory_saver.rs`. This is a minor point but signals fast churn.

**Where its CPU wakeups come from (High, line-cited).** These are the anti-patterns we must not copy:

| Mechanism | Evidence | Better alternative |
|---|---|---|
| Media/volume worker wakes every 200 ms idle, 64 ms active | `services.rs:885-886,914-919`; doc admits "wakes 5 times a second" (`memory.md:134-142`) | GSMTC events `SessionsChanged`/`CurrentSessionChanged` + per-session `MediaPropertiesChanged`/`PlaybackInfoChanged`/`TimelinePropertiesChanged` (MS Learn, verified); `IAudioEndpointVolumeCallback` for volume |
| Default audio device checked by polling every 2 s (two loops) | `services.rs:685-696`, `services.rs:892-909` | `IMMNotificationClient::OnDefaultDeviceChanged` (verified; callbacks must be non-blocking) |
| No event subscriptions at all for media/audio | `grep` for `Changed(`/`TypedEventHandler`/`IMMNotificationClient`/`RegisterEndpointNotificationCallback` → **no hits** in `src-tauri/src` | as above |
| Brightness via WMI query every 1.5 s | `services.rs:1149,1160-1166` | WMI event subscription (`WmiMonitorBrightnessEvent`, **Unverified**) or refresh only while the slider is visible |
| Foreground/fullscreen/overlap loop sleeps 150 ms forever | `services.rs:1172-1194`, `:1542` | `EVENT_SYSTEM_FOREGROUND`, `EVENT_SYSTEM_MOVESIZEEND`, minimize events, `ABN_FULLSCREENAPP` (verified in appbar doc), `SHQueryUserNotificationState` on change |
| **Global** `EVENT_OBJECT_LOCATIONCHANGE` + `EVENT_OBJECT_SHOW` hooks across all processes, then `GetClassNameA` on every event to find the taskbar | `services.rs:231-236,536-555` | LOCATIONCHANGE fires for carets, cursors and scrollbars system-wide. If needed at all, scope it to Explorer's PID/thread, filter `idObject==OBJID_WINDOW` first, and coalesce to ≤1 evaluation per frame |
| Global `WH_MOUSE_LL` hook + a 200 ms `GetCursorPos` watchdog "because the hook goes silent over elevated windows" | `services.rs:2002-2015`, `:2034-2050`; 40 ms dwell poll `:1937,1982` | See §5.3. LL hooks sit synchronously in the input path and are silently removed on timeout |
| Global `WH_KEYBOARD_LL` | `services.rs:54` | `RegisterHotKey` for hotkeys. A keyboard LL hook is an AV-heuristic magnet (Nusa's FAQ admits this too) |
| Audio visualiser: WASAPI loopback, 32 ms loop, FFT | `services.rs:595-700` | Off by default. Nectar issue **#13** "Microphone Request? Why?" shows users see it as mic access |
| Window thumbnails via Windows.Graphics.Capture → staging texture → CPU copy, polled 25×20 ms | `capture.rs:50-70` | `DwmRegisterThumbnail` (live, DWM-rendered, no copy). Nectar also uses this in `live_preview.rs:131` |
| Album art as base64 data URLs over IPC | `memory.md:89-90,166-169`; `services.rs:850` | Decode once (WIC) to display size, cache by hash. Never serialise pixels across a boundary |
| Memory "fixed" by `EmptyWorkingSet` every 3 s | `memory_saver.rs:9,128-139,161-200` | Don't. This is page-fault churn that games Task Manager. Fix the allocation instead |

**Shell integration (High):**
- Hides the taskbar with `ABM_SETSTATE` (autohide) **plus** `ShowWindow(Shell_TrayWnd, SW_HIDE)` and parks it off-screen (`utils.rs:168-260`). It "slaps it back down" whenever Explorer shows or moves it (`services.rs:545-553`). This is a feedback loop with Explorer, and only the documented `ABM_SETSTATE` part is supported.
- A crash marker file lets the next launch restore the taskbar (`utils.rs:186-197`). The README says "Start Nectar again, it restores the taskbar on launch even after a crash". So a crash leaves the user without a taskbar until relaunch or uninstall.
- Pinned apps: shells out to `reg query` and parses the **undocumented** `Taskband\Favorites` binary blob, plus `.lnk` files in `Quick Launch\User Pinned\TaskBar` (`taskbar_pins.rs:18-41`).
- Tray: drives Explorer's tray via **UI Automation** and synthetic key presses (`tray_icons.rs:272-300`). The README roadmap still lists "Background tray programs in the notch" as undone.
- Hides the native volume OSD with `FindWindow("NativeHWNDHost")` + `SW_HIDE` (`services.rs:877-883`), which is undocumented.
- Attention dots via `RegisterShellHookWindow` (`attention.rs:54`). Microsoft documents this as "not intended for general use… may be altered or unavailable in subsequent versions" (verified).
- No `TaskbarCreated`, `ABN_FULLSCREENAPP` or `SHQueryUserNotificationState` handling found by grep (Medium: absent by grep, could be handled indirectly).
- Interesting feature: a local HTTP agent-status API with a bearer token on 127.0.0.1. It auto-installs Claude Code hooks (UserPromptSubmit, PreToolUse, PermissionRequest, …) so the notch shows coding-agent state and can answer permission prompts (`agent_api.rs:248-257,342`).

**Real-world failure modes from issues (High that they were reported):** Kaspersky/Avast flag the unsigned exe (#83, #91, #116); click pass-through fails for browser tabs under the notch (#115); Search flyout z-order (#71); dock doesn't reserve area (#69); fullscreen problems (#56); black previews (#92); the tray flyout keeps DWM styling after exit (#38); DisplayPort brightness/DDC (#66); mic prompt (#13). The README itself says "fixing one issue often causes another one".

**Takeaway:** Nectar proves the *feature set* is buildable by one person. It also shows that most of the pain is shell integration plus a WebView2 floor, not drawing.

---

## 3. Nusa: claims vs what can be verified

**Source availability (High): none.** `rama2555/nusa` contains only `README.md` (one line: `# nusa`), `index.html` and `nusa-icon.png`. There is no license field. Releases ship `NusaSetup.zip` (≈2.4 MB) → `NusaSetup.exe` (2.9 MB). A static scan shows an **Inno Setup 6.7** installer with an LZMA payload, so its runtime tech can't be confirmed without unpacking tools, which I deliberately didn't install. A ~3 MB download **does not prove "no web engine"**: a Tauri app on the system WebView2 is also a few MB.

**Vendor claims (Unverified; quoted from the site):** "Written in Rust and drawn by your graphics card, with no web engine". "~30 MB typical memory use", from a Task Manager screenshot showing 30.6 MB, "About 45 MB with 70 files on the shelf". "0% CPU when idle… even with music in the background". "Draws only when something moves". "Windows tells Nusa when a song changes" (event-driven). Windows 10 1809+ / Windows 11, x64.

**What the Task Manager figure is:** the "Memory" column is a private working set. It is trimmable and excludes shared/GPU memory and DWM's cost of compositing Nusa's surfaces. "0% CPU" in Task Manager is rounded and sampled. Neither measures wakeups, GPU or power. Nusa's own changelog says 1.2 brought "Lower GPU and power use while Nusa sits idle", which implies earlier builds had idle GPU/power cost.

**Product facts relevant to us (High that the site says so):**
- Features: music from any GSMTC player, **synced lyrics via LRCLIB** (sends title/artist/length), Pomodoro "Live Activity", water reminder, file shelf (stores paths, not copies), to-do with reminders, volume/brightness/Caps Lock/IME-language overlays, battery/Wi-Fi/headphone peeks, prayer times computed locally, clipboard history, widgets, mascot, and a "minimal notch" for thick bezels.
- **"Notifications and the Today page need the Microsoft Store version, because Windows only lets Store apps read notifications."** Microsoft's docs say the requirement is the `userNotificationListener` **capability in a package manifest** plus a user consent prompt (`RequestAccessAsync`), with silent failure if revoked. That is package identity, not Store distribution per se. Package identity is also available to unpackaged apps via "packaging with external location" (a signed sparse package). **Whether `UserNotificationListener` works under a sparse identity is not stated in the docs I read (Medium), so it's a kill-test item.**
- Business model: free ZIP, Store version with "Nusa Pro" (widgets, glass, floating style). Unsigned ZIP shows SmartScreen warnings.
- **Nusa is notch-only and does not replace the taskbar.** That is very likely *why* it can be in the Store (see §5.1).

---

## 4. Licensing scope

- **Nectar = GPL-3.0-only** (`Cargo.toml:7`, LICENSE) and itself derived from Bloom (GPL). Copying, translating or "porting" any of its code (e.g., the Taskband blob parser or the tray UIA walker) makes our work a derivative, so distribution must be under GPL-3.0 with source. A paid tier is allowed under the GPL, but anyone may rebuild and redistribute it.
- **Ideas, features and UX patterns are not code rights, but they are also not a licence to copy expression.** Recreate behaviour from Microsoft docs and first principles. Don't reuse Nectar's or Nusa's assets: Nectar's `public/mascot/*.svg`, icons, sounds and screenshots, or Nusa's mascot "eyes" look, name or site copy.
- **Clean-room discipline (actionable):** decide GPL vs proprietary *before* anyone reads Nectar's source in depth. If proprietary, people who studied Nectar's modules write specs ("what it must do") and others implement. This report cites Nectar lines for *diagnosis*, not as templates.
- **Nusa:** no license and closed source, so zero rights to code or binaries. Don't reverse-engineer it. Measure it as a black box only.
- **Store + GPL:** MS Store distribution of GPL apps is common when you are the copyright holder. If we incorporate third-party GPL code, every channel must honour GPL terms (Medium: legal nuance, get advice if monetising).
- **LRCLIB** for lyrics: check its terms and rate limits before relying on it (Unverified).

---

## 5. Windows integration traps (verified where marked)

### 5.1 Store policy vs taskbar replacement (High)
Store policy **10.2.8**: *"You are required to use supported methods and must obtain user consent to change any user's Windows settings… or modify the user's Windows experience in any way. Unsupported methods include but are not limited to use of accessibility APIs or undocumented or unsupported APIs in unsupported ways."*

That rules out of a Store build: `SW_HIDE` on `Shell_TrayWnd`, `NativeHWNDHost` OSD hiding, Taskband blob parsing, and UIA-driving Explorer's tray. **Implication: two distribution flavours, or one product that never hacks the shell.** My recommendation is that the Store/MSIX build ships notch + companion dock using documented APIs only. "Replace taskbar" mode lives in a signed direct-download build, opt-in, with explicit consent.

### 5.2 Appbar and taskbar restore (High for API text, Medium for strategy)
- `ABM_SETSTATE` documents only autohide/always-on-top for the taskbar. It "always returns TRUE", so you get no success signal. There is **no supported API to hide the taskbar.** Autohide still leaves a reveal strip and the taskbar reappears on edge hover, Win key and notifications. Hiding it entirely is unsupported by definition.
- Appbars must send `ABM_REMOVE` before exit. Only **one autohide appbar per edge**, first-come-first-served, and `ABM_SETAUTOHIDEBAR` fails if one exists. Appbars get `ABN_FULLSCREENAPP` and must drop in z-order when it fires. Nectar overrides `ABM_SETPOS` corrections (`services.rs:3174-3176`), which is a recipe for fights with other appbars (DisplayFusion, Rainmeter docks and the like).
- **Restore must not depend on our process surviving.** Use a tiny watchdog/restore path: a scheduled-task or startup "restore if marker present", plus `SetUnhandledExceptionFilter`/panic hook restore, plus an Explorer-side safety (autohide only, never `SW_HIDE`, in the default mode). Explorer restarts broadcast the registered `TaskbarCreated` message, which you must handle (High from long-standing shell docs; not re-fetched this session).
- Uninstall must restore the original `ABM_GETSTATE` value, not a guessed one.

### 5.3 Hooks, threads and the input path (High)
- `SetWinEventHook`: *"The client thread that calls SetWinEventHook must have a message loop"*. Out-of-context events are *"delivered on the same thread that called SetWinEventHook"* and queued asynchronously. **So put hooks on a dedicated hook thread with its own message loop, never the UI/compose thread.** Do O(1) work per event (set a dirty bit, post one coalesced message) and use `WINEVENT_SKIPOWNPROCESS`.
- `LowLevelMouseProc`: called in the installer's thread via a sent message, so it needs a message loop. If it exceeds `LowLevelHooksTimeout` (max 1000 ms since Win10 1709), *"on Windows 7 and later, the hook is silently removed… There is no way for the application to know."* LL hooks are synchronous in the input pipeline, so a stalled thread lags the user's cursor system-wide. **Prefer `RegisterRawInputDevices(RIDEV_INPUTSINK)` + `GetCursorPos` on `WM_INPUT` for edge detection.** It is asynchronous and not in the input chain (Medium: behaviour over elevated/UIPI windows needs testing). The alternative is a 1-px trigger window, but it steals clicks from maximised apps' top edge (exactly Nectar #115).
- `RegisterShellHookWindow` is explicitly "not intended for general use". Use `HSHELL_FLASH` only as best-effort, with a documented fallback (`FlashWindowEx` state isn't queryable, so accept degradation).

### 5.4 Media (High)
`GlobalSystemMediaTransportControlsSessionManager` exposes `SessionsChanged` and `CurrentSessionChanged`. Sessions expose `MediaPropertiesChanged`, `PlaybackInfoChanged` and `TimelinePropertiesChanged`. **No polling needed.** Traps:
- The timeline position is not continuously evented, so interpolate locally. Render the progress bar as a **composition linear animation** re-synced on events, which costs zero app CPU per frame.
- Thumbnails are streams. Decode once off the UI thread to the target pixel size, and dedupe by content hash, because the same art arrives repeatedly.
- Multiple sessions: browsers expose one session per tab or app in varying ways.

### 5.5 Notifications (High for the API, Medium for the packaging path)
`UserNotificationListener` (Win10 14393+) needs the `userNotificationListener` capability in a package manifest, `RequestAccessAsync` consent, and re-checking `GetAccessStatus`. When revoked it *fails silently* (empty lists), so the UI must show the permission state. Options: (a) full MSIX (Store or sideload); (b) unpackaged plus a signed external-location identity package (documented to unlock "toast and push notifications, background tasks, … startup tasks, privacy consent prompts"; listener not explicitly listed, so test it). Only toast-type notifications are retrievable.

### 5.6 Virtual desktops (High)
Public `IVirtualDesktopManager` has exactly three methods: `IsWindowOnCurrentVirtualDesktop`, `GetWindowDesktopId` and `MoveWindowToDesktop`. There is no enumeration, switching, naming or change event. The internal COM interfaces change GUIDs across builds. **Ship "dock shows only current-desktop windows" (supported). Defer the desktop switcher, or isolate it behind a version-checked, disable-able module.** Detect desktop switches indirectly (foreground and cloak events, Medium).

### 5.7 Tray (High that there's no API)
There is no documented API to enumerate *other apps'* notification-area icons. `Shell_NotifyIcon` covers only your own. Nectar uses UIA on Explorer, which violates Store 10.2.8 and is fragile across 24H2/25H2 shell updates. **Defer tray mirroring. In replace-taskbar mode, offer a button that opens the native overflow.**

### 5.8 Thumbnails and capture (High)
DWM thumbnails are *"dynamic, constant connections"* rendered by DWM into your window, with no copy into your process. Register on hover and unregister on leave. **Windows.Graphics.Capture** draws a yellow border unless the app requests `GraphicsCaptureAccessKind.Borderless`, which needs the `graphicsCaptureWithoutBorder` capability and a user prompt. That is one more reason to prefer DWM thumbnails.

### 5.9 UI Automation (High)
*"Conflicts can occur when a client application attempts to interact with its own UI on the UI thread… very slow performance, or even cause the application to stop responding."* Any UIA client work (we should do none in the Store build) goes on a separate MTA thread. **As a UIA *provider*** (our own accessibility), a custom-drawn composition UI has no free accessibility. We must implement `IRawElementProviderSimple`/`Fragment` per dock item and notch control, or use AccessKit's Windows adapter (Medium: crate exists, maturity unverified). Budget for it explicitly (§9).

### 5.10 Other traps
- **Unsigned binaries + LL hooks + taskbar manipulation** are a heuristic AV trifecta (Nectar #83/#91/#116; Nusa's FAQ). Budget code signing from day one.
- **Hidden GPU cost:** acrylic/blur backdrops make DWM re-blur whenever content behind changes, such as video under the dock. That cost lands on `dwm.exe`, not our process. Task Manager shows our 0% while battery drains. Measure DWM separately and make blur optional.
- **Multi-monitor:** per-monitor DPI v2, mixed refresh rates, hot-plug, sleep/resume and RDP sessions. Nectar creates a WebView per window per monitor ("each extra monitor gets its own windows"). We need one process with shared caches and per-monitor HWND/visual trees only.
- **Fullscreen detection:** combine `ABN_FULLSCREENAPP` + `SHQueryUserNotificationState` + foreground-rect check on foreground change events, not a polling loop.
- **Clipboard history (if built):** `AddClipboardFormatListener` (supported). Honour the `ExcludeClipboardContentFromMonitorProcessing` format so password managers' copies are skipped (Medium: from memory, verify). Never persist the history by default.

---

## 6. Architecture options: honest trade-offs

"Native" does not mean "fast". A native app that polls at 64 ms, hooks LOCATIONCHANGE globally and redraws a full-window D2D surface per frame will lose to a well-tuned WebView. The speed comes from the **event model, wakeup discipline and letting the compositor animate**, not the language.

| Dimension | A. Rust + Win32 + Windows.UI.Composition / DirectComposition + D2D/DirectWrite | B. C#/WinUI 3 (Windows App SDK), ideally NativeAOT | C. Tauri (WebView2) like Nectar |
|---|---|---|---|
| Idle floor (commit, procs) | Lowest achievable: 1 process. Target ≤40 MB commit (Medium: depends on D3D device and DirectWrite font cache) | Higher: XAML runtime + WinAppSDK. Measure in the spike rather than guess (Unverified). Single process | Highest: browser + GPU + renderer(s) + utility processes. Nectar measured **217 MB commit, 8 processes** (5 mon) |
| Animation cost | Composition animations (incl. spring `NaturalMotion`) run in the compositor, near-zero app CPU per frame | Same compositor underneath (`Microsoft.UI.Composition`); XAML layout can add per-frame work | Chromium compositor; framer-motion JS animations cost renderer CPU |
| Shaped, transparent, topmost, click-through windows | Full control (`WS_EX_NOREDIRECTIONBITMAP`, `WS_EX_LAYERED`/`TRANSPARENT` toggles, per-pixel hit-testing in our code) | Possible but fights the framework (WinUI 3 window transparency/backdrops are window-level; custom regions need interop) | Works (Nectar does it), but hit-testing round-trips through IPC |
| Text, i18n, RTL (Nectar ships Arabic) | **Hard**: DirectWrite gives shaping, but we write layout, ellipsis, bidi UI mirroring and IME | Free | Free |
| Accessibility (UIA) | **Hard**: hand-written providers or AccessKit | Mostly free | Mostly free (Chromium a11y tree) |
| Settings UI (forms, lists, search) | Painful to build natively; **use a separate on-demand process** (WinUI or WebView2) | Natural | Natural |
| Dev velocity / hiring | Slowest; rare skill set (COM + composition + Rust) | Medium; large .NET talent pool | Fastest; web skills |
| Shell integration code | Same Win32 work in all three. **This is the true critical path regardless of UI choice** | same | same |
| Crash containment | One process: a panic kills both surfaces (mitigate with panic=abort + restart watchdog) | same | Separate renderer crashes are survivable |

**Recommendation (Medium-High confidence): hybrid "A for the always-on surfaces, anything for the rarely-on ones".**

```
roqa.exe  (single always-on process, Rust)
├─ core thread: state store + single event queue (no polling; one coalescing timer max)
├─ hook thread: message loop; SetWinEventHook (narrow ranges, SKIPOWNPROCESS); raw input sink
├─ media/audio: GSMTC events, IMMNotificationClient, IAudioEndpointVolumeCallback (MTA, non-blocking)
├─ compose/UI thread: DispatcherQueue + Compositor; per-monitor HWND + DesktopWindowTarget
│    visuals = shapes/sprites; content = D2D surfaces redrawn ONLY on state change
│    animations = composition spring/keyframe (no app per-frame work)
├─ decode worker (on demand): WIC icon/art decode → shared atlas, LRU by hash, size-capped
└─ shell adapter (feature-flagged, version-checked): appbar, taskbar autohide/restore,
     pins import, shell-hook flash  — isolated so the Store build compiles it out
roqa-settings.exe (on demand, exits on close): WinUI 3 or WebView2; talks via one named pipe
roqa-restore (tiny): restores taskbar state if marker present (startup task / uninstall hook)
```

Why not pure B: if the kill-test shows WinUI 3 NativeAOT idles within ~1.5× of A's floor, **B wins on total cost** because text, a11y and settings come free. I'd accept that outcome. Why not C: the floor is structural (multi-process Chromium), and Nectar already sits at that floor after heroic tuning, so we cannot "significantly beat Nectar" on C.

---

## 7. Challenges to likely mistakes (adversarial review)

1. **Premature custom rendering.** Don't build a retained-mode GPU renderer, wgpu scene graph or custom blur shaders. Windows.UI.Composition already is a retained GPU scene graph with off-thread animation, and DWM does the final composite anyway. Custom swapchains add a present loop, VRAM and hidden GPU work. Use D2D only to fill content surfaces when content changes.
2. **Accessibility debt.** Custom-drawn UI is invisible to Narrator by default. A dock that replaces the taskbar and can't be used with a screen reader is a regression versus Windows. Make UIA for dock items and notch actions a **Stage-1 exit criterion**, not polish.
3. **Too many processes or modules.** No per-feature helper processes, no per-monitor processes, no plugin hosts in v1. One always-on process + an on-demand settings process. Nectar went 11→8 processes by deleting things.
4. **Timer and IPC storms.** No sleep-loops, no `recv_timeout` heartbeat, no per-feature intervals. At most one coalescable timer for the clock, aligned to the minute boundary (or second, only while a seconds display is visible), with tolerance so Windows can batch it. Coalesce all event bursts to ≤1 recompute per vsync. Count wakeups in CI (§8).
5. **Backend polling.** Every Nectar poll in §2 has an event equivalent, except brightness on some hardware, which should poll only while its UI is visible. Weather, calendar and lyrics fetch on schedule with backoff, and only when the user enabled them.
6. **Hidden GPU work.** Blur/acrylic, always-running visualisers, animated mascots, and "breathing" Live Activity pulses all cost GPU and DWM even when our CPU is 0. Default to solid/opaque surfaces with optional blur. Stop all looping animations when the notch is collapsed or the session is locked or idle.
7. **Thumbnail and album-art decode.** Decode to display size × DPI, never full-res. Decode once per (icon, size) across all monitors. Use DWM thumbnails, not capture, for previews. Never base64 pixels. Cap the cache (e.g., 8–16 MB) and evict.
8. **Multi-monitor duplication.** The state store, icon atlas, media session and art are singletons. Each monitor only owns HWND + visual tree + layout. Gate: each extra monitor costs ≤ a few MB and **zero** extra threads.
9. **Fighting Explorer.** Never re-hide the taskbar in a WinEvent callback loop. Pick a stable end state (autohide + our appbar), react to `TaskbarCreated`, and give up gracefully with a toast rather than "slapping it down" (Nectar `services.rs:551-552`).
10. **Trim-gaming metrics.** Ban `EmptyWorkingSet`/`SetProcessWorkingSetSize` tricks. Report commit and private bytes, not Task Manager's column.

---

## 8. Performance goals and gates (proposed, measurable, fair)

**Protocol (applies to us, Nectar and Nusa side-by-side, same machine):** release builds; 2 reference machines (mid-range laptop with iGPU on battery, plus a desktop with 2 monitors at mixed DPI/refresh, e.g., 100%/150% and 60/144 Hz); default settings; blur off and on, reported separately; 2-minute settle then 10-minute idle window, hands off; WPR/WPA trace (CPU Usage Precise, GPU, DWM) plus process counters. Report **commit (private bytes) summed over the process tree, untrimmed WS, process and thread count, CPU-seconds, context-switch wakeups/s, our GPU engine time, and the dwm.exe GPU delta vs a no-app baseline.** Scenarios: S1 idle nothing playing; S2 music playing, notch collapsed; S3 notch expanded with music; S4 dock hover magnification sweep; S5 3 monitors; S6 24 h soak with sleep/resume, Explorer restart and display hot-plug.

| Gate | Target (dock + notch, 1 monitor unless noted) | Notes / confidence |
|---|---|---|
| G1 idle commit (S1) | **≤ 40 MB** total; untrimmed WS ≤ 60 MB | Medium: D3D/DirectWrite baseline may push this; the kill-test sets the real number |
| G2 processes/threads (S1) | 1 process; ≤ 20 threads | High achievable |
| G3 idle CPU (S1) | ≤ 0.5 CPU-s per 10 min; **≤ 2 wakeups/s** averaged | Nectar's 200 ms worker alone is 5/s |
| G4 music collapsed (S2) | No frames presented; ≤ 2 wakeups/s | Progress bar animated by the compositor |
| G5 animation (S3/S4) | 0 dropped frames at native refresh on the iGPU reference; app CPU ≤ ~1 ms per frame while animating | Measured via present/DWM frame stats |
| G6 latency | Edge-dwell → first frame ≤ 1 frame after the dwell threshold; media change → UI ≤ 100 ms; dock click → activation call ≤ 50 ms | |
| G7 per extra monitor (S5) | ≤ +8 MB commit, +0 threads | Medium |
| G8 GPU/DWM | dwm.exe GPU delta at idle within noise with blur off; blur cost documented, not hidden | |
| G9 robustness (S6) | Taskbar restored ≤ 2 s after kill/crash or on next logon after power loss; recover from Explorer restart ≤ 3 s; DPI/topology change without restart; no hook-loss incidents | Non-negotiable for replace mode |
| G10 a11y | Narrator can enumerate, name, invoke and context-menu every dock item and notch control | Stage-1 exit |
| G11 cold start | Visible ≤ 500 ms warm, ≤ 1.5 s cold logon | Medium |

**Comparative claims we can make if the gates pass:** "uses ≥5× less committed memory and ~1/8 the processes of Nectar on the same machine" (credible given 217 MB / 8 procs). Against Nusa, only after measuring with the protocol: "comparable footprint while adding a dock". **Never compare our commit to Nusa's Task Manager screenshot.**

---

## 9. Prototype kill-test (do this before any MVP commitment)

**Duration:** 3–5 weeks for one engineer fluent in Win32/COM (+1–2 weeks if learning windows-rs composition). Optionally run a 1-week WinUI 3 NativeAOT twin of the notch surface in parallel for a fair bake-off.

**Build exactly this (the hardest 20%):**
1. Notch window (top-centre) + dock window (bottom) on **2 monitors with mixed DPI**, using composition visuals, a D2D/DirectWrite text label, and a spring expand/collapse composition animation.
2. Dock lists running windows (EnumWindows + narrow WinEvents create/destroy/show/hide/foreground/cloak), groups by AUMID, and shows icons from `IShellItemImageFactory`/WIC through the shared atlas. Hover magnification is driven by composition expressions.
3. GSMTC event-driven now-playing with art, the composition-animated progress bar, and play/pause.
4. Edge reveal via raw-input sink (and, for comparison, the LL hook), with fullscreen suppression.
5. Taskbar autohide + appbar reservation + crash-safe restore (kill -9 test, Explorer restart test).
6. UIA providers for dock items (Narrator test).
7. *(Packaging probe)* Sparse-identity build calling `UserNotificationListener.RequestAccessAsync`, to learn whether notifications work outside full MSIX.

**Pass:** G1–G3, G5, G7, G9 and G10 met on both reference machines; packaging probe answered either way.
**Kill or pivot if any:** idle commit > 60 MB with no clear fix; text/DPI/a11y work for these two surfaces > 40% of the spike; composition interop in Rust blocks progress for > 1 week. Then **pivot surfaces to WinUI 3 (NativeAOT) and keep the same event-driven core.** If WinUI also misses G1 by > 2×, re-scope the product to "notch first, dock later" rather than going back to WebView.

---

## 10. Staged MVP and rough ranges

Assumptions: 1–2 experienced Windows engineers; Rust path; existing designs; no Store review delays counted. Ranges are wide on purpose, because Explorer quirks dominate the long tail. These are ranges, not estimates to commit to.

| Stage | Scope | Range (1 eng / 2 eng) |
|---|---|---|
| 0. Kill-test | §9 | 3–5 wk / 2–4 wk |
| 1. Notch MVP (Store-eligible) | Media + art + controls; volume/brightness/Caps/IME overlays (*shown alongside* native OSD unless a supported way to suppress it exists); timers/Pomodoro; file shelf (OLE drag/drop, stores paths); battery/charger/BT/Wi-Fi peeks (power/radio/network events); clock/calendar glance (local); settings process; UIA; signing; MSIX + direct download | 8–14 wk / 6–10 wk |
| 2. Companion dock (Store-eligible) | Own pin list (one-time import from `.lnk` folder; blob parse only in replace build); running windows; grouping; attention (best-effort); current-virtual-desktop filter; jump-list-lite via shell context menu; DWM hover previews; multi-monitor modes; Fixed/Smart/Peek via appbar semantics | 10–16 wk / 7–12 wk |
| 3. Replace-taskbar mode (direct-download, opt-in) | Autohide + appbar, restore watchdog, `TaskbarCreated`, Start/Win key, tray overflow button, fullscreen policy; 24 h soak gate | 5–10 wk / 4–8 wk, plus ongoing per-Windows-release maintenance |
| 4. Differentiators | §11 | per feature |

To roughly Nectar parity plus a clearly better footprint: **≈ 6–11 months for one engineer, ≈ 4–7 months for two.** Nectar's ~34k lines in a few months (solo, likely AI-assisted) show parity in features is fast. **Parity in robustness is where native rewrites usually slip.**

---

## 11. Features: what's worth adding, what to defer

**High value, supported APIs:**
1. **Notification pill + Today history** (needs identity; §5.5). Nusa paywalls it behind the Store, which makes it the single biggest differentiator for a direct-download build if sparse identity works.
2. **A "Live Activities" local API:** a named pipe (not TCP) that is authenticated, rate-limited and schema-validated, letting scripts and apps post progress (builds, downloads, timers, CI, coding agents). This generalises Nectar's Claude Code agent panel (`agent_api.rs`). Keep the permission-prompt answering behind explicit opt-in: it lets a UI click approve an agent's tool use, so treat it as security-sensitive.
3. **Keyboard-first dock:** a hotkey that opens the dock with type-to-filter launch and switch (`RegisterHotKey`, no keyboard LL hook).
4. **Per-monitor dock showing that monitor's windows**, and a **current-virtual-desktop filter** (supported subset).
5. **Synced lyrics** (opt-in; discloses the network call). Nusa's headline feature, cheap to match.
6. **Focus/DND glance** and meeting-in-N-minutes from local ICS/CalDAV (Nectar has CalDAV/ICS).
7. **Low-battery and audio-device switcher** in the notch (Nectar #15/#117 show demand).

**Defer (cost, risk or policy):** audio visualiser (loopback, mic-privacy optics, constant wakeups); tray icon mirroring (no API, 10.2.8); virtual desktop switcher (internal COM); replacing the native OSD (undocumented window hiding); live-updating capture previews (use DWM thumbnails); weather (network + location consent); Material You/accent sync that recolours Explorer (Nectar #113); AI chat in the notch (Nectar #37); plugin/scripting system; mascots and continuous ambient animation (GPU); clipboard history (privacy, password-manager exclusion, and Windows already has Win+V).

---

## 12. Sources

Nectar (all at `ab3b34dc3f531e13e185c72cb7bacf9d97a1e08a`): `README.md`, `docs/memory.md`, `src-tauri/Cargo.toml`, `package.json`, `src-tauri/src/{services,utils,commands,taskbar_pins,tray_icons,agent_api,attention,capture,live_preview,memory_saver,on_demand_windows,webview_args,shelf}.rs`. Issues #13, #15, #37, #38, #56, #66, #69, #71, #83, #91, #92, #113, #115, #116, #117: https://github.com/kua8/Nectar/issues

Nusa: https://rama2555.github.io/nusa/ ; https://github.com/rama2555/nusa (tree `8b7f792…`) ; https://github.com/rama2555/nusa/releases/tag/v1.2.0 ; Store listing https://apps.microsoft.com/detail/9NQMSXF0N0WM (not opened).

Microsoft Learn (fetched 2026-11-09):
- Notification listener: https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener ; API https://learn.microsoft.com/en-us/uwp/api/windows.ui.notifications.management.usernotificationlistener
- Package identity for unpackaged apps: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps
- Store policies (10.2.7, 10.2.8): https://learn.microsoft.com/en-us/windows/apps/publish/store-policies
- SHAppBarMessage: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage ; ABM_SETSTATE: https://learn.microsoft.com/en-us/windows/win32/shell/abm-setstate ; Appbars: https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars
- SetWinEventHook: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook ; WinEventProc: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wineventproc
- LowLevelMouseProc: https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc
- RegisterShellHookWindow: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registershellhookwindow
- GSMTC manager/session: https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionmanager ; https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssession
- IMMNotificationClient: https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient
- IVirtualDesktopManager: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager
- Shell_NotifyIcon: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw
- GraphicsCaptureSession.IsBorderRequired: https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired
- DWM thumbnails: https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw
- DirectComposition (recommends Windows.UI.Composition on Win10+): https://learn.microsoft.com/en-us/windows/win32/directcomp/directcomposition-portal ; Visual layer with Win32: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/using-the-visual-layer-with-win32
- UIA and the UI thread: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading
- WebView2 performance: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance

---

## 13. Strongest objections to my recommendation

1. **"Rust + raw composition is a velocity trap."** Text layout, bidi/RTL (Nectar ships Arabic), IME in to-do entry, UIA providers, drag and drop, and DPI: WinUI gives all of these for free, and the always-on footprint difference may be smaller than feared once both are event-driven. *Response:* that is exactly why the kill-test includes a WinUI NativeAOT twin and a pivot rule. I'm not certain A beats B on total cost.
2. **"Users don't feel 40 MB vs 200 MB; they feel bugs."** Nectar's issues are about shell robustness, not RAM. A rewrite resets robustness to zero. *Response:* partly conceded. That's why replace-taskbar mode is last and gated on a 24 h soak, and why the notch ships first.
3. **"Single process is a single point of failure."** A panic in media code kills the dock and may strand a hidden taskbar. *Response:* the restore path must not live in the main process; use panic=abort plus a watchdog relaunch. Still a real trade-off versus process isolation.
4. **"Composition animations aren't free."** DWM still renders each frame, and spring animations on large blurred surfaces cost GPU. *Response:* agreed; G8 measures dwm.exe and blur is off by default.
5. **"Two builds (Store vs direct) double QA."** *Response:* true. The alternative is never replacing the taskbar, which may be the better product decision. See Q1 for Sol.
6. **"Sparse-identity notifications may not work, killing the top differentiator outside the Store."** *Response:* that's why it's a kill-test probe. The fallback is Store/MSIX for the notification feature.

---

## 14. Actionable mistakes to avoid (checklist)

- [ ] Don't port or read-to-reimplement Nectar code unless you choose GPL-3.0 first.
- [ ] No sleep-loops or polling workers. Every OS signal has an event; brightness polls only while visible.
- [ ] No global `WH_KEYBOARD_LL`. Use `RegisterHotKey`. Avoid `WH_MOUSE_LL`; if used, keep the hook proc O(1) and on its own thread.
- [ ] No system-wide `EVENT_OBJECT_LOCATIONCHANGE` without process/object filtering and coalescing.
- [ ] All hooks on a dedicated message-loop thread, never the compose thread. UIA client calls never on a UI thread.
- [ ] Never `SW_HIDE` the taskbar or fight Explorer in a callback loop in the default mode. Handle `TaskbarCreated`. Restore must survive our crash.
- [ ] Don't measure or market Task Manager "Memory". Report commit, wakeups and DWM GPU. No `EmptyWorkingSet`.
- [ ] Decode art and icons once, at display size, shared across monitors, with a capped cache. No base64 pixels.
- [ ] DWM thumbnails for previews, not capture. Register only while visible.
- [ ] Blur off by default. Stop ambient animations when collapsed, locked or on battery saver.
- [ ] UIA providers before shipping any custom-drawn surface.
- [ ] Code-sign from the first public build. Expect AV heuristics anyway if any hooks remain.
- [ ] Show permission state for notifications (revocation fails silently).
- [ ] Keep version-sensitive shell code (Taskband blob, shell hook, any internal COM) behind a kill switch with build-number checks.

---

## 15. Unknowns (explicit)

- Nusa's real tech stack, footprint and wakeup behaviour (closed source; only vendor claims).
- Whether `UserNotificationListener` works under external-location (sparse) identity.
- Actual idle commit floor of Rust + Windows.UI.Composition + D2D on our reference machines versus WinUI 3 NativeAOT.
- Raw-input sink behaviour over elevated, UIPI-protected foreground windows and some fullscreen games.
- Whether a *supported* way exists to suppress the native volume/brightness OSD on Windows 11 (I found none).
- LRCLIB terms and rate limits; the `WmiMonitorBrightnessEvent` reliability across GPUs and drivers.
- Store certification outcome for a "companion dock" that reserves a screen edge via `ABM_NEW` (documented API, but reviewer interpretation of 10.2.8 is unknown).

---

## 16. Three questions for Sol

1. **Should we ever replace the taskbar?** Given Store policy 10.2.8 and that Nectar's worst bugs are taskbar fights, would you ship a *companion* dock only (appbar + autohide suggestion, no hiding) and accept losing "replaces your taskbar" as a selling point? Or is replacement the core value prop that justifies a direct-download-only product?
2. **Rust/composition vs WinUI 3 NativeAOT for the always-on surfaces:** do you have evidence (measurements, not intuition) about WinUI 3's idle commit and wakeups in a transparent, topmost, mostly-idle window? Would you set the bake-off pivot threshold at 1.5× or somewhere else?
3. **Edge-reveal input strategy:** LL mouse hook (precise, but synchronous and silently removable), raw-input sink + `GetCursorPos` (async, unclear behaviour over UIPI), or 1-px trigger windows (cheap, but steal top-edge clicks: Nectar #115). Which did you land on, and what failure case convinced you?
