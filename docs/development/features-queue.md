# Feature pipeline queue

Workers tick an item off (`[x]`) in the batch that lands it. One batch per worker; see AGENTS.md "Feature pipeline".

## Done
- [x] Media island (M4), pins, shelf, countdown timers, fullscreen suppression, multi-monitor, themes.

## Queue
- [x] I1: direction B line icons ([packet](features-icons-glass.md)); native visual checks NOT RUN.
- [x] G1a: Frosted tint rename with `glass` alias ([packet](features-icons-glass.md)).
- [x] G1b: baked Liquid glass theme (cached gradient brushes, no live blur; DWM backdrop FAILED natively, see packet); wallpaper snapshot/lens skipped ([packet](features-icons-glass.md)).
- [x] B1: power/battery pill and system accent colour ([packet](features-b1.md)).
- [x] B2: DWM hover thumbnails on dock slots, plus a virtual-desktop filter ([packet](features-b2.md)).
      (`IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop`).
- [x] B2.5: volume (right edge) and brightness (left edge) hover sliders per monitor; the volume icon switches output device ([packet](features-edge-sliders.md)).
- [x] B3: a non-modal dock context menu (close window, new instance, pin/unpin) ([packet](features-b3.md)).
- [x] T1: notch tabs (hanging handle: Media, Weather, Prayer, Timer, Shelf), a Settings switch per tab and the squash-capsule mascot eyes ([packet](features-tabs-eyes.md)).
- [x] W1: Weather tab content: city (geocoded) plus WinRT Geolocator auto-detect; refresh on open if older than ~1 h, ~3 h in the background only while a notch is visible; `Tab::Weather` available ([packet](features-weather.md)).
- [x] P1: Prayer tab ([packet](features-prayer.md)): wire `prayer.rs`, location, a reminder that expands the notch with the user's chosen sound (MCI, lazy). Then `Tab::Prayer.available()` goes true.
- [x] D1: dock style Floating or Attached to the bottom edge ([packet](features-dock-styles.md)).
- [x] A1: dock slide-up on show/startup/un-hide, both styles; slide-down on hide not done ([packet](features-dock-styles.md)).
- [x] D2: taskbar buttons on the dock: Start, Search, Task View, minimized-apps list, show-desktop strip ([packet](features-dock-system.md)).
- [x] F3: prayer alert over full screen, Weather independent of Prayer ([packet](features-f3.md)).
- [x] U1: Limits tab: Claude, Codex and Antigravity plan-limit percentages ([packet](features-limits.md)).
- [x] M1: mascot states that wait on content: adhan (half-lidded, stepped glow) and rain (droopy), plus the per-tab mood pairing ([packet](features-privacy-moods.md)).
- [ ] T2: tab polish after a native run: SelectionItem UIA pattern, handle hover timing, RTL order, text-scale fit.
- [x] B4: skipped by the user 2026-11-12 (picker as command palette); no code.
- [x] B5: privacy mic/camera indicator ([packet](features-privacy-moods.md)).
- [x] B6: dropped: covered by the edge slider device switch; no code.
- [x] S1: sidebar Settings shell (8 pages, card pane, switches, dark/high contrast/RTL) and seven new settings ([packet](features-settings-shell.md)). Not built: stepper/segmented/tile controls, monitor map, About pinned to the bottom.
- [ ] F5 ([packet](features-f5.md)): P1 monitor map [x]; P2 dock slide-down on hide [x]; P3 water reminder; P4 pomodoro; P5 settings controls (segmented/stepper/tiles).

## Research pending
- AI usage meter.
- Mica/Acrylic on DComp.
- Lock-key detection without hooks.

## Excluded or needs a user decision
- Media-key OSD interception: needs global hooks, against AGENTS.md.
- Switching the default device through the undocumented `IPolicyConfig`: now done, on the user's explicit request, in B2.5 (one module, `policy_config.rs`).
- Global hotkeys via `RegisterHotKey`: opt-in, needs approval.
- Per-app badges: no public API.
- Dock magnification and edge docking: later.
