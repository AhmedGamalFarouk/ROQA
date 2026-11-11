param([string]$EvidenceDirectory = 'artifacts/verification', [ValidateRange(1, 7200)][int]$StepTimeoutSeconds = 900)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$results = @()
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
# Size ratchet: the release binary may not exceed the tracked baseline (scripts/size-baseline.txt) by more than 2%.
$baseline = [int](Get-Content -LiteralPath (Join-Path $PSScriptRoot 'size-baseline.txt') -TotalCount 1)
$size = (Get-Item -LiteralPath (Join-Path $projectRoot 'target/release/roqa.exe')).Length
if ($size -gt $baseline * 1.02) { throw "Release binary is $size B, over the $baseline B baseline plus 2%. If the growth is intended, write $size to scripts/size-baseline.txt in the same commit." }
Write-Host "size: PASS ($size B, baseline $baseline B + 2%)."
