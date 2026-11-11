# M5 S3: pins model and dock merge (candidate, REVIEW NOT RUN)

Branch `feat/m5-s3-pins` on `410af12`. Author: Claude Sonnet 5.5 worker. Grants no acceptance.

## What changed
- `src/pins.rs` (new, pure, 6 tests) and one `mod pins;` line in `src/main.rs`.
- `src/dock.rs`: `items_with_pins(model, &Pins)`, `Action::Launch(String)`, launcher names, `action` for an item with no windows (4 tests).
- `src/store.rs`: a comment only. `window_model.rs` untouched (no accessor needed). No `host.rs`, `graphics.rs`, `dock_uia.rs` edit.
- `items(model)`, `names(..)` and `action(..)` keep their signatures, so the host is unchanged and runs with no pins.

## Design
- **Pin = `Identity`.** Display name and launch target are derived (not stored), so the store keeps one string per pin:
  `app:<aumid>` or `exe:<lowercased path>`. A pin made from a running app shows the AUMID package name or exe stem, not its window title (S4 may improve this).
- **Rejected:** `Identity::Unavailable`, empty and multi-line keys. Exe paths are lowercased. Duplicates and `MAX_PINS = 32` (UNVERIFIED) enforced on `add` and on load.
- **`move_to(from, to)`** is false with no change when out of bounds or equal, so S4 marks the store dirty once per real reorder.
- **Merge:** pins first in pin order (the running group, or a launcher item with no windows), then unpinned groups in today's order.
  An `Executable` pin also claims a group whose identity became an AUMID but whose exe matches (exact identity wins; one claim per group).
- **Launch:** `Action::Launch(target)` is `shell:AppsFolder\<aumid>` or the exe path. `dock::perform` returns false for it; S4 does the shell launch and the existence check on activation. Nothing is scanned at startup.
- **Names:** a launcher reads "<name>, pinned, not running". Pinned-and-running names are unchanged until S4.

## Open-question defaults relied on
- Q8: pin by AUMID when present, otherwise by exe path; the M2 packaged/elevated matrix is unrun, so packaged pins may group differently (S4 states this limit).
- Q6: no review, labelled REVIEW NOT RUN.

## Footprint
- Release `roqa.exe` 804,864 B (S2 landed 802,304 B, +2,560 B), SHA-256 `4FF908F9C39E7D777BF59CD4139299C684B2E0B6984A6351494F031CC1F4B6D0`.
- No new thread, crate, `windows` feature or handle. Host runtime cost: none yet (the host does not call `items_with_pins`).

## Verification
- `scripts/verify.ps1` PASS: fmt, clippy -D warnings, **122 tests** (112 + 6 pins + 4 dock), locked release build. Logs in `artifacts/m5-s3/verification/`.
- Existing dock tests pass unchanged; `empty_pins_equal_todays_items` asserts the empty-pin equivalence.

## NOT RUN / deferred
- Review (paused). Any native or GUI run, a real `%LOCALAPPDATA%` write, shell launch, icon lookup for a not-running pin.
- Host wiring, store dirty flag and flush, pin/unpin/reorder UI, UIA names for pinned state: S4.
- `MAX_PINS` and the AUMID display-name derivation are first guesses.
