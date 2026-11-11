# M5 S10: ROQA-only native results (2026-11-12 Africa/Cairo)

Candidate: `118a6f8`, release `target/release/roqa.exe` 803,840 B, SHA-256 `1873E1F2C6227E569D3FA10732B27AC131E57FA906FE60A947CE4F40E6A9F2B9`. Every result applies to this SHA only. Reviews paused: **REVIEW NOT RUN**. Evidence: `artifacts/native-6/` (not committed).

**SHA note:** the brief expected `D7592F2D...29AC2` (same size). This build ran in a different checkout path (`...\worktrees\roqa\test-native-6-...`), and the binary embeds the absolute path (10 occurrences found), so the hash differs with identical sources and size.

## Machine

Windows 11 Pro 10.0.26200; AMD Ryzen 5 PRO 4650G (12 logical, 3.9 GHz nominal); AMD Radeon Graphics, driver 31.0.21925.1001; 2 monitors 1920x1080 (144 Hz), system AppliedDPI 96 (per-monitor DPI not captured); AC power, display-off 900 s, no screen saver. Run 2026-11-11 23:0x-23:3x UTC.

## Conditions (honest limits)

- User on the desktop; the foreground window was a browser Picture-in-Picture window (not a game, not full screen). A media session was **Playing** (another app), so `smoke.ps1` was skipped. Nectar and Nusa were also running (not mine, untouched), which may add noise.
- The harness refuses a sample that would reach the 900 s display-off idle timeout; it cannot hold the display awake without a code change, so none was made (no power setting changed).

## Suites

| Suite | Result |
|---|---|
| `verify.ps1` | PASS: fmt, clippy `-D warnings`, 185 tests, locked release build |
| `catalog-smoke.ps1` | PASS: 67 checks |
| `packaged-hung-check.ps1` | PASS: 16 checks |
| `picker-smoke.ps1` | **INTERFERENCE** twice (Windows denied initial fixture foreground); not a product failure, not a pass. **NOT RUN** |
| `smoke.ps1` (incl. `--probe-panic` exit 70) | **NOT RUN**: media session active; it also needs a debug build, not made |

## Measurements (`artifacts/native-6/measure/`)

| Scenario | Result |
|---|---|
| idle 15 min | **NOT RECORDED**: 846 s attempt and one retry (836 s) both aborted by user input; the harness records nothing on abort |
| start-warm (30 s from process creation, hidden, 1 process) | private 21,782,528 B (20.8 MiB); CPU 375 ms = 12.41 ms/s (includes startup; CpuVerdict INCONCLUSIVE); handles 488; GDI 40; USER 25; threads 20; wake proxy 1,733 switches = 53.9/s; Complete=true |
| start-cold | **NOT RUN**: cold start cannot be attested without a reboot |
| churn | **NOT RUN**: needs a person to open and close windows; no automated driver is authorized |
| expanded media | **NOT RUN**: only another app's session was available, and expanding needs input |

S9 diagnostics over an idle window: **NOT CAPTURED** (no idle run completed; the harness does not pass `--diagnostics`). The report has no paint counter, only `media_wakes` and `media_refreshes`; paints and wakes per source are not available in this build. From the catalog-smoke fixture run (not an idle window): media_wakes 5, media_refreshes 21,955, private 36.0 MB, handles 556, GDI 40, USER 23, 2 monitors.

## PLAN M5 gates

| Gate | Verdict |
|---|---|
| One-monitor private bytes <= 80 MiB | INCONCLUSIVE: one 30 s start-window point, 20.8 MiB, below the limit; 2 monitors attached; no 15 min sample |
| Three-monitor private bytes <= 120 MiB | NOT RUN (2 monitors) |
| Static CPU <= 1 ms/s over 15 min | INCONCLUSIVE: no 15 min sample; the 30 s start window (12.4 ms/s) includes startup and is not a static figure |
| Zero collapsed paints apart from the clock | INCONCLUSIVE: no paint counter, no idle window |
| Input p95 <= 50 ms / p99 <= 100 ms | NOT RUN |
| Bounded churn, DWM, GPU memory, power | NOT RUN |
| >= 30 % lower than Nectar/Nusa | **NOT RUN (no pinned Nectar/Nusa builds)** |

**Needed to close:** a user-free desktop with input untouched for 15 min, and the harness display guard waived while the process holds `ES_DISPLAY_REQUIRED` (needs a reviewed harness change); a `--diagnostics` idle run; a paint counter; a person-driven churn run; a reboot for cold start.
