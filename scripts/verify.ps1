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
# A raised baseline needs a fresh sign-off in the same commit (baseline-gate.ps1); it never goes up silently.
$baselineFile = Join-Path $PSScriptRoot 'size-baseline.txt'
$baselineLines = @(Get-Content -LiteralPath $baselineFile)
$baseline = [int]$baselineLines[0]
$ErrorActionPreference = 'Continue'
$committedLines = @(& git -C $projectRoot show HEAD:scripts/size-baseline.txt)
if ($LASTEXITCODE -ne 0) { $committedLines = @() }
$ErrorActionPreference = 'Stop'
$unsigned = & (Join-Path $PSScriptRoot 'baseline-gate.ps1') -Current $baselineLines -Committed $committedLines
if ($unsigned) { throw $unsigned }
$size = (Get-Item -LiteralPath (Join-Path $projectRoot 'target/release/roqa.exe')).Length
if ($size -gt $baseline * 1.02) { throw "Release binary is $size B, over the $baseline B baseline plus 2%. If the growth is intended, a gate reviewer or the user must sign it off: write $size on line 1 of scripts/size-baseline.txt and add the lines 'raise-reason: <why>' and 'approved-by: <name> <YYYY-MM-DD>' in the same commit." }
Write-Host "size: PASS ($size B, baseline $baseline B + 2%)."
