# Dock brand review and folders

Source review against the approved `cook/brand/BRAND.md`, board, C12 dock reference,
and the Nusa rhythm references. This is a code/reference review, not a new visual
capture or a measured visual score. No screenshots or unit-test executions were used.

## Evaluation

| Area | Existing dock | Applied direction |
| --- | --- | --- |
| Silhouette | Floating capsule and flush Attached geometry already fit | Preserve both, including glass material and attached corners |
| Rhythm | 56 DIP cells, 40 DIP app icons, 8 DIP padding | Preserve the established baseline and sub-pixel hover lift; picker chrome uses a quieter 24 DIP glyph |
| Tiles | Neutral fallback discs and hover capsules | Keep concentric capsule tiles; folder previews use the same inset geometry |
| Icons | Apps glyph resembled a sharp window | Rounded four-cell glyph in the existing single-stroke icon renderer |
| State | White active marker, no inactive running marker, red notification badges | Lamp active capsule, neutral running dot, high-contrast neutral notification capsules |
| Labels | Native system tooltip styling | Theme-aware capsule labels for buttons, picker, and idle pins; running-app DWM previews remain unchanged |
| Context | Small-radius list rows | Filled capsule rows, rounded container, themed native rename field |
| Motion | Existing smooth pop lift | Retained; folder pop-in and settle-out share the motion clock, with instant reduced-motion transitions |

No mascot decoration is added to app tiles. The mascot remains the notch's companion,
not a competing dock launcher. App-owned icon colours and live preview contents are
not recoloured.

## Folder interaction

- Drag an app onto the centre of another app or folder to group it. The outer part
  of a cell reorders instead. Existing folders move as a group.
- An app's context menu offers Add to folder. The next list offers New folder with
  an app or Add to an existing named folder. Grouping also pins running launchers.
- A folder shows its first four app icons in a 2 by 2 preview. Its name and members
  appear above the dock after the same short hover intent as other dock previews.
- Click the folder to open all members. Running apps are focused, never minimized
  or started again merely because the folder was clicked. This also accounts for
  running folder members on other virtual desktops.
- Click a popover member to open just that app. Right-click it to remove it from
  the folder or use its ordinary app actions. Removing a member leaves it pinned.
- The folder context menu offers Rename and Ungroup apps. Rename uses a standard
  native text control and a pointer-operated Save name action, without any new
  keyboard, hotkey, or UIA handlers.
- One remaining member dissolves a folder. Pins are not lost. Names are bounded
  to 40 characters, membership to the existing 32-pin cap, and nesting is excluded.
- Long member lists scroll inside the available work-area height.

## Persistence and scheduling

The existing version-2 store retains its ordered `pin=` entries. Bounded `folder=`
records add a name and tab-separated canonical pin keys. Missing pins, duplicate
members, singleton groups, and malformed identities are dropped during loading.
Folders follow executable pins even when the catalog upgrades them to AUMIDs.

There is no new worker or periodic timer. Hover intent and the single 180 ms leave
deadline feed the existing blocking host wait. Frames run only for pop/settle
transitions, stop at the final frame, and are instant under reduced motion. The
popover stays open over either the source tile or its own window. Theme rebuilds,
hidden owners, and pin/catalog changes discard stale popovers.

## Validation limits

Layout and folder logic tests are added and compiled by all-targets clippy, but are
not run, per the standing rule. Interactive desktop behaviour and visual results
are not claimed as verified. Ahmed approved a size baseline of 1,182,000 bytes on
2026-11-14 for the dock refresh and folders.
