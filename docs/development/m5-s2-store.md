# M5 S2: persistence store (candidate, REVIEW NOT RUN)

Branch `feat/m5-s2-store` on `badac8a`. Author: Claude Sonnet 5.5 worker. Grants no acceptance.

## What changed
- `src/store.rs` (new, pure model + loader + writer + 10 tests) and one `mod store;` line in `src/main.rs`.
- No `host.rs` edit, no new crate, no new `windows` feature. S3, S4 and S6 do the host wiring.
- `#![allow(dead_code)]` at the top of the module until S3 calls it (remove then).

## Design choices
- **Format:** hand-rolled `key=value` lines: `version=N`, repeated `pin=` / `shelf=`, closing `end`.
  Chosen over JSON/TOML because no parser crate is present and the model is two string lists; the `end` line
  makes truncation detectable. Values with a newline cannot be represented and are dropped on save.
- **Path:** `default_path()` = `%LOCALAPPDATA%\ROQA\store.roqa`; every function takes a path, so tests use a temp dir.
- **Load:** read once, file closed at once. Missing -> defaults. Corrupt, truncated, empty, non-UTF-8, over `MAX_BYTES`
  (64 KiB, checked by metadata before reading) or a newer `version` -> defaults and rename to `store.roqa.bad`
  (a newer file is kept, never overwritten by a downgrade). Present but unreadable (locked) -> defaults, file untouched.
- **Versions:** `VERSION = 1`; 0 or unparsable is corrupt; older versions pass through a `migrate` stub (empty today).
- **Unknown keys and junk lines** are ignored; anything after `end` is ignored.
- **Save:** serialize, refuse above the cap (old file kept), `create_dir_all`, write `store.roqa-<pid>.tmp`, rename over
  the target (the `write_diagnostics` pattern); the temp file is removed if the write or rename fails. A stale temp
  from a crashed process is ignored and left (one tiny file per crash; no sweep).
- **Flush:** `flush_decision(dirty, last_change, now) -> Idle | Wait(due) | Now`, `COALESCE` = 1500 ms (UNVERIFIED, tune in S9).
  Trailing-edge debounce from the last change, so a never-ending burst would defer the write; S3/S4 may add a max-wait if
  drags prove it. At quit the host writes iff dirty, ignoring the delay.

## Footprint (against `badac8a`, release)
- Release `roqa.exe` 802,304 B, SHA-256 `D6B6AA817D0431A9C434576D13649F353D11712896701A796184F7C2EB3C3E55`.
- Baseline not rebuilt here: `d4c3c08` recorded 802,304 B (author build) and 801,792 B (root-checkout landing build); the
  512 B spread between those two builds of the same code is path-dependent, so the S2 delta is 0 to +512 B (the module is
  unreferenced, so the linker drops it). Idle and runtime cost: none yet, nothing calls it.

## Verification
- `scripts/verify.ps1` PASS: fmt, clippy -D warnings, **112 tests** (102 + 10), locked release build. Logs in `artifacts/m5-s2/verification/`.

## NOT RUN / deferred
- Review (paused by the user). Crash-during-write run. Any native or GUI run. Real `%LOCALAPPDATA%` write on this machine.
- Host deadline, quit flush, write counter for S9 diagnostics: S3 onward.
