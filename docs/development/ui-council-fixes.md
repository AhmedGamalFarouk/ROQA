# UI council fixes + motion (U2 / M2)

Source: artifacts/council-3/judge-{opus,gemini,sol}.md and index.md (git-ignored, read them; shots in artifacts/council-3/shots). User wants ALL of it done, plus better motion/animation/transitions. Performance and tiny footprint stay first: no idle timers/threads, paint only on change, animation only while a transition runs, respect existing reduced-motion decision (src/motion.rs:135).

## Batch U2: fixes (order matters; land per package)
1. Settings window: separate non-scrolling footer (content clipped above it, divider), Close never overlapped; wrap/clip captions properly (no half-sentences); dark-styled edit boxes and combos (no white boxes, incl. Pinned apps combo); toggle on-state in the accent colour with clear on/off difference; 2 px accent focus ring (visibly different from selection); real glyphs in sidebar tiles (no duplicate "A").
2. Shelf: persistent filename under each item + file-type icon (shell icon, fallback glyph if load fails).
3. Notch fixes: "Maghrib" never truncated (measure text); Prayer next-vs-current cue so highlight matches the headline; gear not touching chips; Limits Refresh pill not colliding with title, bigger text/adaptive height; timer-done visible state ("Timer finished", accent colour, restart/dismiss); no hover expansion while the notch right-click menu is open.
4. Task panels: Timer tab has Start (presets) on the tab itself; Weather/Prayer empty states have a labeled "Set city" action; Shelf has hint/Add action; tab handle shows short destination labels (icon+label on engagement/hover and via UIA), bigger hit targets; Media empty state says "Nothing playing".
5. Eyes bigger/more readable (still idle/occasional, same rules); Attached dock visibly a different shape (flush to the screen edge, full-width bar or flat bottom with no gap); dock glyphs normalised to one stroke weight and the picker icon distinct from Start; glass-theme corner "ears" cleaned; Classic/Midnight panels get a subtle edge/border so they separate from dark backdrops; Light theme gets a softer, less stark look.
6. Accessibility: documented pointer-free entry route for notch/dock (a discoverable keyboard way in, within the no-global-hooks rule: e.g. an optional hotkey off by default registered with RegisterHotKey is a global registration, so ASK the Chief first; otherwise a Tab-reachable entry from the Settings page and tray-less alternative); edge sliders report keyboard focus (UIA).

## Batch M2: motion (after U2 lands; separate worker)
Better open/close easing and overshoot tuning, tab-switch slide with stagger, smooth shelf/timer/Prayer/Weather content cross-fade, toggle thumb slide and focus-ring fade in Settings, dock hover magnify/lift and slide, notice enter/exit, eyes blink/squash polish, page transition in Settings. One motion clock only (existing motion.rs), no new timers when idle, frames only while an animation is live, animspeed setting respected, reduced motion honoured. Verify idle numbers did not regress.

## Rules
Size: baseline 1,121,280 B (+2% limit). If the work cannot fit, STOP and report exact numbers; only the user may approve a raise. Land via scripts/land.ps1 per package. Native checks only with fresh desktop-free confirmation from the Chief. Do not remove your worktree; do not touch `.tools`.
