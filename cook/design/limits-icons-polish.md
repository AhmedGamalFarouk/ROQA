# Limits bars and dock icons polish (design spec, 2026-11-23)

Designer spec for an implement worker. Build to this; do not redesign. Units are DIP unless marked px.
Owner decisions already made: thin brand bars for AI limits, dock icon box stays 26 DIP, fill is full accent.

## Stills (mocks rendered from `cook/design/limits-icons-polish/mock.html`, 2x, headless Chrome)
| still | shows |
|---|---|
| `limits-icons-polish/limits-before-after.png` | before (live capture, x4) next to after, Classic |
| `limits-icons-polish/limits-themes.png` | the popover in Classic, Midnight, Light, Frosted tint, Liquid glass |
| `limits-icons-polish/limits-states.png` | row anatomy with measurements; 0, 1, 2, 12, 50, 99, 100 % and hover |
| `limits-icons-polish/icons-before-after.png` | the 7 dock glyphs in device pixels at 100/125/150/200 %, before and after |
| `limits-icons-polish/dock-themes.png` | the dock clusters with the new glyphs in all five themes, limits hovered |
Before reference from the live app: `cook/stills/research-graphics/s1-limits-popup.png`, `z-limit-bars-x4.png`, `z-battery-gauge-sliders-x10.png`.
Re-render: `chrome --headless=new --force-device-scale-factor=2 --screenshot=<png> "mock.html?v=themes|states|icons|docks|before"`.

## A. AI limits popover: thin capsule bars

Only rows that come from `Item::Limit` change. Header and message rows (`Item::Label`), the popover panel,
its width (250), pad (6), row height (34), radius (23) and the hover capsule stay exactly as today.
The volume mixer rows (edge.rs list) keep their full-row slider; see Open.

### Limit row anatomy (row cell is 238 x 34, inside the 6 pad)
| part | measurement | token |
|---|---|---|
| label text (`"5h, resets 13:30"`, string unchanged) | box left 12, right = percent box left - 8, top 4, height 18; leading, one line, clipped | 13 / 400 Segoe UI Variable Text, `ink` at 0.70 through `p.ink_alpha` (Light lifts to ~0.81) |
| percent (`"12%"`) | box right = cell right - 12, width 40, top 4, height 18; trailing | 13 / 500 (MEDIUM), `ink` at 1.0, tabular numerals |
| track | left 12, right cell right - 12 (214 wide), top 23, height 4, radius 2 (= h/2) | `ink` at 0.18 through `p.ink_alpha` (same as the notch progress bar, `graphics.rs` `Extra::Progress`) |
| fill | same top, height, radius and left as the track; width below | `p.accent` at 1.0 (Lamp `#FFB454`; Light `#9A5200`; `match_accent` applies) |
| space | label cap top to row top ~8; baseline to bar 6; bar bottom to row end 7 | |
| icon room | none: limit rows do not keep the 20 DIP icon gap | |

Fill width: `w = round(track_w * pct / 100)`; if `pct == 0` draw no fill; else `w = clamp(w, bar_h, track_w)`.
The fill is a capsule of the track's own height, so it is inside the track by construction: no sliver past the
round end, no blob, a 4 x 4 dot at 1-2 %, a full capsule at 100 %. No clip call needed.

Pixel placement (all scales): `bar_h = round(4 * scale)` (4, 5, 6, 8 px at 100/125/150/200 %);
`bar_top = row_top + round(23 * scale)`; left and right ends rounded to whole px. Radius = `bar_h / 2`.

### States
| state | look |
|---|---|
| used 1-100 % | as above |
| `reset at` row (`Some(0)`) | track only, percent `0%` |
| refreshing | header text already says `refreshing`; bars keep their last values |
| hover | the existing 22 % accent capsule behind the whole row (unchanged); bar drawn on top |
| high contrast, `track_outline()` | track is the existing 1 px ink outline capsule; fill system highlight as today |
| reveal | the existing popover reveal; no new motion, no animated fill |

### Themes (surface and ink only; accent rule unchanged)
| theme | panel | label | percent | track | fill |
|---|---|---|---|---|---|
| Classic | `#000000` | `#FFFFFF` 70 % | `#FFFFFF` | `#FFFFFF` 18 % | `#FFB454` |
| Midnight | `#0B1020` | `#E8EEFF` 70 % | `#E8EEFF` | `#E8EEFF` 18 % | `#FFB454` |
| Light | `#ECEEF2` | `#14161A` ~81 % | `#14161A` | `#14161A` ~36 % (ink_alpha lift) | `#9A5200` |
| Frosted tint | rgba(16,20,32,.74) | `#FFFFFF` 70 % | `#FFFFFF` | `#FFFFFF` 18 % | `#FFB454` |
| Liquid glass | sheen, top .78 to bottom .74 | `#FFFFFF` 70 % | `#FFFFFF` | `#FFFFFF` 18 % | `#FFB454` |

## B. Dock glyphs on the 26 DIP box

Path data stays on the 24-unit grid (`GRID = 24`), stroke 2.1 units (= brand 1.75 on 20), round caps and joins.
Redraw three paths in `src/graphics/glyphs.rs` `Glyph::path`:

| glyph | new path | why |
|---|---|---|
| `Limits` | `M4.64 18.25A8.5 8.5 0 1 1 19.36 18.25M12 14l3.2-3.2` | 240 degree dial, centre (12,14); needle starts at the pivot, 4.5 long, ~2 px clear of the rim; ink box centred on 12 |
| `Gear` (Quick sliders) | `M4 7h2M12 7h8M4 17h8M18 17h2M9 4a3 3 0 1 0 0 6a3 3 0 1 0 0-6zM15 14a3 3 0 1 0 0 6a3 3 0 1 0 0-6z` | knob radius 2 to 3, so the hole is ~4 px, not 1; lines break at the knob (the brand's "knob cuts the line") |
| `Battery` | `M5.5 6.5h9.5a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3H5.5a3 3 0 0 1-3-3v-5a3 3 0 0 1 3-3zM21.25 10.5v3` | corner radius 3; body 15.5 x 11; nub 3 tall with a 1.15-unit gap (~1.25 px at 100 %, 2.5 px at 200 %), so it never merges |

Audit of the other dock glyphs (ink box in grid units incl. stroke; target centre 12,12, +-0.5):
| glyph | ink box | centre | verdict |
|---|---|---|---|
| Search | 18.6 x 18.6 | 12.25, 12.25 | keep path; crisp after rule C |
| TaskView | 17.1 x 14.6 | 12.5, 12.25 | keep path; rails crisp after rule C |
| Drive (Places) | 18.1 x 16.1 | 12, 12.5 | keep path; dot stays a stroke-width dot |
| Chevron (tray) | 14.1 x 8.1 | 12, 12 | keep; intentionally small, a secondary control like Windows' own |
| Battery, Limits, Gear | 20.6 x 12.1 / 19.1 x 14.9 / 18.1 x 18.1 | ~12, 12 | redrawn above |
| Windows logo | native blue squares | n/a | out of scope (Windows mark, not a stroke glyph) |
Optical mass after the redraw is even across the row (see `dock-themes.png`). Colour unchanged: `p.glyph`; hover chip unchanged.
The `every_glyph_parses_and_stays_inside_the_grid` test must still pass (all three stay inside 0..24).

## C. Crisp strokes at 100, 125, 150, 200 %: rails, not points

Today `snap()` rounds every point (curve ends too) while arc radii stay exact, which bends corners and
moves circles (`z-battery-gauge-sliders-x10.png`). Replace per-point rounding in `glyphs.rs::build` with these rules.
Notation: `k = side_px / 24`; `sw = stroke_pixels(side)` (unchanged: 2, 3, 3, 5 px for 26, 33, 39, 52 px boxes);
`phase = (sw % 2) / 2`; `snap(v) = round(v*k - phase) + phase` (unchanged). The square's origin stays a whole pixel.

1. Exact by default: every coordinate is `v * k`, no rounding.
2. Rails snap: a straight segment that is horizontal (same y) snaps that y; vertical (same x) snaps that x. Only
   the fixed coordinate moves; the along-line coordinates stay exact. Both end nodes record the delta.
3. Corners ride their rails: an arc endpoint whose x (or y) is not on a rail takes the x (or y) delta of the
   arc's other endpoint if that one is on a rail. A rounded-rect corner therefore moves rigidly with its two
   rails and keeps its exact radius `r * k`. Never round an arc radius here.
4. Circles and dials: a figure made only of arcs snaps its centre with `snap()` and rounds its radius to whole
   px; its points are rebuilt from the new centre and radius. Its centre is recorded as an anchor.
5. Anchors: any node that equals a recorded centre (the gauge needle's pivot) takes that centre's pixel position.
6. Dots (`h.01`): the dot's centre snaps with `snap()`.
7. Diagonals and free ends: exact (rule 1). No other snapping.
Result: straight edges and circle extremes sit on whole-pixel boundaries at every scale; corners and circles
keep their true shape; diagonals antialias evenly. A runnable reference is `geom(..., 'after')` in `mock.html`
(about 30 lines); the Rust version should be about the same size, kept inside `build`.

Stroke weights per scale: 26 px box 2 px, 33 px 3 px, 39 px 3 px, 52 px 5 px (nearest to brand 1.75/20). Unchanged.
Settings glyphs go through the same `build` (GDI DC target), so they improve with no extra work.

## Build list (in order, smallest first)
1. `glyphs.rs`: replace the three path strings (B). Run the existing glyph tests.
2. `glyphs.rs::build`: rails-not-points snapping (C). Update `strokes_snap_to_whole_pixels_at_every_size_and_scale`
   to assert: rail edges land on whole px at 16/20/24/26/32 DIP x 1/1.25/1.5/2; a rounded-rect corner keeps
   radius `r*k` (arc chord unchanged); a circle's centre lands on the phase grid and its radius is whole px; the
   gauge needle start equals the dial centre in px.
3. `edge.rs` `ListRow`: add `bar: Option<u8>` (a limits bar) beside `level` (mixer). `dock_menu.rs` maps
   `Item::Limit(pct)` to `bar: Some(pct)` and stops setting `level` for it, so limit rows lose the icon room.
4. `graphics.rs` `paint_list`: for `row.bar`, draw label, percent and track/fill per section A (track via
   `p.ink`/`p.ink_alpha(0.18)` or `draw_track` with the high-contrast path kept; fill `p.accent` at 1.0;
   width rule above). Percent with the existing trailing font at weight 500; enable tabular figures if the
   shared text helper has them, otherwise Segoe UI Variable's default figures are already tabular.
5. `scripts\verify.ps1`; put the exe byte delta in the report.
6. Live check on the second monitor only (`scripts\run.ps1 start -Test`): hover AI limits in Classic and Light;
   zoom the dock right cluster; compare with the stills above. Then one capture round and the two critics.

## Self-review (cook/RUBRIC.md, on the five stills)
| dimension | score | reason |
|---|---|---|
| Shape language | 8.7 | every bar a true capsule, fill never escapes, popover radius unchanged |
| Color | 8.8 | Lamp only on fills; track and labels are ink alphas; themes differ by surface only |
| Motion / character | n/a | no motion change in scope |
| Icon set | 8.5 | one stroke, even optical mass; chevron stays lighter by design |
| Typography | 8.4 | label 70 % vs percent 500 gives the hierarchy; tabular percent aligns |
| Coherence | 8.7 | same 4 DIP bar and 0.18 track as the notch progress bar |
Blur at 8 px: yes, warm capsule bars on a black capsule panel read as ROQA.
Fixed during review: hover chip mock was a rounded square (now the real capsule); track alpha matched to the
notch bar (0.16 to 0.18); battery body enlarged after it read small next to Search; redline labels tidied.

## Open (owner)
- Volume mixer rows share `paint_list` and still use a 40 % accent fill with text on it. Same thin style there
  would need its own design (it is a drag control). Suggest only the fill-width rule (no sliver) for now.
- The notch progress bar has the same sliver at tiny percents; the same width rule fixes it in 1 line.
- The popover hover capsule (22 % accent) looks brownish on Classic; it is shared by every dock menu.
