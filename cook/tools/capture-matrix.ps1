param(
    [switch]$SkipThemes,
    [switch]$OnlyThemes
)
. "$PSScriptRoot\capture-lib.ps1"

if (-not $OnlyThemes) {
    foreach ($tab in 'media','weather','prayer','timer','shelf','limits','widgets') {
        $process = Start-Roqa $tab @() "tab-$tab"
        try {
            Shot "01-$tab-collapsed" 800 0 320 75
            Open-Notch
            Select-Tab $tab
            Shot "10-$tab-rest"
            if ($tab -eq 'timer') {
                [Mouse07]::Click(825, 80); Wait-Ms 900; Shot '11-timer-running'; Park; Wait-Ms 1300; Shot '11-timer-collapsed' 800 0 320 75
            }
            if ($tab -eq 'shelf') {
                [Mouse07]::Click(868, 20); Wait-Ms 700; Shot '12-shelf-copied'
            }
            if ($tab -eq 'media') {
                [Mouse07]::Move(1000, 135); Wait-Ms 600; Shot '13-rail-hover'
            }
        } finally {
            Stop-Roqa $process
        }
    }

    $process = Start-Roqa 'media' @() 'settings'
    try {
        Control 'show-settings'; Wait-Ms 1500; Settings-Shot '20-settings-general'
        if ($true) {
            $r = Settings-Rect
            [Mouse07]::Click(($r.Left + 80), ($r.Top + 96)); Wait-Ms 400
            [Mouse07]::Click(($r.Left + 500), ($r.Top + 137)); Wait-Ms 500; Shot '21-settings-combo-open' $r.Left $r.Top ($r.Right - $r.Left) ($r.Bottom - $r.Top)
            [Mouse07]::Click(($r.Left + 300), ($r.Top + 70)); Wait-Ms 300
            $h = [NativeProbe]::FindWindow('#32770', 'ROQA settings')
            [void][NativeProbe]::SendMessage($h, 0x115, [UIntPtr]::new([uint32]3), [IntPtr]::Zero)
            Wait-Ms 700; Settings-Shot '22-settings-scrolled'
            [Mouse07]::Click(($r.Left + 80), ($r.Top + 96)); Wait-Ms 400
            $combo = [Mouse07]::GetDlgItem($h, 110)
            [void][NativeProbe]::SendMessage($combo, 0x14E, [UIntPtr]::new([uint32]1), [IntPtr]::Zero)
            [void][NativeProbe]::SendMessage($h, 0x111, [UIntPtr]::new([uint32](65536 + 110)), $combo)
            Wait-Ms 1000; Settings-Shot '23-settings-live-light-appearance'
            [Mouse07]::Click(($r.Left + 80), ($r.Top + 59)); Wait-Ms 500; Settings-Shot '24-settings-live-light-general'
            [void][NativeProbe]::SendMessage($combo, 0x14E, [UIntPtr]::Zero, [IntPtr]::Zero)
            [void][NativeProbe]::SendMessage($h, 0x111, [UIntPtr]::new([uint32](65536 + 110)), $combo)
            Wait-Ms 1000; Settings-Shot '25-settings-live-classic-general'
        }
    } finally {
        Stop-Roqa $process
    }
}

if (-not $SkipThemes) {
    foreach ($theme in 'classic','midnight','light','frosted-tint','liquid-glass') {
        $process = Start-Roqa 'prayer' @("theme=$theme") "theme-$theme"
        try {
            Shot "30-$theme-collapsed" 800 0 320 75
            Shot "31-$theme-dock" 0 900 1920 180
            Open-Notch
            Select-Tab 'prayer'
            Shot "32-$theme-prayer"
            Control 'show-settings'; Wait-Ms 1000; Settings-Shot "33-$theme-settings"
        } finally {
            Stop-Roqa $process
        }
    }
}
