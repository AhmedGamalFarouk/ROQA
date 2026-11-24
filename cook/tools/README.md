# ROQA capture tools

Run these from the repository root or pass their paths to PowerShell. Captures use the existing `target\release\roqa.exe`; they never build it.

## Capture

```powershell
powershell -File cook\tools\run-capture.ps1 -Round rNN
powershell -File cook\tools\run-capture.ps1 -Round rNN -SkipEyes
powershell -File cook\tools\run-capture.ps1 -Round rNN -Only matrix,dock,extra
powershell -File cook\tools\run-capture.ps1 -Round rNN -Only matrix,eyes
powershell -File cook\tools\run-capture.ps1 -Round rNN -DryRun
```

The available passes are `matrix`, `dock`, `eyes`, and `extra`. The matrix pass includes all seven tabs, Settings samples, and five themes. A real capture quits ROQA with `--quit`, closes Nusa, shows the gray backdrop, and captures the same filenames as the r10 scripts. Its `finally` block closes the backdrop and restarts ROQA with `scripts\run.ps1 start` and Nusa.

`-DryRun` validates the round and pass selection and reports the current exe size, SHA256, and git HEAD. It does not stop or start applications. A real capture reuses an existing round only when git HEAD, exe SHA256 and all requested phases match. Older records without phases are not reused. Use a new round number to capture missing phases without overwriting an earlier round. New rounds receive an `INDEX.md` and `provenance.json` with git HEAD, exe SHA256, byte size, UTC capture time, and the completed phases.

Captures use the second (non-primary) monitor when available; `scripts\monitor.ps1` picks it. With one monitor the tools warn and use the primary. Pass and tool coordinates are local to that monitor (1920x1080 at its origin), the backdrop covers only it, Settings is centred on it, and ROQA is kept off the other monitors with `off=` lines in the capture store.

A still that is only the gray backdrop (ROQA was not on screen) fails the pass at once.

Each capture starts ROQA from its own private store, which sets `notchauto=0` and `dockauto=0` (notch and dock never tuck, or the notch cannot be found). It is thrown away with the run; the owner's real store is never changed, so nothing needs restoring.

## Compare

```powershell
powershell -File cook\tools\compare.ps1 -Before r10 -After r11
powershell -File cook\tools\compare.ps1 -Before r10 -After r11 -Names 10-media-rest,20-settings-general
```

Matching PNGs are written as side-by-side images in `cook\compare\<After>\`. `-Names` accepts comma-separated names, with or without `.png`.

## Files

- `run-capture.ps1` coordinates a capture and records its index and provenance.
- `capture-matrix.ps1`, `capture-dock.ps1`, `capture-eyes.ps1`, and `capture-extra.ps1` capture their named passes.
- `capture-lib.ps1` holds shared capture helpers; `Cap.cs` performs screen grabs; `scripts\monitor.ps1` (shared with `run.ps1`) chooses the test monitor.
- `backdrop.ps1` supplies the gray screen backdrop; `compare.ps1` makes round comparisons.
