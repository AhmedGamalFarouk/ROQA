# Feature pipeline queue

Workers tick an item off (`[x]`) in the batch that lands it. One batch per worker; see AGENTS.md "Feature pipeline".

## Done
- [x] Media island (M4), pins, shelf, countdown timers, fullscreen suppression, multi-monitor, themes.

## Queue
- [x] B1: power/battery pill and system accent colour ([packet](features-b1.md)).
- [x] B2: DWM hover thumbnails on dock slots, plus a virtual-desktop filter ([packet](features-b2.md)).
      (`IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop`).
- [x] B2.5: volume (right edge) and brightness (left edge) hover sliders per monitor; the volume icon switches output device ([packet](features-edge-sliders.md)).
- [x] B3: a non-modal dock context menu (close window, new instance, pin/unpin) ([packet](features-b3.md)).
- [x] T1: notch tabs (hanging handle: Media, Weather, Prayer, Timer, Shelf), a Settings switch per tab and the squash-capsule mascot eyes ([packet](features-tabs-eyes.md)).
- [x] W1: Weather tab content: city (geocoded) plus WinRT Geolocator auto-detect; refresh on open if older than ~1 h, ~3 h in the background only while a notch is visible; `Tab::Weather` available ([packet](features-weather.md)).
- [x] P1: Prayer tab ([packet](features-prayer.md)): wire `prayer.rs`, location, a reminder that expands the notch with the user's chosen sound (MCI, lazy). Then `Tab::Prayer.available()` goes true.
- [x] D1: dock style Floating or Attached to the bottom edge ([packet](features-dock-styles.md)).
- [x] F3: prayer alert over full screen, Weather independent of Prayer ([packet](features-f3.md)).
- [ ] M1: mascot states that wait on content: adhan (half-lidded, stepped glow) and rain (droopy), plus the per-tab mood pairing.
- [ ] T2: tab polish after a native run: SelectionItem UIA pattern, handle hover timing, RTL order, text-scale fit.
- [ ] B4: picker as command palette (installed apps from Start menu shortcuts, inline math).
- [ ] B5: privacy mic/camera indicator.
- [ ] B6: audio output device list (switching opens Sound settings).

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
