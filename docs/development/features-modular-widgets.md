# Modular notch widgets

- Candidate: Week and Battery cards with persisted independent slot choices and a Widgets tab.
- Calendar uses local Windows date and pure Monday-first arithmetic; battery is read during paint and power broadcasts repaint an open Widgets page. No widget timer or worker was added.
- Defaults: Week left, Battery right. Unknown or absent battery is displayed as unavailable; no task data is fabricated.
- Verification: `scripts/verify.ps1` (fmt, clippy, tests, release and size gate); final results are in the progress entry.
- Native/GUI visual checks NOT RUN (user instruction); layout, text fit, charging glyph and display transitions remain unverified on screen.
- Milestone council review NOT RUN; this is an intermediate feature candidate.
