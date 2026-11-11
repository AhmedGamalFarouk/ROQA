param([ValidateSet('start','quit','show-notch','hide-notch','show-dock','hide-dock','show-picker','show-settings')][string]$Action = 'start')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
if ($Action -eq 'start') {
    & "$PSScriptRoot/cargo.ps1" build --locked --release
    $logDirectory = Join-Path $projectRoot 'artifacts/runtime'
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    $log = Join-Path $logDirectory ((Get-Date -Format 'yyyyMMdd-HHmmss-fff') + '.log')
    $hostInstance = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru -RedirectStandardError $log
    try {
        if ($hostInstance.WaitForExit(1000)) { throw "ROQA exited with $($hostInstance.ExitCode). Diagnostics: $log" }
        Write-Host "ROQA is running (PID $($hostInstance.Id)). Quit with: ./scripts/run.ps1 quit"
    } finally { $hostInstance.Dispose() }
} else {
    if (!(Test-Path -LiteralPath $executable)) { throw 'Build/start ROQA first.' }
    $control = Start-Process -FilePath $executable -ArgumentList "--$Action" -WindowStyle Hidden -PassThru
    try {
        if (!$control.WaitForExit(5000)) { $control.Kill(); throw 'Control client timed out.' }
        if ($control.ExitCode -ne 0) { throw "Control signal failed ($($control.ExitCode)); check that ROQA is running." }
    } finally { $control.Dispose() }
}
