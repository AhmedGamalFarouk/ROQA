# M5 S6: file shelf (references only)

Author: Claude Sonnet 5.5 (Tier 2), branch `feat/m5-s6-shelf`, base `9ebf962`. REVIEW NOT RUN (paused by user 2026-11-11). Gate: `scripts/verify.ps1` PASS, **161 tests** (151 + 10). Release `roqa.exe` 929,792 B, SHA-256 `F4E948B6028CD68095ED769202511FE5BB41214EECF71463AD87E8DBBEAEB5FC`; base 884,736 B, so **+45,056 B (+5.1 %)** (drop target, `IFileOpenDialog`, OLE).

## What changed
- `src/shelf.rs` (new, pure, 401 lines): up to 12 `Entry` (path, display name, `Check`), no file read. `validate` rejects `\.\`, `\?\`, `\??\`, reserved names (`NUL`, `COM1`), relative and drive-relative paths, `:` streams, wildcards, control chars and over 1024 chars; UNC is `NotChecked`. `add_all` dedupes (case, `/`, trailing `\`), puts a drop at the front in list order, evicts the oldest, reads at most 64 items.
- `src/shelf_os.rs` (new, 239 lines): the notch `IDropTarget` (CF_HDROP only, names only, effect LINK), `IFileOpenDialog` multi-select, shell open or reveal. The target only queues events; `Desktop::refresh_shelf` applies them in its normal pass, so a drop adds no wake. UNC open runs on one short thread per click.
- `host.rs`: `OleInitialize` replaces `CoInitializeEx` (the brief said OLE was initialised; it was not), `ShelfView`, register or revoke per notch, store dirty through `edit_shelf`, icons via the existing worker at the dock's size, requested only while a panel is open. `layout.rs`/`graphics.rs`/`notch_uia.rs`: slots, hit test, drawing, UIA entries. `dock.rs`: `launch_with`. `Cargo.toml`: feature `Win32_System_SystemServices`.
- Input: click or UIA Invoke opens; Ctrl+click reveals; right click removes; Ctrl+O (notch has keyboard focus) opens the file dialog. Existence is checked only on activation (`Missing` is dimmed, not removed, never for UNC).
- Idle: nothing runs collapsed or with an empty shelf; no timer, handle or poll.

## Tests (10)
Validation and normalising; UNC marking; device, relative and malformed rejects; dedupe; drop list order, junk and duplicates; cap eviction and the 64-item bound; remove and missing transitions; store round trip and a hostile store; layout overlap and single-target hit test at 96 to 240 dpi (both panels); `edit_shelf` dirty-once rule.

## Defaults relied on (m5-preparation section 4)
Q3: a row in the expanded panel (PROVISIONAL spec addendum), 12 references; the cap evicts the oldest instead of refusing. Q6: proceed with review paused. Mine: only the newest 3 are drawn and in UIA (the free pocket fits 3); icons share the dock's size (a second size would empty the worker cache); no toggle.

## NOT RUN
A real Explorer drop, activation, reveal, Ctrl+O, a UNC and a missing target, a drag over a collapsed notch, Narrator, 150 % and mixed DPI, RTL names, a drop under a full-screen app.

## Deferred
Entries 4 to 12 are not reachable (no overflow or scroll UI); mapped network drives are not detected (only `\` paths skip the check, so `Z:\` can stall the host at activation); the file dialog is modal on the host thread (timers and media wait); shelf count in the S9 report; keyboard focus on entries (S8).
