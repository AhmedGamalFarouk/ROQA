# Volume and brightness on the dock

The two screen-edge sliders move into the dock as two buttons. Brightness gets one slider per monitor. Owner choices (2026-11-24): two dock buttons; one brightness slider per monitor.

## What is wrong today

- Both sliders live on hidden strips at the left and right screen edges. Nothing on the dock says they exist; they are off by default and found by accident.
- The volume pill is a tall vertical capsule, a different shape from every dock popup. The mixer rows beside it are the only part that matches.
- Brightness has one slider per monitor, but each shows only on its own monitor's left edge. From the dock you cannot reach the other monitor.

## Interaction

- **Buttons:** a Volume button (the speaker glyph for the current level, muted shows the muted glyph) and a Brightness button (sun), after Quick in the dock. Shown by the existing "Volume slider" and "Brightness slider" switches in Settings, relabelled "Volume on the dock" and "Brightness on the dock" (same stored keys `edgevol`, `edgebri`, so a stored choice is kept; default off). Brightness shows only when some monitor answers DDC/CI, else the button is hidden.
- **Popup:** hover the button (`preview::DELAY`) or click it. It opens above the button through the shared owned placement, like the Limits and Moon popups, and closes 180 ms after the pointer leaves both the button and the popup. A click on the button toggles it. It holds the dock open while it shows (the dock never tucks under it).
- **Rows, not a pill:** every row is the mixer row that exists today: icon, name, a capsule slider, the level at its right. No vertical pill, no hot edge strip.
  - Volume: first row is the output device (speaker glyph, device name, master slider; a click on the icon or name opens the existing device list in the same popup, a click on a device makes it default and returns). Then one row per app playing sound, as the mixer shows today, or one quiet row "Nothing is playing".
  - Brightness: one row per monitor, left to right by screen position: sun, "Screen 1", "Screen 2" (the dock's own monitor reads "This screen"), slider, level. A monitor that does not answer shows "Not supported" in place of the slider, dimmed, and is not draggable. One monitor gives one row.
- **Input:** press or drag on a slider sets the level; the mixer icon mutes (volume rows only); the wheel over the popup or over the button moves the hovered row by 3, Shift by 1 (the existing steps). The wheel over the bare button, with the popup closed, changes the master volume (or, for brightness, the dock's own monitor) and opens the popup so the change is seen.
- **Brightness timing:** read each monitor once when the popup opens (the existing worker, `READ_LIMIT` 2 s then "Not supported"), writes throttled to five a second per monitor, the last value wins. Nothing runs while the popup is closed: no timer, no thread, no polling.
- **Removed:** the edge strip windows, the vertical pill, its hover timing and its UIA hooks. Colours follow the five themes like every dock popup. Accent only on the slider fill and the selected device. No tooltips, no sounds, no new motion curve: the popup opens with the existing popup motion.

## States

| State | Result |
| --- | --- |
| Switch off | no button, nothing created |
| Volume, device muted | muted glyph on the button, master slider dimmed at 0 |
| Volume, no output device | button hidden |
| Brightness, no supported monitor | button hidden |
| Brightness, monitor slow to answer | row reads "Reading..." until it answers or 2 s, then "Not supported" |
| Monitor unplugged or topology change while open | popup closes; the next open lists current monitors |
| Full-screen app on the dock's monitor | dock hidden, so no button; unchanged |
| Several docks (one per monitor) | each opens its own popup; both show every monitor |

Copy: "Screen 1", "This screen", "Not supported", "Nothing is playing". Nothing new beyond these.

## Touch points

`src/dock.rs` (Button::Volume, Brightness; glyph, order, shown rule), `src/settings_window.rs` (two relabelled rows), `src/host/menus.rs` and `src/host.rs` (hover popup: `show_hover_popup` branches, wheel, the press and drag on rows; dirty with other threads' work), `src/dock_menu.rs` (slider rows in a popup: press, drag, capture), `src/edge.rs` (shrinks to the worker, throttle, reading and level maths; the strip and pill code is deleted), `src/brightness.rs` (per-monitor reads and writes by target; no change expected), `src/graphics/` (the pill painter `paint_slider` deleted). Net lines go down.

Ceilings: `host.rs` and `graphics.rs` must not grow; the deletions pay for it.

## Order of work (each step passes verify.ps1)

1. `edge.rs`: split the pure parts (levels, throttle, wheel steps) from the windows; keep their tests.
2. `dock.rs` and settings: the two buttons and the relabelled switches, popup shows rows read-only.
3. Slider input in the popup (press, drag, wheel, mute, device list).
4. Delete the strips, pill and `paint_slider`.
5. Second-monitor live check: hover, click, drag, wheel, two monitors, one unsupported monitor, dock tuck, full screen.

Step 3 needs `host.rs` and `host/menus.rs`, which hold other threads' uncommitted work: do not start it until `git diff --quiet` on both.

## Open

- Brightness for a monitor with no DDC/CI (many laptop panels use another route): "Not supported" today, same after. A laptop's built-in panel is the common case; supporting it is a separate piece of work (WMI), not here.
- No mock stills yet.
