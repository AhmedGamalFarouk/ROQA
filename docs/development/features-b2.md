# Features B2: dock hover thumbnails and virtual-desktop filter

Branch `feat/features-b2` from `1ec54dd`. No native/GUI run (NOT RUN). Queue: [features-queue.md](features-queue.md).

## Hover thumbnail (`preview.rs`)
- The host reads the hovered dock slot each pass; `Intent` (pure) arms a 400 ms deadline on a slot with windows (not the picker, not a pin that is not running). One host deadline, no thread or timer; nothing hovered costs a short surface scan.
- When due, `Popup::show` picks the window (`most_recent`: the group's foreground window, else its topmost in z-order walked lazily from `GetTopWindow`, else the first), registers `DwmRegisterThumbnail`, sizes the popup from `DwmQueryThumbnailSourceSize` (fits 240x150 DIP, never upscaled, centred over the slot, clamped to the work area, 8 DIP above the dock) and shows it `SW_SHOWNOACTIVATE`. The DWM renders; ROQA captures and copies nothing.
- Ends by dropping the `Popup` (unregisters, destroys the window) on leave, slot change, click or UIA invoke (stays quiet until the pointer changes slot), a hidden dock (full screen resets hover) and the target leaving the group. One preview at a time; none exists when idle.
- UIA: a top-level topmost non-activating tool-window popup titled "Preview, <app>" (its window text is its name). It is not owned by the dock HWND, so a destroyed dock cannot double-destroy it.

## Virtual-desktop filter
- Setting "Show only windows on this desktop" (`desktop=` in the store, default off, settings window checkbox).
- The catalog used to omit every cloaked window, which also hid other desktops' windows. Now a window cloaked exactly `DWM_CLOAKED_SHELL` is kept, flagged `current_desktop = false`, when `IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop` says it is elsewhere (so suspended apps stay omitted). The manager is created once on the worker, on the first shell-cloaked window; a failed call drops it for re-creation (Explorer restart). Cloak/uncloak events (already hooked) re-inspect on a desktop switch; no polling.
- `dock::items_with_pins(.., desktop_only)` drops those windows; pins stay (as launchers). Toggling rebuilds the dock next pass.
- Behaviour change: with the filter off (default) the dock and the picker now list windows from other desktops; they were hidden before. Clicking one activates it (Windows switches desktop). The picker is not filtered.

## Checks
- `verify.ps1`: fmt, clippy `-D warnings`, 197 tests (+5: hover delay x2, which window, popup bounds, filter merge) pass. Size ratchet tripped: 821,248 B (+5,632 over B1's 815,616, +2.16% over 803,840); baseline raised to 821248 (B1's +11,776 was never baselined; B2 adds the popup, COM and setting). SHA `ED47ABD72B9E2AC79C8C5A47277F3CFA3EAF95424D57B6014BD37B144314C20A`. Manifest `artifacts/features-b2/inputs.sha256`.

## NOT RUN
- Any launch: real hover, DWM thumbnail drawing/sizing, UIA name read, multi-monitor/DPI placement, desktop switch and other-desktop activation, minimized-window preview content. Reviews paused (REVIEW NOT RUN).
- Deferred: true MRU (z-order stands in); the picker ignores the filter.
