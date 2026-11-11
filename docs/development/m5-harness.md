# M5 S1: comparison harness (scripts and docs only)

Slice S1 of `m5-preparation.md`. No `src/` change. **Nothing here has been run natively**: no build, no ROQA or competitor launch, no counter read. Only argument validation, `-DryRun` and the verdict self-test ran. Every other path is **NOT RUN**.

## Files

| File | Role |
|---|---|
| `scripts/measure-idle.ps1` | Scenario runner. The M1-M3 idle script extended in place: `QueryProcessCycleTime`, WMI wake proxy, `GetGuiResources`, abort on user input, display-off/screen-saver guard and `--quit` cleanup are reused unchanged. One invocation is one repetition. |
| `scripts/verdict.ps1` | Pure verdict functions (CPU floor, median, 30 % ratio), dot-sourced by the other two. |
| `scripts/compare-report.ps1` | Reads saved run JSON, validates pinning and comparability, writes a comparison JSON. Launches nothing. `-SelfTest` runs the fixed-input checks. |

## Running (needs the user)

Every run needs a fresh, explicit desktop-free confirmation from the user plus a foreground-window check (`AGENTS.md`); the script injects no input but stops on any (except churn and media). Never build during a measurement: `-Executable` skips the build, and a pre-built `-SkipBuild` run is the baseline path.

```powershell
# ROQA, 15-minute static-gate idle sample, repetition 1 (repeat with -Repetition 2, 3):
scripts/measure-idle.ps1 -SkipBuild -Scenario idle -SampleSeconds 900 -Repetition 1 -EvidenceDirectory artifacts/m5-baseline
# Competitor: a pinned path AND its SHA-256 are mandatory; the script fails before launching without both or on a mismatch:
scripts/measure-idle.ps1 -Competitor nectar -Executable D:\pinned\nectar.exe -ExpectedSha256 <64 hex> -Scenario idle -SampleSeconds 900
scripts/compare-report.ps1 -RoqaRuns artifacts/m5-baseline/idle-roqa-r*.json -CompetitorRuns artifacts/m5-baseline/idle-nectar-r*.json -Output artifacts/m5-baseline/idle-vs-nectar.json
```

Output file: `<EvidenceDirectory>/<scenario>-<build>-r<N>.json` (build is `roqa` or the competitor name). The old fixed `idle.json` name is gone.
Sleep and display: while sampling the harness holds `SetThreadExecutionState(ES_CONTINUOUS|ES_DISPLAY_REQUIRED|ES_SYSTEM_REQUIRED)` and clears it in `finally`, so `-SampleSeconds` may exceed the display-off or screen-saver timeout (it never changes power settings; the old refusal applies only if the call fails). Group-policy lock and sleep timeouts are not covered.
Idle diagnostics: the `idle` scenario for ROQA passes `--diagnostics <build>.diagnostics.json` and records the counters at the start and end plus `Delta` (`paints`, `media_wakes`, `media_refreshes`, `store_writes`) in `Diagnostics`; the report is as old as the last catalog scan (ages recorded), and `null` means unreadable, never zero. `paints` counts committed canvas draws (notch, shadow, meter slot, dock).
Abort: user input (or any early stop) writes `<scenario>-<build>-r<N>.aborted.json` (`Status` `ABORTED`, reason, elapsed seconds) and still fails the run.

## Scenarios

| `-Scenario` | Window | Stimulus |
|---|---|---|
| `idle` | settle 5 s + WMI read, then `-SampleSeconds` | none; input aborts the run |
| `start-warm`, `start-cold` | from process creation (every `*Start` value is 0 by construction) | none; input aborts. Warm = relaunch after a prior exit. Cold = first launch after reboot; the script cannot verify it, so `-Note` must attest it |
| `churn`, `media` | as `idle` | a person opens/closes windows or changes tracks while it samples; input abort is off; `-Note` must describe the stimulus. Automated drivers are **DEFERRED** (they inject input and launch other programs, so each needs its own authorization) |

## What is recorded

Scope is the launched process plus its descendants (WebView2, helpers), found by one `Win32_Process` read. Per run: summed private bytes, handles, USER/GDI objects, threads (start and end), CPU time delta, CPU cycle delta, wake proxy, machine (OS build, CPU, GPU, monitor count, system `AppliedDPI`; per-monitor DPI is **not** captured, note it by hand), the sampler's own cycles per second (the harness overhead, wait loop only), and for a competitor `{Name, Executable, PinnedSha256}`. No working-set trimming is done or recorded anywhere.

`Complete` is false when a descendant exited or could not be read between the reads (its CPU is then lost). An incomplete run is INCONCLUSIVE, never understated.

## Verdicts

- **CpuVerdict** per run: `INCONCLUSIVE` when incomplete or when one scheduler tick per process over the sample (15.625 ms x processes / seconds) exceeds a tenth of the 1 ms/s gate. Example: 30 s and one process is 0.52 ms/s, so INCONCLUSIVE; 900 s and one process is 0.017 ms/s, measurable. Otherwise `WITHIN`/`ABOVE` the proposed 1 ms/s gate for `idle`, and `MEASURABLE` (no gate) for other scenarios.
- **Comparison** (`compare-report.ps1`), per metric (`PrivateBytesEnd`, `CpuCyclesPerSecond`): `MET` needs at least 3 runs on each side, complete runs, a measurable CPU counter (for CPU) and a median at least 30 % lower than the competitor; otherwise `NOT MET`, or `INCONCLUSIVE` with a reason.
- It refuses runs that mix scenarios, builds (SHA-256), or machine configurations, a competitor run without a matching pinned SHA-256, or a ROQA side containing a competitor run.
- Per-scenario absolute gates other than idle CPU (80/120 MiB private bytes, input p95/p99, bounded churn) are **not** judged by the harness; read them from the JSON. DWM increment, GPU memory, power and input latency stay manual.

## Test status

| Item | Status |
|---|---|
| Verdict and report logic (`compare-report.ps1 -SelfTest`: 21 assertions incl. floor, 30 % edge, run count, pin, build and machine mismatch) | PASS |
| Parse of the three scripts (PowerShell parser, 0 errors) | PASS |
| Argument validation (`measure-idle.ps1 -DryRun`: competitor without path or SHA, bad SHA, SHA mismatch, unsafe name, missing note, bad scenario, bad repetition, missing path) | PASS |
| Real run of every scenario; descendant tree walk; `Read-Tree`, `Measure-TreeDelta`; start scenario empty-start logic; churn/media with input allowed | **NOT RUN** |
| Competitor launch and the `CloseMainWindow` then `taskkill /T /F` cleanup | **NOT RUN**; a competitor's `--quit` is unknown, so cleanup closes its main window then ends only the owned process tree. Helpers that outlive their root are not handled |
| Running-instance check against real processes | **NOT RUN** |
| Sampler overhead value | **NOT RUN** (recorded per run, no number yet) |
| ROQA-only M5 baseline, any competitor comparison | **NOT RUN / DEFERRED** (needs the user; competitor runs need S10 pre-conditions) |

## Known limits

- A competitor starts with its default window state; ROQA starts hidden. Whether that changes a competitor's behaviour is unverified.
- Grandchildren whose parent already exited are missed; a reused PID can mislink (shortcut comment in the script).
- `AppliedDPI` is the sign-in system scale. A root with no USER objects fails the run (existing rule), which suits ROQA and any windowed notch.
- A build that ignores `CloseMainWindow` is force-ended, so its exit code is not checked.
- `-Executable` no longer triggers a build; before this slice it did.
