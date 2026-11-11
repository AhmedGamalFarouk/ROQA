param(
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ManifestSha,
    [Parameter(Mandatory)][string]$Message,
    [Parameter(Mandatory)][string[]]$PacketPaths,
    [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedBinarySha,
    [switch]$DryRun
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
function Stop-Land([string]$Why) { throw "LAND STOPPED: $Why" }
function Get-Sha([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
function Invoke-Git { $ErrorActionPreference = 'Continue'; & git @args 2>&1 | ForEach-Object { Write-Host "$_" }; if ($LASTEXITCODE -ne 0) { Stop-Land "git $($args -join ' ') failed ($LASTEXITCODE)." } }

foreach ($f in @($Manifest, $Message)) { if (!(Test-Path -LiteralPath $f -PathType Leaf)) { Stop-Land "Missing file: $f" } }
if ((Get-Sha $Manifest) -ne $ManifestSha) { Stop-Land "Manifest hash mismatch for $Manifest." }
if ((git branch --show-current) -ne 'main') { Stop-Land 'Not on main.' }
if (git diff --cached --name-only) { Stop-Land 'Index already has staged changes.' }

# Manifest lines: "<sha256>  <repo-relative path>", possibly CRLF.
$entries = @(Get-Content -LiteralPath $Manifest | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_ })
$paths = @()
foreach ($line in $entries) {
    if ($line -notmatch '^([0-9A-Fa-f]{64}) +(.+)$') { Stop-Land "Bad manifest line: $line" }
    if (!(Test-Path -LiteralPath $Matches[2] -PathType Leaf)) { Stop-Land "Manifest file missing: $($Matches[2])" }
    if ((Get-Sha $Matches[2]) -ne $Matches[1]) { Stop-Land "Stale input, hash differs: $($Matches[2])" }
    $paths += $Matches[2]
}
$paths = @($paths + $PacketPaths | ForEach-Object { $_ -replace '\\', '/' } | Select-Object -Unique)
$banned = $paths | Where-Object { $_ -match '^(\./)?(\.gitignore$|artifacts(/|$)|target(/|$)|\.tools(/|$))' -or $_ -in @('.', '*') }
if ($banned) { Stop-Land "Refusing to stage: $($banned -join ', ')" }
foreach ($p in $paths) { if (!(Test-Path -LiteralPath $p)) { Stop-Land "Path not found: $p" } }

& "$PSScriptRoot/verify.ps1"
$bin = Join-Path $root 'target/release/roqa.exe'
if ((Get-Sha $bin) -ne $ExpectedBinarySha) { Stop-Land "Binary SHA mismatch (got $(Get-Sha $bin))." }

Invoke-Git fetch origin main
& git merge-base --is-ancestor origin/main HEAD
if ($LASTEXITCODE -ne 0) { Stop-Land 'origin/main is not an ancestor of HEAD; rebase and re-verify first.' }
if ($DryRun) { Write-Host "DRY RUN OK: would stage $($paths.Count) paths, commit, push origin main."; return }

Invoke-Git add -- @paths
Invoke-Git commit -F $Message
Invoke-Git push origin main
Write-Host "LANDED $(git rev-parse --short HEAD) on origin/main; $($paths.Count) paths; binary $ExpectedBinarySha."
