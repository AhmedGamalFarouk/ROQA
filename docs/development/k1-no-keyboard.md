# K1: keyboard, hotkey and UIA/Narrator removal (2026-11-13)

User decision: "Remove all of it". Mouse behaviour is unchanged.

- **Removed code:** `notch_uia`, `dock_uia`, `edge_uia`, `menu_uia`, the `roqa-uia-probe` bin, `WM_GETOBJECT`, live-region announcements, notch/dock/menu key handling, focus ring drawing (notch, dock, handle, cog, shelf, Refresh, Settings sidebar/toggles/tiles), the Settings "Focus the notch/dock" buttons, access keys, tab stops, `IsDialogMessageW` in Settings and the picker, edge slider device-name plumbing, UIA-only accessors and strings.
- **Kept on purpose:** menus and the minimized popup still take the foreground so a click elsewhere dismisses them, and give it back on close; the picker filter and Settings edits still take typed text; Ctrl+click on a shelf entry reveals it (a mouse modifier); the high-contrast palette (`HIGHCONTRASTW`, so `Win32_UI_Accessibility` stays); the dock's taskbar buttons still send their shell shortcut on a click.
- **Behaviour change in text fields:** with no dialog message pump, Enter in an Edit does not press a default button (a field commits when it loses focus) and Esc no longer closes Settings or the picker; use Close.
- **Scripts:** `picker-smoke.ps1` and `packaged-hung-check.ps1` dropped (both read the dock/picker through UIA and injected keys; the mouse-only parts, dock click minimize/restore, 35 picker cycles, hung-window catalog and `--quit` timing, are now uncovered). `smoke.ps1`, `topology-check.ps1`, `catalog-smoke.ps1` use no keyboard or UIA and are unchanged; `tests/PickerProbe.cs` keeps only `Click`. `land.ps1` now accepts a tracked file deleted in the working tree as a packet path.
- **Cargo:** no windows feature became unused (`Variant`, `StructuredStorage`, `Accessibility` are still needed by Core Audio, the window catalog and the high-contrast palette).
- **Tests:** 370 -> 355 (UIA, focus step, ring, key, name and revision tests removed; chip bounds and limits/countdown tests kept without the spoken text).
- **Size:** release 1,150,976 -> 1,035,264 B (-115,712 B, -10.1%). Baseline file untouched at 1,152,000 B; the user may want to lower it.
- **NOT RUN:** native/GUI (Settings and picker without the dialog pump, shelf/handle drawing, menu dismissal); reviews paused (REVIEW NOT RUN).
