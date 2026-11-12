# Features B1: power pill and system accent

> K1 (2026-11-13): keyboard and UIA/screen-reader support was removed by user decision. Keyboard, Tab, focus-ring, access-key and UIA lines below are history, not current behaviour.

Branch `feat/features-b1` from `20cc689`. No native/GUI run (NOT RUN). Queue: [features-queue.md](features-queue.md).

## Power pill
- `WM_POWERBROADCAST`/`PBT_APMPOWERSTATUSCHANGE` posts `native_window::POWER_STATUS`; the host reads `GetSystemPowerStatus` (new `Win32_System_Power` feature) and `power::Power` decides.
- A change of state (Charging, On battery, Low, Critical) starts a 4 s notice: "Charging 54%", "On battery 54%", "Battery low 10%" (system low and critical flags). Percentage ticks, unknown readings and no-battery machines start nothing; the state at start is a silent baseline.
- One host deadline (`refresh_power`, folded into the existing wait minimum): no thread, timer or poll. Notches open via `open_notches` (shared with the timer alert) so a notch hidden by a full-screen app stays hidden.
- Drawn in the open panel (plain: the status row; media: the timer pocket), replacing timer panel text while it shows. UIA: the notch root is a polite live region, its name carries the notice, and `UIA_LiveRegionChangedEventId` is raised once when it appears.

## Accent
- `TextSettings` also subscribes `ColorValuesChanged` (flag plus the existing scalar wake); `GetColorValue(Accent)` is read at start and on that flag, which rebuilds surfaces like a palette change.
- `Palette.accent` colours the progress fill, volume fill and focus rings (media and shelf). Light and high contrast keep ink; an accent under 3:1 over the body on a white or a black backdrop falls back to the theme ink (white in Classic and Glass, near-white in Midnight), not pure white.
- Glass is judged over its worst backdrop, so a mid-tone accent often falls back there (deliberate, matches the text-contrast rule).

## Checks
- `verify.ps1` PASS: fmt, clippy `-D warnings`, 192 tests (+6), locked release. Size ratchet PASS: 815,616 B (+11,776 vs baseline 803,840, +1.46%, limit 820,916); baseline left unchanged.
- Binary SHA `8AEF5ABA335AFC3E523D0788E61CA6C2898532FD219F6B5F706ED327DE47C1A9` (includes the embedded checkout path). Manifest `artifacts/features-b1/inputs.sha256`.

## NOT RUN
- Any launch: real plug/unplug, low/critical battery, UIA client announcement, accent change at runtime, notice text fit (11 DIP in the 84 DIP media pocket is estimated, not measured). Reviews paused (REVIEW NOT RUN).
- Deferred: "Charging" while full or held back (shortcut in `power.rs`); a timer alert ending closes the notch early if it overlaps a notice.
