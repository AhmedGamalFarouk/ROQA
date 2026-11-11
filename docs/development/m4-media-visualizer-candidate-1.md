# M4 media visualizer, candidate 1 (slice 7: visualizer, plus unmute-on-set)

Status: **CANDIDATE. REVIEW NOT RUN (paused by user).** Nothing here is approved or accepted. The author approves nothing.

- **Author:** Claude Opus 5.5 High (`claude-opus-5-5`), a ROQA worker thread launched by the ROQA Chief Thread at the user's request ("start working on the next features"). Sole source writer in worktree `feat-m4-visualizer-29fbb01fac4ab1bb`, branch `feat/m4-visualizer`.
- **Base:** local `main` `8443a1e` (slice 6, volume). `origin/main` is behind at `6200705` because GitHub was unreachable; work is from local `8443a1e`. Uncommitted, by instruction; the Chief lands it.
- **Scope change (user, 2026-11-11, via the Chief):** (1) no review workers for now: step 4 of the brief (clean-code-guard review rounds) is skipped and no subagent was started; (2) setting the volume from the notch while muted also unmutes. Both are in this candidate.
- **Code style:** written under `/ponytail:ponytail` (smallest complete change), performance and footprint first. No new thread, timer, wait handle, FFT, loopback capture, compositor clock or allocation per sample.

## What changed

| File | + / − | Change |
|---|---|---|
| `src/audio.rs` | +115 / −9 | `IAudioMeterInformation` activated from the bound endpoint in `bind` and held in `Endpoint` (released with it, rebound with it). A fixed `[f32; BARS]` shift ring (`push`), `metered`, `peaks`, `sample` (one `GetPeakValue`) and `stop`. Rebinding flattens the ring. `set` also calls `SetMute(false, &ECHO)` when the shown level is muted (`unmutes`). 2 tests |
| `src/host.rs` | +99 / −3 | `METER_PERIOD` (67 ms), the pure gate and rate function `meter_due`, `Desktop::refresh_meter` joined into `update`'s wait minimum, `Desktop::sampled`, `Surface::bars` (heights last drawn) set in `paint_notch`. 1 test |
| `src/layout.rs` | +54 / −2 | The visualizer slot `METER` and bar width, `meter`, and `meter_heights` (whole-pixel quantization). 1 test |
| `src/graphics.rs` | +74 / −2 | `NotchScene::bars`, `MediaPlan::meter`, `meter_bars` and its draw (one brush). The slice-5 art-upload shortcut comment now lists the visualizer repaints. 1 test |
| `docs/design/roqa-visual-spec.md` | +9 / −4 | PROVISIONAL visualizer placement and behaviour, the repaint note, and unmute-on-set in the volume row |
| `docs/development/m4-media-visualizer-candidate-1.md` | new | This packet |

No change to `Cargo.toml`, `Cargo.lock`, scripts, UIA code or `.gitignore`.

## Decisions

1. **Data source.** `IAudioMeterInformation` activated on slice 6's bound default render endpoint (eRender/eConsole) with `IMMDevice::Activate`, right after the volume interface. It is endpoint-wide (all apps mixed, before endpoint volume), as the user accepted on 2026-11-10. It is not per player and not a spectrum.
2. **One call per sample.** Each sample is exactly one `GetPeakValue` call. `GetChannelsPeakValues` is not used (prep rule 6), so mono, stereo and multichannel endpoints take the same path.
3. **Binding and release.** The meter lives in `Endpoint`, so it is released with the endpoint and rebound with it on every `REBIND` (default change, device added, removed or state change). An endpoint without a meter keeps a working volume row; the bars stay flat. A failed `GetPeakValue` logs once, drops the meter and makes no further call until the next rebind, so a lost device is never polled.
4. **Gate.** `meter_due` returns a deadline only when a visible notch is open or opening (`Surface::expanded`: visible and `motion.target() >= 1`, so collapsed, hidden and full-screen-suppressed all fail it), the shown track is playing, and the endpoint has a meter. Otherwise `refresh_meter` makes no meter call, flattens the ring (a 20-byte store) and returns `INFINITE`. With no open notch it paints nothing.
5. **Rate bound.** A host-loop deadline, joined into `update`'s existing wait minimum alongside the clock and progress waits. The next sample is due `METER_PERIOD` (67 ms) after the previous sample's actual time, not its scheduled time, so a late wake never causes a catch-up burst: at most 15 samples a second. The last sample time outlives a stop, so a quick stop and restart cannot sample faster either. Early wakes (input, other deadlines) only return the remaining wait. There is no compositor clock; while a notch animates, the existing frame loop paints it and the meter adds no second tick.
6. **Ring.** `[f32; BARS]` with `BARS = 5`, oldest first: `rotate_left(1)` and the new peak clamped to 0..=1 (NaN to 0) in the last slot. No allocation. It is flattened on stop and on rebind, so a reopened panel never shows old samples.
7. **Quantize and skip unchanged.** `layout::meter_heights` maps each peak to whole physical pixels: bar width (a dot at silence) plus `round(peak × (slot height − bar width))`. `refresh_meter` repaints a settled open notch only when its `meter_heights` differ from the heights it last drew (`Surface::bars`), so silence or a steady level costs samples but no paints.
8. **Paused.** `paint_notch` draws flat bars whenever the track is not playing, so the pause repaint already shows them flat and the next pass's stop causes no second paint. With no meter or after a stop, the ring is zero and the bars are flat dots.
9. **Placement (PROVISIONAL).** No geometry was pinned (the addendum only reserved "the visualizer slot right of the controls"). Minimal choice inside the 360 × 120 panel: five 4 DIP fully rounded bars spread across x 312–344 (trailing 16 from the right, like the progress total and the volume bar), bottom-aligned at y 92 in a 20 DIP slot (72–92), centred on the controls row (66–98). White 60 %, like the artist, revealed and placed like the controls (`motion::reveal`, body clip). A layout test proves no overlap with the controls, the progress target or the volume bar at 96, 144, 192 and 240 DPI. It needs design review.
10. **Reduced motion.** The user decided "always animate"; no existing code special-cases reduced motion, so neither does the visualizer.
11. **UIA.** The bars are decorative and are not part of the notch UIA provider, which exposes only the listed controls; nothing to hide.
12. **Unmute on set (user, 2026-11-11).** `Audio::set` is called only from a notch click on the volume bar. When the shown level is muted, it follows `SetMasterVolumeLevelScalar` with `SetMute(false, &ECHO)`, ROQA's own event context, so the echo wakes nothing; the existing `changed` queues the one re-read, which updates the level, the repaint and the UIA name to "Mute, volume N%". External level changes never touch mute (they arrive only as `OnNotify` re-reads). The mute toggle is unchanged. A failed level set skips the unmute.

## Footprint

| Item | Value |
|---|---|
| Base release `roqa.exe` (`8443a1e`) | **794,624 B** (the figure in the brief and `progress.md`; not rebuilt here) |
| Candidate release `roqa.exe` | **800,768 B**, SHA-256 `035507119E9A3E935848FA9E2ED9F4FC41DE9DB64B264CD5FD9F51B01C085E34` |
| Delta | **+6,144 B (+0.77 %)** |
| New `windows` features | **None.** `IAudioMeterInformation` is in `Win32_Media_Audio_Endpoints`, enabled by slice 6 (checked in the pinned `windows` 0.62.2 source under `.tools`) |
| New crates | None |
| New ROQA threads, timers, wait handles | 0. One host-loop deadline, active only while open, playing and metered |
| Meter calls and paints | At most 15 a second each per open notch while open and playing; 0 otherwise (by construction, unit-tested gate; not measured) |
| Per-paint cost while running | A full notch repaint, including uncached text layouts and, with art, the art upload (the existing slice-5 shortcut), up to 15 times a second while open and playing. Bounded and unmeasured |
| Runtime CPU, private bytes, handles | NOT MEASURED |

## Verification (author-run; not a review)

- `scripts/check.ps1`: PASS. fmt, clippy `--all-targets -D warnings`, **99 tests passed**, 0 failed (94 at base, plus 5 new).
- `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-visualizer-1/round-1/verification`: PASS. checks exit 0, `build --locked --release` exit 0; logs are in that folder.
- New tests:
  - `host::clock_wait_tests::the_meter_runs_only_open_playing_and_metered_and_never_above_15_hz`: no deadline when not open (collapsed, hidden or full-screen-suppressed), paused, or without a meter (no device), with or without a previous sample; immediate when it starts. A wake sequence that is early, on time, late, and stopped then restarted within a period samples at 0, 67, 300, 367 and 1000 ms only, every gap at least 67 ms.
  - `layout::meter_bars_quantize_to_whole_pixels_and_fit_beside_the_controls`: heights at 96 and 192 DPI, sub-pixel and steady changes quantize equal (no repaint), and the slot fits the body and overlaps no control, the progress target or the volume bar at four DPIs.
  - `audio::peaks_shift_the_oldest_out_clamp_and_reset_on_rebind`: the ring drops the oldest sample, clamps out-of-range and NaN peaks, makes no change without a meter, and is flattened by a rebind.
  - `audio::setting_the_level_unmutes_only_a_muted_endpoint`: a set while muted unmutes; a set while unmuted makes no mute call.
  - `graphics::meter_bars_sit_right_of_the_controls_bottom_aligned_and_fade_in_with_them`: bar positions (312–344), bottom edge 92, heights, opacity and reveal, and nothing collapsed.
- A rerun of verify must use a different evidence folder, or the frozen log entries change.

## Native evidence plan: all NOT RUN

This slice is static and unit-test only. No native or GUI run was made, and the system volume and mute were never changed. Each run below needs fresh, explicit desktop-free confirmation from the user via the Chief, a foreground-window check first (scripts inject input and move the cursor), and separate authorization for any audio-setting change. Only the owned test process may be stopped, preferably with `roqa --quit`.

| # | Scenario | Pass looks like |
|---|---|---|
| 1 | Meter-call and paint counts (a counting debug build or ETW), playing audio, panel expanded for 60 s, then collapsed for 60 s | Expanded: at most 15 meter calls and 15 visualizer paints a second per open notch; silence or a steady tone gives samples but almost no paints. **Collapsed: 0 meter calls and 0 visualizer paints**; also 0 when hidden by a full-screen app, paused, and with no player |
| 2 | CPU while expanded and playing, against expanded and paused | ROQA CPU per second for both; the difference is the visualizer's cost |
| 3 | Default-device switch while expanded and playing | One rebind; bars restart flat then follow the new endpoint; no error lines; volume row follows as in slice 6 |
| 4 | Mono, stereo and multichannel (5.1 or 7.1) endpoints | `GetPeakValue` succeeds and the bars move on each; no "Audio: peak" or "no peak meter" lines |
| 5 | Unplug the default device while expanded | Rebinds or disables; with no endpoint, no meter call and flat bars; recovers on replug |
| 6 | Exclusive-mode stream | Bars flat (the documented 0.0 for a software meter); no errors |
| 7 | Unmute on set: mute, then click the volume bar (pointer); also an external level change while muted | ROQA click: level set and unmuted, glyph and UIA name read "Mute, volume N%". External change: stays muted |
| 8 | Footprint | Private bytes, handles and threads while expanded and playing, compared with base |

## UNVERIFIED

1. The per-call CPU cost of `GetPeakValue`, and whether it can block during a driver reset. If it can, sampling moves off the host thread in a later slice.
2. `GetPeakValue` behaviour on real mono, stereo and multichannel endpoints, Bluetooth and USB devices, and exclusive-mode streams.
3. Whether `IMMDevice::Activate` for the meter can fail on real endpoints that support volume (handled: flat bars, logged).
4. The visual result of linear peaks: quiet sources may look small. A perceptual curve is a design-review question, not built.
5. That `SetMute(false, &ECHO)` arrives as one echo notification that wakes nothing; the same documented behaviour slice 6 relies on, not re-checked natively.
6. Every runtime footprint figure.

## Deferred and non-goals

- FFT or spectrum, loopback capture, per-app or per-player meters, and any collapsed visualizer: non-goals.
- Design review of the PROVISIONAL placement, colour and peak curve.
- Caching the art bitmap per canvas (slice-5 shortcut), now also hit by visualizer repaints; upgrade if profiling shows cost.
- From slice 6, still deferred: drag on the bar, arrow-key or RangeValue adjustment, UIA property-change events on level change, and RTL mirroring of the row. Unmute-on-set is no longer deferred: it is built here.
- **Review: REVIEW NOT RUN (paused by user).** No clean-code-guard review, no council review and no other subagent was started. This candidate is not accepted.

## Manifest

Frozen at `artifacts/m4-visualizer-1/round-1/inputs.sha256` after this packet was written; its SHA-256 is reported to the Chief outside this file, since the manifest includes this packet.
