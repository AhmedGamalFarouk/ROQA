# BRIEF - ROQA character and design language
Started: 2026-11-14

## Target
Give ROQA (native Windows dock + notch, Rust/Win32/D2D) one recognisable design language and character, so a blurred screenshot is still "ROQA".

## Personality
Companion-like AND playful/bouncy: calm, caring, lightly poetic, but springy motion and a mascot that reacts (blink, squash, doze, wave, lean to the timer, look at media).

## References
- Nusa (Windows notch app, installed, running: C:\Program Files\WindowsApps\citoxstd.Nusa_1.2.1.0_x64__r9cmnhct3ptgm\nusa.exe; site https://rama2555.github.io/nusa/). Take: idle eyes that doze and wave back, breathing timer, line-by-line lyrics, split pill, sun arc, calm short copy. Take from it, never copy 1:1.
- Own mascot (capsule eyes + squash) - may be changed freely.

## Bar (stretch, critic median after calibration)
Per dimension >= 8.5: shape language, color, motion/character, icon set, typography, coherence across notch/dock/Settings. Gate: "recognisable when blurred".

## Rules
- Perf first: event-driven, frames only while animating, reduced motion respected, no new idle timers/threads.
- Size ratchet: only the user approves baseline raises. New deps: none by default.
- No keyboard/hotkey/UIA work (user decision).
- User gave FULL freedom to change anything visual, incl. mascot, themes, tabs, copy.
- Slop ban: purple gradients, glowing orbs, shockwave rings, decorative particles, em dashes, eyebrow labels.
- Copy tone: warm, brief, no exclamation marks.

## Process
1. Phase A: capture Nusa + current ROQA references (Gemini 3.8 Flash, fallback Luna), write SHOTS.md.
2. Phase B: /brandkit board (logo/mascot, capsule shape grammar, accent + neutral ramp, 3 motion curves, icon style, voice). User approves board.
3. Phase C: code rounds by Sol 6.1 (codex gpt-6.1-sol, medium). Capture on Gemini, judge on Opus 5.5 medium (fresh context, side-by-side only).
4. Gate: send stills each round; user feedback beats critics.

## Time
At least 4h of rounds, pace usage, no 100%.

## Deliverable
Merged to main, brand board in cook/ and docs, honest numbers (size, idle CPU not measured unless measured).

## Permissions
Desktop free; may --quit user's ROQA for captures. Do not touch .tools/. No publishing/push without asking.
