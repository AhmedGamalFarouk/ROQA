# Features B2.5: edge sliders (volume right, brightness left)

Branch `feat/edge-sliders` from `61ce1f6`. No native/GUI run (NOT RUN). Queue: [features-queue.md](features-queue.md).

## Behaviour (`edge.rs`, `edge_uia.rs`)
- Per monitor and edge, one non-activating topmost window: an invisible 2 DIP strip over the middle third of the work area (corners stay free). After 250 ms of hover it grows into a rounded pill (theme and accent, DPI-scaled: level fill, icon at the bottom, percent while dragging); 800 ms after leave it shrinks back and drops its canvas. Drag or wheel (5% a notch) sets the level. Hidden under fullscreen suppression; two settings checkboxes (`edgevol`, `edgebri`, default on).
- Event-driven: mouse messages feed `Hover`; its deadlines are the only waits; a pill repaints only when its view changes. `Throttle` coalesces brightness writes to about 5 a second, latest value wins.
- UIA: Slider with RangeValue 0-100, named "Volume 40%, Speakers" or "Brightness 70%". The element exists while the pill is hidden, so a screen reader can adjust it. Core Audio starts on first touch of the volume strip, or at once while a UIA client listens.

## Volume and devices
- Reuses `audio.rs` (set, unmute, echo suppression). The icon opens an in-window list of active render endpoints (`IMMDeviceEnumerator`, current one dotted, at most 8); a click calls `policy_config::set_default` for all three roles.
- USER DECISION (explicit request): `IPolicyConfig::SetDefaultEndpoint` is undocumented and may change between Windows builds. It lives alone in `policy_config.rs`; on any failure the slider opens `ms-settings:sound`.

## Brightness (`brightness.rs`)
- DDC/CI via dxva2 on one lazily started worker thread (queue of 8, reads and the latest write per monitor only). Physical handles are opened and destroyed per job; none is held. The level is read when the pill shows. No DDC/CI: disabled pill plus "Brightness not supported".
- Deferred: laptop panels (WMI `WmiMonitorBrightnessMethods` needs a WBEM COM client, not cheap); they show "not supported". Arrow keys: the windows never take focus, so keyboard control is only through UIA SetValue.

## Checks
- `verify.ps1`: fmt, clippy `-D warnings`, 210 tests (+13: zone geometry, drag mapping, wheel, hover timing, throttle, write coalescing, DDC scaling, device list, row pick, settings). Ratchet tripped: 902,144 B (+80,896, +9.85% over 821,248); baseline raised to 902144. New code: 4 modules (~1.4k lines with tests), a worker thread and channels, a 4-interface UIA provider, `IPolicyConfig`, two D2D paint paths, dxva2. SHA in `artifacts/edge-sliders/inputs.sha256` run and `progress.md`.

## NOT RUN
- Any launch: strip hit-testing without composition content, hover timing, drag/wheel feel, DPI and multi-monitor placement, DDC reads and writes on a real monitor, device list and switch, `IPolicyConfig` on this build, fullscreen hiding, UIA read and SetValue, Glass/high-contrast looks. Reviews paused (REVIEW NOT RUN).
