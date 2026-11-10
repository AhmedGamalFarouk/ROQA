# M3 resilience candidate 1: sleep/resume, lock/unlock, Explorer restart

- **Author.** A Claude Opus 5.5 High worker, launched by the ROQA Chief Thread at the user's request ("continue the work"). The author approves nothing.
- **Base.** `26cbb0a` (origin/main), branch `feat/m3-resilience`. Uncommitted, not frozen for milestone acceptance.
- **Scope.** PLAN.md M3: "resume and Explorer restart resilience in coexistence mode … Exercise lock/unlock". Graphics device loss and DWM previews are separate later slices and are not covered here.
- **Changed files.**
  - `src/native_window.rs`: +57/-8, including a new unit test.
  - `src/host.rs`: +3/-2, comments only.
- **Final manifest.** `artifacts/m3-resilience-1/round-3/inputs.sha256`, 9 entries, SHA-256 `8D25DB6E22BBB2B646EC9998EB2C20AFF415042A57B0A22EB95E080320016612`.

## What breaks in each case

Each claim below rests on Microsoft Learn text quoted during this slice, on the code, or on a read-only query. No native run was made.

### Sleep/resume

- **Wait timeouts stop while asleep.**
  - MS Learn, `MsgWaitForMultipleObjectsEx`: "Windows 8 and newer … The dwMilliseconds value does not include time spent in low-power states."
  - So after a resume, the idle loop's next timed wake is delayed by the time spent asleep.
- **The clock deadline does count sleep.**
  - Rust's `Instant` on Windows is QPC. MS Learn ("Acquiring high-resolution time stamps") says QPC counts ticks "including the time when the machine was in a sleep state such as standby, hibernate, or connected standby."
  - Any loop wake after a resume therefore sees `clock_due` and the catalog's 30 s repair deadline as already elapsed.
- **Resume itself wakes the loop.**
  - MS Learn: `PBT_APMRESUMEAUTOMATIC` "is delivered every time the system resumes", through `WM_POWERBROADCAST` to the window procedure.
  - This sent message wakes the `QS_ALLINPUT` wait whether or not ROQA handles it.
- **Conclusion: by documentation nothing goes stale.** Round-2 reviewer M3R2-OPUS-NIT-1 corrected the author's first claim of an up-to-60 s stale minute.
- **The handler is cheap insurance on that wake.** Forwarding resume to `REPOSITION`:
  - sets `clock_due = now`, so the minute refreshes even if this hardware's monotonic clock did not count the sleep (not measured);
  - requests one coalesced topology read, for monitors that changed while asleep.
  - Cost: nothing at idle; on each resume, one post per surface and one topology read.
- **Machine coverage.** `powercfg /a` (read-only) lists only Standby (S3). Modern Standby (S0ix), hibernate and hybrid sleep are not available here and are not covered.

### Lock/unlock

- **No documented failure was found, so no handler was added.**
  - The system does not suspend while locked, so waits, clock and catalog keep running.
  - WinEvent hooks (`idProcess = 0`) cover "all processes on the current desktop". App windows stay on the Default desktop while Winlogon's secure desktop is shown.
  - Z-order and positions are not changed by a desktop switch.
  - While locked, `GetForegroundWindow` yields no foreign candidate, so fullscreen hiding stays off.
- **`WTSRegisterSessionNotification` is deliberately absent.**
  - MS Learn states no privilege requirement for it, but every registration "must call WTSUnRegisterSessionNotification before being destroyed".
  - Surfaces are created and destroyed per monitor, so this needs an owner window. That is code with no observed failure to fix.
  - It becomes the follow-up only if native lock/unlock evidence shows a failure, such as a stuck hover or a stale clock.
- **UNVERIFIED.** Whether `WM_MOUSELEAVE` arrives across the desktop switch is undocumented. A notch hovered at Win+L may stay expanded until the next pointer move. This is a pass criterion below.

### Explorer restart

- **WinEvent hooks survive.**
  - MS Learn, `UnhookWinEvent`: "If the client's thread ends, the system automatically calls this function". `SetWinEventHook`: out-of-context events are "delivered on the same thread that called SetWinEventHook."
  - The hooks belong to ROQA's thread, not Explorer's.
  - Explorer's destroy/create burst flows through the existing ring. On overflow it triggers a full scan, and the 30 s repair remains.
- **ROQA-owned state is unaffected.** Composition, UIA providers and HWNDs are all owned by ROQA, and DWM is not restarted. ROQA has no tray icon, so nothing must be re-added.
- **The work area can change.**
  - Recreating the taskbar can change the work area. Whether Explorer broadcasts `WM_SETTINGCHANGE(SPI_SETWORKAREA)` when it does is not documented.
  - MS Learn, Taskbar: "When the taskbar is created, it registers a message with the TaskbarCreated string and then broadcasts this message to all top-level windows." On Windows 10 and later it is also sent when the primary display's DPI changes.
  - That documented signal now also requests a topology read. An unchanged topology is a no-op diff.
- **Fullscreen detection.** While Explorer is down, `SHQueryUserNotificationState` may fail. A failure already reads as "not full screen", so surfaces stay visible.

### What the existing `REPOSITION` path already covers

`REPOSITION` already coalesces every surface's post into one topology read: an unchanged monitor keeps its HWNDs, and a transient or failed read gets a bounded 8 × 250 ms retry. It also resets the clock. This slice only routes two more documented broadcasts into it.

## Change

- **`src/native_window.rs`:**
  - **New pure predicate `repositions(message, wparam, taskbar_created) -> bool`.** It is true for:
    - the existing `WM_DPICHANGED`, `WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE` and `WM_TIMECHANGE`;
    - `WM_POWERBROADCAST` with `PBT_APMRESUMEAUTOMATIC`;
    - the registered `TaskbarCreated` id. The id must be non-zero, so a failed registration never matches `WM_NULL`.
  - **`TaskbarCreated` registration.** `RegisterWindowMessageW(w!("TaskbarCreated"))` runs once in `WindowClass::register` and is stored in a `static AtomicU32`.
    - `Relaxed` ordering suffices: the store and loads are on the UI thread, and the store happens before any window exists.
    - Registered messages need no unregistration.
  - **`dispatch` arm.** One guard arm `_ if repositions(...)` posts `REPOSITION` with `WPARAM(0)`. It returns `TRUE` for `WM_POWERBROADCAST`, as MS Learn asks, and 0 otherwise as before.
    - Other power events still go to `DefWindowProcW`.
    - Registered ids are 0xC000–0xFFFF, so the arm cannot shadow `WM_GETOBJECT` or `PANIC_PROBE`.
    - The arm stays inside the existing `catch_unwind`, and the predicate cannot panic.
  - **Dead data removed** (M3R1-OPUS-NIT-1): the `dpi` wParam posted with `REPOSITION` is gone. Its only consumer never read it.
  - **New test** `repositions_on_display_resume_and_taskbar_recreation_only`.
- **`src/host.rs`:** the two comments in `Desktop::message` now name resume and taskbar recreation.
- **Not added:**
  - no timer, thread, poll, hook, session registration or catalog API;
  - no crate feature: the constants and `RegisterWindowMessageW` are in the already-enabled `Win32_UI_WindowsAndMessaging`.
  - Deferred comment nits M3SR-OPUS-NIT-1 and -2 stay deferred, because those `host.rs` spots were not touched.
- **Footprint.**
  - Zero added idle wakes, because both new triggers are system broadcasts that already wake the loop.
  - Per message, the cost is one relaxed atomic load plus a small match.
  - Per resume or Explorer restart, the cost is N posts (one per surface) and one topology read, plus a rebuild only for monitors whose work area actually changed.

## Verification (author, pinned `scripts/verify.ps1`)

| Round | Evidence | Result | Release `roqa.exe` SHA-256 |
| --- | --- | --- | --- |
| 1 | `artifacts/m3-resilience-1/verification/` | PASS (fmt, clippy -D warnings, 30 tests, locked release build) | `880EBE8BA75E96B8EFE06092892B5BC5197FF06868A1B63AA32CBBE0DD97F048` |
| 2 | `.../verification-2/` | PASS, 30 tests | `A6F3FDF240D2A4672FD858058620AE46CF3E0EB5E5D1DD247F35E9876547F152` |
| 3 (final) | `.../verification-3/` | PASS, **30 passed, 0 failed** | `84641A1917AEDBB0EE6BB22C9AE7D832D4389513898A2D2707174EFBF302C4F0` |

- **Round-1 formatting fix.** The first round-1 run failed on rustfmt only; `cargo fmt` (pinned) fixed it before the PASS above.
- **Pre-checks.** Before every build, no `roqa.exe` and no `measure-idle` or `idle-matrix` PowerShell was running.
- **Toolchain.** `.tools` is a git-ignored directory junction to the root checkout's pinned toolchain. Nothing was downloaded, and PATH is unchanged.
- **Line endings.** The index is LF (`core.autocrlf=true`). On disk, `host.rs` is CRLF and `native_window.rs` is LF after rustfmt. Manifest hashes are of the bytes on disk.

## Independent review (Opus 5.5 High, clean-code-guard, read-only, a fresh task each round)

| Round | Manifest SHA-256 | Verdict | Report | Task |
| --- | --- | --- | --- | --- |
| 1 | `110C965303475B403F7433E4B563F2F550E732BACDBA456CA35F15D638B7D61D` | ACCEPT, 1 optional nit | `artifacts/m3-resilience-1/round-1/review-opus-1.md` | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A6537233b-6a37-4751-9cec-4ab3c3cd319b%3Adelegate-task%3Aaeef81e2-46f2-4144-bb13-60b75e6ed705> |
| 2 | `F38299F0A9F2D331FE21226B169D8CBE4394F63E7EAADBE56D27EC847853A4AE` | ACCEPT, 2 nits | `artifacts/m3-resilience-1/round-2/review-opus-2.md` | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A6537233b-6a37-4751-9cec-4ab3c3cd319b%3Adelegate-task%3Ac64f9d5c-1b98-492b-84ae-d4472fd74459> |
| 3 | `8D25DB6E22BBB2B646EC9998EB2C20AFF415042A57B0A22EB95E080320016612` | **ACCEPT, 0 findings** | `artifacts/m3-resilience-1/round-3/review-opus-3.md` | <t3-thread://v2/20b131d6-becb-42ea-b20c-104902347b7e/thread:delegated-task:command%3Amcp%3A6537233b-6a37-4751-9cec-4ab3c3cd319b%3Adelegate-task%3A25ed71e4-41d7-4c8b-8e9d-f972761ed768> |

In every round the reviewer:
- verified all 9 manifest entries at both start and end;
- confirmed `git diff` was byte-equal to `candidate.diff`;
- ran `scripts/check.ps1`, which exited 0 with 30 tests passing.

No reviewer ran the release build. Per the user's budget, Sol was not used.

### Findings and dispositions

- **M3R1-OPUS-NIT-1:** the `dpi` wParam on `REPOSITION` was unused. **Fixed** in round 2, by posting `WPARAM(0)`.
- **M3R2-OPUS-NIT-1:** the `repositions` doc comment claimed the pre-sleep minute would otherwise persist, contradicting the QPC note in `host.rs`. **Fixed** in round 3 with the reviewer's reasoning, and round 3 confirmed it.
- **M3R2-OPUS-NIT-2** (optional): a failed `TaskbarCreated` registration is silent. **Declined.** The failure needs atom-table exhaustion, and the fallback (work-area and display messages) is documented at the store. Round 3 agreed.
- **Round-3 process note:** PLAN's packet rule asks for hashes of `docs/council/decision.md` and `docs/council/assessment-notes.md`. This slice manifest omits them. The milestone packet must include them.

## Native evidence plan (NOT RUN)

Every case below runs on the final binary `84641A19…C4F0`, or on a rebuild whose hash is recorded. Each case needs:
- **a fresh desktop-free confirmation from the user;**
- the existing read-only pre-check: foreground, input-idle time, and no ROQA, cargo or measurement process running;
- a graceful `--quit` at the end.

Evidence goes under `artifacts/m3-resilience-1/native-*`.

Baseline for every case, recorded before and after:
- the work area of each monitor;
- surfaces: HWND, rect, visible and topmost, for each monitor;
- the clock text;
- the catalog diagnostics: `hooks_live`, `generation` and `reconciliations`;
- private bytes, handles, GDI and USER counts.

1. **Sleep/resume (S3).** The user starts sleep and wakes the machine. Sleep is a power-state action, so it needs explicit user consent for that run.
   - Same HWNDs, or one rebuild per monitor whose work area changed.
   - Notch minute correct within 2 s of the desktop appearing.
   - `hooks_live` unchanged.
   - Catalog generation advances after the next window change.
   - Taskbar and work area unchanged.
   - No added steady-state wakes afterwards: one 30 s idle sample before and after, reported inconclusive below the measurement floor.
   - Exit 0.
2. **Lock/unlock.** Run it twice: once with the pointer over the notch at Win+L, and once with the display allowed to turn off while locked for at least 2 minutes.
   - Surfaces are still visible, at the same rects and topmost, on every monitor.
   - The notch is collapsed after unlock, with no stuck hover, or it collapses on the first pointer move. Record which.
   - Clock correct.
   - Dock items still track window changes.
   - Monitor removal while the display was off, if it happens, is recovered.
   - No leaked handles or GDI objects.
   - A failure here is the trigger for the `WTSRegisterSessionNotification` follow-up.
3. **Explorer restart.** This is a **shell-affecting action and needs separate explicit user authorization**, beyond the desktop-free confirmation. For example: the user restarts Explorer from Task Manager. The worker must not do it unasked.
   - ROQA keeps running, with exit 0 at the end.
   - Native taskbar restored and untouched by ROQA.
   - Final work area per monitor equals the baseline.
   - Dock and notch end flush within that work area, with no stranded or offset surface.
   - `hooks_live` unchanged.
   - The dock repopulates with the same window groups within about 30 s.
   - Rebuilds and handles counted. Any brief overlap or flicker is recorded, not hidden.
4. **Regression.** The existing suites rerun on this binary with no new failures: `topology-check.ps1`, `smoke.ps1`, `picker-smoke.ps1`, `catalog-smoke.ps1` and `packaged-hung-check.ps1`.

## Limits and unknowns

- **NOT RUN:** all of the native cases above, idle and trace measurement, the release-build hash check by a reviewer, the council (Sol High plus Opus High), accessibility, and Windows 10.
- **Not covered:** Modern Standby (S0ix), hibernate, hybrid sleep, graphics-device loss after resume (separate slice), DWM, GPU and power.
- **UNVERIFIED:**
  - whether QPC counted S3 time on this hardware;
  - when `TaskbarCreated` arrives relative to the work area being restored;
  - `WM_MOUSELEAVE` delivery across a desktop switch.
- **Existing risk, unchanged.** A resume is when displays are most likely to be in flux. If every placement fails through 8 retries and no surface is left, the existing orderly-exit path ends the host. A transient read never releases surfaces, so this needs placement failures on every monitor.
