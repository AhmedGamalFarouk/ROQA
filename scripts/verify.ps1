param([string]$EvidenceDirectory = 'artifacts/verification', [ValidateRange(1, 7200)][int]$StepTimeoutSeconds = 900)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$results = @()
# A src file past 2,500 lines must be split by job into sibling `impl` files (as src/host/ does for Desktop). The four files
# already over the limit may not grow: a ceiling at today's size; shrink the ceiling when one is split.
$limit = 2500
$ceiling = @{ 'host.rs' = 4907; 'graphics.rs' = 3971; 'settings_window.rs' = 3218; 'layout.rs' = 2703 }
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src') -Recurse -Filter *.rs) {
    $max = if ($ceiling.ContainsKey($file.Name)) { $ceiling[$file.Name] } else { $limit }
    $lines = [System.IO.File]::ReadAllLines($file.FullName).Count
    if ($lines -gt $max) { throw "$($file.FullName) has $lines lines (limit $max). Split it by job into sibling files, for example impl Desktop blocks in src/host/." }
}
foreach ($command in @(@{ Name = 'checks'; Script = 'check.ps1'; Arguments = '' }, @{ Name = 'build'; Script = 'cargo.ps1'; Arguments = ' build --locked --release' })) {
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = 'powershell.exe'
    $info.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + (Join-Path $PSScriptRoot $command.Script) + '"' + $command.Arguments
    $info.WorkingDirectory = $projectRoot
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $child = New-Object System.Diagnostics.Process
    $child.StartInfo = $info
    try {
        if (!$child.Start()) { throw "Could not start $($command.Name)." }
        $stdout = $child.StandardOutput.ReadToEndAsync()
        $stderr = $child.StandardError.ReadToEndAsync()
        if (!$child.WaitForExit($StepTimeoutSeconds * 1000)) { $child.Kill(); throw "Verification $($command.Name) timed out after $StepTimeoutSeconds s." }
        $outText = $stdout.GetAwaiter().GetResult(); $errText = $stderr.GetAwaiter().GetResult()
        [System.IO.File]::WriteAllText((Join-Path $evidence "$($command.Name).stdout.log"), $outText)
        [System.IO.File]::WriteAllText((Join-Path $evidence "$($command.Name).stderr.log"), $errText)
        $results += [ordered]@{ Name = $command.Name; ExitCode = $child.ExitCode; Command = $info.Arguments; Utc = [DateTime]::UtcNow.ToString('o') }
        $results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence 'verification.json') -Encoding UTF8
        if ($child.ExitCode -ne 0) { throw "Verification $($command.Name) failed: see $evidence." }
        Write-Host "$($command.Name): PASS (exit 0); stdout/stderr captured."
    } finally { $child.Dispose() }
}
# Size is reported, not gated (owner decision 2026-11-16).
$size = (Get-Item -LiteralPath (Join-Path $projectRoot 'target/release/roqa.exe')).Length
Write-Host "size: $size B (reported, no limit)."
