# U4 polish (council-4 defects)

Branch `feat/u4-polish`. Native/GUI NOT RUN; the Chief's capture worker verifies by eye.

1. Shelf icons: `icon_size()` was `None` without a dock, so a notch-only desktop never asked for shell icons (all slots kept the File glyph). It now falls back to the notch DPI (`icon_dpi`, tested). An icon that arrives while the Shelf page is shut is kept as `stale` so the next open draws it.
2. Limits: panel 360 wide, 20 DIP rows, 11.5 DIP text, bar 14 + percent + reset in separate columns with gaps, "as of" at the right edge; Refresh pill 1 DIP shorter. Height is `38 + 20 x rows + 22` (rows are always the three tools, so it only changes with the row count).
3. Dock hover: the pop curve (0.25, 1.6) finished in about 40 ms; now (0.3, 0.8, 0.5, 1.2) over 260 ms in, 220 ms out, lift 5 DIP. Test: no 16 ms frame covers a quarter of the travel.
4. Settings: edits lose the client edge and sit in a parent-drawn 1 px dark frame; drop-downs are clipped 1 px inside the same frame (hides the theme's bright border). Edit text gets 7 px side margins. Page fade for Settings NOT done (native child controls, no cheap fade).
5. Light: faint ink and glyph alphas lifted (`ink_alpha` curve), 2.6x firmer edge, shadow 0.55. Frosted and Liquid: tint kept at the 7:1 readability test (alpha 0.74 to 0.78), a dark 1 px edge under a brighter rim, specular and rim raised. No DWM backdrop.
6. Eyes 9.5 x 18 DIP, gap 25, look +-4, wider Happy, shorter Surprised. Notice fades out (its text is kept for the exit); the leaving page fades and slides out as the new one comes in (Media leaving is not animated).
7. Edge slider shows its percent at rest.
