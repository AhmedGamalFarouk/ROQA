# Feature pipeline queue

Workers tick an item off (`[x]`) in the batch that lands it. One batch per worker; see AGENTS.md "Feature pipeline".

## Done
- [x] Media island (M4), pins, shelf, countdown timers, fullscreen suppression, multi-monitor, themes.

## Queue
- [x] B1: power/battery pill and system accent colour ([packet](features-b1.md)).
- [ ] B2: DWM hover thumbnails on dock slots, plus a virtual-desktop filter
      (`IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop`).
- [ ] B3: a non-modal dock context menu (close window, new instance, pin/unpin).
- [ ] B4: picker as command palette (installed apps from Start menu shortcuts, inline math).
- [ ] B5: privacy mic/camera indicator.
- [ ] B6: audio output device list (switching opens Sound settings).

## Research pending
- AI usage meter.
- Mica/Acrylic on DComp.
- Lock-key detection without hooks.

## Excluded or needs a user decision
- Media-key OSD interception: needs global hooks, against AGENTS.md.
- Switching the default device through the undocumented `IPolicyConfig`.
- Global hotkeys via `RegisterHotKey`: opt-in, needs approval.
- Per-app badges: no public API.
- Dock magnification and edge docking: later.
