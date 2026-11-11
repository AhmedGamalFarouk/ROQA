# Refresh storm 1 (2026-11-12 Africa/Cairo)

Branch `perf/refresh-storm`, from `299dcaa`. No native/GUI run.

**Root cause.** `Media::refresh` (`src/media.rs:764`) bumped `MEDIA_REFRESHES` on *every call*, and `Desktop::update` (`src/host.rs:2222`) calls it on every host pass. Passes come from any message, the 67 ms meter timer (`METER_PERIOD`, open notch + playing) and animation frames. So 21,955 was the host pass count, not media work: with no event `refresh` is one atomic swap (it was never a GSMTC call per pass). No loop re-arms it: a 0 ms `pending()` timeout clears when `refresh` swaps the bits.

**Fix.** `refresh` returns before counting when no wake bit is set and no grace expired; `MEDIA_REFRESHES` and the new per-instance `Media::refreshes` count only real work (a wake or an expiry). `wakes`, `uia_revision` and the repaint result are unchanged.

**Test.** `host_passes_without_a_media_event_are_not_refreshes`: 1000 passes give 0 refreshes and 0 revision bumps; a reconnect gives exactly 1 across 1001 passes. It cannot compile on the old code (no per-instance count), and the old static counter would read 1000.

**Before/after.** Old: one count per host pass, about 900/min at the meter rate (15/s) and up to 3,600/min animating (60 fps), plus every message. Expected: 0/min idle, one per media wake or grace expiry (the run's 5 wakes would show about 5). Not re-measured (NOT RUN).

**Harness** (`scripts/measure-idle.ps1`, `m5-harness.md`): holds `SetThreadExecutionState` while sampling (cleared in `finally`), so samples may pass the display-off timeout; idle passes `--diagnostics` and records start, end and delta; partial `*.aborted.json` (Status ABORTED, elapsed) on abort. New counter `paints` (one atomic add per committed canvas draw in `graphics.rs::draw_within`, shadow included) in the report beside the wakes.

**Ratchet.** `verify.ps1` fails if `target/release/roqa.exe` exceeds `scripts/size-baseline.txt` (803840) by more than 2%; the message says to write the new size there in the same commit. Current: 804,864 B, PASS.

**Checks.** verify.ps1 PASS (fmt, clippy -D warnings, 186 tests, locked release); measure-idle parse, Add-Type compile and `-DryRun` PASS.

**NOT RUN.** Any ROQA launch, idle sample, abort/keep-awake behaviour in a real run, refresh counts per minute, reviews (paused).
