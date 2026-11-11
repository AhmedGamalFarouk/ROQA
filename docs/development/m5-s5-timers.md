# M5 S5: countdown timers

Author: Claude Sonnet 5.5 (Tier 2), branch `feat/m5-s5-timers`, base `65212ac`. REVIEW NOT RUN (paused by user 2026-11-11). Gate: `scripts/verify.ps1` PASS, **151 tests** (142 + 9). Release `roqa.exe` 884,736 B, SHA-256 `AE535ABEC902897BCEA5CFDA5FFD1712B1FD7BF5030012BCAD395EC2881D61EA`; base 865,280 B, so **+19,456 B (+2.2 %)**.

## What changed
- `src/timer.rs` (new, pure): `Timers` with a monotonic `deadline` and a wall-clock `expiry` per timer, `MAX` 3, 24 h cap. `restore`, `start`, `resync` (clock change keeps the remaining time, moves the stored expiry; under 2 s of drift is ignored), `take_done` (removes a done timer, so it fires once), `alert_over`, `next_wake(expanded, now)`, `write_to` (marks the store dirty only on a change), `view` (pill, panel and UIA text).
- `src/host.rs`: `refresh_timers` is one more term in `update`'s wait minimum (no new loop term). `INFINITE` with no timers; collapsed, hidden or suppressed it waits only for the completion; only a visible expanded notch gets the 1 s seconds tick. `WM_TIMECHANGE`/resume arrive as `REPOSITION` and call `resync`. Completion opens each visible notch once, plays `MessageBeep(MB_ICONASTERISK)` (`timer_sound`, the S7 hook) and closes it 10 s later unless pointed at. No thread, wait handle, `WM_TIMER` or toast.
- `src/store.rs`: `timer=<unix secs>` lines (additive, VERSION stays 1). `src/graphics.rs`, `src/layout.rs`: the timer text and slots, PROVISIONAL, in `docs/design/roqa-visual-spec.md`. `src/notch_uia.rs`: the root name carries "Timer, 4:32 remaining". `Cargo.toml`: feature `Win32_System_Diagnostics_Debug` (`MessageBeep`).

## Tests (9)
Deadline math and bad stored entries (expired, over 24 h, `u64::MAX`); cap of 3 and range; restart round trip through a real store file; clock changes forward and back and the dirty rule; completion fires once and the alert ends once; the wake gate (none, collapsed, expanded, completion before a tick, alert alone); text and names; layout overlap at 96 to 240 dpi; UIA root name.

## Defaults relied on (m5-preparation section 4)
Q4: up to 3 countdown timers, expand the notch and play the system sound, no toast, a hook for the settings toggle (not wired). Q6: proceed with review paused. Mine: a timer that ran out while ROQA was closed is dropped silently; the collapsed minute text can lag by under a minute.

## NOT RUN
A real countdown; the completion sound and the open and close; `WM_TIMECHANGE` and a sleep across an expiry (this assumes `Instant` counts suspend, as the clock code does: UNVERIFIED); a restart with `timer=` lines; paint and wake counts across a minute while collapsed; completion under a full-screen app; Narrator reading the name; 150 % and mixed DPI. To try one, write `timer=<now + 300>` (Unix seconds) into `%LOCALAPPDATA%\ROQA\store.roqa` before starting.

## Deferred
`--timer <seconds>`: the IPC is payload-less named events, so it needs a new channel (`WM_COPYDATA` to the notch, about 40 lines in `native_window`, `ipc`, `main`); until then nothing starts a timer in the app, and `Timers::start` carries a dead-code allow. Also cancel, pause, the settings toggle (S7), a property-changed UIA event, a collapsed pill for the media notch.
