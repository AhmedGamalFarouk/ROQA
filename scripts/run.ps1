param([ValidateSet('start','quit','show-notch','hide-notch','show-dock','hide-dock','show-settings')][string]$Action = 'start',
    # A live test: ROQA on the test monitor only, from a private copy of the store. Without it, start is the
    # owner's own ROQA on the monitors they chose.
    [switch]$Test)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
if ($Action -eq 'start') {
    & "$PSScriptRoot/cargo.ps1" build --locked --release
    $logDirectory = Join-Path $projectRoot 'artifacts/runtime'
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    $log = Join-Path $logDirectory ((Get-Date -Format 'yyyyMMdd-HHmmss-fff') + '.log')
    # Launched through the shell so ROQA inherits none of the caller's pipes; a caller that
    # captures output would otherwise wait on ROQA forever.
    $command = '/c start "" /b "{0}" 2> "{1}"' -f ($executable -replace '/', '\'), ($log -replace '/', '\')
    # -Test: ROQA runs on the test monitor only (scripts/monitor.ps1): a private copy of the owner's store with
    # the other monitors switched off, so the real store is never changed. Refreshed from the real store each start.
    $realLocal = $env:LOCALAPPDATA
    # One ROQA per session: note who already runs, so only the process this call starts counts as started.
    $before = @(Get-Process roqa -ErrorAction SilentlyContinue | ForEach-Object Id)
    if ($Test -and $before.Count) {
        throw "ROQA is already running (PID $($before -join ', ')); a test would only see it. Quit it first: ./scripts/run.ps1 quit"
    }
    try {
        if ($Test) {
            . "$PSScriptRoot/monitor.ps1"
            $env:LOCALAPPDATA = New-TestProfile (Join-Path $logDirectory 'profile')
        }
        Start-Process -FilePath $env:ComSpec -ArgumentList $command -WindowStyle Hidden
    } finally { $env:LOCALAPPDATA = $realLocal }
    Start-Sleep -Milliseconds 1000
    $hostInstance = Get-Process roqa -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id } |
        Select-Object -First 1
    if (-not $hostInstance -and $before.Count) {
        Write-Host "ROQA was already running (PID $($before -join ', ')); this start launched no other."
        return
    }
    if (-not $hostInstance) { throw "ROQA did not stay running. Diagnostics: $log" }
    Write-Host "ROQA is running (PID $($hostInstance.Id)). Quit with: ./scripts/run.ps1 quit"
} else {
    if (!(Test-Path -LiteralPath $executable)) { throw 'Build/start ROQA first.' }
    $control = Start-Process -FilePath $executable -ArgumentList "--$Action" -WindowStyle Hidden -PassThru
    try {
        if (!$control.WaitForExit(5000)) { $control.Kill(); throw 'Control client timed out.' }
        if ($control.ExitCode -ne 0) { throw "Control signal failed ($($control.ExitCode)); check that ROQA is running." }
    } finally { $control.Dispose() }
}
