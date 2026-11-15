. "$PSScriptRoot\capture-lib.ps1"
$apps = @('app:Microsoft.WindowsCalculator_8wekyb3d8bbwe!App','app:Microsoft.WindowsNotepad_8wekyb3d8bbwe!App','app:Microsoft.Paint_8wekyb3d8bbwe!App')
$pins = @($apps | ForEach-Object { "pin=$_" }) + @('folder=' + ((@('Daily tools') + $apps) -join [char]9))
foreach ($style in 'floating','attached') {
    $process = Start-Roqa 'timer' ($pins + @("dockstyle=$style")) "dock-$style"
    try {
        Control 'show-dock'; Park; Wait-Ms 1000; Shot "40-$style-folder-closed" 0 850 1920 230
        $pos = if ($style -eq 'floating') { 889 } else { 187 }
        $bottom = if ($style -eq 'floating') { 1036 } else { 1048 }
        [Mouse07]::Move($pos, $bottom); Wait-Ms 200; Shot "41-$style-folder-hover" 0 760 1920 320
        Wait-Ms 1000; Shot "42-$style-folder-popover" 0 720 1920 360
        Park; Wait-Ms 800; [Mouse07]::Move(($pos - 114), $bottom); Wait-Ms 750; Shot "43-$style-hover-label" 0 850 1920 230
        Park; Wait-Ms 600; [Mouse07]::Right($pos, $bottom); Wait-Ms 700; Shot "44-$style-context-menu" 0 570 1920 510
        [Mouse07]::Click(1600, 600); Park
    } finally {
        Stop-Roqa $process
    }
}
