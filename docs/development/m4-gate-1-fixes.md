# M4 gate 1 fixes (branch `fix/m4-gate-1`, on `3be07ad`)

Findings: `review-m4-gate-1-.../artifacts/m4-gate-1/review-opus.md`, SHA-256 `ab9eb630…d57f`. Static review only; REVIEW NOT RUN on this fix (paused by the user).

| Finding | State | Change |
|---|---|---|
| I1 | done | A bars-only change paints only the meter slot (`Canvas::paint_meter`: update rect, black refill, bars) and skips `publish_notch`. Stale `text_layout` comment fixed. Per-canvas layout cache not needed. |
| I2 | done | `end_session` orphans the cancelled command (`Media::orphans`, at most `ORPHANS` = 4, pruned by `Status()`); later sessions are served. |
| I3 | done | Permission/Error: one 11 DIP line in the open plain panel, UIA root name "ROQA Notch, <line>". Geometry PROVISIONAL in the spec addendum. |
| M1 | done | `Repaint` (Nothing < Meter < Full) per notch; clock, progress, meter, redraw and volume only request; `paint_requested` paints once per pass. |
| M2 | done | A graphics rebuild calls `media.reconnect()` and `audio.reconnect()`. |
| M3 | done | `Audio::reconnect` resets `started` when no enumerator exists; the next `start` retries. |
| M4 | done | Device add/remove/state events rebind only for the bound endpoint's ID (FNV hash in an atomic), an unknown ID, or while none is bound. |
| M5 | done | `refocus`: an unchanged focused index after a session swap is re-announced. |
| M6 | done | A failed `when` cancels the operation and counts it as an orphan until it finishes; the slot stays free. |
| M7 | done | Comment corrected (the timeline is also read with each playback event); no behaviour change. |
| M8 | done | No notch surface: `cover_pixels` is 0 and the art read is skipped; the next pass with a notch re-reads the properties. |

## Status copy (I3, PROVISIONAL)
- Permission: "Windows blocked media access". Error: "Media controls unavailable". NoPlayer and tracks: none.

## Tests
- 142 pass (135 + 7): orphan bound and seek reset, status mapping, art skip and re-read, rebind filter and audio retry, meter update rect, root name, one paint per pass and `refocus`.
- Changed existing test: `cancel_ignoring_command_keeps_its_budget_across_presses_and_sessions` asserted the I2 defect (a hung command's budget survives sessions). Replaced by `a_hung_command_blocks_its_own_session_only_and_orphans_are_bounded`. No other test changed.

## Deferred, unverified
- Per-canvas text-layout cache (I1 alternative): not done. Full paints from clock, progress ticks and animation frames still call `publish_notch`.
- M6: orphans are bounded by `Status()` polling, not by a completion slot, because the subscription itself failed.

## NOT RUN
- All native/GUI, player, audio-device, Narrator, sleep and footprint runs. Whether a DComp `BeginDraw` update rect preserves the rest of the surface and the offset maths in `draw_within` are unverified until a native run (a wrong rect would show as a clipped or stale meter).
- Hung player, GPU reset on resume, real Bluetooth device churn, Narrator after a player swap, and the status line at 150/200 % text scale.
- Release `roqa.exe` 865,280 B vs 858,112 B at `3be07ad` (+7,168 B); SHA-256 `D7DF15FD5334E7AD1871AB0C1BA980A169833851996C9AEBF27206E5A7B0500F`.
