[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^r\d+$')][string]$Round,
    [string[]]$Only = @(),
    [switch]$SkipEyes,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot)
$exe = Join-Path $root 'target\release\roqa.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "target\release\roqa.exe missing. Build it first." }
$head = (& git -C $root rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or -not $head) { throw 'Unable to read git HEAD.' }
$hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$bytes = (Get-Item -LiteralPath $exe).Length
$phases = @('matrix', 'dock', 'eyes', 'extra')
$selected = @()
if ($Only.Count -eq 0) { $selected = $phases }
else {
    $requested = @($Only | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim().ToLowerInvariant() } | Where-Object { $_ })
    $unknown = @($requested | Where-Object { $_ -notin $phases } | Select-Object -Unique)
    if ($unknown.Count) { throw "Unknown phase(s): $($unknown -join ', '). Choose matrix,dock,eyes,extra." }
    $selected = @($phases | Where-Object { $_ -in $requested })
}
if ($SkipEyes) { $selected = @($selected | Where-Object { $_ -ne 'eyes' }) }
$outDir = Join-Path $root "cook\stills\$Round"

if ($DryRun) {
    Write-Host "Dry run: would capture $Round using existing exe ($bytes B, SHA256 $hash) at HEAD $head."
    Write-Host "Phases: $($selected -join ', ')"
    Write-Host 'No processes will be stopped or started.'
    exit 0
}

$existing = Get-ChildItem -LiteralPath (Join-Path $root 'cook\stills') -Directory -ErrorAction Stop | ForEach-Object {
    $file = Join-Path $_.FullName 'provenance.json'
    if (Test-Path -LiteralPath $file) {
        # A record without these fields (an older format) never matches.
        try {
            $record = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
            if ($record.exeSha256 -eq $hash -and $record.gitHead -eq $head -and
                @($selected | Where-Object { $_ -notin $record.phases }).Count -eq 0) { $_ }
        } catch { return }
    }
}
if ($existing) {
    Write-Host "Existing round $($existing[0].Name) has this git HEAD, exe SHA256 and all requested phases; skipping capture."
    exit 0
}
if (Test-Path -LiteralPath $outDir) { throw "Round $Round already exists without matching evidence for this request. Choose a new round number." }

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$prov = [ordered]@{ gitHead = $head; exeSha256 = $hash; bytes = $bytes; capturedUtc = [DateTime]::UtcNow.ToString('o'); phases = $selected }
$backdrop = $null
$oldPhase = $env:ROQA_CAPTURE_PHASE
$oldDir = $env:ROQA_CAPTURE_DIR
$oldErrorAction = $ErrorActionPreference
try {
    if (Get-Process roqa -ErrorAction SilentlyContinue) {
        $quit = Start-Process -FilePath $exe -ArgumentList '--quit' -WindowStyle Hidden -PassThru
        if (-not $quit.WaitForExit(6000)) { throw 'ROQA quit command timed out.' }
    }
    Get-Process roqa,nusa -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    $backdropErr = Join-Path $outDir 'backdrop.err'
    $backdrop = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-STA','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'backdrop.ps1')) -WindowStyle Hidden -PassThru -RedirectStandardError $backdropErr
    Start-Sleep -Milliseconds 1000
    $env:ROQA_CAPTURE_PHASE = $Round
    $env:ROQA_CAPTURE_DIR = $outDir
    foreach ($phase in $selected) {
        Write-Host "Starting phase: $phase"
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "capture-$phase.ps1")
        if ($LASTEXITCODE -ne 0) { throw "Phase '$phase' failed with exit code $LASTEXITCODE" }
    }
    $provPath = Join-Path $outDir 'provenance.json'
    $prov | ConvertTo-Json | Set-Content -LiteralPath $provPath -Encoding UTF8
    $files = Get-ChildItem -LiteralPath $outDir -Recurse -File | Where-Object { $_.Name -notin @('INDEX.md','provenance.json','backdrop.err') -and $_.FullName -notmatch '\\private\\' } | Sort-Object FullName
    $rows = @("# $Round capture index", '', "Captured UTC: $($prov.capturedUtc). Git HEAD: ``$head``. Exe SHA256: ``$hash`` ($bytes B).", '', "## Captured stills ($($files.Count) files)", '', '| File | Size (bytes) |', '|---|---:|')
    foreach ($file in $files) {
        $relative = $file.FullName.Substring($outDir.Length).TrimStart('\','/') -replace '\\','/'
        $rows += "| $relative | $($file.Length) |"
    }
    [IO.File]::WriteAllLines((Join-Path $outDir 'INDEX.md'), $rows, (New-Object Text.UTF8Encoding $false))
    Write-Host "Capture complete: $outDir"
} finally {
    $ErrorActionPreference = 'Continue'
    if (Get-Process roqa -ErrorAction SilentlyContinue) {
        $quit = Start-Process -FilePath $exe -ArgumentList '--quit' -WindowStyle Hidden -PassThru
        [void]$quit.WaitForExit(6000)
        Get-Process roqa -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    if ($backdrop -and -not $backdrop.HasExited) {
        $backdrop.CloseMainWindow() | Out-Null
        if (-not $backdrop.WaitForExit(3000)) { $backdrop.Kill() }
    }
    Get-Process nusa -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    & (Join-Path $root 'scripts\run.ps1') start
    $nusaApp = Get-StartApps | Where-Object Name -eq 'Nusa' | Select-Object -First 1
    if ($nusaApp) { Start-Process explorer.exe -ArgumentList "shell:AppsFolder\$($nusaApp.AppID)" -WindowStyle Hidden }
    if ($null -eq $oldPhase) { Remove-Item Env:\ROQA_CAPTURE_PHASE -ErrorAction SilentlyContinue } else { $env:ROQA_CAPTURE_PHASE = $oldPhase }
    if ($null -eq $oldDir) { Remove-Item Env:\ROQA_CAPTURE_DIR -ErrorAction SilentlyContinue } else { $env:ROQA_CAPTURE_DIR = $oldDir }
    $ErrorActionPreference = $oldErrorAction
}
