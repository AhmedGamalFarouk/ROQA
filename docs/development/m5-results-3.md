# M5 results 3: ROQA-only native run, sliders off vs on (2026-11-12 Africa/Cairo)

Candidate `4a41cb4` (M5 gate 1 fixes), release `roqa.exe` 925,696 B, SHA-256 `5B21B4F110A209D5B364272728FFEB0065F3073C04B6A1CE67FCBD345C81F19C` (path-embedding: differs from the ledger SHA `46B7E3FB...` at identical size). Applies to this SHA only. Reviews paused: **REVIEW NOT RUN**. Evidence: `artifacts/native-8/` (not committed). Idle samples 11:33-12:05 local.

**Machine:** Windows 11 Pro 10.0.26200; Ryzen 5 PRO 4650G (12 logical); Radeon driver 31.0.21925.1001; 2 monitors 1920x1080 at 144 Hz, AppliedDPI 96 (per-monitor not captured); AC power, display-off 900 s held awake by the harness.

**Conditions:** user confirmed the desktop free. Foreground: T3 Code (console session 1). No game or full-screen app. Riot Client Services and `vgtray` ran with `vgk` loaded (`vgc` stopped), no League or Valorant process, accepted by the user. One media session, **Paused**, so `smoke.ps1` was skipped. Sliders-on used a process-scoped temp `LOCALAPPDATA` whose store held `edgevol=1`, `edgebri=1`; the real `%LOCALAPPDATA%\ROQA` was not touched. No input aborted a sample.

## Suites

| Suite | Result |
|---|---|
| `verify.ps1` | PASS: fmt, clippy `-D warnings`, 232 tests, locked release, size ratchet (925,696 B within 2%) |
| `picker-smoke.ps1` | PASS: 226 checks |
| `catalog-smoke.ps1` | PASS: 67 checks |
| `packaged-hung-check.ps1` | PASS: 16 checks |
| `topology-check.ps1` (`none`) | **FAIL as shipped**: StrictMode error at line 95, `$strips` is `$null` for `none` (`switch` returns `@()`), so `.Count` throws before any check; host cleaned up, not interference. A copy with `@(switch ...)` (scratchpad, repo untouched): PASS, 25 checks (3 surfaces per monitor) |
| `topology-check.ps1 -EdgeSliders both` (sliders-on store) | PASS: 29 checks (5 surfaces per monitor, strips on edges over the middle third, graceful quit) |
| `smoke.ps1` | **NOT RUN**: media session active |

## Measurements (`artifacts/native-8/m5-off`, `m5-on`; 1 process, Complete=true, 2 monitors)

| 900 s idle, r1 | Sliders off | Sliders on |
|---|---|---|
| Private bytes (end) | 22,093,824 (21.07 MiB) | 22,069,248 (21.05 MiB) |
| CPU | 218.75 ms = 0.243 ms/s (cycles 1.04 M/s) WITHIN | 296.88 ms = 0.330 ms/s (cycles 0.99 M/s) WITHIN |
| Handles | 519 to 508 | 513 to 500 |
| GDI / USER | 40/25 to 40/23 | 40/29 to 40/27 |
| Threads | 22 to 19 | 22 to 18 |
| Wake proxy (switches/s) | 3.54 | 3.12 |
| Paints / media wakes | 30 / 0 | 30 / 0 |

CPU difference is 4 scheduler ticks (15.625 ms) and the cycle counter is lower with sliders on, so no CPU cost is shown. Sliders add 4 USER objects (2 strips x 2 monitors) and nothing else visible.

Start-warm (30 s, r1, INCONCLUSIVE CPU): off 22,261,760 B, 13.44 ms/s, 521 handles, USER 25; on 22,142,976 B, 16.01 ms/s, 515 handles, USER 29. Incl. startup; paints not captured. `store_writes` unreadable (null) in all runs.

## PLAN M5 gates

| Gate | Verdict |
|---|---|
| One-monitor private bytes <= 80 MiB | PASS: 21.07 off / 21.05 on MiB, measured with 2 monitors; one repetition each |
| Three-monitor private bytes <= 120 MiB | INCONCLUSIVE: 2 monitors only |
| Static CPU <= 1 ms/s over 15 min | PASS: 0.243 off / 0.330 on ms/s; one repetition each, paused media session present |
| Zero collapsed paints apart from the clock | INCONCLUSIVE: 30 paints in 15 min (2/min) in both, no per-source attribution; consistent with a once-a-minute clock on 2 monitors, not confirmed |
| Input p95 <= 50 ms / p99 <= 100 ms | NOT RUN |
| Bounded churn, DWM, GPU memory, power | NOT RUN |
| >= 30 % lower than Nectar/Nusa | NOT RUN (no pinned builds) |

**NOT RUN:** Nectar/Nusa, churn, cold start, input p95/p99, expanded media, `smoke.ps1`, a real edge-strip mouse test (hover, pill, drag, DDC, device switch), three-monitor, repetitions 2-3. Fix `topology-check.ps1` for `-EdgeSliders none` (`@(switch ...)`).
