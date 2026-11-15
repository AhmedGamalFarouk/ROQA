# Critic rubric (one copy; briefs point here)

You judge images only. No source code, no earlier scores, no fix lists.
Brand to judge against: the "Brand" section of `AGENTS.md` and `cook/brand/board.png`.

Score each dimension 0-10, one decimal. 8.5 = a paying user would call it finished. 7 = fine but visibly uneven. 5 = generic.
1. Shape language: one capsule grammar, radii, insets, no stray boxes.
2. Color: one accent used only where the brand says, neutral ramp, themes distinct.
3. Motion / character: mascot poses and handoffs (from contact sheets), does it feel alive and clean.
4. Icon set: one stroke, one weight, same optical size.
5. Typography: scale, hierarchy, alignment, clipping.
6. Coherence: notch, dock, popovers and Settings look like one product.
Also answer: blurred to 8 px, is it still recognisably ROQA (yes/no, why).

Out of scope: accessibility, keyboard, FPS, anything not visible in the images.
By design, not defects: eyes appear only when idle; Light theme accent is the darker `#9A5200`; no glow; Settings on/off toggles are amber (owner decision); Prayer shows two highlights (the past and the next prayer); from 23:00 to 05:00 the eyes take the sleepy half-lidded shape; the Shelf's dashed drop zone is the one outline the brand allows; weather shows 3 hours at 29 degrees (sample data).

Output, at most 40 lines, written to the path the brief names:
- a table: dimension, score, one-line reason
- up to 10 defects, ranked: surface, still file name, what is wrong, smallest fix
- the blur answer
