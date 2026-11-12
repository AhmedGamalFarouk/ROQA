# D1: dock style, Floating or Attached (2026-11-12)

> K1 (2026-11-13): keyboard and UIA/screen-reader support was removed by user decision. Keyboard, Tab, focus-ring, access-key and UIA lines below are history, not current behaviour.

Branch `feat/f3-dock`. No native/GUI run; nothing drawn or hit-tested on a real screen (NOT RUN).

- **Setting:** Settings "Dock style" list, stored `dockstyle=floating|attached`; default Floating (the current look, unchanged). Applies live: each dock moves, resizes and repaints; no new timer or surface.
- **Attached:** `layout::placement_styled` puts the dock flush on the work area's bottom edge (no 12 DIP margin), so it sits above a bottom taskbar and on the screen edge otherwise; per monitor, negative origins and tiny work areas covered by tests. Same width, height, slot grid and DPI maths, so slot hit-testing, drag-reorder, UIA bounds, dock menu and hover previews (all keyed to the window rect) need no change.
- **Look:** `graphics::attached_outline` is the notch outline turned over: flat bottom, top corners at `dock::RADIUS_DIPS` (`layout::attached_radius`, clamped to the body), filled with the theme's notch `body` and the same rim/hairline; Floating keeps `dock_fill` and its hover tint. Themes follow the palette. Not reused: `notch_outline` itself (its ears and bottom corners are the wrong way round; the attached path is 10 lines).
- **Hide/show:** unchanged (full-screen hiding, `--hide-dock`, Settings toggle).
- **Skipped:** concave ears at the bottom; a region clipping the transparent top corners from hit-testing (the floating bar has the same corners).
- **Tests:** placement of both styles (flush bottom, floating unchanged, notch ignores style), tiny work areas, radius clamp, key round trip, settings store round trip.
- **Risk:** the Settings window gains one row (taller); fit on small screens NOT RUN.

## A1 slide-up (2026-11-12)

- **What:** when a dock becomes visible (startup, `--show-dock`, un-hide after a full-screen app, display change: all go through `Surface::set_visible(true)`), its content rises from below its window over `dock::SLIDE_TIME` (180 ms, `EASE_OUT`) in both styles. No window moves, so it never draws over the taskbar or another monitor; each monitor's dock has its own `Surface::slide` tween.
- **Idle:** the tween rides the existing `live`/`animate` frame loop; no timer, nothing runs once it lands. Reduced motion: none by the standing user decision (ROQA always animates).
- **Geometry:** `dock::slide_offset(progress, height)` pushes the painted content down (`Canvas::paint_dock` translates the context; identity is restored in `draw_within`). Hit-testing follows it: `paint_dock` sets the window region to the rows from the offset down (`set_notch_region`, one rect), and UIA cell bounds subtract the same offset (`dock_uia::State::slide`).
- **Tests:** offset endpoints and monotonic steps, clamping, zero height; timeline starts hidden, ends exactly settled and not running, retargets from where it is.
- **Not done:** slide-down on hide (full-screen hiding, `--hide-dock`, Settings toggle still hide at once: the first is under another app anyway, the others remove the surface), hover reveal (the dock has no auto-hide).
- **Native (Chief-confirmed, private store, own process):** the dock appears with a 1 px sliver at the bottom (window region `0,63,224,1`), then settles at the full region `0,0,616,64` with the normal bar (both styles); screen capture is too slow (about 250 ms a frame) to catch mid-slide frames. NOT RUN: the motion itself, clicks during the slide, multi-monitor, a bottom taskbar.
- **Size:** 1,242,112 B, ratchet limit 1,242,408 (baseline unchanged); the G1b hover wash on the glass dock bar was dropped to fit.
