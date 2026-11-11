# M5 S9: redacted diagnostics report and resource budgets

Author: Claude Sonnet 5.5 (Tier 2), branch `feat/m5-s9-diagnostics`, base `410af12`, rebased onto S3 `5ebef9c`. REVIEW NOT RUN (paused by user 2026-11-11). Gate: `scripts/verify.ps1` PASS, **128 tests** (122 on S3 + 6). Release `roqa.exe` 809,984 B, SHA-256 `25F2CC842E75F9306FB95AED0B7ECF4F3383BFC50A8EEF4AB7722E5602EB038D` (ledger figure at the M4 landing was 801,792 B; the S1/S2/S3 deltas were not measured separately, so the S9 delta is not isolated).

## What changed
- `src/diagnostics.rs` (new, 428 lines incl. tests): pure `report()` builder, `Counters::read()`, budget table, atomic write. The catalog dump moved here from `window_catalog.rs`; its keys and order are unchanged (`catalog-smoke.ps1` fields kept), with `counters` and `budgets` appended after `windows`.
- Counters: `media_wakes`, `media_refreshes` (host calls of `Media::refresh`), `icon_cache`, `store_writes`, `handles`, `gdi`, `user`, `private_bytes`, `monitors`. Process statics, one relaxed atomic add or store where the work already happens; no timer, no thread, report written only with `--diagnostics`.
- Budget rows: private bytes 80 MiB (applies at 1 monitor), 120 MiB (3 or more), icon-cache cap (256), static CPU 1 ms/s (needs the 15 min harness sample, so always INCONCLUSIVE here). A gate that does not apply or has no value is `inconclusive`, never guessed. 2 monitors has no PLAN gate.
- Edits: `window_catalog.rs` (dump moved), `media.rs` (+2 counter lines), `icons.rs` (cap made `pub`, cache size published in 3 places), `main.rs` (one `mod` line), `Cargo.toml` (`Win32_System_ProcessStatus` for private bytes). `host.rs`, `store.rs`, `pins.rs`, `dock.rs` untouched.

## Privacy audit
- Report: the builder only takes numbers, and per-window handle, process id and the title *length*. Tests assert titles, paths, AUMID and track text never appear and every quoted token is on an allowlist (a new string field fails the test).
- Media log: text lengths and integers only. Runtime stderr lines (host, audio, icons, graphics, picker, catalog): static text and OS error codes/messages; no title, path, artist or file name found. IO errors from `std::fs` do not include the path. **No leak found.** The IPC object name embeds the user SID but is never logged.
- Residual: the `title_utf16_length` field is a count, kept for `catalog-smoke.ps1`; drop it if the user wants lengths gone.

## Defaults relied on (section 4)
Q6 (proceed, REVIEW NOT RUN). Q2 (three-monitor gate applies only when 3 or more displays exist; otherwise INCONCLUSIVE). Section 3 diagnostics rule: opt-in, on request, atomic, no timer.

## Deferred / NOT RUN
- `store_writes` reads `null` until `store.rs` calls `diagnostics::note_store_write()` (that file belongs to S3); one line to add after S3 lands.
- Wake counters by source (catalog, clock, icon), paint and scan counts, thread count, art-cache size, timer/shelf counts, handle/USER/GDI churn deltas: not available without `host.rs` or a toolhelp snapshot. Handles/GDI/USER are absolute values only.
- Not wired into the host loop, so the report is rewritten on catalog snapshots only (as before).
- Native: a real-session report reviewed by hand, and churn runs for the caps, are NOT RUN (user confirmation needed).
