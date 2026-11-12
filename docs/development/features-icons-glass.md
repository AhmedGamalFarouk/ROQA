# I1 icons and G1 glass

## Part 1: I1

- Direction B paths ported from the root checkout's ignored `artifacts/design-icons/variants.html`: 24-unit grid, 1.75-unit nominal strokes, round caps/joins, no icon fonts or rasterized glyphs.
- Shared art covers tabs, weather, transport, volume/mute, brightness, settings, dock system buttons, show-desktop, shelf add/more and picker. Active tabs and play/pause controls use a 22% accent wash. App/file bitmaps and mascot eyes are unchanged.
- Four geometry sizes per glyph/factory are retained at most; physical-pixel vertices and stroke edges are snapped at the actual rendered size. Old glyph drawing code is removed.
- Power notices are text-only in the existing application; native Settings close buttons are OS-rendered controls, not custom ROQA art. No replacement raster/font symbols are introduced for these.
- Automated geometry bounds (including arcs and stroke extents), parser, volume-state and snapping checks cover 16/20/24/32 DIP at 100/125/150/200% DPI.
- Native visual sharpness, actual colors, accessibility and input checks: NOT RUN; no desktop authorization received.
- Verification: `scripts/verify.ps1 -EvidenceDirectory artifacts/verification/i1` PASS (fmt, clippy, 314 tests, locked release, unchanged 2% ratchet). Binary 1,135,616 B; SHA256 `DA4D8F913DE461410844CCDC25853A073C344EFC5BADEFFACC2ECE7E575123FA`.
- Limits gauge is supplied in the shared registry for the parallel U1 tab integration; that tab has not landed yet. Author guard pass performed; no independent milestone review claimed.

## Part 2: G1

- **Landed:** the old "Liquid glass" is now **Frosted tint**, look unchanged. Stored key `frosted-tint`; the old saved value `glass` still loads (tests: alias, key, label; palette unchanged).
- **NOT landed, DWM Liquid glass fails on our windows (native check 2026-11-12, Chief-confirmed desktop-free, own process only, private `%LOCALAPPDATA%`):** a build set `DWMWA_SYSTEMBACKDROP_TYPE` (Acrylic and Mica) plus `DwmExtendFrameIntoClientArea(-1)` on the notch, dock and edge windows, with the Frosted body tint over it and `SetWindowRgn` shapes. Result on `WS_EX_NOREDIRECTIONBITMAP` + DirectComposition + no-activate windows: no blur. DWM fills the whole window rectangle with a flat fallback colour (light grey for Acrylic, near-white for Mica, as for an inactive window), and `SetWindowRgn` does not clip it (`GetWindowRgnBox` confirmed the regions were set): a 440x164 grey rectangle around the notch pill, square grey corners around the rounded dock. That is not a clean fallback, so it was removed, not shipped (about 90 KB of binary growth avoided too). Windows-side fallbacks (build < 22621, transparency off, Battery Saver, RDP) were not exercised natively, per instruction.
- **Left open:** real blur in our own shape needs the compositor route (Windows.UI.Composition backdrop brush, research candidate 2) or the undocumented accent policy; the baked D2D look (research candidate 3) is the safe alternative. Not started.
- **Checks:** `verify.ps1` PASS on the final tree (see ledger).

## Part 3: G1b baked Liquid glass

- **Landed:** theme `Liquid glass` (key `liquid-glass`) beside `Frosted tint`, which is unchanged. No live blur, no per-frame effect, no timer, no new crate.
- **Bake:** `Palette.sheen` (top/bottom stops around `body`, specular alpha) plus `rim`. `graphics::Glass` holds three brushes over the unit square (body gradient, radial specular blob near the top, diagonal lit rim: bright top/left, dim bottom/right), built once per canvas on first paint (`baked`, keyed by the sheen; a theme change replaces the canvas, device loss rebuilds it) and placed on each body by a brush transform, so resize, the morphing notch and repaints never rebuild them. One shared `Canvas::body` draws notch, both dock styles, edge pills, the list panel and the meter slot refill; flat themes keep their paths.
- **Contrast:** `Palette::fills()` lists every body stop; the readability and accent tests use all of them, plus a test for the specular peak (ink >= 4.5:1). Hover on the floating dock adds a faint white wash.
- **Skipped (size/cheapness):** wallpaper snapshot with GaussianBlur and edge DisplacementMap lens (needs WIC decode and effects; would exceed the 2% ratchet headroom), inner shadow, noise.
- **Native (2026-11-12, Chief-confirmed, private store, own process):** screenshots of the floating and attached dock and the resting notch show the gradient (floating: top 84,93,113 to bottom 61,70,90; Frosted flat 64,69,79) with readable icons. NOT RUN: open notch (the hover did not expand it), edge pill, DPI, Settings drop-down.
- **Checks:** verify.ps1 (see ledger). Binary 1,231,360 B, within the 2% limit (1,242,408).
