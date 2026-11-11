# M5 readiness implementation: round 3 recheck
Reviewer: Claude Opus 5.5 (Tier 1, non-author). Date: 2026-11-11. Rubric: clean-code-guard review mode, AGENTS.md. Static only.
Inputs: root `main` `238c2c8` + uncommitted candidate. Manifest `m5-readiness-implementation.sha256` SHA-256 `9C10C945…8791D551` (CR stripped):
matched at start and end; 24/24 entries OK; HEAD unchanged. Scope: only the lines the round-3 fixes touched.
## Verdict: ACCEPT WITH NITS
All six RC findings are fixed, with no regression in the touched lines. Outside high contrast, `p.track(a)` = `(ink, a)`, so fills render as before.
## Fix verification
- RC-M1 fixed: cover slot `graphics.rs:1280` and dock hover chip `:1665` fill with `tint(p.track(..))` (tile or grey in high contrast, never
  ink), so the play/pause and picker glyphs stay visible. Test `theme.rs:278-279` asserts track ≠ ink/glyph.
- RC-L1 fixed: `theme.rs:197-199` `track_outline` is true exactly when track == grey (this also covers tile == grey). `draw_track`
  (`graphics.rs:1754-1772`) outlines in ink, so a muted (grey) level sits inside an ink outline. Both track sites (`:1302`, `:1337`) use it. Test `theme.rs:276`.
- RC-L2 fixed: `settings_window.rs:224` trims `\\.\` like `build` (`:263`).
- RC-N1 fixed: `host.rs:1340-1344` selects `to` (clamped, `settings_window.rs:179-186`), then `change_pins` refreshes once. The edges are a no-op.
- RC-N2 fixed: `host.rs:1949` takes `impl Fn(usize) -> bool`. Callers `:1691` (`>= SHELF_BASE`) and `:2231` (`|_| replaced`) keep the meaning.
- RC-N3 fixed: `ipc.rs:107-113` maps `E_ACCESSDENIED` (= `HRESULT_FROM_WIN32(ERROR_ACCESS_DENIED)`) from `CreateMutexW` before `?`.
## Findings
| ID | Sev | Evidence | Note | One-line fix |
|---|---|---|---|---|
| RC2-N1 | Nit | `graphics.rs:1765` | The outline stroke is 1.0 *physical* px (other notch geometry uses `scale`), a hairline at 200-300% for low-vision high-contrast users. | Pass `scale` to `draw_track`; stroke `1.0 * scale`. |
| RC2-N2 | Nit | `settings_window.rs:220-225`, `:259-264` | The monitor label format is duplicated, which is how RC-L2 drifted (DRY). | One `fn monitor_label(index, device) -> String`. |
High contrast still snaps the hover chip on and off (`ink_alpha` is 1.0 for any alpha > 0). That's consistent with every other high-contrast fade here, so it isn't raised.
## Not checked
No build, `verify.ps1`, tests, smoke, GUI/native run or measurement. The author's 184 tests, 1,081,856 B and the binary SHA are cited, not re-run.
Not seen on a real high-contrast theme, at high DPI, with Narrator, with an elevated second instance, or after a topology change with Settings open.
Code outside the round-3 lines is not re-reviewed (e.g. the high-contrast shelf placeholder fill, `graphics.rs:786`). Gates deferred in
`m5-readiness-review.md` stay NOT RUN. No milestone acceptance is claimed.
