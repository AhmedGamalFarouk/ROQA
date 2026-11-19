# Dock and the Windows 11 taskbar

What the taskbar shows and does, against what ROQA's dock has (`dock.rs`, `dock_menu.rs`, `preview.rs`,
`window_catalog.rs`, `pins.rs`).
Byte figures are release-exe estimates; "built" rows show the measured change.

Status: **has** = already in the dock, **built** = added in this pass, **gap** = could be built, **no** = cannot
or should not be (ROQA is not the shell).

| Taskbar feature | Dock today | Status | Bytes |
|---|---|---|---|
| Start button | Windows 11 logo, opens Start | has (logo built) | +0 net |
| Search, Task View, Show desktop | Buttons, same glyph box, optional | has | 0 |
| Pinned apps, drag to reorder | Pins, drag reorder, folders | has | 0 |
| Running app indicator | Lamp dot; focused app a wider capsule | has | 0 |
| Window grouping and count | One slot per app; up to three dots, one per window | built | ~+300 |
| Taskbar badges (unread count) | Read from the window title "(3)" or "[2]" only | has (title only) | 0 |
| Overlay badges and progress (ITaskbarList3) | Apps send these to the shell, not to us | no | n/a |
| Attention flash | Dot badge while an app flashes, cleared on focus | built | ~+700 |
| Thumbnails on hover | Live DWM thumbnail of the most recent window, 400 ms | has | 0 |
| Thumbnails of every window in a group | Only the most recent window | gap | ~2,500 |
| Click: focus, cycle, minimize | Same behaviour | has | 0 |
| Middle click: new window | New instance | has | 0 |
| Right-click menu | New instance, close, close all, pin, folder items | has | 0 |
| Jump lists (recent files, tasks) | Not available | gap | ~12,000 (reads the shell's jump list files; fragile) |
| Minimized apps list | Not shown (minimized apps keep their dock icons) | no | n/a |
| Right-click on empty bar | ROQA menu (Settings, Quit), not taskbar settings | has | 0 |
| Clock and date | On the notch, not the dock | has (notch) | 0 |
| Volume, brightness, battery | On the notch (volume, brightness, charging notice) | has (notch) | 0 |
| Network, Bluetooth, quick settings flyout | Not shown | gap | ~3,000 (launch `ms-actioncenter:` from a button) |
| Notification area (tray icons) and overflow | The shell owns it; no API to list another app's tray icons | no | n/a |
| Notification centre, toasts | Shell owned | no | n/a |
| Widgets, Copilot buttons | Not wanted | no | n/a |
| Virtual desktops | Task View button; the dock can filter to this desktop | has | 0 |
| Multiple monitors | One dock per monitor | has | 0 |
| Auto-hide | Slide in and out at the edge | has | 0 |
| Alignment, size, combine labels | Floating or attached style, one size | has | 0 |
| Keyboard: Win+number, Alt-tab, tab focus | Out of scope by owner rule | no | n/a |

## Built in this pass
- Window dots: a resting group shows one dot per open window, up to three (`draw_slot`).
- Attention flash: a hidden window registers for `SHELLHOOK`; a flash event puts the existing dot badge on that
  app until it is activated or closed. No timer, no polling. Respects the "badges" setting.
- Measured together: exe 1,176,064 B to 1,177,088 B (+1,024 B).

## Left, in order of use
1. Thumbnails of every window in a group (~2,500 B): the preview popup holds one thumbnail, so it needs a row of
   popups or one wider popup.
2. Quick settings button (~3,000 B): a button that opens the system flyout.
3. Jump lists (~12,000 B): parse the shell's automatic destinations. Large and brittle; not recommended.
