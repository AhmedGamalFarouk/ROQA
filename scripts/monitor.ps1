# Owner decision 2026-11-18: live tests and screenshots use the second (non-primary) monitor.
# Dot-source this file. ROQA_TEST_MONITOR=primary picks the primary monitor instead; a PC with one monitor
# falls back to it with a warning.
function Get-TestScreen {
    Add-Type -AssemblyName System.Windows.Forms
    $screens = [Windows.Forms.Screen]::AllScreens
    $primary = $screens | Where-Object Primary | Select-Object -First 1
    if ($env:ROQA_TEST_MONITOR -eq 'primary') { return $primary }
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
