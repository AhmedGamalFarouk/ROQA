# ROQA visual spec: notch and dock (design pass 1)

Author: `/root`, 2026-11-09. Reference study: `nusa-reference.md`. All sizes are in DIPs, scaled per monitor DPI; settled edges are snapped to whole pixels. Colours are sRGB.

## Surfaces and theme

- **Always dark.** Both surfaces use a black body with white content, like Nusa and hardware notches, and follow neither light nor dark mode: a black notch is part of the screen edge, not app chrome.
- **High contrast.** System high-contrast colour mapping isn't implemented yet; see the limits.

## Notch (`src/layout.rs`, `src/graphics.rs`, `src/host.rs`)

| Token | Collapsed pill | Expanded panel |
|---|---|---|
| Body width × height | 150 × 30 | 320 × 80 |
| Bottom corner radius | 14 | 24 |
| Concave top ear | 10 | 16 |
| Fill | `#000`, opaque | `#000`, opaque |
| Shadow | none | Body inset 6 at the sides, ending 7 above its bottom; Direct2D shadow σ 20, black at 0.9 × open progress |

### Window, hit area and region

- **Window.** A fixed **400 × 124** HWND, centred and flush with the work-area top. It holds the expanded body plus ears, room for the open overshoot, and the shadow below and beside the body. It never moves or resizes.
- **Region (`layout::notch_region`).** Updated every animation frame. It covers the current body and ears, plus a shadow reach of 44 × √smooth(p) DIP to the sides and below, clamped to the window. While growing it leads by two frames.
  - At rest, it is exactly the collapsed footprint (170 × 30), so the screen edge stays clickable.
  - During the hover-intent swell, it is barely larger than the pill.
  - Fully open, it is the whole window, because the shadow is drawn there.
- **Hover (`layout::notch_hit`).** The pointer must be over the drawn body or ears, plus 4 DIP of slop. Transparent margins and the shadow never hold or re-open the panel.
- **Drawing, hover and region share one geometry** (`layout::notch_body`): whole-pixel body, centred.
- **Text size.** The Windows text-size factor (Accessibility > Text size, `UISettings.TextScaleFactor`) multiplies the notch's effective DPI. The whole notch, whose only content is text, scales and is rebuilt live on the change event.

### Type

The font is Segoe UI Variable, from the system; no font is bundled.

| Role | Size | Weight | Colour |
|---|---|---|---|
| Clock (one element, pill to panel) | 15 → 30 (Text below 20, Display above), am/pm at 0.72× | Bold | white |
| Expanded date (long, user locale) | 13 (Text) | Regular | white 60 % (≈ 7.4 : 1 on black, AAA) |

- **Designator.** The am/pm designator is whatever text precedes the first or follows the last digit in any script, so the user-locale format is never re-ordered.
- **Overflow.** Text is centred and clipped to the body; long dates end in an ellipsis.

## Notch motion (`src/motion.rs`), fitted to the live Nusa capture

| Phase | Token |
|---|---|
| Hover intent | On entering the hover band, the pill swells to progress 0.07 (about 162 × 33) over 320 ms (ease-out `cubic(0.2,0.9,0.3,1)`). It opens only if the pointer is still there. Re-entering while still closing reopens directly |
| Open | Damped spring, ζ 0.65, ω 18 rad/s, about 7 % overshoot peaking at about 230 ms. Bounded at 500 ms, then it lands exactly on target |
| Close | Damped spring, ζ 0.61, ω 16 rad/s. Reaches the pill at about 190 ms, undershoots about 9 %, and is bounded at 560 ms |
| Size | Width and height follow the spring, including overshoot and undershoot. Radius, ears and shadow follow the progress clamped to 0–1 |
| Content | One clock morphs: its size goes 15 → 30 and its centre moves from the pill middle (15) to the time row (29), both by `smooth(p)`. It never fades, so there is no gap. The date fades in by `smooth((p − 0.35) / 0.45)`, sliding 6 down into place |
| Interruption | Every re-target starts from the current value; velocity is not carried over |
| Reduced motion | None. By user decision (2026-11-09) ROQA always animates, like Nusa, whatever the Windows "Animation effects" setting says |

## Dock (`src/dock.rs` geometry unchanged; `src/graphics.rs` drawing)

| Token | Value |
|---|---|
| Bar | Height 64, padding 8, slot 56, icon 40. Unchanged, so hit-testing and UIA rectangles are identical |
| Bar shape | Anti-aliased rounded rectangle, radius 20. The corners are transparent; there's no window region |
| Bar fill | Idle `#000`; pointer inside `rgb(20,20,22)`. This keeps M1's hover semantic |
| Edge | One-physical-pixel inner hairline, white 10 % |
| Slot hover | A 48 × 48 chip, white 12 %, radius 12, centred on the icon and lifting with it |
| Icon lift | 3, snapped to whole pixels so shell bitmaps never resample |
| Active window | White 92 % pill, 12 × 3, 4 above the bar bottom |
| Picker glyph | 2 × 2 tiles of 12, gap 4, radius 3, `rgb(220,225,232)` |
| Motion | Hover in over 160 ms, out over 180 ms, ease-out `cubic(0.2,0.9,0.3,1)`. Highlight, lift and bar fill move together |

## Frame pacing and idle cost

- **Painting.** A surface paints per frame only while a tween runs or hover intent is pending, then one final exact frame.
- **Waiting while animating.** The loop waits on `DCompositionWaitForCompositorClock` (the same IPC handles, timeout ≤ 33 ms), then drains input, so input waits at most one frame. That function is Windows 11 only, so it is resolved at run time with `GetProcAddress` and ROQA still loads elsewhere. When it is absent or fails, the loop falls back to a ≤ 16 ms message wait.
- **Idle.** The original blocking `MsgWaitForMultipleObjectsEx`, with only the catalog and minute deadlines. No timer and no polling.

## Accessibility

- **Dock.** UIA rectangles and hit-testing are unchanged; the picker remains the keyboard and Narrator route.
- **Contrast.** The time (white) is 21 : 1 and the date (60 % white) about 7.4 : 1 on black.
- **Pointer targets.** The collapsed notch hit area equals its visible footprint.
- **Motion.** Reduced motion is deliberately not honoured, by user decision. This is a known accessibility regression against the original brief, recorded in the ledger.

## Known limits of this pass

- **Text scaling.** The notch follows the Windows text-size setting; this is verified only at 1.0 on this machine, and other factors use the same path as DPI scaling. The dock has no text.
- **High contrast.** There's no high-contrast colour mapping.
- **Shadow band.** While open, the drawn shadow takes clicks (it is inside the region), but it doesn't hold the hover. As the panel closes, the region shrinks with the shadow.
- **Dock corners.** The dock's transparent anti-aliased corners are inside the HWND and take input on a few pixels.
- **Evidence.** Frame pacing is evidenced by screen-capture sequences (about 140 Hz sampling), not by a compositor frame-time trace.
