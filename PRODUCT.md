# ROQA

## Platform

Native Windows desktop. Windows 11 x64 is the initial implementation assumption, not a promise of exclusive long-term support.

## Purpose

A dock and notch for Windows. The dock should grow toward Nectar's functionality; the notch takes inspiration from Nusa. The user explicitly prioritizes broader capabilities, significantly better performance, and architecture that can support growth.

## Development contract

The user asked to start implementation, write a plan, and have GPT-6.1 Sol High and Opus 5.5 High review each step in an iterative hill-climbing process. No agent reviews its own work. `PLAN.md` defines independently reviewed milestones.

## Stack

Initial prototype candidate from the feasibility council: Rust, Win32 and Windows composition. C#/WinUI remains a measured fallback. The user authorized starting the project following the feasibility review; the final renderer remains subject to prototype evidence.

## Constraints

- Performance claims require matched measurements; none have been established yet.
- First milestone keeps the native Windows taskbar intact.
- No application implementation or artwork from Nectar or Nusa is reused.
- Licensing, distribution, monetization, complete Windows support matrix and the final feature shortlist remain open.
- DPI/text scaling and RTL/IME in text fields are engineering acceptance concerns, not decorative polish. Keyboard navigation, hotkeys and Narrator/UIA support were removed by user decision (K1, 2026-11-13).

## Evidence

`docs/council/` contains both independent feasibility reports, the chair's qualifications and the decision. Those reports distinguish source findings, hypotheses and proposed performance targets.
