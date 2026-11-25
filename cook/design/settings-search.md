# Settings search

**Design handoff · Operate.** Find a setting and land on its original control without changing it. Keep the frame, native controls and apply-as-you-go behavior in [Settings redesign](settings-redesign.md). [Mock still](settings-search.svg): Classic, 880 × 680 client DIPs.

## Layout

- Wide: one 232 × 32 DIP native single-line edit at the trailing edge of the heading (x 610, y 24 at 880 × 680). Reserve its width plus 16 DIP from the title; the caption keeps full width. Compact: full pane width below the caption (below the picker when the caption is absent), above tabs; reserve a 40 DIP band. Also stack on wide layouts if title and field cannot fit. Keep the edit in place during search.
- Placeholder: **Search settings**. Reuse Settings edit styling, caret, focus ring and contrast colors. A neutral 20 DIP magnifier is not a button. Empty or whitespace-only query leaves the original page and tabs visible. No extra window or motion.
- A nonempty trimmed query replaces the page body and hides local tabs, reusing the standard no-tabs viewport and scrollbar gutter. Keep the underlying page heading, rail/picker, footer and Close. Use one group surface with 52 DIP minimum result rows: exact control label at 13/18 semibold, route then local tab at 12/16 secondary. Add group only to disambiguate identical results. Wrap on compact layouts. Active result uses neutral rounded fill and the existing selected dot. Show **No settings found.** in the viewport if empty.

## Matching and live data

- Derive one result per actual control from ROUTES, PAGES, their groups and rows. Skip text-only rows, include disabled controls. Split the trimmed query on whitespace; every term must occur case-insensitively in its label, route, tab or group. Rank exact label, then all terms in label, then path-assisted matches. Break ties by Settings order. No captions, option values, help text, fuzzy match, aliases or history.
- Use existing control IDs. For changing actions such as agent hooks, match the stable setting label, not progress/status text. App pickers are findable by their labels, not the app names or paths inside them.
- Include connected Displays controls by their visible identity (for example, **Display 2**), in on-page order; exclude device IDs and mode values from matching. Resolve by device key, not reusable tile index. Use existing topology/control rebuild events to refresh; keep the active target if it survives, otherwise select the first match. If activation finds that a target vanished, refresh and stay in search instead of choosing another. Recompute on events only; no timers or workers.

## Interaction

- Click the edit or press **Ctrl+F** to focus it; the shortcut selects existing query text. Tab reaches search after the rail/picker and before tabs/page controls. While results replace the page, search is one Tab stop: Tab moves to Close and Shift+Tab returns to navigation. Preserve native edit shortcuts, IME and open native dropdown key handling.
- Query edits reset selection to the first match and result scroll to the top. In the edit, Up/Down move the active result without wrapping and scroll it into view. Enter opens that result; clicking anywhere on a row opens that exact result. Hover does not move the keyboard selection. Enter with no match does nothing; Enter in the search edit never becomes dialog OK.
- Deleting the query returns to the same page, scroll and footer status. Esc with a nonempty trimmed query does the same and focuses search; Esc otherwise keeps the existing Close behavior. Explicit category navigation clears search and navigates normally. Closing Settings discards the query. Result scrolling never overwrites remembered page scroll.
- On activation, clear search and open the exact route and local tab. Focus the existing enabled control and use the existing focused-row reveal, even on the same page. Consume the search gesture; do not invoke, toggle or open the control. If disabled, reveal its row and focus the selected local tab, or the rail/picker on an untabbed page. Normal Settings input resumes after landing.

**Acceptance:** Search **sound** across General, Timers & water → Timer and Prayer. Opening **Finish sound** leaves the switch unchanged. Escape restores the scrolled page. No-match Enter does not close Settings. Unplugging a matched display cannot select its replacement. Repeat at compact size and in a contrast theme.
