# Features B3: non-modal dock context menu

Branch `feat/features-b3` from `4809e91`. No native/GUI run (NOT RUN). Queue: [features-queue.md](features-queue.md).

## Behaviour (`dock_menu.rs`, `menu_uia.rs`)
- Right-click on an app slot opens a small owned popup above it, drawn with the edge sliders' list panel (theme, accent, DPI). No `TrackPopupMenu`: the host loop keeps running. The picker slot has none. Right-click used to toggle the pin (S4); that is now the menu's Pin / Unpin.
- Actions per slot (`items`, pure): New instance (the app has a launch target: `launch_target`, so an exe path or `shell:AppsFolder\<AUMID>`, via `dock::launch`), Close window (only with windows; `WM_CLOSE` posted to each revalidated window of the group, apps may refuse), Pin or Unpin. A pinned-only slot has no Close.
- Dismiss: Escape; a button press on any other ROQA window; losing the foreground (`DEACTIVATED`); the slot changing (`describes`: another app in that slot, or its actions changed); the dock hidden by full screen; a palette, device or topology rebuild. Choosing an action also ends it.
- Keyboard: Down/Tab and Up move (wrapping), Enter/Space choose once, Escape dismisses. The menu takes the foreground (`take_foreground`) and gives it back to the previous window on every end except New instance. If Windows refuses the foreground, no menu opens (logged), since a click elsewhere could not dismiss it.
- UIA: Menu root named "<app> menu" with MenuItem children (Invoke posts to the host loop, same as a click).
- Cost: `Option<Menu>` is `None` when closed; the pass adds one `is_some` check. The window, canvas and UIA root exist only while open. Repaint only when the highlighted row changes. A hover thumbnail is suppressed while it is open.

## Checks
- `verify.ps1`: fmt, clippy `-D warnings`, 214 tests (+4: actions per slot, placement and clamping, dismiss rule, key handling). Ratchet tripped: 921,600 B (+19,456, +2.16% over 902,144); baseline raised to 921600. Cost: two modules with a UIA provider (Root and Item COM classes) and the menu popup; no new dependency. SHA `87a0f72197569ef17299c24cda54c56ff0b070cf90fd5e7524662e8a8b3b68f7`.
- gstack `/verify` detected only global-toolchain `cargo check`/`clippy`, untrusted; not run, `verify.ps1` (pinned toolchain) is the gate.

## NOT RUN
- Any launch: menu drawing and placement, hover and click on rows, `SetForegroundWindow` success and foreground restore, Escape/arrows/Enter, click-outside and focus-loss dismissal, `WM_CLOSE` on real windows, New instance for exe and packaged apps, Pin/Unpin, UIA read and Invoke, fullscreen dismissal, DPI and multi-monitor. Reviews paused (REVIEW NOT RUN).
- Deferred: Shift+Tab, mouse-leave clearing the highlight, swallowing the click that dismisses the menu over the dock (it still reaches the slot), UIA selection/focus events.
