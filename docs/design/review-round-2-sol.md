# Design judgement round 2 - GPT-6.1 Sol
Inputs: manifest match YES. Manifest SHA-256 matches `334FB2DA…55A3BE1`; all 29 listed inputs match, including graphics, host, motion, layout and the release executable. No mismatches.

## Round-1 finding status

| Round-1 ID | Status (resolved / not resolved / deferral accepted / deferral rejected) | Evidence |
|---|---|---|
| DESIGN-SOL-1 | not resolved | Hover now checks X and Y, and the region follows growth. However, `src/layout.rs:66` still claims the entire 400×124 HWND when open, including the 44-DIP shadow band. `src/native_window.rs:105` applies a rectangular region with no click-through mechanism. At window-relative (200,100), outside the 80-DIP body, underlying clicks remain intercepted during closing. Separate noninteractive shadow rendering from body input, then verify clicks reach an underlying process during open and close. |
| DESIGN-SOL-2 | resolved | `src/host.rs:230` multiplies monitor DPI by the text factor; creation, drawing, regions and hover use it. `rescale_text` rebuilds on the existing text-settings event. This resolves the ignored-setting implementation defect; actual non-1.0 rendering and live relayout remain unverified. |
| DESIGN-SOL-3 | resolved | `src/graphics.rs:381` draws one fully opaque clock throughout the morph. `13-roqa-morph-frames.png` and open/close GIF frames show continuous clock content. |
| DESIGN-SOL-4 | deferral accepted | Per-frame resource creation remains, but work is bounded to interaction and returns to the blocking idle loop. Accept as minor pending the promised frame-time trace; no performance pass inferred. |
| DESIGN-OPUS-1 | not resolved | Side margins no longer sustain hover through the former Y-only check. Shadow and transparent rectangular-region input interception remain as described under DESIGN-SOL-1. |
| DESIGN-OPUS-2 | resolved | Clock is bold, 15 DIP, with a 0.72 designator factor. Evidence `01`, `04` and the aligned GIF frames show the stronger compact clock. |
| DESIGN-OPUS-3 | resolved | Same continuous-clock fix as DESIGN-SOL-3; the previous crossfade gap is removed. |
| DESIGN-OPUS-4 | resolved | Shadow source now uses 6-DIP side insets and a 400-DIP window. Evidence `02` and `03` shows the shadow wrapping the sides without an obvious abrupt cutoff. |
| DESIGN-OPUS-5 | resolved | `src/graphics.rs:538` centres both icon and 48×48 chip on the lifted centre. Dock GIFs and `12-roqa-dock-hover-settled.png` support the fix. |
| DESIGN-OPUS-6 | deferral accepted | Same bounded resource-churn deferral as DESIGN-SOL-4. Actual frame cost remains unknown. |
| DESIGN-OPUS-7 | resolved | `src/host.rs:632` resolves the compositor-clock export dynamically; absence/failure uses a bounded message wait. Windows 10 runtime compatibility is not established by this static fix. |
| DESIGN-OPUS-8 | resolved | Visual spec states approximately 7.4:1 date contrast; shadow comment references `nusa-reference.md`. |
| DESIGN-GEMINI-1 | deferral accepted | Same resource-churn deferral as DESIGN-SOL-4. |
| DESIGN-GEMINI-2 | not resolved | Shadow-band clicks are explicitly still claimed by the open region. Apply and verify the input/rendering separation recommended under DESIGN-SOL-1. |
| DESIGN-GEMINI-3 | deferral accepted | Unchanged, narrowly bounded dock-corner interception remains a minor refinement. Accept deferral for this pass. |

## New findings (round 2)

| ID | Severity | Dimension | Evidence | Problem | Recommended fix |
|---|---|---|---|---|---|
| — | — | — | — | No additional findings within this recheck’s scope. | — |

## What works well

The stronger typography, persistent clock morph, wrapping shadow and aligned dock chip improve polish. All supplied PNGs and GIFs were inspected, including sampled animation frames. The aligned sequences and motion curves support a close match to Nusa’s opening overshoot and closing undershoot within the chosen panel size.

Shared body geometry improves consistency between drawing and interaction. Dynamic compositor-clock resolution removes the identified static-import problem, and animation returns to a blocking idle wait.

The full native suite on this binary, compositor frame-time trace and text-scale factors other than 1.0 remain open. This judgment does not certify milestone acceptance or measured performance.

## Verdict: REVISE

The major click-interception finding is only partially fixed: the shadow still owns input outside the notch body.
