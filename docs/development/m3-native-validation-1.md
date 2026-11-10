# M3 native validation 1

- **Author.** A Claude Opus 5.5 worker, launched by the Chief Thread. Evidence only: it approves nothing and makes no council or milestone claim.
- **Date.** 2026-11-10 (UTC).
- **Source.** No production code was changed. Evidence is under `artifacts/m3-native-1/`, which is git-ignored.
- **Desktop-free confirmations.** The user gave two, relayed by the Chief Thread:
  - "yes the desktop is free" (about 11:25 UTC) covered the topology check, the four suites and `measure-idle.ps1`.
  - "do all" (about 12:30 UTC) covered `packaged-hung-check.ps1`, `measure-idle.ps1` (3×3) and the `idle-matrix.ps1` helper (3×3).
- **Pre-checks.** Before every script, a read-only check recorded:
  - the foreground window;
  - the input-idle time from `GetLastInputInfo`;
  - that no ROQA process was running;
  - from the second confirmation on, that no cargo or rustc process was running.

## Machine

- **OS.** Windows 11 Pro 10.0.26200.
- **CPU.** AMD Ryzen 5 PRO 4650G, 12 logical processors.
- **GPU.** AMD Radeon(TM) Graphics, driver 31.0.21925.1001; WMI reports 1920×1080 at 144 Hz.
- **Displays.** Two monitors, both at 96 DPI with an auto-hide taskbar, so the work area equals the monitor area:
  - `DISPLAY1` is primary, at 0,0, 1920×1080.
  - `DISPLAY2` is at -1920,0, 1920×1080.
- **Desktop.** Other user applications kept running (browser, T3 Code, chat apps).

## Binaries

Each of the three binaries was built from an exact commit:

- the commit was exported with `git archive` into `artifacts/m3-native-1/src/<sha>/`;
- it was built with the pinned project-local toolchain: `scripts/cargo.ps1 build --locked --release --manifest-path <src>/Cargo.toml --target-dir artifacts/m3-native-1/target-<sha>`;
- each build used its own target directory.

The root checkout was never checked out or reset.

| Commit | Meaning | Archive build `roqa.exe` SHA-256 |
|---|---|---|
| `1e47cc2` | M3 slice 2, before the performance fixes | `F00B58244F24DF479D16CADE0852E3BAF852F4CA95D3812B1FB771B02DAE3B82` |
| `9e449d2` | performance fixes, before the clock fix | `0DB85A44DC9C1625FC7ADDB74EC769E753A94066FCA94C35FD893C30A23267EC` |
| `402941c` | clock fix (current code) | `796DB5C0CB705E6C9A17DF5E627DF2F9377B318396E24FAD78F217E4BE3F1C9C` |

**Reproducibility.** The root `target/release/roqa.exe` (`595E7AA2B4CD993135B6EE50D6DFCC346CD59F875E0A1CC3AF1D1D2D3AD85581`, verified against the brief) and the `402941c` archive build are the same size, 610,816 bytes. They differ in exactly 24 bytes:

- the deterministic PE/debug-directory timestamp, four 2-byte copies;
- the 16-byte PDB GUID.

Both are linker values derived from the build inputs, which here include the build path. The code is identical. `roqa-uia-probe.exe` shows the same 24-byte pattern, so the root probe is current.

**What runs where.**
- The suites ran on the root `595E…` binary, because the scripts hard-code `target/release`.
- All idle measurements used the three archive builds, so the comparison uses one build method.

**A rejected first attempt.** A shared target directory first produced one identical binary for all three commits: archive mtimes predate the outputs, so cargo reused the first build. That attempt was discarded.

## Suites (402941c, `595E7AA2…5581`)

| Script | Command | Result | Checks | Notes |
|---|---|---|---|---|
| `topology-check.ps1` | `-ExpectedSha256 595E… -EvidenceDirectory artifacts/m3-native-1/topology` | **PASS** | 25/25 | Passive. 6/6 surfaces, one notch, shadow and dock per monitor, centred and flush. Foreground unchanged. Graceful `--quit` in 203 ms. 17.5 MiB private, 445 handles, 42 GDI, 21 USER. |
| `smoke.ps1` | `-SkipBuild -EvidenceDirectory artifacts/m3-native-1/smoke` | **PASS** | 329/329 | Moves the cursor and sets the foreground to its own anchor; restores both. Work area unchanged. 6 resource samples, flat at 21.4 MiB, 479 handles, 42 GDI, 29 USER. |
| `picker-smoke.ps1` | `-SkipBuild -EvidenceDirectory artifacts/m3-native-1/picker` | **PASS** | 226/226 | Injects keyboard (`SendInput`) and real clicks. Samples 18.4→18.6 MiB, 501→505 handles, 54 GDI, 32 USER. |
| `catalog-smoke.ps1` | `-SkipBuild -EvidenceDirectory artifacts/m3-native-1/catalog` | **PASS** | 67/67 | Owned fixture and foreground changes. Storm: 501 changes in 5 delta scans. Shutdown during the storm took 368 ms. Samples flat at 17.5 MiB, 447 handles, 42 GDI, 21 USER. |
| `packaged-hung-check.ps1` | `-EvidenceDirectory artifacts/m3-native-1/packaged` | **PASS** | 16/16 | Opens Calculator and Settings; closes only the instances it started. Quit took 123 ms with a hung window present. |

All five exited gracefully through `--quit`, with no forced cleanup.

**Note on the evidence.** The packaged check's JSON and dock capture include titles of the user's own windows. That is another reason to keep them out of version control.

## Idle measurement: `measure-idle.ps1` (observation, not an M5 gate verdict)

- **Command.** `scripts/measure-idle.ps1 -SkipBuild -Executable artifacts/m3-native-1/bin/<sha>/roqa.exe -EvidenceDirectory artifacts/m3-native-1/measure-idle/rep<N>-<sha>`.
- **Method.** Unchanged from M1: hidden start, a 5 s settle, then a 30 s sample with `System.Diagnostics.Process` start and end counters, then a graceful `--quit`.
- **Order.** Three repetitions, interleaved in rotation:
  - rep 1: 1e47 → 9e44 → 4029;
  - rep 2: 9e44 → 4029 → 1e47;
  - rep 3: 4029 → 1e47 → 9e44.
- **Gating.** There was a 3 s gap between runs. A pre-check ran before every run; each one showed T3 Code in the foreground, input idle from 554 s rising to 883 s, and no ROQA or cargo process (`measure-idle-prechecks.log`).
- **Exit.** All 9 runs quit gracefully.

| Rep | Commit | Sample s | CPU ms | CPU ms/s | Private start → end (bytes) | Handles start → end |
|---|---|---|---|---|---|---|
| 1 | 1e47cc2 | 30.013 | 31.25 | 1.041 | 22,106,112 → 22,384,640 | 430 → 435 |
| 1 | 9e449d2 | 30.018 | 0 | 0 | 20,963,328 → 21,164,032 | 430 → 435 |
| 1 | 402941c | 30.006 | 46.875 | 1.562 | 21,164,032 → 21,430,272 | 430 → 435 |
| 2 | 9e449d2 | 30.006 | 0 | 0 | 18,235,392 → 18,477,056 | 430 → 435 |
| 2 | 402941c | 30.016 | 0 | 0 | 17,760,256 → 17,981,440 | 430 → 435 |
| 2 | 1e47cc2 | 30.017 | 0 | 0 | 17,952,768 → 18,153,472 | 430 → 435 |
| 3 | 402941c | 30.005 | 0 | 0 | 18,300,928 → 18,505,728 | 430 → 435 |
| 3 | 1e47cc2 | 30.013 | 31.25 | 1.041 | 17,973,248 → 18,186,240 | 432 → 435 |
| 3 | 9e449d2 | 30.012 | 0 | 0 | 18,329,600 → 18,530,304 | 431 → 436 |

Per binary over 3 repetitions:

| Commit | CPU ms over 90 s | Mean CPU ms/s | Private end, MiB |
|---|---|---|---|
| 1e47cc2 | 62.5 (4 ticks) | 0.69 | 21.3, 17.3, 17.3 |
| 9e449d2 | 0 (0 ticks) | 0 | 20.2, 17.6, 17.7 |
| 402941c | 46.9 (3 ticks) | 0.52 | 20.4, 17.1, 17.6 |

**CPU is inconclusive.**
- Every CPU delta is a whole multiple of the 15.625 ms scheduler tick. One tick in a 30 s sample is 0.52 ms/s, so the reliable floor for this method is about 0.5 ms/s per run.
- All 9 runs sit at 0 to 3 ticks, which is at or below that floor.
- The ordering (9e449d2 lowest) is not distinguishable from tick-sampling noise. **No CPU improvement or regression is claimed.**

**Memory and handles are flat across commits.**
- Private bytes: no difference between commits beyond run-to-run variance.
  - Every rep-1 run was about 3 MiB higher (about 21 MiB) than reps 2 and 3 (about 17.5 MiB), for all three binaries.
  - That is a position or system effect, not a commit effect.
- Handles were 435 or 436 at the end of every run.

## Idle measurement: `idle-matrix.ps1` helper: NOT RUN

- **What it is.** A passive helper, `artifacts/m3-native-1/idle-matrix.ps1`. It is not a repo script.
- **Method.** The same 5 s settle and 30 s sample, rotated 3×3, plus:
  - GDI and USER objects from `GetGuiResources`;
  - the cumulative per-thread context-switch count of the ROQA process, from `Win32_PerfRawData_PerfProc_Thread.ContextSwitchesPersec` raw values, read at two endpoints inside the CPU window and timed by read midpoints. This is a **wake proxy, not a wake count**: each WMI read takes about 3 s, and the switches of threads that exit inside the window are lost.
- **Gating.** It stops when there has been user input within 10 s before a run or any input during a run.
- **Two attempts launched nothing:**
  1. A binaries-argument quoting error. The pre-check for that attempt had already shown input idle of 1 s.
  2. Its own input gate fired. The pre-check showed Zen Browser in the foreground with input idle of 0 s: the user was active.
- **Result.** No ROQA process was started. GDI/USER at idle (only the suite samples above exist) and context switches remain **unmeasured** until the user confirms a free desktop again.

## Still unmeasured or deferred, not passed

- **Wake and context-switch counts.** The helper is NOT RUN.
- **Fine-resolution CPU.** CPU below the tick floor needs a longer sample or an ETW trace. The optional 15-minute trace was NOT RUN: it was not authorized.
- **Fullscreen enter and exit on either monitor.** NOT RUN. Only a user-assisted run is allowed, and none was authorized.
- **Hot-plug and DPI or topology changes.** NOT RUN: these are display changes.
- **Graphics, power and other platform scope.** DWM, GPU memory, compositor, power and allocation counts are deferred, as are Windows 10 and accessibility (deferred by the user).
- **Council review and acceptance.** Not done.
- **Limits.**
  - One machine and one session.
  - Other desktop applications kept running.
  - `measure-idle.ps1` records only the endpoints.
  - The suites ran once each.
