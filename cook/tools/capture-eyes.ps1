. "$PSScriptRoot\capture-lib.ps1"
New-Item -ItemType Directory -Force "$out\eyes" | Out-Null
$process = Start-Roqa 'timer' @('eyes=1','tabmedia=0','dock=0','notchopen=click') 'eyes'
try {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.ElapsedMilliseconds -lt 23000) {
        $at = [int]$watch.ElapsedMilliseconds
        if ($at -ge 17500) {
            Shot ("eyes\doze-{0:D5}" -f $at) 800 0 320 55
        }
        Wait-Ms 45
    }
    [Mouse07]::Move(960, 12)
    $watch.Restart()
    while ($watch.ElapsedMilliseconds -lt 2000) {
        $at = [int]$watch.ElapsedMilliseconds
        Shot ("eyes\wake-{0:D5}" -f $at) 800 0 320 55
        Wait-Ms 30
    }
    Park
} finally {
    Stop-Roqa $process
}
Build-WakeContactSheet "$out\eyes" "$out\50-wake-contact.png"
