# Open file location

One secondary action in the dock app right-click menu. It opens File Explorer and selects the executable, without launching the app. Borrow only the disclosure principle from LumaBar: show the action where it applies, not as a disabled promise elsewhere.

## Menu

- Show **Open file location** for a top-level app tile whose existing identity is a resolved absolute path to a local `.exe`. Running, pinned-but-closed and recent apps follow the same rule. Multiple windows for one executable can qualify; a dock folder cannot.
- Omit it for packaged/app-ID identities (even when process metadata includes an executable), unresolved or known-unavailable targets, relative paths, links, shell targets, UNC and known network mappings. No change to folder-member, folder, Shelf, Windows/quick links, empty dock or notch menus. **New instance** alone does not establish eligibility.
- Insert directly after **New instance**, before window and dock-management actions. No blank space, disabled row, divider, loading placeholder or mid-open insertion. Keep every other row in order.
- Use the existing text-only row: 34 DIP tall, 6 DIP list inset, 12 DIP text inset within the row, 13 DIP Segoe UI Variable Text. Same 250 DIP panel, hover capsule, hit area, scrolling and dismissal. No icon, badge, path, tooltip, submenu, custom accent or new motion. All five themes inherit their existing menu palette.

## Selection and ownership

| State | Behavior |
| --- | --- |
| Still the same target | Close the menu through the existing selection flow. Validate the captured executable off the UI thread and ask Explorer to select that exact file; never refocus the old window after Explorer opens. |
| Slot, owner or resolved target changed | Dismiss via the existing menu refresh. Reject stale activation even if it arrives before the refresh. Never retarget to the app now in that slot or fall back to a window's process path. |
| File vanished, is inaccessible or nonlocal, or Explorer refused | No substitute, directory-only fallback or app launch. Use the existing **Windows couldn't open that.** notice for a valid click that fails. Stale menu dismissal is silent. |

Build the menu from resolved in-memory identity, not a disk probe. A stored executable path can be stale without ROQA knowing yet: recheck existence, file type and locality on activation, including drive mappings and redirected paths. Fail closed if locality cannot be established. Reuse on-demand worker infrastructure for filesystem and shell work; no UI-thread waits, polling, new persistent worker or persistence.

Keep the clicked dock HWND as owner from opening through dismissal. Use the current center-over-icon placement, originating monitor work-area clamp and DPI behavior. Release ownership on selection, dismissal or invalidation; full-screen and explicit visibility still win. Explorer is a separate Windows window, not an interaction owner. Neither the request nor Explorer extends the dock tuck hold; do not reposition Explorer.

## Evidence and review

- Checked `dock_menu::Item`, `items`, `describes`, `Desktop::open_item_menu`, `run_menu`, and `refresh_menu`: no file-location row or Explorer-select action found. `pins::launch_target` also accepts packaged app IDs, so it is not the eligibility predicate.
- [Classic still](open-file-location/classic.png) and [Light still](open-file-location/light.png): current menu at left, eligible menu with hover at center, packaged app with the row omitted at right. Identical 96 DPI crop and dock anchor. Dock icons are illustrative.
- Offline self-review against the current renderer and order: an initial append would shift every existing row relative to the dock. Inserting beside **New instance** preserves the close, top and pin controls at their familiar positions when unclamped. Static mocks are not Direct2D captures; no live or independent critique and no proof of Explorer behavior.

## Rust handoff

Read `.agents/skills/rust-best-practices/SKILL.md`. Keep the row in the existing menu and slot/identity guard. Cover running and closed local apps, packaged and unresolved apps, folder-member exclusion, stale slot/path, vanished file and nonlocal aliases. Check Explorer selection, failure feedback, foreground and tuck on the second monitor, including clamping near the work-area edge. Run `scripts\verify.ps1` after implementation.
