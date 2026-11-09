# -Executable measures another build (for example a saved baseline) instead of target/release.
param([switch]$SkipBuild, [string]$EvidenceDirectory = 'artifacts/m1', [string]$Executable = '')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (!$SkipBuild) { & "$PSScriptRoot/cargo.ps1" build --locked --release }
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$executable = if ($Executable) { (Resolve-Path -LiteralPath $Executable).Path } else { Join-Path $projectRoot 'target/release/roqa.exe' }
$machine = [ordered]@{
    Os = (Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber)
    Cpu = (Get-CimInstance Win32_Processor | Select-Object Name, NumberOfLogicalProcessors)
    Gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate)
}
$measuredHost = Start-Process -FilePath $executable -PassThru -WindowStyle Hidden
try {
    if ($measuredHost.WaitForExit(5000)) { throw 'ROQA exited during the five-second settle period.' }
    $measuredHost.Refresh()
    $startCpu = $measuredHost.TotalProcessorTime.TotalMilliseconds
    $startPrivate = $measuredHost.PrivateMemorySize64
    $startHandles = $measuredHost.HandleCount
    $sampleClock = [System.Diagnostics.Stopwatch]::StartNew()
    if ($measuredHost.WaitForExit(30000)) { throw 'ROQA exited during the thirty-second sample.' }
    $sampleClock.Stop()
    $measuredHost.Refresh()
    $cpuMilliseconds = $measuredHost.TotalProcessorTime.TotalMilliseconds - $startCpu
    $observation = [ordered]@{
        Label = 'M1 short idle observation only; no product-budget or competitor verdict'
        Utc = [DateTime]::UtcNow.ToString('o')
        BinarySha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash
        Method = 'Release build; five-second settle; thirty-second wait; System.Diagnostics.Process start/end counters'
        Machine = $machine
        DurationSeconds = $sampleClock.Elapsed.TotalSeconds
        CpuMilliseconds = $cpuMilliseconds
        CpuMillisecondsPerSecond = $cpuMilliseconds / $sampleClock.Elapsed.TotalSeconds
        PrivateBytesStart = $startPrivate
        PrivateBytesEnd = $measuredHost.PrivateMemorySize64
        HandlesStart = $startHandles
        HandlesEnd = $measuredHost.HandleCount
        Limits = 'One short run; CPU counter quantization and scheduling variance; two endpoint queries only; sampler outside target; DWM/GPU/power/shared allocations not attributed; other desktop applications remain running.'
    }
    $observation | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $evidence 'idle.json')
}
finally {
    if (!$measuredHost.HasExited) {
        $quit = Start-Process -FilePath $executable -ArgumentList '--quit' -WindowStyle Hidden -PassThru
        if (!$quit.WaitForExit(5000) -or $quit.ExitCode -ne 0) { Write-Warning 'Idle sample quit signaling failed.' }
        $quit.Dispose()
        if (!$measuredHost.WaitForExit(5000)) { $measuredHost.Kill(); throw 'Forced cleanup after idle sample; graceful shutdown failed.' }
        if ($measuredHost.ExitCode -ne 0) { throw "Measured host exited with $($measuredHost.ExitCode)." }
    }
    $measuredHost.Dispose()
}
Write-Host "Idle observation saved to $evidence/idle.json."
