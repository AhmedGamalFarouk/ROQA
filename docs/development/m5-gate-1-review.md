# M5 gate 1: independent static review

- **Reviewer:** Claude Opus 5.5 High (Tier 1, non-author), clean-code-guard review mode. 2026-11-12.
- **Inputs:** `git diff 65212ac..6292169 -- src/ Cargo.toml` (36 files, +9,304/-376): M5 S1-S7 and S9, themes, readiness fixes, fat LTO, `panic = "abort"`, refresh storm, B1, B2, B2.5, B3 and the prayer engine. PLAN M5 text and the last 60 lines of `progress.md`.
- **Method:** read only. Nothing was built, tested, run or committed.

## Verdict: CHANGES REQUIRED (static code only)

No memory-safety or unwinding defect found. But the edge strips are on by default and will swallow edge clicks and wheel input (I1), and two issues make the M5 footprint gate unmeasurable on HEAD (I2, I3).

## Critical

None.

## Important

- **I1** `src/edge.rs:583,588,600`, `src/native_window.rs:87`, `src/settings.rs:44-45`. Each strip is a plain topmost popup with `WS_EX_NOACTIVATE`. Non-layered windows are hit-tested by their rectangle, not by what they draw, so the 2 px strip receives input. With no canvas, `WM_LBUTTONDOWN` and `WM_MOUSEWHEEL` return false and reach `DefWindowProc`, which drops them. Failure: a user with a maximized browser throws the pointer to the right edge and clicks or scrolls its scrollbar, and nothing happens in the middle third of every monitor. The left edge behaves the same way. Both strips are on by default. The packet marks "strip hit-testing" as NOT RUN. **Fix:** make both sliders opt-in (`false` by default) until a native run proves edge behaviour. In the meantime, have a wheel or click on a bare strip open the pill and apply the input, so the gesture is never silently lost.
- **I2** `src/edge.rs:565,859-866`, `src/host.rs:2495`. `wants_audio` starts Core Audio as soon as any UIA client is listening, and it latches on `touched` after one pass over the right edge. `UiaClientsAreListening` can be true with no screen reader (input services, automation or accessibility tools). So Core Audio's threads and handles load at startup on some machines, and after a stray edge crossing on others. Failure: the M5 idle private-bytes, thread and handle figures depend on the machine and on pointer history, so repeated runs do not match. **Fix:** start audio when `WM_GETOBJECT` reaches a volume strip, or when its pill opens, rather than on the global query or on a bare strip move.
- **I3** `scripts/size-baseline.txt`, `progress.md` (S10, B2, B2.5, B3). The only native numbers (S10) are from `118a6f8`. After that, B1-B3 added two topmost windows and two UIA providers per monitor, a DDC worker thread and +117 KB of binary (803,840 to 921,600 B). The 15-minute idle run aborted. The size ratchet tripped three times and its own authors raised the baseline each time. Failure: no M5 gate in PLAN M5 (private bytes, CPU, churn, DWM, Nectar/Nusa) can be accepted on HEAD, and the ratchet blocks nothing. **Fix:** re-measure the M5 scenarios on HEAD with sliders on and off, and allow a baseline raise only through a gate reviewer's sign-off.

## Minor

- **M1** `src/edge.rs:946-955`, `src/brightness.rs:20,59-61`. `Throttle::take` clears the latest value before `try_send`. When the queue is full (a slow or hung monitor), the final drag value is dropped. The comment's "the next throttled write resends the latest" is false for the last write. **Fix:** have `write` return whether it queued, and push the value back into the throttle when it did not.
- **M2** `src/brightness.rs:56`, `src/edge.rs:505`. A hung DDC call blocks the worker forever, and later reads are dropped once the queue is full. The slider stays `Pending`: an empty, draggable-looking pill with no "not supported" note. **Fix:** treat `Pending` as `Unsupported` after about 2 s while the pill is shown (one deadline).
- **M3** `src/brightness.rs:167`. Only `physical[0]` is read or written. On an HMONITOR with several physical monitors (a duplicated output), only one display changes. **Fix:** write to every entry of `physical`.
- **M4** `src/policy_config.rs:44-46`. The vtable is correct: 10 slots before `SetDefaultEndpoint`, matching the Win7+ layout, with CLSID `870af99c` and IID `f8679f50` as used by EarTrumpet and SoundSwitch. A changed IID fails `QueryInterface` with `E_NOINTERFACE`, which falls back to Settings safely. A same-IID layout change would be undefined behaviour; that risk is accepted by the user decision. Failure: if `eConsole` succeeds and a later role fails, the device is half switched and Settings also opens. **Fix:** treat a successful `eConsole` as success and log the remaining roles.
- **M5** `src/error_log.rs:75-80`. On panic, `TerminateProcess` skips `flush_store(true)`. Pin, shelf, timer and settings edits still within the store delay (up to `MAX_WAIT`, 10 s) are lost. **Fix:** record this as a known limit in `panic-abort-1.md`. Flushing from the hook is not safe.
- **M6** `src/native_window.rs:439-441`. Every ROQA window, now including two strips per monitor, posts `POWER_STATUS` for each `PBT_APMPOWERSTATUSCHANGE`. On a laptop that broadcast also fires on percent changes. The host repeats `GetSystemPowerStatus` once per window and once per wake. **Fix:** coalesce with a pending bit, like the `media` and `audio` reconnect bits.

## Checked and found sound

- **Idle and wake:** power, preview, menu, timers (collapsed: the expiry only), store (after a change only), edge hover and throttle each return `INFINITE` when idle. `Hover::tick` re-arms only while a drag holds the pill. A past deadline after resume fires once, and there is no spin path.
- **Hook:** `try_lock` avoids self-deadlock and poisoning. It logs the location and thread only, never the payload. All four `catch_unwind` sites are gone. `PANIC_PROBE` is debug-only.
- **FFI:** every new `unsafe` block carries a SAFETY comment. `Slider`, `Menu` and `Popup` drop UIA, then canvas, then window, and `DwmUnregisterThumbnail` runs before the window is destroyed. `PROPVARIANT` and `CoTaskMemFree` are each freed once.
- **Bounds:** devices ≤8, shelf ≤12, timers ≤3, store ≤64 KB, error log ≤16 KB, one thumbnail popup and a DDC queue of 8. The DDC `ranges` grow only per distinct work-area centre.

## Not checked

`graphics.rs` paint paths, `settings_window.rs`, `shelf.rs` and `shelf_os.rs` (S6 drop target and dialog STA), `theme.rs` contrast math, `prayer.rs` maths (not wired), the internals of the UIA providers (`notch_uia`, `dock_uia`, `menu_uia`, `edge_uia`) beyond their lock use, the `window_source` virtual-desktop filter, `picker.rs` and `diagnostics.rs`. Not done at all: builds, tests, `verify.ps1`, any native run, hit-testing, DDC, `IPolicyConfig` on this build, and measurements.
