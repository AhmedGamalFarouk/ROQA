# M2b candidate 1: M2b1 fixes + real dock (M2b2) + notch

Author: `/root` (this session took over from the thread that merged the `../roqa-m2b2` working copy into the main tree, which itself continued thread `53d0a5f2-b6cf-4975-ac6e-b7fa1fead022`). Date: 2026-11-09.
Status: author verification complete; frozen for independent review. Per the user's instruction, reviews are **not launched yet**. This candidate accepts no milestone by itself.

## Baseline and scope

M2a is accepted (manifest `1E8AEE10…2247`). M2b1 candidate 1 (manifest `22923E0D…755118`) received an Opus **REVISE** and no Sol review (capacity failure). Those inputs are preserved for audit and can't be accepted unchanged. This candidate supersedes them. It combines the following:

1. **M2b1 picker** (unchanged scope: modeless standard-control picker on the live catalog; see `m2b1-candidate-1.md`) plus fixes for the Opus findings below.
2. **M2b2 real dock:**
   - **Slots:** one slot per real catalog group, plus a leading "All windows" picker slot (slot 0). There are no placeholder rows.
   - **Grouping:** by the window's explicit AppUserModelID, read through `SHGetPropertyStoreForWindow` on the catalog worker. The worker now initializes an MTA and balances it at exit. Grouping falls back to the lowercased executable path, then to `Unavailable(pid)`.
   - **Icons:** taken from a bounded STA worker (`src/icons.rs`). It tries `IShellItemImageFactory` on `shell:AppsFolder\<AUMID>`, then the executable. The queue holds 64 requests and never blocks the UI. The cache holds 256 entries and resets wholesale at that bound; a failure is cached as `None`. Shutdown joins the worker or detaches it after 1 s.
   - **Drawing and size:** D2D draws the slots. The dock resizes in place on the same HWND, with `16+56n` DIP width, clipped to the work area. Clipped slots stay reachable through the picker.
   - **Click:** behaves like the taskbar (`src/dock.rs`). It activates the group, cycles within a focused group, or minimizes a lone focused window. The target is revalidated first (same PID, still a visible window, not ROQA). ROQA never forces focus: a refused activation is left alone.
3. **Dock UI Automation (`src/dock_uia.rs`):**
   - **Tree:** a ToolBar fragment root ("ROQA Dock") on the dock HWND, with one Button per slot.
   - **Buttons:** each has an InvokePattern and is named from real window titles, plus the window count and an "active" marker.
   - **Rectangles:** bounding rectangles come from the live dock geometry.
   - **Runtime IDs:** scoped by layout version. A stale element returns `UIA_E_ELEMENTNOTAVAILABLE`.
   - **Thread safety:** providers read only an `Arc<Mutex>` snapshot. Invoke posts `WM_APP+4` with slot and version, and the host checks the version again.
   - **Teardown:** `UiaReturnRawElementProvider(NULL)` and `UiaDisconnectProvider` run before the HWND is destroyed.
   - **Dependency:** `windows-core =0.62.2` became a direct dependency for `#[implement]`. It was already locked: Cargo.lock gains one dependency line and no new package.
4. **Notch, Nusa-inspired by the user's choice (our own implementation; no Nusa code, assets or tokens exist in the repo):**
   - **Shape and placement:** black, flush with the work-area top, rounded corners on the bottom only.
   - **Compact clock:** 200×32 DIP, in the user-locale time format (`GetTimeFormatEx`, `TIME_NOSECONDS`).
   - **Hover:** expands in place to 360×84 DIP and adds the long date. Pointer leave collapses it.
   - **Redraw policy:** the notch redraws only when the minute text changes or on hover. The minute deadline joins the catalog deadline in the single `MsgWaitForMultipleObjectsEx` wait. There's no `WM_TIMER` and no polling.

Not in scope (still owed to full M2 or later): own pins, Store-app grouping verified on a real packaged app, Narrator speech, real IME composition, OS text-scale setting changes, an elevated/packaged/hung target matrix, the full-M2 stack go/no-go. Also out of scope are multi-monitor and fullscreen suppression (M3; the notch currently sits above fullscreen apps), and expand/collapse animation and reduced-motion handling (M4).

## Finding dispositions (M2b1 iteration 1, Opus)

| ID | Disposition | Where |
| --- | --- | --- |
| `M2B1-OPUS-MAJ-1` | **Fix submitted.** The `--show-picker` client calls `AllowSetForegroundWindow(pid)` for the process that owns ROQA's window class, and for no other process. A denial status now persists until the picker is actually focused. | `src/ipc.rs` (`grant_foreground_to_resident`), `src/picker.rs` |
| `M2B1-OPUS-MIN-1` | **Accepted as a documented limit; not fixed.** See "Modal loops" below. Packet claim corrected. | this packet |
| `M2B1-OPUS-MIN-2` | **Fix submitted.** The picker smoke no longer grants foreground itself. If the requesting app keeps focus, the result is FAIL (product denial), not interference. | `scripts/picker-smoke.ps1` |
| `M2B1-OPUS-MIN-3` | **Fix submitted.** A restore failure during activation becomes a picker status message instead of exiting the host. | `src/picker.rs` |
| `M2B1-OPUS-MIN-4` | **Fix submitted.** The snapshot zip keeps manifest-relative directory paths (it's built from the manifest list, not flattened). | `artifacts/review-snapshots/m2b-iteration-1.zip` |

### Modal loops (MIN-1 decision)

The picker is `WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_BORDER`. A caption drag, keyboard move or size, or the system menu runs `DefWindowProc`'s modal loop inside `DispatchMessageW`. **While that loop runs, the host's outer pass doesn't run.** That pass covers:

- catalog deadline servicing and publication;
- dock refresh (new groups, icons, UIA names);
- the notch's minute redraw;
- picker list sync;
- IPC events (`--quit`, `--show-*`).

IPC events are auto-reset events: the client's `SetEvent` returns at once and the command is serviced when the loop ends. Window messages still dispatch inside the loop, so hover and paint of existing surfaces continue. WinEvent ingress stays bounded with its overflow full-scan fallback. Opus measured that publication resumes about 50 ms after the loop ends.

Decision: the pause is bounded by an explicit, ongoing user action, so it is accepted as a limit. A timer-driven re-entrant service from inside the loop would need host state to be reachable from the picker's window procedure, and that adds re-entrancy risk for little benefit. Untested: drags of several minutes, and `--quit` during a drag (inferred to be serviced when the drag ends).

## Author self-check on changed production code (not an approval)

I ran a clean-code/correctness pass over `src/dock.rs`, `src/dock_uia.rs`, `src/icons.rs`, `src/clock.rs`, and the diffs in `host.rs`, `ipc.rs`, `picker.rs`, `window_source.rs`, `window_catalog.rs`, `window_model.rs`, `graphics.rs`, `layout.rs`, `native_window.rs` and `main.rs` against the M2b1 snapshot. Nothing required a code change. For the reviewers to judge:

- **SC-1, size/SRP:** `host::Desktop` now owns notch, dock items, hover, UIA state, icon service and clock text (`host.rs` is 535 lines). It isn't extracted into a separate dock type; reviewers should decide whether that is now warranted.
- **SC-2, trust:** the `--show-picker` foreground grant goes to whichever process owns the first window of ROQA's class on the desktop (`FindWindowW`). A same-user process that registers the class could receive that one-time right. Same user and desktop only. Not hardened.
- **SC-3, hung targets:** the AUMID is read per window per scan through `SHGetPropertyStoreForWindow`, with no per-window cache. Whether this call can block on a hung target is **not verified**; the hung-target matrix is untested.
- **SC-4, clock after sleep:** clock wakes rely on the wait timeout, which excludes time spent asleep on Windows 8 and later. After resume, the next message or catalog wake corrects the time, and the clock pass runs on every loop pass. There's no explicit `WM_TIMECHANGE`/resume handling, and this is untested.
- **SC-5, silent refusal:** a dock activation that Windows refuses, or that hits a stale target, is silent: the dock has no status line, unlike the picker.
- **SC-6, click on release:** `WM_LBUTTONUP` acts even when the press started outside the slot.
- **SC-7, icon alpha:** the icon premultiply heuristic can treat a straight-alpha icon whose channels never exceed its alpha as already premultiplied (a small visual error only).

## Author verification (main tree, final binary)

Release `target/release/roqa.exe` SHA-256 `D00AB9C8E1C1E4B61FF233691826B3AE57C79E6152EB39183DBAC071C4D949C2`. All evidence is under `artifacts/m2b-candidate-1/` and records this hash. Each script ran in a fresh process with its own evidence directory, after the user confirmed the desktop was free.

- **`verification/`:** `scripts/verify.ps1` (now with `-StepTimeoutSeconds`, default 900; see below) ran fmt, locked all-target Clippy with warnings denied, **17 tests** and a locked release build. Both subprocesses **exit 0**.
- **`picker-run1/`:** `picker-smoke.ps1 -SkipBuild`, **PASS, 226 checks**.
  - It covers all the M2b1 picker checks through the real `--show-picker` route, with no harness foreground grant. The keyboard route takes focus.
  - Dock checks:
    - the fixture group is exposed as a named UIA button that supports Invoke;
    - slot 0 is the invokable picker button;
    - a real click on a focused lone window minimizes it;
    - a click on the minimized group restores and activates it;
    - a slot-0 click opens a focused picker.
  - Arabic filter code points are retained (`arabic-picker.png`).
  - Cycle samples (private bytes / handles / USER / GDI):

    | Sample | Private bytes | Handles | USER | GDI |
    | --- | --- | --- | --- | --- |
    | warm | 21,200,896 | 484 | 27 | 53 |
    | block 1 | 21,200,896 | 484 | 27 | 53 |
    | block 2 | 21,262,336 | 487 | 27 | 53 |
    | block 3 | 21,331,968 | 488 | 27 | 53 |

  - These are higher than M2b1's figures (about 12 MB / 347 / 17 GDI). That fits shell icon extraction plus loaded shell/UIA libraries, but it isn't attributed in detail. They're workload samples, not a leak-freedom proof.
- **`shell-run1/`:** `smoke.ps1 -SkipBuild`, **PASS, 304 checks**.
  - Notch: centered, flush with the work-area top at 200×32 DIP, black background. **Hover expands it in place to 360×84 DIP** and pointer leave collapses it. This is the first runtime evidence for the hover expansion.
  - Dock: dynamic `16+56n` geometry, plus background, picker-glyph and hover pixels.
  - The shell resource samples were flat across 5 blocks: 19.53 to 19.60 MB, 442 handles, 40 GDI, 16 USER.
- **`catalog-run1/`:** **PASS, 67 checks**. Generation 617, 6 groups, `hooks_live` 0, `stopped` true.
- **`idle-run1/`:** release build, 30.016 s.
  - CPU was **15.625 ms**: one scheduler-tick quantum. Earlier candidates measured 0 ms. No minute boundary fell inside the window, and the tick isn't attributed.
  - Private bytes went from 16,572,416 to 16,662,528, and handles from 406 to 411.
  - Observation only: no budget verdict and no competitor comparison.

### Failed and inconclusive runs (kept)

- **Main-tree `verify.ps1`, first attempt:** **failed** with exit 1 and no JSON. The cold build exceeded the old fixed 60-second step limit. This is recorded in the ledger, and the evidence is in `artifacts/m2b2-work/` (excluded WIP).
  - Fix: `scripts/verify.ps1` gains `-StepTimeoutSeconds`, validated to the range 1–7200 with a default of 900. A step that overruns is still killed and still fails; it never produces a false PASS.
- **Working-copy runs on earlier binaries:** two shell INCONCLUSIVE runs (cursor instability), one picker INCONCLUSIVE run (initial fixture foreground denied) and one picker FAIL (`picker2`, a PowerShell 5.1 JSON array-wrapping bug in the harness, fixed in the harness only). They used the working copy's earlier binaries and are recorded in `progress.md`. Their evidence (`artifacts/m2b2-work/`) is excluded from this manifest as WIP from other binaries.
- No run on the final binary failed or was inconclusive. Each script ran once.

## Known risks and limits

- **Untested:** Narrator speech, real IME composition and OS text-scale setting changes. Text scale is verified only as API integration.
- **UIA Invoke foreground:** UIA Invoke of a dock **app** slot was refused foreground by Windows in the author's probe, because ROQA had no input. This is the documented foreground rule, not forced. The slot-0 Invoke opens the picker unfocused with a persistent denial status.
- **Store-app grouping:** AUMID grouping and `shell:AppsFolder` icons are unverified on a real packaged Store app. The PASS runs used a Win32 fixture and executable fallback for most apps.
- **Target matrix:** elevated, packaged and hung targets are not tested. The modal-loop pause is accepted (MIN-1).
- **Inherited M2a minors** are still open: constructor decomposition, diagnostics transient-lock policy, source-side fault injection. M3 carry-forwards: DPI/topology authority, a physical changed-scale test, multi-monitor, fullscreen suppression.

## Independent review packet (not launched yet)

Manifest `reviews/m2b-iteration-1.manifest`; snapshot `artifacts/review-snapshots/m2b-iteration-1.zip` (paths preserved). The manifest excludes `progress.md`, reviewer output and WIP evidence. Candidate inputs must not change while a review runs.

- **Sol High:** offline. Scope: the diff against the M2b1 snapshot and the accepted M2a manifest, the packet claims, the finding dispositions, and the **independent clean-code-guard** on the changed production code (`src/dock.rs`, `dock_uia.rs`, `icons.rs`, `clock.rs`, `host.rs`, `ipc.rs`, `picker.rs`, `window_source.rs`, `window_catalog.rs`, `window_model.rs`, `graphics.rs`, `layout.rs`, `native_window.rs`, `main.rs`, `tests/support/uia_probe.rs`). **No GUI.**
- **Opus High:** holds the exclusive native slot. It runs the frozen scripts unmodified into its own evidence directory, and verifies the MAJ-1 route without a harness grant and the notch hover.

Neither reviewer may have authored any input. Reports use stable IDs (`M2B-SOL-*`, `M2B-OPUS-*`), the exact manifest SHA-256 and a disposition scoped to these inputs.
