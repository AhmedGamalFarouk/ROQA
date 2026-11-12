# F3: prayer alert over full screen, independent Weather (2026-11-12)

Branch `feat/f3-dock`. No native/GUI run; drawing over a real full-screen app is NOT RUN.

1. **Prayer alerts always fire.** `fire_prayer` plays the sound first, whatever the notch state. A notch that a full-screen app hides is painted, shown (`SW_SHOWNOACTIVATE`, already topmost and `WS_EX_NOACTIVATE`) and flagged `overlay`; `animate` hides it again once it has retracted (`close_alert` after the 8 s notice) and `refresh_fullscreen` clears the flag. Other notices (timer, power) keep their suppression. The "woke more than 2 min late" rule (`prayer_tab::GRACE`) is unchanged. Limit: the overlay is a normal notch window, so a pointer over its strip still hovers it (keeps it open); it takes no focus.
2. **Weather independent of Prayer.** The audit found the feed already ran with only Weather on; the changes: `weather::feed_wanted` (Weather on, or Prayer on with no place yet), so Prayer alone does one place lookup and never refreshes the report, and Weather alone never needs Prayer. City/detect settings stay shared (one place); the per-tab flags are separate. Settings label "Prayer tab (coming soon)" corrected.

Tests: `feed_wanted` truth table, tab flags independent both ways.
