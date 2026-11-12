# M5 results 4: native run on main 16d5785 (2026-11-12 Africa/Cairo)

Release `roqa.exe` 1,223,680 B (m5-results-3: 925,696 B; B5 ledger 1,224,192 B), SHA-256 `39743F402B49B3769FC5CE6A3E1F7DF6802A5A75596A6D92A2717E1806E063EB`. Built first (`scripts/cargo.ps1 build --locked --release`), no build during any sample. Reviews paused: **REVIEW NOT RUN**. Evidence: `artifacts/native-9/` (not committed). Source untouched.

**Conditions:** user confirmed the desktop free. Foreground before each run: T3 Code, then OpenBot (not a game). Nectar and Nusa windows exist on the machine and `vgtray` ran; no League/Valorant process. ROQA was not running. Every ROQA launch except the first picker, catalog and hung runs used a private `%LOCALAPPDATA%` store (scratchpad); the user's real store was only read by those first runs (it has `edgevol=1`, `edgebri=1`, Start/Search/Min/Desktop dock buttons on).

## Suites

| Suite | Result |
|---|---|
| `picker-smoke.ps1` | **FAIL as shipped**: "Dock slot 0 is the invokable picker button". Default dock now has Start (D2) in slot 0, so the script's slot-0 assumption is stale (real and empty private store both fail). With a private store `dbstart=dbsearch=dbtask=dbmin=dbdesk=0`: PASS, 226 checks |
| `catalog-smoke.ps1` | PASS: 67 checks (real store) |
| `packaged-hung-check.ps1` | PASS: 16 checks |
| `topology-check.ps1 -EdgeSliders none` | PASS: 25 checks, 6 of 6 surfaces (empty private store; the first run on the real store saw 10 surfaces because its sliders are on: not a defect) |
| `topology-check.ps1 -EdgeSliders both` | PASS: 29 checks, 10 of 10 surfaces (private store `edgevol=1 edgebri=1`). One earlier attempt failed "launch does not change the foreground window" (user switched windows); the rerun passed |
| `smoke.ps1` | **NOT RUN**: media-session state not verified |

## Idle measurements (15 min)

All three runs **ABORTED by the harness input guard** (user input arrived): nothing recorded, so the numbers below do not exist.

| Run | Aborted at | Result |
|---|---|---|
| m5-off (defaults, sliders off) | 150 s | NOT RUN (aborted) |
| m5-on (sliders on) | 332 s | NOT RUN (aborted); start snapshot only: 19,496,960 B private, 380 handles, USER 34, GDI 4, 16 paints, 1 media wake at t0 |
| m5-tabs (Weather+Prayer+Limits, fixed `wxplace`, no city) | 10 s | NOT RUN (aborted) |

Per the brief the sequence stopped on user activity; I did not retry. The start snapshot is not a measurement.

## Comparison with m5-results-3

| Metric | m5-results-3 | Now |
|---|---|---|
| Binary | 925,696 B | 1,223,680 B (+32 %; ratchet baseline raised to 1,218,048 by U1, approved) |
| Private bytes / CPU / paints per min | 21.07 MiB / 0.243 ms/s / 2 | INCONCLUSIVE (no complete run) |

**Dock/notch `--diagnostics` counters:** NOT RUN (idle runs aborted). No regression can be claimed or excluded for memory, CPU, paints, wakes.

**Needed:** three uninterrupted 15-minute idle runs with the user away from keyboard and mouse. Update `picker-smoke.ps1` for the default Start slot (or pin the dock buttons off in the script).
