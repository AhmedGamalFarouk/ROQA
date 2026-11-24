# Live tests and screenshots use a non-primary monitor when available.
function Get-TestScreen {
    Add-Type -AssemblyName System.Windows.Forms
    $screens = [Windows.Forms.Screen]::AllScreens
    $primary = $screens | Where-Object Primary | Select-Object -First 1
    $second = $screens | Where-Object { -not $_.Primary } | Sort-Object DeviceName | Select-Object -First 1
    if ($second) { return $second }
    Write-Warning 'Only one monitor found: testing on the primary monitor.'
    $primary
}

# store.roqa lines that keep ROQA off every monitor except the test one (a test store starts from these).
function Get-TestOffLines {
    $test = (Get-TestScreen).DeviceName
    [Windows.Forms.Screen]::AllScreens | Where-Object { $_.DeviceName -ne $test } | ForEach-Object { "off=$($_.DeviceName)" }
}

# A private profile folder for a test run: a copy of the owner's store with ROQA switched off on every monitor but
# the test one, refreshed from the real store each time, so the real store is never changed. Returns the folder; a
# caller points LOCALAPPDATA at it only while it starts ROQA (run.ps1 -Test) or for the whole run (the test scripts).
function New-TestProfile([string]$ProfileDir) {
    New-Item -ItemType Directory -Force -Path (Join-Path $ProfileDir 'ROQA') | Out-Null
    $real = Join-Path $env:LOCALAPPDATA 'ROQA\store.roqa'
    $lines = @(Get-Content -LiteralPath $real -ErrorAction SilentlyContinue | Where-Object { $_ -ne 'end' -and $_ -notmatch '^off=' }) + @(Get-TestOffLines) + 'end'
    [IO.File]::WriteAllLines((Join-Path $ProfileDir 'ROQA\store.roqa'), $lines, (New-Object Text.UTF8Encoding $false))
    $ProfileDir
}
