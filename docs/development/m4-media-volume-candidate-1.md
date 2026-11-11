# M4 media volume, candidate 1 (slice 6: volume and device events)

Status: **CANDIDATE, REVIEW PENDING.** Nothing here is approved. The author approves nothing.

- **Author:** Claude Opus 5.5 High (`claude-opus-5-5`), a ROQA worker thread launched by the ROQA Chief Thread at the user's request ("start working on the next features"). Sole source writer in worktree `feat-m4-volume-83c248a3d92e067e`, branch `feat/m4-volume`.
- **Base:** local `main` `450d46c` (equal to `origin/main`). Uncommitted, by instruction; the Chief lands it.
- **Parallel work:** slice 4 (progress and seek) and slice 5 (album art) are on other branches. This slice keeps Core Audio in a new module, `src/audio.rs`, and its edits to shared files are small and additive. It touches no progress, seek or art code.
- **Code style:** written under `/ponytail:ponytail` (smallest complete change). No audio abstraction layer, no polling, no new thread.

## What changed

| File | + / − | Change |
|---|---|---|
| `src/audio.rs` (new) | +365 | Core Audio owner (`Audio`), one `IMMNotificationClient` and one `IAudioEndpointVolumeCallback` via `#[implement]`, the wake, the echo context, the shown `Level`, and 3 tests |
| `src/host.rs` | +68 / −29 | `audio` field; lazy start; `refresh` in `update`; repaint gate; `press` (media command or mute); `enabled()` with four controls; `step` generic over the control count; click on the bar sets the volume |
| `src/graphics.rs` | +138 / −4 | `NotchScene::volume`; speaker and muted glyphs as the fourth chip; the level bar (`VolumeBar`); 1 new test and the chips test extended by the mute chip |
| `src/layout.rs` | +62 / −8 | Mute toggle as the fourth `MEDIA_CONTROLS` entry; `volume_bar`, `volume_at`; 1 new test |
| `src/notch_uia.rs` | +38 / −2 | `mute()` name and enabled state; AutomationId `RoqaVolumeMute`; name assertions added to an existing test |
| `src/main.rs` | +1 | `mod audio;` |
| `Cargo.toml` | +4 | Two `windows` features, each with a one-line justification comment |
| `docs/design/roqa-visual-spec.md` | +14 / −3 | PROVISIONAL volume row in the "Cover accent" addendum; repaint rule; UIA names |

Line counts are `git diff --numstat 450d46c` (normalized), plus the new file's line count.

## Decisions

1. **Lazy start: at the first shown media track, not at media start.** `Desktop::update` calls `Audio::start` once `media.view().shown()` is first `Some`. Before that, Core Audio is never loaded: no `MMDeviceEnumerator`, no callbacks, no MMDevAPI or endpoint-volume DLL loads. The panel can only open on a shown track, so this is the first moment the row can be seen. Starting at media start would load Core Audio on machines that never play media, for the same code size. The first-shown pass binds and reads in the same `update`, so the first panel paint already has the level. Once started, the binding stays until shutdown: re-registering on every track end would add churn for no visible gain.
2. **Endpoint.** `GetDefaultAudioEndpoint(eRender, eConsole)`, the overall level (user decision, 2026-11-10). No per-app sessions.
3. **Callbacks only post a wake.** Each callback sets a bit in an `AtomicU32` and posts one scalar thread message (`WM_APP + 51`) only when no bit was pending. It never blocks, allocates, panics, registers or unregisters, and never touches `Desktop`, HWNDs or Direct2D. A lost post leaves the bit set for the loop's next finite deadline, the clock's (at most a minute); this is the same accepted limit as media and text settings. Audio has its own small `Wake` instead of sharing `media::Wake`, because that one records the caller thread for media diagnostics and audio callbacks would overwrite it.
4. **Rebind rule.** `OnDefaultDeviceChanged` raises `REBIND` only for `eRender` + `eConsole`. `OnDeviceAdded`, `OnDeviceRemoved` and `OnDeviceStateChanged` always raise `REBIND`, because any of them can create, remove or restore the default endpoint. That covers the no-device case without retries. `OnPropertyValueChanged` raises nothing. On the host thread a `REBIND` drops the old `Endpoint` first, which unregisters its callback and releases it, then binds the current default afresh. There is no device-ID comparison: these events are rare, and a rebind repaints only if the shown level changed.
5. **Echo suppression.** ROQA's own `SetMasterVolumeLevelScalar` and `SetMute` pass a fixed event context GUID (`ECHO`). `OnNotify` ignores a notification carrying it. The setter queues its own re-read by setting the `VOLUME` bit locally, without a post: the click arrives in `drain_messages`, and the loop's next `update` runs right after it. A null notification pointer still wakes. Any other context, such as the Windows flyout or volume keys, wakes and is re-read.
6. **Re-read on notify.** On `VOLUME`, the host calls `GetMasterVolumeLevelScalar` and `GetMute`; it never trusts the callback's payload.
7. **Repaint only on a shown change.** The shown state is `Option<Level { percent, muted }>`, with `percent` the rounded whole percent, as Windows shows it. The bar's fill and the UIA name are drawn from it alone, so a change smaller than a percent draws identical pixels and repaints nothing. A shown change republishes UIA for every notch (through the existing `reconcile_focus`), but paints only visible notches whose panel is open or opening (`motion::reveal(progress) > 0`). A collapsed pill draws no volume, so it never repaints for one. Hidden notches are painted current before they are restored, as before.
8. **Set by click or tap.** A `WM_LBUTTONUP` on the bar maps x linearly: the first pixel is 0.0 and the last 1.0 (`layout::volume_at`). A click on the toggle flips mute. Setting the level leaves mute unchanged: whether Windows' own slider unmutes is not verified here, so ROQA does not imitate it. There is no drag and no hover or press state, like the controls row.
9. **Placement.** The volume row is y 100–116 in the 360 × 120 panel: the mute toggle 24 × 16 at x 100 (the title column's left), and the bar's hit box 212 × 16 at x 132, ending 16 before the body's right edge, like the clock. The prep doc and the addendum reserve "about 101–115" for volume. Slice 4's progress row (about 50–64) and slice 7's visualizer slot (right of the controls row, 66–98) stay untouched. Nothing existing moved. The geometry is marked PROVISIONAL in the addendum.
10. **The mute toggle is the fourth control.** It is the fourth `MEDIA_CONTROLS` entry, so hit-testing, the UIA bounding rectangle, `ElementProviderFromPoint`, the focus ring and Tab/arrow order all reuse the slice-3 code. `step` became generic over the control count; its test is unchanged.
11. **UIA.** The toggle is a fourth Invoke Button: "Mute, volume 40%", or "Unmute, volume 40%" while muted. Without an endpoint it is "Volume unavailable, no audio device", disabled: present in the tree, refusing focus and Invoke. The level is read-only text inside that name. There is no RangeValue and no arrow-key adjustment, because the arrows already move focus. No property-change event is raised when the name changes; that limit is shared with play/pause and already deferred (slice-3 ledger).
12. **No-device state.** With no default render endpoint, or after a failed bind or read, the toggle and the bar's fill draw at 30 % with an empty fill, and the toggle is disabled for pointer, keyboard and UIA. Nothing polls or retries: the next device notification rebinds. If `CoCreateInstance` or `RegisterEndpointNotificationCallback` fails at start, the failure is logged once and the row stays unavailable for the session.
13. **Teardown.** `Audio` field order is teardown order. `Endpoint` (`UnregisterControlChangeNotify`, then release) drops before `Devices` (`UnregisterEndpointNotificationCallback`, then release). A failed unregister is logged and never panics. `Desktop` declares `audio` after `media` and is dropped on the host thread before the COM `Apartment` guard. The `IMMDevice` from `GetDefaultAudioEndpoint` is released on the host thread inside `bind`, never in a callback.

## `windows` crate features

| Feature | Why | Size cost |
|---|---|---|
| `Win32_Media_Audio` | `IMMDeviceEnumerator`, `MMDeviceEnumerator`, `IMMNotificationClient(_Impl)`, `EDataFlow`/`ERole`, `DEVICE_STATE`, `AUDIO_VOLUME_NOTIFICATION_DATA` | Not measured separately |
| `Win32_Media_Audio_Endpoints` | `IAudioEndpointVolume`, `IAudioEndpointVolumeCallback(_Impl)` | Not measured separately |

Both were named in the prep doc for slice 6 (line "Features to add in the slice that first needs them"). Features only gate which bindings compile; unused bindings are not linked. The binary delta below covers the features together with all new code. Attributing it between them would need a probe build, which was not done.

## Footprint

| Item | Value |
|---|---|
| Base release `roqa.exe` at `450d46c` | 740,352 B (figure given in the brief; not rebuilt in this worktree) |
| Candidate release `roqa.exe` | **763,392 B**, SHA-256 `73EFAC8C60DF7F762022DF5DDA447A5612D9DB04573721E6768E2BB2F9F82478` |
| Delta | **+23,040 B (+3.11 %)** |
| New ROQA threads, timers, deadlines, polls, wait handles | 0 |
| Runtime private bytes, handles, Core Audio pool threads | NOT MEASURED (prep doc estimate about 0.5–2 MiB, unverified) |

## Verification (author-run; not a review)

- `scripts/check.ps1`: PASS. fmt, clippy `--all-targets -D warnings`, **76 tests passed**, 0 failed (71 at base, plus 5 new).
- `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-volume-1/round-1/verification`: PASS. checks exit 0, `build --locked --release` exit 0; logs are in that folder.
- New tests:
  - `audio::only_the_default_console_render_endpoint_and_device_changes_rebind`: the real `IMMNotificationClient` object. Default-changed for render/console rebinds; render/multimedia, render/communications and capture/console do not; device removed, added and unplugged rebind; property changes do not.
  - `audio::our_own_changes_echo_without_a_wake_and_others_wake`: the real callback object. `ECHO` context does not wake, another context does, and null data does.
  - `audio::only_a_whole_percent_or_mute_change_repaints`: percent rounding and clamping (NaN, below 0, above 1). Sub-percent changes compare equal; percent or mute changes do not. Without an endpoint a volume wake changes nothing, and a rebind that loses the endpoint reports a change once.
  - `layout::volume_bar_maps_its_first_and_last_pixel_to_silent_and_full`: x to scalar at the first, middle and last pixel; just outside on each side and above and below; the mute hit; collapsed and swelling progress; 150 %, 200 % and 250 % edges inside the body.
  - `graphics::volume_row_fills_to_the_shown_percent_and_dims_while_muted_or_unavailable`: track and fill rectangles, alphas, speaker and muted glyphs, no-endpoint look, the focus ring around the toggle, nothing collapsed, and a full fill at 200 %.
- A reviewer re-running verify must use a different evidence folder, or the frozen log entries change.

## Native evidence plan: all NOT RUN

This slice is static and unit-test only. No native or GUI run was made, and the system volume and mute were never changed. Each run below needs fresh, explicit desktop-free confirmation from the user via the Chief, a foreground-window check first, and separate authorization for the audio changes it makes. Only the owned test process may be stopped, preferably with `roqa --quit`.

| # | Scenario | Pass looks like |
|---|---|---|
| 1 | Default-device switch: change the default output between two endpoints in Windows Sound settings while the panel is open | One `Audio: rebound, endpoint=true` line per switch; the bar shows the new endpoint's level; mute follows the new endpoint |
| 2 | Unplug: remove the default device, with and then without another device present | Rebinds to the next default; with none, the row disables, UIA says "Volume unavailable, no audio device", there is no retry (log quiet), and it recovers on replug |
| 3 | Mute: the ROQA toggle (pointer, keyboard Enter/Space, Narrator Invoke), and externally (flyout or mute key) | Glyph and fill dim state and UIA name flip; external mute updates ROQA within one wake |
| 4 | Volume change: a ROQA bar click at both ends and mid-bar; external volume keys | Level set to 0 %, 100 % and the clicked percent; external changes are re-read; a collapsed pill does not repaint (paint count) |
| 5 | Echo: a ROQA change versus an external change | A ROQA change causes one host re-read and no callback wake (breakpoint or ETW; the log does not count volume wakes); an external change wakes once per notification |
| 6 | Teardown | `roqa --quit` exits 0 with no "cleanup" error lines; no Core Audio callback after exit |
| 7 | Footprint | Private bytes, handles and threads before and after the first shown track, compared with base |

## UNVERIFIED

1. Which threads deliver the two callbacks, and how many Core Audio or RPC pool threads they add.
2. Whether `GetDefaultAudioEndpoint`, `Activate` or the volume getters can block during device arrival or driver resets. If they can, the bind moves off the host thread in a later slice.
3. That the event context passed to `Set*` arrives unchanged in `OnNotify`. This is documented behaviour, not re-checked natively.
4. Bluetooth and USB endpoint behaviour, hardware-stepped volume (the shown percent may differ from the clicked one; it is re-read), and exclusive-mode streams.
5. The runtime footprint figures above.
6. Rust 1.91 aborting rather than unwinding on a panic out of an `extern "system"` callback. Not re-checked here; the callbacks contain no panicking code path.

## Deferred and non-goals

- Per-app sessions, the visualizer and meter (slice 7), and volume in the collapsed pill: out of scope.
- Drag on the bar, arrow-key or RangeValue adjustment, UIA property-change events on level change, RTL mirroring of the row, and unmute-on-set.
- Council reviews (Sol and Opus) per `PLAN.md`: not requested by this brief. Only the independent clean-code-guard review below.

## Review

Independent clean-code-guard review on Claude Opus 5.5 High via a fresh `delegate_task`; the reviewer is not the author. Up to 3 rounds, each a new task on a new frozen manifest at `artifacts/m4-volume-1/round-N/inputs.sha256`. Verdicts, report paths and SHA-256s go to the suggested progress entry, not into this frozen packet.

## Round 2 changes (after the round-1 review)

Round 1 (fresh Claude Opus 5.5 High, non-author) returned ACCEPT WITH NITS: 0 blocker, 0 major, 1 minor, 3 nits (`artifacts/m4-volume-1/round-1/review-opus.md`). The author applied all four; they are fix-submitted until an independent recheck:

- **M4V1-OPUS-MINOR-1:** the bar's track was white 20 %, about 1.66 : 1 on black. It is now its own token, `VOLUME_TRACK` = white 40 %, about 3.7 : 1, which meets the 3 : 1 non-text contrast for a target. The cover slot keeps `COVER_FILL`. The addendum is updated.
- **M4V1-OPUS-NIT-1:** `Audio::set` now acts only while a level is shown, as `toggle_mute` does. A click on a row that shows "unavailable" after a failed read no longer changes the system volume.
- **M4V1-OPUS-NIT-2:** the "Properties not read yet" comment is back above the code it describes in the `notch_uia` test.
- **M4V1-OPUS-NIT-3:** the `notch_uia.rs` module doc is rewrapped to 100 columns.

Round-2 verification and footprint are recorded in `artifacts/m4-volume-1/round-2/verification` and the round-2 review; the round-1 figures above stay as evidence for round 1.

## Round 3 changes (after the round-2 recheck)

Round 2 (fresh Claude Opus 5.5 High, non-author) returned ACCEPT WITH NITS: the four round-1 findings resolved, and one new minor caused by the round-2 track change (`artifacts/m4-volume-1/round-2/review-opus.md`). The author applied it; it is fix-submitted until an independent recheck:

- **M4V2-OPUS-MINOR-1:** with the 40 % track, the muted fill at 30 % stood out from the track at only about 1.89 : 1. The muted fill now has its own token, `MUTED_FILL` = white 60 %: about 3.2 : 1 against the track, and still visibly dimmer than the audible white fill (about 1.8 : 1). The no-endpoint fill keeps `DISABLED` (it is empty). The addendum is updated.

Round-3 verification is in `artifacts/m4-volume-1/round-3/verification`. Round 3 is the last review round this brief allows.

## Integration 1: onto slices 4 and 5 (`6200705`)

Round 3 was ACCEPT (`artifacts/m4-volume-1/round-3/review-opus.md`). Slices 4 (progress and seek, `624e14f`) and 5 (album art, `6200705`) then landed on `main`. At the user's request, the same author rebased the accepted state onto local `main` `6200705` as one local WIP commit, `eabf0e8`, on `feat/m4-volume`. Nothing was committed to `main` and nothing was pushed. The accepted pre-rebase state is preserved in `artifacts/m4-volume-1/pre-rebase.diff` and `artifacts/m4-volume-1/pre-rebase-files/`.

**Conflicts and resolutions.** The conflicts were in `Cargo.toml`, `src/host.rs`, `src/graphics.rs`, `src/layout.rs` and the visual spec. Every resolution keeps both sides:

- **`Cargo.toml`.** Slice 5's `Storage_Streams` and this slice's two Core Audio features are all kept.
- **`src/host.rs`.**
  - `Surface::paint_notch` keeps slice 4's revealed-only progress read (`self.played`) and gains the `&Audio` parameter; `NotchScene` takes both `played` and `volume`.
  - Slice 4's `refresh_progress` repaint call and the slice 4/5 callers now pass `&self.audio`. Their semantics are unchanged.
  - `Desktop` field order is `media`, `icons`, `audio`. Slice 5's rule holds: `icons` is dropped after `media`. `audio` only has to drop before the COM apartment guard, which it does.
  - `update` keeps slice 5's `media.refresh(self.cover_pixels())`, then this slice's lazy `audio.start()` and `audio.refresh()`. Slice 4's `refresh_progress` deadline is still joined with the clock's afterwards.
  - The click order is: control (`press`, which routes mute), then seek (`media_seek_at`), then volume (`volume_at`).
- **`src/graphics.rs`.**
  - `MediaPlan` carries slice 4's `bar`, slice 5's `art` and `badge`, and this slice's `volume`.
  - The paint order is: art or cover slot, then badge, then progress bar, then text, then chips (including the mute toggle), then the volume bar, then the focus ring last.
  - The test scene builders are `played(...)` (slice 4) and `volume(...)`, both delegating to one `planned(...)`. Slice 5's art test scene adds `volume: None`.
- **`src/layout.rs`.**
  - `volume_at` now reuses slice 4's `panel_offset` (reveal gate and body clip) instead of its own copy, with the same results; the round-3 edge tests are unchanged and pass.
  - `media_progress` now uses `scaled_bounds`, giving the same bounds.
  - Both slices' tests are kept. The volume test also proves that no panel point hits two of {control, progress bar, volume bar} at 96, 144 and 192 DPI, so the hit order cannot matter.
- **Visual spec.** Both PROVISIONAL rows are kept, and the volume repaint rule is added to slice 4's repaint paragraph. The volume track (white 40 %) is brighter than slice 4's progress track (white 20 %) for target contrast; the spec leaves aligning them to design review. Slice 4 was not changed.

**Panel layout, 360 × 120, in DIPs from the body.**

| Element | Box | Overlap |
|---|---|---|
| Cover slot or art | (16, 14)–(88, 86) | None |
| Progress row | elapsed text 100–136, bar target 140–304, total text 308–344; y 49/50–64/65 | None |
| Controls | x 100–228, y 66–98 | None |
| Volume row | mute toggle 100–124, bar target 132–344; y 100–116 | None |

Only one focus ring is drawn at a time. The controls row's ring reaches y 101, and the mute toggle's ring spans 97–119. Each touches the neighbouring row's box by at most 1 DIP, outside any glyph.

**UIA.** The indices are previous 0, play/pause 1, next 2, mute 3 (`audio::MUTE`). Slice 4's progress `ItemStatus` stays on play/pause (index 1) only. The names and AutomationIds are unchanged from round 3.

**Equivalence.** `src/audio.rs` and `src/main.rs` are byte-identical to the accepted round-3 commit (`2e84211`). The other differences from round 3 are the resolutions above, plus content that came from slices 4 and 5. Evidence: `artifacts/m4-volume-1/integration-1/range-diff.txt` (`git range-diff 450d46c..2e84211 6200705..eabf0e8`) and `integration.diff` (`git diff 6200705 eabf0e8`).

**Verification.** `scripts/check.ps1` and `scripts/verify.ps1 -EvidenceDirectory artifacts/m4-volume-1/integration-1/verification` both PASS: fmt, clippy `-D warnings`, **94 tests passed** (89 on `6200705` plus 5), and `build --locked --release`.

**Footprint.** Release `roqa.exe` is **794,624 B**, SHA-256 `5EEB9DA321E492AEE42B16311D5458FCC6C6EB8E3E8FC526C09FFEACF4AC119C`. Against `6200705`'s 774,144 B (the figure given by the user; not rebuilt here) that is **+20,480 B (+2.65 %)**.

All native evidence remains NOT RUN, exactly as listed above.

**Integration 1 review result (appended after the review; the code inputs are unchanged).** A fresh, independent Claude Opus 5.5 High clean-code-guard review returned **ACCEPT WITH NITS** on manifest `artifacts/m4-volume-1/integration-1/inputs.sha256` (50 entries, SHA-256 `455149D90E4A005FFFD7FAEE2F78025561A4F311841894FEAF6F43322765253A`), HEAD `eabf0e8`. Report: `artifacts/m4-volume-1/integration-1/review-opus.md`, SHA-256 `84CCE597402D796E2DB75B6ADB62D1D3CA12BD894690741B280947EB184E765E`.

- **Findings:** 0 blocker, 0 major, 0 minor, 1 nit. **M4VI1-OPUS-NIT-1** is deferred, not fixed. The slice-5 album-art shortcut comment at `src/graphics.rs:1003` does not list every open-panel repaint that re-uploads the art: slice 4's progress ticks and this slice's whole-percent volume changes. The cost is bounded and there is no functional effect. Fixing it would change frozen inputs for a comment in slice 5's code.
- **Equivalence:** the reviewer confirmed it against round 3 by regenerating a byte-identical range-diff. `audio.rs` and `main.rs` are identical to the round-3 commit, and the `notch_uia` patch is unchanged.
- **Integration behaviour confirmed by the reviewer:** hit-testing (no point hits two targets), paint order, deadline interaction and teardown order.
- **Reviewer's verify:** run into `integration-1/review-verify`, 94 tests, the same release SHA.

This section changes only this packet's raw bytes after the freeze. The frozen manifest entry for this file therefore records its pre-result version. Every code, config and evidence input is unchanged. No second integration round was needed.
