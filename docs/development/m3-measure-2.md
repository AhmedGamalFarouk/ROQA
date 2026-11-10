# M3 measurement 2: finer idle counters in `measure-idle.ps1`

- **Author.** A Claude Opus 5.5 worker, launched by the Chief Thread. The author approves nothing.
- **Date.** 2026-11-10.
- **Base.** `main` at `26cbb0a`.
- **Why.** `m3-native-validation-1.md` found idle CPU **inconclusive**. `Process.TotalProcessorTime` moves in 15.625 ms ticks, so one tick in a 30 s sample is 0.52 ms/s. That is too coarse to test the proposed M5 gate of ≤1 ms/s.
- **Scope.** One file, `scripts/measure-idle.ps1`. There is no Rust or production change, no new tool, no ETW/WPR, and no system-setting change. The script only reads power and screen-saver settings.

## Change

The script keeps every existing output field, so old `idle.json` files can still be read. It is **not strictly comparable** with M1-method runs, though: the settle period is now about 10 s, not 5 s (see `SettleSeconds` below). The additions:

| Addition | Source | Notes |
|---|---|---|
| `-SampleSeconds` (default 30, range 1 to 86400) | parameter | Longer runs are bounded by the idle-timeout guard below. |
| `CpuCycles`, `CpuCyclesPerSecond` | `QueryProcessCycleTime`, start and end, next to the stopwatch | Not tick-quantized. **Compare builds by cycles/s.** |
| `NominalClockMegahertz`, `ApproxCpuMillisecondsPerSecondFromCycles` | `Win32_Processor.MaxClockSpeed`; throws if it is missing or 0 | **Approximate.** The `QueryThreadCycleTime` docs advise against converting cycles to elapsed time; the `QueryProcessCycleTime` page says nothing either way. The conversion assumes the counter runs at `MaxClockSpeed`, which is unverified. WMI reports 3900 MHz here, and the Ryzen 5 PRO 4650G's published base clock is 3.7 GHz. |
| `GdiObjectsStart/End`, `UserObjectsStart/End` | `GetGuiResources` (`GR_GDIOBJECTS` 0, `GR_USEROBJECTS` 1) | A 0 return means either failure or no objects. Every windowed ROQA build owns USER objects, so 0 USER objects throws. GDI is recorded as returned, because M1 to M2a builds legitimately owned 0. |
| `WakeProxyContextSwitches`, `WakeProxySeconds`, `WakeProxyContextSwitchesPerSecond`, `WakeProxyReadSeconds`, `ThreadsStart/End/Exited/Born` | WMI `Win32_PerfRawData_PerfProc_Thread` raw `ContextSwitchesPersec`, which is cumulative per thread and **matched by `IDThread`** | **A wake proxy, not a wake count.** See the wake-proxy notes below. |
| `SettleSeconds`, `IdleSecondsAtLaunch` | `Stopwatch` from launch; `GetLastInputInfo` | `SettleSeconds` is the 5 s settle plus the first WMI read. |
| `Machine.IdleTimeouts` and a guard | `powercfg /q SCHEME_CURRENT SUB_VIDEO VIDEOIDLE`, matched to AC or DC by `GetSystemPowerStatus`; `HKCU\Control Panel\Desktop` `SCRNSAVE.EXE` and `ScreenSaveTimeOut` | See the idle-timeout guard below. |
| Abort on user input | `GetLastInputInfo` | See abort on user input below. |

**Wake-proxy notes.**
- Both reads sit **outside** the CPU window, one just before it and one just after. One read took about 4 to 5 s in the self-checks.
- The window runs between read midpoints, measured with `Stopwatch`. The snapshot instant inside each read is unknown, so the window is uncertain by ± half the summed read time.
- **Threads alive at both reads** add their difference.
- **Threads born between the reads** add their whole count.
- **Threads that exit between the reads** lose their in-window switches, so the proxy can **understate**. `ThreadsExited` counts them, but it is not a bound: a reused thread ID can hide an exit and understate further.
- If an ID's count goes down, the ID was reused, and that thread is counted as new.
- A read that returns no threads throws, so a failed counter source cannot pass as zero wakes.

**Idle-timeout guard.**
- Before launch, the script refuses to start if this sum reaches the display-off or screen-saver timeout: the current idle time + 5 s settle + the sample + 30 s of allowance for the WMI reads.
- After the final read, it checks the real elapsed time against the same timeout again.
- **Not read:** group-policy screen-saver and lock timeouts, and the system sleep timeout. AC sleep is set to never on this machine.
- A screen saver with no `ScreenSaveTimeOut` is treated as never.

**Abort on user input.**
- The input tick is taken just before launch.
- It is checked once a second during the sample, and again after the final read.
- Input during the settle is caught at the first poll, because every check compares against the tick taken before launch.
- Any input throws `STOP`, quits ROQA gracefully and writes no new `idle.json`.
- The once-a-second poll runs in the sampler, not in ROQA.
- An older `idle.json` already in the same folder is left untouched. This behaviour predates the change.

**Other fixes.**
- `PrivateBytesEnd` and `HandlesEnd` are now captured right after the end `Refresh()`, before the final WMI read.
- `Label`, `Method` and `Limits` describe the new method and its limits.

### What this machine allows

`powercfg` reports an AC display-off timeout of 900 s, and no screen saver is set. So the guard **refuses a 900 s sample**.
- With input just before launch, the longest sample it accepts is 900 − 5 − 30 = about **864 s** (`-SampleSeconds 860`).
- A 30 s run needs input in the last ~834 s before launch.

Running a full 15 minutes would need the user to change their display timeout. That is the user's decision, and this worker changed nothing.

## Self-checks (author-run; no ROQA launch)

The check script is `artifacts/m3-measure-2/selfcheck.ps1`, run in Windows PowerShell 5.1. It runs the script's own statements and helpers, taken from the script's syntax tree.

- **Parse.** 0 errors.
- **Compile.** The `Add-Type` block compiles.
- **Pre-launch statements**, run as written in the script:
  - read the timeouts as AC display-off 900 s, no screen saver, not on battery;
  - the guard refuses a sample of 86400 s.
- **Helpers, run on the self-check's own process:**
  - `Cycles`: a positive `UInt64` delta over a busy loop.
  - `UserObjects`: 4.
  - `GetGuiResources` GDI: 0, recorded without a throw.
  - `OnBattery`: false.
  - `Read-ContextSwitches`: 26 threads in about 4.8 s; a missing PID throws.
- **`Measure-SwitchDelta`:** synthetic cases covering kept, exited, born, reused-ID and empty threads give the expected results.
- **Rate math:** not unit-tested. It is the single expression cycles ÷ (MHz × 1000) ÷ seconds.
- **Encoding:** ASCII with CRLF line endings and no BOM, matching the committed file.
- **NOT RUN:** the script itself against ROQA. That is a native run, and it needs a fresh desktop-free confirmation from the user.

## Binaries for the next run

| Path | Commit | SHA-256 |
|---|---|---|
| `artifacts/m3-native-1/bin/1e47cc2/roqa.exe` | `1e47cc2` (before the performance fixes) | `F00B5824…3B82` |
| `artifacts/m3-native-1/bin/9e449d2/roqa.exe` | `9e449d2` (performance fixes) | `0DB85A44…67EC` |
| `artifacts/m3-measure-2/bin/26cbb0a/roqa.exe` | `26cbb0a` (current), copied from the root `target/release` | `1F61CABB…9227` |

The current binary is a copy, so a later rebuild of `target/release` cannot change what is measured. Unlike the other two, it is a root build, not a `git archive` build. Its SHA-256 matches the release hash that `progress.md` records for `26cbb0a`.

## Review

- **Round 1.** A fresh Opus 5.5 High reviewer returned **REVISE**:
  - 2 MAJOR: the wake proxy can understate or go negative when threads exit, and the display-off timeout falls inside a 15-minute run;
  - 5 MINOR and 4 NIT.
  - All were addressed except NIT-4, which is the old `idle.json` surviving a STOP. That behaviour predates this change and is now documented above.
  - Manifest: `artifacts/m3-measure-2/round-1/inputs.sha256`.
- **Round 2.** A fresh Opus 5.5 High reviewer returned **ACCEPT WITH MINORS**.
  - Prior findings: 9 RESOLVED; MINOR-3 PARTIAL; NIT-4 OPEN but documented.
  - New: 2 MINOR (a real 0 GDI count threw; an empty WMI read passed as 0 wakes) and 3 NIT, all wording.
  - The author fixed all five.
  - Manifest: `artifacts/m3-measure-2/round-2/inputs.sha256`.
- **Round 3.** A fresh reviewer on `artifacts/m3-measure-2/round-3/inputs.sha256`. The verdict is recorded in `progress.md`.

## Run 1 results (3×3, 30 s; 2026-11-10 about 15:42 to 15:49 UTC)

- **Authorization.** The user said "all ready, go for it" and confirmed the desktop was free. This covered **one** 3×3 run, and the run was not repeated.
- **Run by.** A Claude Opus 5.5 worker. This is the worker that committed the script; the script's author and reviewers were separate threads. No reviewer has checked these results.
- **Inputs.** `artifacts/m3-measure-2/round-3/inputs.sha256` was checked 6/6 OK just before the run. That covers the script, this packet, the self-check and all three binaries (`F00B5824…3B82`, `0DB85A44…67EC`, `1F61CABB…9227`).
- **Command.** `scripts/measure-idle.ps1 -SkipBuild -Executable <bin> -EvidenceDirectory artifacts/m3-measure-2/run-1/rep<N>-<sha>`, with the default 30 s sample, run in Windows PowerShell 5.1.
  - **Gap.** 3 s between runs.
  - **Order.** The start rotated each rep:
    - rep 1: `1e47cc2` → `9e449d2` → `26cbb0a`;
    - rep 2: `9e449d2` → `26cbb0a` → `1e47cc2`;
    - rep 3: `26cbb0a` → `1e47cc2` → `9e449d2`.
  - **Packet section.** This packet had no "READY TO MEASURE" section; the order came from the Chief Thread's brief.
- **Pre-checks before every run** (`run-1/prechecks.log`):
  - T3 Code was in the foreground.
  - No `roqa`, `cargo` or `rustc` process was running.
  - Input idle time rose from 101 s to 466 s, so no input reached the machine during the whole run.
- **Outcome.** No STOP and no guard refusal. Every run quit gracefully, and all 9 runs wrote `idle.json`.
- **Machine.** Ryzen 5 PRO 4650G, 12 logical processors, WMI MaxClockSpeed 3900 MHz; Windows 11 Pro 26200; 1920×1080 at 144 Hz; on AC power; display-off timeout 900 s.

| Run | Build | Cycles/s | Approx. ms/s | Tick ms/s | Wake proxy (switches, per s) | Threads (start/end, born) | GDI | USER | Private KB (start→end) | Handles |
|---|---|---|---|---|---|---|---|---|---|---|
| rep1 | 1e47cc2 | 1,688,864 | 0.433 | 0.521 | 127, 3.55 | 20/20, 0 | 42 | 21 | 20,448→20,592 | 442→444 |
| rep1 | 9e449d2 | 941,381 | 0.241 | 2.083 | 74, 2.31 | 18/20, 2 | 42 | 21 | 20,380→20,516 | 440→442 |
| rep1 | 26cbb0a | 771,367 | 0.198 | 0 | 51, 1.59 | 18/20, 2 | 42 | 21 | 24,012→24,148 | 440→442 |
| rep2 | 9e449d2 | 1,516,635 | 0.389 | 0.521 | 102, 3.19 | 18/20, 2 | 42 | 21 | 20,752→20,928 | 440→442 |
| rep2 | 26cbb0a | 1,607,389 | 0.412 | 1.041 | 122, 3.78 | 20/20, 0 | 42 | 21 | 20,044→20,172 | 440→442 |
| rep2 | 1e47cc2 | 932,780 | 0.239 | 0.521 | 70, 2.17 | 18/20, 2 | 42 | 21 | 19,928→20,068 | 440→442 |
| rep3 | 26cbb0a | 774,056 | 0.199 | 0 | 66, 2.05 | 18/20, 2 | 42 | 21 | 20,252→20,388 | 440→442 |
| rep3 | 1e47cc2 | 1,584,132 | 0.406 | 0 | 78, 2.44 | 18/20, 2 | 42 | 21 | 19,868→20,004 | 440→442 |
| rep3 | 9e449d2 | 1,449,162 | 0.372 | 0.521 | 71, 2.23 | 18/20, 2 | 42 | 21 | 19,876→20,012 | 440→442 |

**How to read the table.**
- **Cycles/s** is the comparison metric. It comes from `QueryProcessCycleTime`.
- **Approx. ms/s** is approximate. It assumes the cycle counter runs at 3900 MHz, which is unverified.
- **Tick ms/s** is the old tick-quantized figure, shown only for comparison.
- **Wake proxy** is a proxy, not a wake count. Its window is about 32 s, with WMI reads of about 2 s each; rep1 `1e47cc2`'s first read took 9.3 s.
- **Threads exited** was 0 in every run.
- **Settle** was about 7 s, except rep1 `1e47cc2` at 14.5 s, the first run, when WMI was cold.

**Per-build summary** (median, then range):

| Build | Cycles/s | Approx. ms/s | Wake proxy /s |
|---|---|---|---|
| `1e47cc2` | 1,584,132 (932,780 to 1,688,864) | 0.41 (0.24 to 0.43) | 2.44 (2.17 to 3.55) |
| `9e449d2` | 1,449,162 (941,381 to 1,516,635) | 0.37 (0.24 to 0.39) | 2.31 (2.23 to 3.19) |
| `26cbb0a` | 774,056 (771,367 to 1,607,389) | 0.20 (0.20 to 0.41) | 2.05 (1.59 to 3.78) |

### Analysis

**Builds are not distinguishable beyond the spread across reps.**
- All three builds' cycles/s ranges overlap. So do their wake-proxy ranges.
- The medians differ only because of which runs happened to catch a minute boundary (see below).
- With n = 3 per build, this run supports no ranking of the builds.

**Every run fell into one of two clusters.**
- A low cluster at about 0.77M to 0.94M cycles/s.
- A high cluster at about 1.45M to 1.69M cycles/s.
- In all 9 runs, the cluster matches whether the sample window crossed a wall-clock minute. The window was estimated as `Utc` minus the final read time, minus the duration.
  - High: rep1 `1e47`, rep2 `9e44`, rep2 `26cb`, rep3 `1e47` and rep3 `9e44` all crossed a minute.
  - Low: the four runs that did not cross one.
- **Hypothesis, not verified.** The extra cost of about 0.7M cycles is the per-minute clock update and repaint. No trace attributes it.
- A fair comparison needs either samples long enough to average over minute boundaries, or windows aligned to cross the same number of minute boundaries.

**Within each cluster, the samples are too few to compare.**
- Low cluster: `26cbb0a` ran at 771k and 774k, against 941k for `9e449d2` and 933k for `1e47cc2` (one run each). That is a hint that `26cbb0a` costs less between minutes, and only a hint.
- High cluster: 1.45M to 1.69M, with no clear order.

**GUI objects, private bytes and handles.**
- GDI 42 and USER 21 in every run, at both start and end: no growth within 30 s and no difference between builds.
- Private bytes started at about 19,870 to 20,750 KB. The exception was rep1 `26cbb0a`, which started at 24,012 KB; that is one outlier with no cause found. Each run grew by about 130 to 175 KB over the window, which may be WMI-query or allocator activity, and is not shown to be a leak.
- Handles were 440→442 in every run, except 442→444 in the first run.

**Two threads were born in ROQA in 7 of 9 runs.**
- The thread count went from 18 to 20 between the reads, and no thread exited.
- The origin is unknown. It could be a system or thread-pool worker, for example one triggered by the WMI query. It is not attributed to ROQA code.

**M5 gate.** The highest approximate figure was 0.43 ms/s, below the proposed ≤1 ms/s gate. But this is a 30 s observation that converts cycles to time on an unverified basis. **It is not an M5 15-minute gate verdict.**

**Evidence.**
- `artifacts/m3-measure-2/run-1/rep<N>-<sha>/idle.json` (9 files).
- `artifacts/m3-measure-2/run-1/prechecks.log`.

## Still NOT RUN or deferred

- No reviewer has checked the run 1 results above.
- The 15-minute run: NOT RUN, and blocked on this machine's 900 s display timeout, as explained above.
- Council review, accessibility, Windows 10 and DWM/GPU/power: deferred.
- The fine-resolution CPU result is unknown until the run happens.
