# M5 readiness implementation: round 2 recheck

Reviewer: Claude Opus 5.5 (Tier 1, non-author). Date: 2026-11-11. Rubric: clean-code-guard review mode, AGENTS.md; perf/footprint first.
Inputs: root `main` `238c2c8` + uncommitted candidate. Manifest `m5-readiness-implementation.sha256` SHA-256 `BE4C20B6…3051B7E` (CR stripped):
matched at start and end; 24/24 entries OK at start and end; HEAD unchanged. Scope: `git diff 238c2c8` of the files the round-2 fixes touched
(`error_log.rs`, `ipc.rs`, `main.rs`, `host.rs`, `theme.rs`, `graphics.rs`, `settings_window.rs`, `shelf_os.rs`, `media.rs`). Static only.

## Verdict: REVISE (narrow)

RR-H1, RR-M2, RR-M3 and RR-L1 to L6 are fixed with no regression found. RR-M1 is only partly fixed: two background fills still go through
`ink_alpha`, so in high contrast a glyph is drawn in ink on an ink-filled tile. A two-line fix; recheck only those lines plus RC-L1.

## Performance / footprint (static)

- No new thread, timer, poll or idle wake. `system_palette` runs only on create, theme change, WM_SETTINGCHANGE/WM_SYSCOLORCHANGE.
- `refresh_monitors` returns early when the device list is unchanged (`settings_window.rs:182-189`), so the per-REPOSITION call is cheap.
- Pin combo rebuild happens only on an applied pin edit. Release +12,288 B over round 1 (1,072,128 B, author-cited, not re-run).

## Fix verification

- RR-H1 fixed: `host.rs:2332-2336` admits first; `initialize()` is skipped only for `ALREADY_RUNNING` (`ipc.rs:110`), so other admission errors still log.
  A second launch no longer opens/truncates the log and shows no dialog (`error_log.rs:54-61`, tested). Main's `eprintln!` is a no-op before init.
- RR-M2 fixed: `change_pins` (`host.rs:1583-1599`) re-syncs on every applied edit (dock toggle, drag, settings Move); `clamp_selection` tested.
- RR-M3 fixed: refresh after placement, error logged and Settings closed (`host.rs:2164-2175`); host never ends on it. Uses post-failure monitor list.
- RR-L1 fixed: pins created before Reset/Close (`settings_window.rs:314-331`); recreated checkboxes `SetWindowPos` after the previous sibling.
- RR-L2 fixed: `theme.rs:144-158` logs and falls back; all three callers infallible. RR-L3 fixed: `shelf_os.rs:418-420`. RR-L4 fixed:
  `host.rs:1954` (SHELF_BASE=4 is the last control family). RR-L5, RR-L6 fixed (`media.rs:681-687`, `settings_window.rs:527-529`, `error_log.rs:62-63`).

## Findings

| ID | Sev | Evidence | Defect | One-line fix |
|---|---|---|---|---|
| RC-M1 | Med | `graphics.rs:1281` (media cover slot, no art), `graphics.rs:1669` (dock hover chip) | RR-M1 residual, candidate-introduced: `ink_alpha(COVER_FILL)` and `ink_alpha(0.12*hover)` become 1.0 in high contrast, so the cover tile is solid ink and the play/pause glyph drawn in ink on it vanishes (compact notch has no other play-state cue); the dock picker glyph vanishes while hovered and the hover snaps on/off. | Fill both backgrounds with `tint(p.track(...))` instead of `with_alpha(p.ink, p.ink_alpha(...))`; extend the contrast test to assert cover/hover fill ≠ glyph ink. |
| RC-L1 | Low | `theme.rs:167`, `:197-202`; `graphics.rs:1339-1342` | When BTNFACE == WINDOW (usual for Windows 11 contrast themes) the track falls back to GRAYTEXT, which is also the dim fill: a muted volume level is grey on grey and disappears. The test asserts this fallback but not the collision. | In that case draw the track as a 1 px ink outline (`DrawRoundedRectangle`) instead of a grey fill; assert track ≠ dim fill. |
| RC-L2 | Low | `settings_window.rs:212` vs `:248-250` | `refresh_monitors` labels checkboxes with the raw `\\.\DISPLAYn`, `build` trims the prefix: the accessible name changes after a topology change. | Use `device.trim_start_matches(r"\\.\")` in the refresh label too. |
| RC-N1 | Nit | `host.rs:1340-1341` | A Settings Move rebuilds the combo twice (`change_pins` with the old row, then `sync_settings_pins(to)`). Harmless, user-triggered. | Select `to` before calling `change_pins`, drop the second sync. |
| RC-N2 | Nit | `host.rs:1946` | `reconcile_focus(replaced: bool, shelf_only: bool)`: two boolean flag args (rule 3). | Pass a small enum (`Keep/Session/Shelf`) or the predicate. |
| RC-N3 | Nit | `ipc.rs:105`, `host.rs:2333` | If an instance at another integrity level holds the mutex, `CreateMutexW` fails with access denied, not ALREADY_EXISTS: the log is truncated and the generic dialog shows. Unusual (elevated launch). | Treat `E_ACCESSDENIED` from `CreateMutexW` as `ALREADY_RUNNING`. |

## Not checked

No build, `verify.ps1`, test run, smoke, GUI/native run or measurement; the author's 184 tests, fmt/clippy and binary size/SHA are cited only.
Not seen on a real high-contrast theme, Narrator, keyboard tab order, topology change with Settings open, second-instance launch, lost
picker wake. Unchanged round-1 code (B1, B3, P1-P3, smoke envelope, `dock_uia`, `notch_uia`, `native_window`, `audio`, `shelf.rs`) not re-reviewed.
All gates deferred in `m5-readiness-review.md` stay NOT RUN. No milestone acceptance is claimed.
