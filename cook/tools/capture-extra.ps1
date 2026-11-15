. "$PSScriptRoot\capture-lib.ps1"
$process = Start-Roqa 'media' @() 'settings-pages'
try {
    Control 'show-settings'
    Wait-Ms 2500
    $pages = @('general','appearance','notch','dock-edges','weather','prayer','limits','about')
    for ($i = 0; $i -lt $pages.Count; $i++) {
        $complete = $false
        for ($retry = 0; $retry -lt 3 -and -not $complete; $retry++) {
            try {
                $r = Settings-Rect
                [Mouse07]::Click(($r.Left + 80), ($r.Top + 59 + 37 * $i))
                $waitMs = if ($i -ge 3) { 1500 } else { 800 }
                Wait-Ms $waitMs
                Settings-Shot ("70-settings-" + $pages[$i])
                $h = Settings-RectHandle
                [void][NativeProbe]::SendMessage($h, 0x115, [UIntPtr]::new([uint32]3), [IntPtr]::Zero)
                Wait-Ms $waitMs
                Settings-Shot ("71-settings-" + $pages[$i] + "-scrolled")
                $complete = $true
            } catch {
                if ($retry -eq 2) { throw "Settings page '$($pages[$i])' failed after 3 attempts: $($_.Exception.Message)" }
                Control 'show-settings'
                Wait-Ms 700
            }
        }
    }
} finally {
    Stop-Roqa $process
}
