# M5 S4: pins interaction (pin, unpin, reorder, launch, store wiring)

Author: Claude Sonnet 5.5 (Tier 2), branch `feat/m5-s4-pins-ui`, base `34dea95`. REVIEW NOT RUN (paused by user 2026-11-11). Gate: `scripts/verify.ps1` PASS, **135 tests** (128 + 7 new; existing tests unchanged). Release `roqa.exe` 858,112 B, SHA-256 `2E283876FEFBDD8B12D220247AFF803DD483985051A380B8436E7C8002C0D3FD`; base `34dea95` was 809,984 B (S9 packet), so **+48,128 B (+5.9 %)**, mostly S2/S3 code that was dead-stripped until now (store, pins) plus the host wiring.

## What changed
- `src/host.rs`: the dock is built by `dock::items_with_pins`. Right-click on a dock slot toggles its pin (`dock::pinned_as` finds the pin that claimed the group). Left press on a pinned slot arms a drag with pointer capture; past `DRAG_DIPS` (6) the release drops the pin on `dock::drop_index` (one `move_to`, one dirty mark); a lost release (no button bit on a move) cancels. A click on a not-running pin calls `dock::launch`.
- Store: loaded once in `Desktop::create`; `edit_pins` rewrites the pin section and marks `store::Pending` only on a real change; `store_wait` is one more term of the loop's wait minimum (`INFINITE` when clean); `flush_store` runs each pass and once after the loop ends. No timer, no thread, no wait handle, no new wake when nothing is pending.
- `src/store.rs`: `flush_decision_capped` and `Pending` add a 10 s max-wait (a burst cannot postpone the write forever); a failed write retries after 30 s; `Store::flush` = `save` + `diagnostics::note_store_write()` (saves in unit tests stay uncounted).
- `src/dock.rs`: `launch` (ShellExecuteExW, `SEE_MASK_FLAG_NO_UI`, so a missing target never opens a modal box on the host thread), `drag_started`, `drop_index`, `pinned_as`. `perform(Action::Launch)` is unchanged and still false.
- `src/pins.rs`: `launch_target` rejects non-absolute executable paths. `Cargo.toml`: feature `Win32_System_Registry` (gates `SHELLEXECUTEINFOW`).
- Not-running pin icons: no new code; `dock_icons` already asks the bounded icon worker by AUMID (`shell:AppsFolder\..`) or path, at first paint.

## New tests
`drag_started` threshold at 96 and 144 dpi; `drop_index` against `slot_at`, clamping, clipped bar, 150 %; `pinned_as`; `flush_decision_capped` with a 60 s burst (writes at 10, 21, 32, 43, 54 s); quit writes only when dirty; failed write retry; `edit_pins` marks once and ignores no-ops; absolute-path launch rule.

## Defaults relied on (m5-preparation section 4)
Q6 proceed with review paused; Q8 pin by AUMID else exe path, packaged-pin grouping unverified. Gesture choice (mine, the plan left it open): right-click toggles the pin; no context menu (a modal `TrackPopupMenu` would block the host loop).

## NOT RUN
Real launch (packaged and unpackaged), drag on a real dock, right-click on a real dock, persistence across a restart (`%LOCALAPPDATA%\ROQA\store.roqa` never written by this run), a packaged app, a locked or full disk, 150 % and mixed DPI, RTL slot order.

## Deferred
Pin from the picker; keyboard and Narrator reorder, UIA reorder actions and a "pinned" state on running items (S8); a missing-target indicator (a failed launch is silent); a drag ghost or drop marker (the hover highlight shows the slot); a context menu.
