# M5 results 2: ROQA-only native run (2026-11-12 Africa/Cairo)

Candidate `6292169` (prayer engine), release `roqa.exe` 921,600 B, SHA-256 `3CE3511F4398CEA1D674CB7854DE97FF5E236D65A5AC40835E0E58DF53FC757A` (path-embedding: differs from the ledger SHA at identical size). Applies to this SHA only. Reviews paused: **REVIEW NOT RUN**. Evidence: `artifacts/native-7/` (not committed). Runs 08:14-08:33 UTC.

**Machine:** Windows 11 Pro 10.0.26200; Ryzen 5 PRO 4650G (12 logical); Radeon, driver 31.0.21925.1001; 2 monitors 1920x1080 at 144 Hz, AppliedDPI 96 (per-monitor not captured); AC power, display-off 900 s held awake by the harness.

**Conditions:** user confirmed the desktop free. Foreground: T3 Code. No game or full-screen app. Riot Client Services and `vgtray` were running with the `vgk` driver loaded (`vgc` stopped), no League or Valorant process, no input injected during measurement. One media session, **Paused** (browser), so `smoke.ps1` was skipped; the media session stayed present during all runs. Other desktop apps (browser, Claude, T3) were open.

## Suites

| Suite | Result |
|---|---|
| `verify.ps1` | PASS: fmt, clippy `-D warnings`, 224 tests, locked release build, size ratchet |
| `picker-smoke.ps1` | PASS: 226 checks |
| `catalog-smoke.ps1` | PASS: 67 checks |
| `packaged-hung-check.ps1` | PASS: 16 checks |
| `topology-check.ps1` | **FAIL** (not interference, no retry): "Exactly one notch, shadow and dock per monitor (10 of 6)". Inferred cause, not read from evidence: B2.5 edge sliders add 2 surfaces per monitor and the script still expects 3. A stale script check; product topology not judged |
| `smoke.ps1` | **NOT RUN**: media session active |

## Measurements (`artifacts/native-7/m5/`, 1 process, Complete=true)

| Scenario | Private bytes | CPU | Handles | GDI/USER | Threads | Wakes | Paints |
|---|---|---|---|---|---|---|---|
| idle 900.2 s, r1 | 22,519,808 (21.5 MiB) | 218.75 ms = 0.243 ms/s (cycles approx 0.245), WITHIN | 548 to 532 | 40/29 to 40/27 | 22 to 19 | proxy 3.21 switches/s; media_wakes 0, media_refreshes 0 | 30 (47 to 77) |
| start-warm, 30.2 s, r1 | 21,827,584 (20.8 MiB) | 421.9 ms = 13.96 ms/s incl. startup, INCONCLUSIVE | 549 | 40/29 | 22 | proxy 43.7/s | not captured |

Idle: no user-input abort, sampler 2.16 M cycles/s. `store_writes` unreadable (null).

## PLAN M5 gates

| Gate | Verdict |
|---|---|
| One-monitor private bytes <= 80 MiB | PASS: 21.5 MiB at 15 min, measured with 2 monitors; one repetition |
| Three-monitor private bytes <= 120 MiB | INCONCLUSIVE: 2 monitors only (21.5 MiB) |
| Static CPU <= 1 ms/s over 15 min | PASS: 0.243 ms/s, measurable at 900 s; one repetition, paused media session present |
| Zero collapsed paints apart from the clock | INCONCLUSIVE: 30 paints in 15 min (2/min), no per-source attribution; matches a once-a-minute clock on 2 monitors but that is not confirmed by the counters. No sign of repaint storms |
| Input p95 <= 50 ms / p99 <= 100 ms | NOT RUN |
| Bounded churn, DWM, GPU memory, power | NOT RUN |
| >= 30 % lower than Nectar/Nusa | NOT RUN (no pinned builds) |

**NOT RUN:** Nectar/Nusa comparison, churn, cold start, input p95/p99, expanded media, `smoke.ps1`, three-monitor, repetitions 2-3. Fix `topology-check.ps1` for the slider surfaces before re-running it.
