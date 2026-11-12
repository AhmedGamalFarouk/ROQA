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

Not implemented yet. Native DWM/DComp compatibility and exact shape clipping require a fresh Chief-confirmed desktop-free native check before landing.
