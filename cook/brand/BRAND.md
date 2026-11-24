# ROQA brand spec v1 (approved by the owner)

Board: `cook/brand/board.png` (source `board.html`). Units: DIP (device-independent px). Everything below is implementable in the existing D2D code; nothing needs a new dependency, font file or timer.

## 1. Idea
Name: roqa. Role: a small companion that lives at the top of the screen.
Personality: calm and caring, lightly poetic, and springy when it greets you. Still when you are busy.
Blur test: a black pill with two warm capsule eyes must read as ROQA when blurred. Eyes are the only accent-coloured thing at rest.

## 2. Shape grammar
```
capsule      radius = height / 2           chips, buttons, tabs, progress, dividers, toggles, eyes, dock
panel        top flare 14, bottom radius 40   (collapsed pill: flare 10, radius = h/2 = 15)
tile         radius = parent radius - inset   (panel 40, inset 26 -> 14)
min radius   3    never a sharp corner, never a 1 px hairline box: separate with fill (n3 on n0) not outline
outline      only the shelf drop zone (dashed, r14)
```
Notch silhouette: concave top flares (as now), bottom radius 40 open, 15 collapsed. Collapsed pill 150 x 30, open panel 420 x 140 (wider only for split states, e.g. 520).
Eye capsule: 9.5 x 18 DIP (existing `eyes.rs` BASE), centre gap 25 (existing GAP), ratio 1 : 1.9, radius = w/2.

## 3. Colour
One accent + a neutral ramp. Accent options (hex, contrast on #000):
| id | name | hex | on black | note |
|---|---|---|---|---|
| A | **Lamp (recommended)** | `#FFB454` | 11.9 | warm night-light, doubles as the prayer sun and the water-day accent, nowhere near Nusa's blue/violet |
| B | Lagoon | `#3FD9C0` | 11.9 | calm, cool, reads techy |
| C | Coral | `#FF7A7A` | 8.3 | affectionate, risks reading as error red |

Recommended: **Lamp `#FFB454`**. Ink on Lamp: `#14161A` (10.3:1).
Light theme deepened accent for text/eyes/strokes: `#9A5200` (5.05:1 on `#ECEEF2`; the old `deepen` rule gave ~4.0, too low).

Neutral ramp (from the existing palettes, so no theme re-tune):
```
n0 #000000  notch body (Classic)      n5 #6B6B73  disabled ink
n1 #0E0E10  recessed                  n6 #9A9AA3  secondary ink (7.5:1 on n0)
n2 #141416  dock hover / tile         n7 #DCE1E8  glyph
n3 #26262A  raised tile               n8 #ECEEF2  Light body
n4 #3A3A40  tab selected / track (Classic tile 58,58,64)   n9 #FFFFFF  ink on dark
ink-on-light #14161A
```
Accent usage rule (all must hold): eyes at rest; the single primary action per screen (Start); progress fill; selected indicator dot/segment; today marker; sun in the prayer arc. Never gradients, never glow except the prayer sun, never accent text smaller than 12 px on n0/n2 without checking 4.5:1.
State colours (not accent): error `#FF7A7A`-family is NOT used; errors are copy + n9 ink on n3 tile with the warning glyph.

Settings window: drop the multicolour icon tiles (currently grey, #AF52DE, blue, cyan, #FFB069, #30C85E, red, grey). Use the one icon style, ink n7, selected item accent dot.

## 4. Motion
Three named curves, same names in code (`motion.rs`): 
| name | cubic-bezier | time | used for |
|---|---|---|---|
| settle | `0.2, 0.9, 0.3, 1.0` | 260 ms | fades, hover out, value changes (= current `EASE_OUT`) |
| pop | `0.34, 1.45, 0.5, 1.0` | 360 ms | hover grow, squash recover, greet, wake, chip press (overshoot ~6%; replaces `POP` 0.3,0.1,0.55,1.15) |
| slide | `0.3, 0.0, 0.15, 1.0` | 240 ms | tab/page switch, notice in/out (= current `PAGE_TIME`) |

Notch open/close keep the existing springs (OPEN damping 0.66 freq 19, 500 ms bound; CLOSE damping 0.64 freq 17, 560 ms). They are the "pop" of the big shape.
Rules: frames only while a curve is running (event-driven); hidden = zero work; `--fast`/reduced motion: time x0.6, overshoot clamp to 1.0 (no bounce), mascot poses become a 120 ms cross-fade; never more than one running animation clock per window.

## 5. Mascot (eyes)
Eye = capsule, colour = accent (Lamp; Light theme `#9A5200`). No glow, no halo, no particles, no "z".
Parameters are factors on the 9.5 x 18 base (`eye(mood,pose)` already returns w,h,dy,lids):
| state | trigger | shape (w x, h x, dy) | motion | time |
|---|---|---|---|---|
| idle (hidden) | clock shown | none | eyes show 40-100 s after idle (existing `MIN_GAP` 40 s + spread) | cross-fade 220 ms settle |
| blink | random 3.2-6.5 s while eyes are visible | h x0.12, w x1.1 | close 70 ms, open 110 ms (settle) | 180 ms total |
| squash on hover | pointer enters the pill | w x1.28, h x0.82, dy +2 | down 90 ms then pop back | 360 ms (pop) |
| doze | eyes visible and the pointer idle for 20 s (Nusa dozes at ~21 s; ROQA's existing Sleepy mood after `IDLE_AFTER` stays as is) | w x1.15, h x0.5, dy +3, top lid 0.2 | lids slide, then static (no loop) | 900 ms slide |
| wave | first input after doze or on greet | eyes to happy arc; right eye rotate +22 deg, dy -5, hop twice | 2 x (up 140 ms, down 140 ms) with pop | 700 ms |
| lean to timer | a timer is running and the panel is open | dx +7, w x0.9, h x0.92 | slide; plus accent ring in the collapsed pill that breathes 0.35 to 0.75 opacity over 3.2 s only while the pill is on screen | 260 ms settle |
| look at media | media is playing and the panel is open on Media | dx -8 toward the album tile; 2 px nod on track change | slide | 240 ms slide |
Existing moods map: Idle, Happy (wave end), Sleepy (doze), Surprised (greet), Adhan, Rain (droopy: dy +2, top lid 0.4).

## 6. Icons
One style: 20 px grid, stroke 1.75 DIP, `round` caps and joins, corner radius 3 on joins, open stroke ends are capsule ends, no fills (the settings knobs are filled with the surface colour to cut the line), no two-tone. Active: stroke = accent. Inactive: stroke n7 (or n6 on dock). Optical size: draw at 20, render 16-20. The 8 glyphs on the board (timer, water, prayer, media, shelf, weather, limits, settings) are the template; widgets, clipboard, window picker follow the same rules.

## 7. Typography
No font files (size ratchet): Segoe UI Variable. Display (time, titles): Segoe UI Variable Display 600. Text: Segoe UI Variable Text 400/500. Numerals tabular everywhere a number can tick. Sentence case. No uppercase labels (replace the "THIS WEEK" / "BATTERY" tile labels).
Scale (DIP/weight): time 28/600 (timer 30/600), title 15/600, body 12/400, caption 10.5/500, collapsed clock 13/600. Line height 1.25. Letter spacing 0 (display 28+: -0.01 em).
Fallback chain: Segoe UI Variable Display, Segoe UI Variable Text, Segoe UI.

## 8. Voice
Warm, brief, 7 words or fewer, sentence case, second person, no exclamation marks, no em dashes, no emoji. Say what happened, then what happens next only if it matters. Contractions always ("Couldn't", "Can't"). Every notice ends with a period except a live readout of a value (`Volume 41%`, `Ejecting E:`) and text that is not ROQA's (a Windows notification, a `notify` or agent line).
```
Timer done      Round done. Rest a little.
Timer cancelled Timer cancelled.
Empty shelf     Drop a file here. It will wait.
Water           Time for a glass of water.
Prayer          Maghrib in 12 min.
Weather error   No weather yet. Trying again soon.
Nothing playing Nothing playing.        (the eyes do the rest)
Empty todo      No tasks. A quiet day.
Limits error    Allow ROQA to run agy in Settings.   (keep factual)
```

## 9. Five themes, one system
Constant across themes: shape grammar, curves, mascot parameters, icon style, type scale, accent hue, copy.
Changing per theme: surface and ink only.
| theme | body | tile | ink | accent used for eyes/strokes | extra |
|---|---|---|---|---|---|
| Classic | `#000000` | `#3A3A40` | `#FFFFFF` | `#FFB454` | notch shadow 0.9 |
| Midnight | `#0B1020` | `#283254` | `#E8EEFF` | `#FFB454` | shadow 0.9 |
| Light | `#ECEEF2` | `#C6CAD1` | `#14161A` | `#9A5200` | shadow 0.55, ink on accent `#FFFFFF` |
| Frosted tint | rgba(16,20,32,.74) | rgba(255,255,255,.16) | `#FFFFFF` | `#FFB454` | rim alpha .70/.14, baked, never live blur |
| Liquid glass | rgba(16,24,42,.76) with sheen (top rgba(24,32,56,.78) bottom rgba(14,20,38,.74), spec .16) | rgba(255,255,255,.16) | `#FFFFFF` | `#FFB454` | rim .95/.16 |
`palette.accent` today equals ink (white) in all themes: change `accent` to Lamp (Light: `#9A5200`) and keep `ink` as is. "Match system accent" (`match_accent`) stays a user option and overrides eyes/strokes only.

## 10. Do not
Purple gradients, glowing orbs, shockwave rings, decorative particles, em dashes, eyebrow labels, exclamation marks, uppercase micro labels, a new always-on timer or thread, Nusa's cyan-to-indigo bottom glow or its hand glyph.

## 11. Apply order (for Phase C, smallest first)
1. palette.accent + eyes colour. 2. Curves renamed/retuned (`pop`). 3. Collapsed ring for running timer. 4. Chip/button grammar (filled capsules, no outline pills) on Timer/Focus. 5. Rail as hanging-handle tabs. 6. Icon pass. 7. Settings window to one icon style. 8. Prayer sun arc. 9. Mascot states table.
