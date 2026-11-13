# U6 climb (council-6 round-3 defects; judges 7.6 / 8.2 / 7.8, mean 7.87, target 8.0)

Native/GUI NOT RUN; the Chief's capture worker verifies by eye. Reviews paused.

## U6-A Glass isolation (defect 1)

- The Frosted/Liquid content backing now follows the body's own outline inset only `layout::BACKING_INSET` (2 DIP, bottom corner radius 24 - 2) instead of 9 DIP, at alpha 0.97; the tab rail uses the same 0.97 (was 0.90), so backdrop text no longer shows through either. Only an unoccupied 2 DIP border stays glass.
- Plain panel 86 -> 92 DIP. Footers (Weather credit, Hijri date) come from `layout::footer` (a step larger, 11 DIP, bottom 12 DIP above the body's bottom); the gear moved to 16 from the right / 10 from the bottom. Test `every_footer_and_the_gear_sit_six_dips_inside_the_glass_backing` measures each footer corner and the gear's disc against the rounded backing (>= 6 DIP) for both panels.
