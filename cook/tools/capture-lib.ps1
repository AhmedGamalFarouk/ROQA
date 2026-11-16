$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$phase = if ($env:ROQA_CAPTURE_PHASE) { $env:ROQA_CAPTURE_PHASE } else { 'r11' }
$out = if ($env:ROQA_CAPTURE_DIR) { $env:ROQA_CAPTURE_DIR } else { "$root\cook\stills\$phase" }
$exe = "$root\target\release\roqa.exe"

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Path "$root\tests\NativeProbe.cs" -ReferencedAssemblies System.Drawing -ErrorAction SilentlyContinue
Add-Type -Path "$PSScriptRoot\Cap.cs" -ReferencedAssemblies System.Drawing -ErrorAction SilentlyContinue

[NativeProbe]::EnablePhysicalPixels()
New-Item -ItemType Directory -Force $out | Out-Null
if (Test-Path -LiteralPath $exe) {
    $provFile = "$out\provenance.json"
    if (-not (Test-Path -LiteralPath $provFile)) {
        [ordered]@{phase=$phase;capturedUtc=[DateTime]::UtcNow.ToString('o');binary=$exe;bytes=(Get-Item $exe).Length;sha256=(Get-FileHash $exe -Algorithm SHA256).Hash;backdrop='#696969';dpi=96} | ConvertTo-Json | Set-Content $provFile -Encoding UTF8
    }
}

function Wait-Ms($ms) { Start-Sleep -Milliseconds $ms }
function Raise-Backdrop {
    $back = [Mouse07]::Backdrop()
    if ($back -eq [IntPtr]::Zero) { throw 'Gray backdrop missing' }
    [void][NativeProbe]::SetWindowPos($back, [IntPtr](-1), 0, 0, 0, 0, 67)
}
function Park { [Mouse07]::Move(1650, 600) }
function Shot($name, $x=560, $y=0, $w=800, $h=300) {
    if ([Mouse07]::Backdrop() -eq [IntPtr]::Zero) { throw 'Backdrop closed before capture' }
    # The window can exist yet be covered (r14 caught the desktop): check a pixel ROQA never draws on.
    $probe = New-Object Drawing.Bitmap 1, 1
    $pg = [Drawing.Graphics]::FromImage($probe)
    $pg.CopyFromScreen(40, 540, 0, 0, $probe.Size)
    $c = $probe.GetPixel(0, 0); $pg.Dispose(); $probe.Dispose()
    if ($c.R -ne 105 -or $c.G -ne 105 -or $c.B -ne 105) { throw 'Backdrop not visible; refusing to capture the desktop' }
    [Cap]::Still($x, $y, $w, $h, "$out\$name.png")
}
function Control($action) {
    $control = Start-Process $exe -ArgumentList "--$action" -WindowStyle Hidden -PassThru
    if (-not $control.WaitForExit(6000)) { throw 'control timeout' }
}
function Start-Roqa($tab='media', [string[]]$extra=@(), $tag='default') {
    Raise-Backdrop
    # ROQA is single-instance: a copy still shutting down makes the new one exit at once, which looked like a crash.
    for ($i = 0; $i -lt 25 -and (Get-Process roqa -ErrorAction SilentlyContinue); $i++) { Wait-Ms 200 }
    if (Get-Process roqa -ErrorAction SilentlyContinue) { throw 'Another ROQA is still running; quit it before capturing' }
    $dir = "$out\private\$tag-$([guid]::NewGuid().ToString('N').Substring(0,8))"
    New-Item -ItemType Directory -Force "$dir\ROQA" | Out-Null
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $hour = (Get-Date).Hour
    $lines = @('version=2','dock=1','eyes=0','fshide=0','city=Cairo','wxplace=300444,312357,0,Cairo',"wxdata=$now,285,1,310,190,$($hour):285:1,$(($hour+1)%24):290:2,$(($hour+2)%24):280:3", "lasttab=$tab",'tablimits=1') + $extra + @('end')
    [IO.File]::WriteAllLines("$dir\ROQA\store.roqa", $lines, (New-Object Text.UTF8Encoding $false))
    $old = $env:LOCALAPPDATA
    try {
        $env:LOCALAPPDATA = $dir
        $process = Start-Process $exe -WindowStyle Hidden -PassThru
    } finally {
        $env:LOCALAPPDATA = $old
    }
    Wait-Ms 1200
    if ($process.HasExited) { throw "ROQA exited $($process.ExitCode)" }
    Park
    return $process
}
function Stop-Roqa($process) {
    Control 'quit'
    if ($process) { [void]$process.WaitForExit(6000) }
    Wait-Ms 250
}
function Open-Notch {
    Control 'show-notch'
    [Mouse07]::Move(960, 4)
    Wait-Ms 100
    [Mouse07]::Move(962, 6)
    Wait-Ms 1100
}
function Settings-Rect {
    for ($i = 0; $i -lt 30; $i++) {
        $handle = [NativeProbe]::FindWindow('#32770', 'ROQA settings')
        if ($handle -ne [IntPtr]::Zero) {
            $rect = New-Object NativeProbe+Rect
            if ([NativeProbe]::GetWindowRect($handle, [ref]$rect)) {
                return $rect
            }
        }
        Wait-Ms 200
    }
    throw 'Settings not found'
}
function Settings-RectHandle {
    for ($i = 0; $i -lt 30; $i++) {
        $handle = [NativeProbe]::FindWindow('#32770', 'ROQA settings')
        if ($handle -ne [IntPtr]::Zero) { return $handle }
        Wait-Ms 200
    }
    throw 'Settings not found'
}
function Select-Tab($tab) {
    $index = ([array]@('media','weather','prayer','timer','shelf','limits','widgets')).IndexOf($tab)
    $pos = if ($index -eq 0) { 908 } else { 931 + 17 * ($index - 1) }
    [Mouse07]::Click($pos, 135)
    Wait-Ms 350
    [Mouse07]::Move(960, 40)
    Wait-Ms 1000
}
function Settings-Shot($name) {
    Raise-Backdrop
    $handle = [IntPtr]::Zero
    for ($i = 0; $i -lt 30; $i++) {
        $handle = [NativeProbe]::FindWindow('#32770', 'ROQA settings')
        if ($handle -ne [IntPtr]::Zero) { break }
        Wait-Ms 200
    }
    if ($handle -eq [IntPtr]::Zero) { throw 'Settings not found' }
    [void][NativeProbe]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 67)
    Wait-Ms 400
    $rect = Settings-Rect
    Shot $name $rect.Left $rect.Top ($rect.Right - $rect.Left) ($rect.Bottom - $rect.Top)
}
function Build-WakeContactSheet($eyesDir, $targetFile) {
    $files = Get-ChildItem -Path $eyesDir -File -Filter "wake-*.png" | Sort-Object Name
    if (-not $files -or $files.Count -eq 0) { return }
    $sel = @()
    for ($i = 0; $i -lt 12; $i++) {
        $idx = [int][Math]::Round(($files.Count - 1) * $i / 11.0)
        $sel += $files[$idx]
    }
    $first = [Drawing.Image]::FromFile($sel[0].FullName)
    $w = $first.Width
    $h = $first.Height
    $first.Dispose()

    $sheetW = $w * 3
    $sheetH = ($h + 24) * 4
    $bmp = New-Object Drawing.Bitmap($sheetW, $sheetH)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $bgBrush = New-Object Drawing.SolidBrush([Drawing.Color]::FromArgb(105, 105, 105))
    $textBrush = [Drawing.Brushes]::White
    $font = New-Object Drawing.Font("Segoe UI", 9)
    $g.FillRectangle($bgBrush, 0, 0, $sheetW, $sheetH)

    for ($i = 0; $i -lt $sel.Count; $i++) {
        $col = $i % 3
        $row = [Math]::Floor($i / 3)
        $x = $col * $w
        $y = $row * ($h + 24)
        $stem = [IO.Path]::GetFileNameWithoutExtension($sel[$i].Name)
        $g.DrawString($stem, $font, $textBrush, [float]($x + 4), [float]($y + 4))
        $frame = [Drawing.Image]::FromFile($sel[$i].FullName)
        $g.DrawImage($frame, $x, ($y + 24), $w, $h)
        $frame.Dispose()
    }
    $g.Dispose()
    $font.Dispose()
    $bgBrush.Dispose()
    $bmp.Save($targetFile, [Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}
