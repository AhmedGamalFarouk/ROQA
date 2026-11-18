# Code review 2026-11-18

Range: `147952a..6712f90` (HEAD when the review started; 54 commits, waves A and B, the gap fixes, dock changes).
Read-only review. Each finding was checked against the code. Size: S = a few lines, M = one function, L = a redesign.
Assumed load: one person on one PC.

What this range does: adds coding-agent status, progress and Allow/Deny questions (`roqa.exe agent|progress`, hook writer for
seven agents), language and Caps Lock pills, global shortcuts, reminders, stopwatch, drive eject, start at sign-in, per-app
volume, and dock changes (drag to reorder, hidden-icons and Start buttons open Windows' own popups). Removes the picker,
minimized-apps and Your apps.

## Must fix

1. **Language and Caps Lock pills stop for good after any ROQA dialog** (`src/keys.rs:46`, `src/keys.rs:221`, `src/host.rs:6155`)
   - Impact: after one "Add hook" confirm box or file picker, the language pill never shows again until ROQA restarts; Caps Lock pressed while the box is open does not toggle.
   - Why: `LOOK` and the Caps `WM_HOTKEY` are thread messages (no window). A modal loop (`MessageBoxW` in `settings_window.rs:1790`, `IFileOpenDialog` in `shelf_os.rs:231`) drops thread messages. The box itself is a foreground change, so `LOOKING` is set, its message is dropped, and nothing ever clears it. `audio.rs` has a deadline fallback for this; `keys.rs` has none.
   - Fix (S): post `LOOK` to one of ROQA's windows and register the Caps hotkey on that window (window messages survive modal loops), or clear `LOOKING` and call `looked()` on every loop pass.

2. **Hidden-icons button can press Enter in the user's app** (`src/dock.rs:805`, `src/dock.rs:809`)
   - Impact: if the shell is slow or Win+B does not move focus within 200 ms, Enter lands in whatever app was in front (sends a chat message, submits a form).
   - Why: Win+B, a fixed 200 ms sleep, then Enter, with no check of who has the focus. Same path for Ctrl+Alt+B.
   - Fix (S): before Enter, check that the foreground window's class is `Shell_TrayWnd` or `Shell_SecondaryTrayWnd`; otherwise stop.

3. **"Add hook" can overwrite changes made while its dialog was open** (`src/settings_window.rs:1743`, `src/settings_window.rs:1775`, `src/agent_hooks.rs:704`)
   - Impact: anything Claude Code (or the user) writes to `~/.claude/settings.json` between opening the confirm box and clicking OK is lost from the live file (the backup taken at apply time still holds it).
   - Why: the new file text is computed before `MessageBoxW` and written after it, unchanged.
   - Fix (S): after OK, call `plan` again and write only if its `shown` text matches what the user saw; else say "The file changed. Try again."

## Should fix

4. **An Allow click can come from a click meant for something else** (`src/host.rs:4997`)
   - Impact: a permission question that pops up under the pointer as the user clicks the notch can be allowed by accident; Allow is the rightmost button, where other notch controls sit.
   - Fix (S): ignore Allow/Deny clicks in the first ~500 ms after the question shows (the notice start time is already kept).

5. **A symlinked settings file is replaced by a plain copy** (`src/agent_hooks.rs:721`)
   - Impact: users who link `~/.claude/settings.json` (or `.gemini`, `.codex`) to a dotfiles repo lose the link; later edits stop reaching their repo.
   - Fix (S): write to `fs::canonicalize(path)` when the path is a symlink (`symlink_metadata`), so the temp file and rename happen beside the real file.

6. **Ctrl+Alt shortcuts eat AltGr keys on many layouts** (`src/hotkeys.rs:54`)
   - Impact: with the apps group on, German, Polish, French and other layouts can no longer type characters on AltGr+7/8/9 (`{ [ ]`) and similar; Windows treats AltGr as Ctrl+Alt. Off by default.
   - Fix (S): say so on the Settings row, or skip a key whose Ctrl+Alt combination types a character in the current layout (`VkKeyScanEx`/`ToUnicodeEx`).

7. **Start button: a thread polls for 2.5 s and moves any XAML popup** (`src/dock.rs:853`)
   - Impact: breaks the "no polling" rule (100 checks, 25 ms apart, on a second thread). In that window any other visible `Xaml_WindowedPopupClass` (another shell flyout) is moved above the dock too.
   - Fix (M): stop at the first successful placement plus one re-check, or listen for `EVENT_SYSTEM_MENUPOPUPSTART`/`EVENT_OBJECT_LOCATIONCHANGE` on that window (event-driven). Owner call whether the bounded poll is acceptable.

8. **Eject runs on the UI thread** (`src/system_os.rs:89`, called from `src/host.rs:4987`)
   - Impact: lock and dismount flush the drive; on a slow USB stick the dock and notch freeze until it finishes.
   - Fix (M): move the call to the existing icon/window worker pattern, or accept and note it with a `shortcut:` comment.

## Nice to have

9. **Two hook changes in the same second overwrite the first backup** (`src/agent_hooks.rs:709`)
   - Impact: adding "Claude Code" and "Claude Code questions" within one second (same file) keeps only the second backup, so the untouched original is gone. Undo still works.
   - Fix (S): skip the copy when the backup name already exists.

10. **Stale comment from the minimized-apps removal** (`src/dock_menu.rs:237`)
    - Impact: says titles are "the minimized list"; they are now quick settings, Places and folder rows.
    - Fix (S): reword the comment.

## Checked, no finding
- IPC input: names limited to 24 safe characters, spool capped at 4 KB, question text squeezed to one line of 400 chars, stdin capped at 64 KB, JSON parser depth-capped at 32 and file capped at 1 MB. Hook command text is limited to a safe character set (`shell_path`).
- Hook merge/strip: only lines whose command runs `roqa ... agent <id>` are removed; other hooks and keys stay; tests cover add/undo per agent.
- Run key: written only on a switch change; `uninstall.ps1` removes it.
- Store: new `hide=` and `remind=` lines round-trip; labels are encoded.
- No keyboard hook, no new idle timer; stopwatch ticks only while the notch is open; reminders use one deadline.
- No motion curve outside `src/motion.rs`; no new GDI objects; COM interfaces drop on scope exit.
- Removed features: no `ShowPicker`, Your apps or minimized-apps code left (clippy `-D warnings` would flag unused items).

Not checked: live behaviour (no run, per scope); the Caps pill's on/off text right after `SendInput` (may read the old state; needs a live test).
