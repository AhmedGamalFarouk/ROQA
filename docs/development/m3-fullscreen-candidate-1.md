# M3 fullscreen candidate 1: per-monitor suppression

Author: Claude Opus 5.5 High (primary agent), 2026-11-10. Base commit 8a8c7ed. Not committed and not frozen. The council stays deferred and accessibility checks are skipped (NOT RUN, not passed). The user asked to hold off on native tests, so none were run.

## Change

Surfaces are `WS_EX_TOPMOST`, so before this change they covered full-screen apps.

- **`native_window::fullscreen_device()`**
  - Reads `SHQueryUserNotificationState`. Only `QUNS_BUSY` and `QUNS_RUNNING_D3D_FULL_SCREEN` count.
  - Takes the foreground window. That window must be visible, must not belong to ROQA (`dock::valid_target`), and must not be the desktop (`Progman`/`WorkerW`).
  - Returns the monitor's device name when the window rect covers that monitor's whole rect (`topology::covers`, tested).
  - Every failed query reads as "not full screen", so surfaces stay visible.
  - Using the shell's state avoids treating maximized windows as full screen when the work area equals the monitor area. This machine has an auto-hide taskbar.
- **Host**
  - Each `Surface` has a `visible` flag and `set_visible()`, which hides or restores the body and shadow HWNDs without releasing the canvas or UIA.
  - Hiding settles hover through a shared `leave()`.
  - Surfaces are now created hidden and shown after their first paint.
  - `Desktop` holds the currently suppressed monitor. It checks again on each catalog generation change and each topology change, and otherwise every 2 s.
- **Microsoft docs (verified on Learn):**
  - "There are no notifications sent when the user starts or stops a full-screen application." This is the reason for the poll, which is marked with a `shortcut:` comment.
  - `RegisterShellHookWindow` is "not intended for general use", so it was not used.
  - A system-wide `EVENT_OBJECT_LOCATIONCHANGE` hook was also rejected, because it would wake ROQA on every cursor move.

## Verification and review

- `verify.ps1` passed both times (28 tests and the release build). Evidence: `artifacts/m3-fullscreen-1/verification*`. Release binary `1CC88A122F6A87F1B3890769CFE4786FA72331312B0C124641DCF22B5787D49A`.
- Independent clean-code-guard review of `inputs.sha256` (`5BF7C8F2…79B8`): **ACCEPT** with two minors:
  - **M3F1-CCG-MIN-1:** `Surface::create` took a visibility flag, and a comment was wrong.
  - **M3F1-CCG-MIN-2:** under Presentation Settings, a desktop foreground could hide surfaces.
- The author fixed both.
- A fresh recheck of `inputs-2.sha256` (`784CD0E22BF902E2E4653B04777AA16EC6896CDCD5614C3304138EFB56D3DD0E`) returned **ACCEPT**. Both minors are resolved and there are no new findings. The recheck confirmed:
  - showing a window only after its first paint is safe with this composition setup (DirectComposition, layered shadow);
  - the error paths in `place()` stay consistent;
  - the M1 failure probes still work.

## Open

- **No native evidence yet** for:
  - real full-screen video, an F11 browser window or exclusive D3D;
  - a full-screen app on one monitor while the foreground is on another;
  - Presentation Settings;
  - how quickly Explorer updates its state;
  - the idle cost of the 2 s wake.
- **Store/UWP apps** may report `QUNS_APP` and would not be suppressed.
- **Maximized windows under Presentation Settings** can still read as full screen on monitors whose work area equals the monitor area; this is recorded as a `shortcut:`.
- **Other foreground behaviour** (accepted by the review):
  - When the foreground window moves to another monitor, a full-screen app on the first monitor gets ROQA's surfaces back.
  - While the picker has focus, the surfaces also show again.
- **Later slices:** previews, device loss, resume and Explorer restart.
