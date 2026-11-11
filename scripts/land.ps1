param(
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ManifestSha,
    [string]$Message,  # needed only when paths are left to commit
    [string[]]$PacketPaths = @(),
    [ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedBinarySha,  # checked when given, always printed
    [switch]$DryRun
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
function Stop-Land([string]$Why) { throw "LAND STOPPED: $Why" }
function Get-Sha([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
function Invoke-Git { $ErrorActionPreference = 'Continue'; & git @args 2>&1 | ForEach-Object { Write-Host "$_" }; if ($LASTEXITCODE -ne 0) { Stop-Land "git $($args -join ' ') failed ($LASTEXITCODE)." } }

foreach ($f in @($Manifest, $Message | Where-Object { $_ })) { if (!(Test-Path -LiteralPath $f -PathType Leaf)) { Stop-Land "Missing file: $f" } }
if ((Get-Sha $Manifest) -ne $ManifestSha) { Stop-Land "Manifest hash mismatch for $Manifest." }
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
$binSha = Get-Sha $bin
if ($ExpectedBinarySha -and $binSha -ne $ExpectedBinarySha) { Stop-Land "Binary SHA mismatch (got $binSha)." }

Invoke-Git fetch origin main
& git merge-base --is-ancestor origin/main HEAD
if ($LASTEXITCODE -ne 0) { Stop-Land 'origin/main is not an ancestor of HEAD; rebase and re-verify first.' }
if ($DryRun) { Write-Host "DRY RUN OK: would stage $($paths.Count) paths, commit if pending, push HEAD to origin main, fast-forward root. Binary $binSha."; return }

Invoke-Git add -- @paths
if (git diff --cached --name-only) {
    if (!$Message) { Stop-Land 'Staged changes need -Message.' }
    Invoke-Git commit -F $Message
}
Invoke-Git push origin HEAD:main   # never forced: non-fast-forward is rejected

# Fast-forward the root checkout. Local edits blocking it are cleared only if they are CR-only.
$main = ((git worktree list --porcelain)[0]) -replace '^worktree ', ''
if ((git -C $main branch --show-current) -ne 'main') { Stop-Land "Root checkout $main is not on main." }
$incoming = @(git -C $main diff --name-only HEAD origin/main)
$blocking = @(git -C $main diff --name-only HEAD | Where-Object { $_ -in $incoming })
if ($blocking -contains '.gitignore') { Stop-Land 'Root .gitignore edit blocks the fast-forward.' }
foreach ($b in $blocking) { git -C $main diff --ignore-cr-at-eol --quiet -- $b; if ($LASTEXITCODE -ne 0) { Stop-Land "Root edit blocks fast-forward and is not CR-only: $b" } }
if ($blocking) { Invoke-Git -C $main restore -- @blocking }
Invoke-Git -C $main merge --ff-only origin/main
Write-Host "LANDED $(git rev-parse --short HEAD) on origin/main; root fast-forwarded; binary $binSha."
