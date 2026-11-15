# ROQA r09: ranked remaining defects

Evaluation only. Build 1,183,232 B, limit 1,205,640 B (headroom 22,408 B). Mean 6.94 (r07 7.60), 0/6 dimensions at 8.5. Scores are eyeballed critic medians, not measurements. Stills: cook/stills/r09, comparisons: cook/compare/r09. Cost classes are unmeasured risk, not byte promises: C = constants/layout/styling, S = reuse existing plumbing, M = new machinery (avoid).

| Rank | Sev | Surface | Still | Issue | Suggested fix | Size |
|---:|---|---|---|---|---|---|
| 1 | P1 | Limits | 10-limits-rest.png (compare 09) | Codex/Antigravity rows draw empty bars with right-edge dashes clipped at the panel edge; status text sits on the bar row; stray white dot after Claude weekly 2% | Hide bars when no data; one message line per provider; remove the dot; keep the value column inside the inset | C |
| 2 | P2 | Tune disc, all notch tabs | 10-weather-rest, 10-shelf-rest, 10-limits-rest | Corner disc crowds content (forecast card, shelf edge, Antigravity row) | Reserve a bottom-right safe area or move it into the header row | C |
| 3 | P2 | Notch headers | 10-media-rest, 10-weather-rest, 10-shelf-rest | Media has no title, Shelf has segmented tabs + floating plus, others title+clock; clock repeated on every tab | One header grammar: title left, context action right | C |
| 4 | P2 | Themes | 32-midnight / frosted-tint / liquid-glass-prayer | Bodies #0B1020, ~#111521, ~#121A2C read as one navy | Separate rim/sheen/tint with the existing renderer; no real blur | C/S |
| 5 | P2 | Dock family | 31-*-dock, 40-44 | No amber anywhere: grey running dots, thin glyphs, black popovers with small icons and no hover rows | Amber running dot, shared popover radius/inset, hover fill, larger icons | C/S |
| 6 | P2 | Settings chrome and icons | 20-settings-general, 33-*-settings | Stock title bar; sidebar glyphs vary in weight (Notch play box, About) | Re-trace glyphs on the shared stroke; skip a custom frame (M) | C/S |
| 7 | P2 | Timer running | 11-timer-running | 5:00 with an empty bar reads unstarted; Cancel floats; Focus chip styled as CTA among presets | Fill bar from start, right-align Cancel on the action grid, distinguish selected preset | C |
| 8 | P2 | Folder tile | 40-*-folder-closed, 41-*-folder-hover | Three minis still a cramped collage beside squircle tiles | 2x2 inset grid on a squircle | C |
| 9 | P3 | Settings display cards | 20-settings-general | Three equal-weight lines, no hierarchy, unexplained amber dot | Semibold name, quiet secondary line | C |
| 10 | P3 | Mascot wake end pose | 50-wake-contact (compare 18) | Half-dome eyes read as clipped at the bottom edge; doze end resembles idle squash. May be intentional happy arc | Lift 2 px or add a lid cue, only if the owner agrees it reads wrong | C |
| 11 | P3 | Empty states | 10-media-rest, 10-widgets-rest | Floating "Nothing playing"; empty battery ring | Muted glyph + one line; quiet the unavailable ring | C |
| 12 | P3 | Settings toggle knobs | 33-light-settings | Dark knob in Light, white in dark | One knob rule | C |
| 13 | P3 | Context menu / hover label | 44-*-context-menu, 43-*-hover-label | Blank left gutter; label floats above the bar | Tighter inset, align to icon | C |

## Rejected critic claims
Mascot absent from collapsed tabs (idle-triggered by design); Light amber muddy (deliberate #9A5200); Prayer double highlight (past vs next).

## r07 top-10 verification (r07 vs r09 stills)
Fixed: 1 selection patch, 2 prayer overlap, 3 clock/eyes, 4 folder names, 5 settings controls, 6 settings icons, 7 theme parity, 8 search label. Partly: 9 Limits (rank 1), 10 folder spacing (rank 8).

## Not captured
Empty Media, running-app previews, second monitor, other Settings pages in the critic set, populated Shelf, idle CPU, FPS, accessibility.
