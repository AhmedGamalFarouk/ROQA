# F2: fixes after F1 (2026-11-12)

Branch `feat/f2-prayer`. No native/GUI run; every item is NOT RUN visually.

1. **Eased volume bar.** The notch bar's fill is a `Tween` on the surface (`volume_fill`, 120 ms `EASE_OUT`), retargeted on each paint; a drag resets it to the pointer at once. Settled, the surface stops being live: no timer. The edge pill is a separate drawing and is unchanged.
2. **Wake on unlock and display-on.** Notch windows register `WTSRegisterSessionNotification` and `RegisterPowerSettingNotification(GUID_CONSOLE_DISPLAY_STATE)` (released in `Drop`). Unlock, or a display state of 1 (on), posts one coalesced `WAKE` to the host, which plays the eyes' wake show. No polling. New feature `Win32_System_RemoteDesktop`.
3. **Cog in UIA and keyboard.** `notch_uia::GEAR` is a Button named "Settings" (id after the tabs), last in the Tab/arrow order, Enter/Space/Invoke opens Settings, a focus ring is drawn, bounds and hit-test follow `layout::gear`.

Tests: unlock/display-on filter, cog element and name. The old "clock fallback has no children" test now expects only the cog.
