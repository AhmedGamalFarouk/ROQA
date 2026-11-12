# M5 gate 1 fixes

Branch `fix/m5-gate-1` from `f7adb50`; fixes the findings in [m5-gate-1-review.md](m5-gate-1-review.md). No native/GUI run.

- **I1 done.** Both edge sliders default to `false` (`settings.rs`); the Settings checkboxes read the setting, so they start unchecked. A stored `edgevol`/`edgebri` value is kept; only a store without the key gets the new default. A click or wheel on a bare strip now shows the pill at once (`Hover::request`) and applies the input when the level is ready (`Input`, `apply_input`; the click sets the level at its row, the wheel steps it). It is dropped only if the pill closes or the device has no level.
- **I2 done.** `wants_audio` is true only for a volume slider with an open pill or a first `WM_GETOBJECT` on it (`edge_uia::QUERIED`, posted once by `dock_uia::get_object`). `UiaClientsAreListening` and the stray-move latch are gone. `refresh_edges` starts Core Audio in the pass that opens the pill and draws it again with the level.
- **I3 done (gate), re-measure deferred.** `baseline-gate.ps1`, called by `verify.ps1`, fails a raised `size-baseline.txt` unless the file has new `raise-reason:` and `approved-by: <name> <YYYY-MM-DD>` lines absent from HEAD's copy; the failure text says how. Baseline unchanged (921,600): this batch's binary is 925,696 B, inside the 2% ratchet. `topology-check.ps1` takes `-EdgeSliders none|volume|brightness|both` (default `none`) and expects 3 + that many surfaces per monitor, with strip edge and middle-third checks. Gate logic tested with 8 sample inputs; scripts parse-checked only.
- **M1 done.** `Brightness::write` returns whether it queued; on a refusal `Throttle::restore` puts the value back while the pill is up (a newer value wins; a closed pill drops it, so a hung monitor never polls). Unit test.
- **M2 done.** Pending becomes `Unsupported` on one deadline (`READ_LIMIT`, 2 s, set when the pill shows, part of the wait). Unit test (`read_expired`).
- **M3 done.** `with_monitor` runs the action on every physical monitor; a read uses the first that answers, a write goes to all.
- **M4 done.** A successful `eConsole` is success; later role failures are logged. Two unit tests (`switch`).
- **M5 done.** Known limit recorded in `panic-abort-1.md`.
- **M6 done.** `POWER_STATUS` is one thread post until the host reads (`POWER_PENDING`, reset before the read; a failed post clears it). Unit test.

verify.ps1 PASS (fmt, clippy -D warnings, 232 tests, locked release); roqa.exe 925,696 B, SHA `46B7E3FB968156ABB5E123F45318502BD43DD41F91A08295D6D58E182BB5D45D`.

**NOT RUN:** real hover/click/wheel on a strip, `WM_GETOBJECT` with a screen reader, DDC on a hung or duplicated monitor, device switch, power broadcast on a laptop, `topology-check.ps1` and `smoke.ps1`, and the re-measure of the M5 scenarios on HEAD with sliders on and off (needs a native run and user confirmation). Review NOT RUN (paused).
