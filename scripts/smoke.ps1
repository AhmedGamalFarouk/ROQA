param([switch]$SkipBuild, [string]$EvidenceDirectory = 'artifacts/m1')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (!$SkipBuild) { & "$PSScriptRoot/cargo.ps1" build --locked --release }
Add-Type -Path "$projectRoot/tests/PickerProbe.cs"
Add-Type -AssemblyName System.Drawing
Add-Type -Path "$projectRoot/tests/NativeProbe.cs" -ReferencedAssemblies System.Drawing
[NativeProbe]::EnablePhysicalPixels()
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
$observations = [ordered]@{ Label = 'M1 observation only; not a product-budget or competitor result';
    BinarySha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash;
    Utc = [DateTime]::UtcNow.ToString('o'); Checks = @(); ResourceSamples = @(); ClickProbes = @() }

function Assert-Check([bool]$Condition, [string]$Name) {
    if (!$Condition) { throw "FAIL: $Name" }
    $observations.Checks += $Name
}
function Surface-Window([string]$Title) {
    $window = [NativeProbe]::PrimarySurface($Title)
    if ($window -ne [IntPtr]::Zero -and $null -ne $script:hostProcess) {
        [uint32]$owner = 0
        [void][NativeProbe]::GetWindowThreadProcessId($window, [ref]$owner)
        if ($owner -ne $script:hostProcess.Id) { throw 'A surface belongs to another instance.' }
    }
    return $window
}
function Wait-Surface([string]$Title, [bool]$Present) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $window = Surface-Window $Title
        # Absence means gone from every monitor, not only the primary one.
        $exists = if ($Present) { $window -ne [IntPtr]::Zero } else { [NativeProbe]::Surfaces($Title).Count -gt 0 }
        if ($exists -eq $Present) { return $window }
        if ($script:hostProcess.HasExited -and $Present) { throw "Host exited with $($script:hostProcess.ExitCode) waiting for $Title." }
        Start-Sleep -Milliseconds 20
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Title present=$Present."
}
function Invoke-Control([string]$Command) {
    $client = Start-Process -FilePath $executable -ArgumentList $Command -PassThru -WindowStyle Hidden
    if (!$client.WaitForExit(5000)) { $client.Kill(); throw "Control command hung: $Command" }
    $exitCode = $client.ExitCode
    $client.Dispose()
    Assert-Check ($exitCode -eq 0) "Control signal delivered: $Command"
}
function Resource-Sample([string]$Phase) {
    $script:hostProcess.Refresh()
    return [ordered]@{ Phase = $Phase; PrivateBytes = $script:hostProcess.PrivateMemorySize64;
        Handles = $script:hostProcess.HandleCount; Gdi = [NativeProbe]::GetGuiResources($script:hostProcess.Handle, 0);
        User = [NativeProbe]::GetGuiResources($script:hostProcess.Handle, 1) }
}
function Cycle-Notch {
    Invoke-Control '--hide-notch'
    [void](Wait-Surface 'ROQA Notch' $false)
    Invoke-Control '--show-notch'
    [void](Wait-Surface 'ROQA Notch' $true)
}
function Assert-Foreground([IntPtr]$Expected, [string]$Name) {
    $actual = [NativeProbe]::GetForegroundWindow()
    if ($actual -ne $Expected) {
        [uint32]$owner = 0
        [void][NativeProbe]::GetWindowThreadProcessId($actual, [ref]$owner)
        if ($owner -ne $script:hostProcess.Id) { throw "INTERFERENCE: Foreground changed outside ROQA during $Name." }
    }
    Assert-Check ($actual -eq $Expected) $Name
}
# The plain-notch shape checks need no media session: another app's session switches the notch to the media shape.
function Assert-NoMedia {
    $states = @(Select-String -LiteralPath (Join-Path $evidence 'runtime.log') -Pattern '^Media: (playing|paused|no player|permission|error) ')
    if ($states.Count -gt 0 -and $states[-1].Matches[0].Groups[1].Value -in @('playing', 'paused')) { throw 'INTERFERENCE: A media session is active; the plain-notch shape checks need none.' }
}
function Test-DockWidth([int]$Width, [int]$Dpi) {
    # Dock = 8-DIP padding each side plus one 56-DIP slot per item (picker + real app groups).
    foreach ($slots in 1..64) { if ($Width -eq [int][Math]::Floor(((16 + 56 * $slots) * $Dpi + 48) / 96)) { return $true } }
    return $false
}
function Assert-Pixels([IntPtr]$Window, [string]$File, [string]$Expected, [int]$X, [int]$Y, [object[]]$Points = $null, [int]$SettleMs = 0) {
    # Retry only observed input interference; a stable wrong-color capture remains a failure.
    for ($attempt = 1; $attempt -le 15; $attempt++) {
        Assert-Check ([NativeProbe]::SetCursorPos($X, $Y)) "Position cursor for $File"
        $stable = $true
        foreach ($sample in 1..10) {
            Start-Sleep -Milliseconds 20
            $point = New-Object NativeProbe+Point
            if (![NativeProbe]::GetCursorPos([ref]$point) -or $point.X -ne $X -or $point.Y -ne $Y) { $stable = $false; break }
        }
        if (!$stable) { continue }
        # Animated surfaces are captured only after their motion has settled.
        if ($SettleMs -gt 0) { Start-Sleep -Milliseconds $SettleMs }
        $path = Join-Path $evidence $File
        $actual = [NativeProbe]::Capture($Window, $path)
        $after = New-Object NativeProbe+Point
        if (![NativeProbe]::GetCursorPos([ref]$after) -or $after.X -ne $X -or $after.Y -ne $Y) { continue }
        if ($null -ne $Points) {
            # The dock shows real app icons, so only its background strip and our own picker glyph are known colors.
            foreach ($point in $Points) {
                $observed = [NativeProbe]::PixelAt($path, $point.X, $point.Y)
                Assert-Check ($observed -eq $point.Color) "Screen pixel $File $($point.Name) = $($point.Color) (observed $observed)"
            }
        } else {
            Assert-Check ($actual -eq $Expected) "Actual screen center $File = $Expected (observed $actual)"
            $mismatch = [NativeProbe]::CheckInteriorPixels($path, $Expected)
            Assert-Check ($mismatch -eq '') "Screen interior pixels $File are uniform $Expected ($mismatch)"
        }
        $observations.Checks += "Stable cursor capture $File on attempt $attempt"
        return
    }
    throw "INTERFERENCE: Cursor did not remain stable for $File after 15 bounded attempts."
}

$script:hostProcess = $null
$anchor = [IntPtr]::Zero
$originalForeground = [NativeProbe]::GetForegroundWindow()
$originalCursor = New-Object NativeProbe+Point
[void][NativeProbe]::GetCursorPos([ref]$originalCursor)
try {
    Assert-Check ([NativeProbe]::Surfaces('ROQA Notch').Count -eq 0 -and [NativeProbe]::Surfaces('ROQA Dock').Count -eq 0) 'No pre-existing ROQA surfaces'
    $anchor = [NativeProbe]::StartAnchor()
    foreach ($attempt in 1..5) {
        [void][NativeProbe]::SetForegroundWindow($anchor)
        Start-Sleep -Milliseconds 150
        if ([NativeProbe]::GetForegroundWindow() -eq $anchor) { break }
    }
    $focusBaseline = [NativeProbe]::GetForegroundWindow()
    if ($focusBaseline -eq [IntPtr]::Zero) { throw 'INTERFERENCE: No stable foreground window before launch.' }
    # Windows may deny a background test process foreground rights; any known existing
    # foreground is an equally valid passive nonactivation reference. Do not force another app.
    Start-Sleep -Milliseconds 100
    if ([NativeProbe]::GetForegroundWindow() -ne $focusBaseline) { throw 'INTERFERENCE: Foreground changed before ROQA launch.' }
    Assert-Check $true 'Known foreground baseline established before launch'
    $workBefore = [NativeProbe]::WorkArea()
    [void][NativeProbe]::SetCursorPos($workBefore.Left + 5, $workBefore.Top + 150)
    $script:hostProcess = Start-Process -FilePath $executable -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $evidence 'runtime.log')
    $notch = Wait-Surface 'ROQA Notch' $true
    $dock = Wait-Surface 'ROQA Dock' $true
    Start-Sleep -Milliseconds 300
    Assert-Foreground $focusBaseline 'Launch does not steal focus'
    foreach ($surface in @(@{ Window = $notch; Width = 440; Height = 164; Name = 'notch' }, @{ Window = $dock; Width = 0; Height = 64; Name = 'dock' })) {
        $rect = New-Object NativeProbe+Rect
        Assert-Check ([NativeProbe]::GetWindowRect($surface.Window, [ref]$rect)) "Read $($surface.Name) geometry"
        $dpi = [NativeProbe]::GetDpiForWindow($surface.Window)
        $height = [int][Math]::Floor(($surface.Height * $dpi + 48) / 96)
        if ($surface.Name -eq 'dock') {
            $width = $rect.Width
            Assert-Check ((Test-DockWidth $rect.Width $dpi) -and $rect.Height -eq $height) "dock matches DPI-scaled slot geometry (width=$($rect.Width))"
        } else {
            $width = [int][Math]::Floor(($surface.Width * $dpi + 48) / 96)
            Assert-Check ($rect.Width -eq $width -and $rect.Height -eq $height) "$($surface.Name) matches DPI-scaled bounds"
        }
        $margin = [int][Math]::Floor((12 * $dpi + 48) / 96)
        $expectedX = $workBefore.Left + [int][Math]::Floor(($workBefore.Width - $width) / 2)
        $expectedY = if ($surface.Name -eq 'notch') { $workBefore.Top } else { $workBefore.Bottom - $margin - $height }
        Assert-Check ($rect.Left -eq $expectedX -and $rect.Top -eq $expectedY) "$($surface.Name) matches intended centered placement"
        Assert-Check ($rect.Left -ge $workBefore.Left -and $rect.Right -le $workBefore.Right -and $rect.Top -ge $workBefore.Top -and $rect.Bottom -le $workBefore.Bottom) "$($surface.Name) stays in work area"
        $originalRect = $rect
        # Existing edge overlays react to the cursor; isolate only our test-owned HWND for pixel checks.
        $isolatedY = $workBefore.Top + [int]($workBefore.Height / 2) - $rect.Height
        Assert-Check ([NativeProbe]::SetWindowPos($surface.Window, [IntPtr](-1), $rect.Left, $isolatedY, $rect.Width, $rect.Height, 0x0010)) 'Isolate test-owned window from existing edge overlays'
        Start-Sleep -Milliseconds 200
        [void][NativeProbe]::GetWindowRect($surface.Window, [ref]$rect)
        $awayX = $workBefore.Left + 5; $awayY = $workBefore.Top + 150
        $idlePoints = $null; $hoverPoints = $null
        $px = { param([double]$dips) [Math]::Floor(($dips * $dpi + 48) / 96) }
        $hoverX = $rect.Left + [int]($rect.Width / 2); $hoverY = $rect.Top + [int]($rect.Height / 2); $settle = 0
        if ($surface.Name -eq 'notch') {
            # The HWND is a fixed 440x164 DIP (room for the media panel, overshoot and shadow); with no media session its region limits input to the collapsed 150x30 body plus both 10-DIP ears.
            $body = & $px 150; $ears = & $px 10; $bodyHeight = & $px 30
            $bodyLeft = [int][Math]::Floor(($rect.Width - $body) / 2)
            $restRegion = "$($bodyLeft - $ears),0,$($body + 2 * $ears),$bodyHeight"
            Assert-NoMedia
            $observedRegion = [NativeProbe]::RegionBox($surface.Window)
            Assert-Check ($observedRegion -eq $restRegion) "Collapsed notch takes input only over its visible body and ears (expected $restRegion, observed $observedRegion)"
            $idlePoints = @(@{ Name = 'collapsed body'; X = [int]($bodyLeft + (& $px 6)); Y = [int]($bodyHeight / 2); Color = '0,0,0' })
            # A point inside the 320x80 DIP expanded body but outside the collapsed body proves the opened shape.
            $expandedLeft = [int][Math]::Floor(($rect.Width - (& $px 320)) / 2)
            $hoverPoints = @($idlePoints[0], @{ Name = 'expanded body'; X = [int]($expandedLeft + (& $px 6)); Y = [int](& $px 60); Color = '0,0,0' })
            $hoverY = $rect.Top + [int]($bodyHeight / 2)
            # Hover intent (320 ms) plus the 500 ms open spring, or the 560 ms close spring, must settle first.
            $settle = 1100
        } elseif ($surface.Name -eq 'dock') {
            $midY = [int]($rect.Height / 2); $padX = [int](& $px 4)
            $glyphX = [int]((& $px 8) + (& $px 28) - (& $px 8)); $glyphY = [int]($midY - (& $px 8))
            $glyph = @{ Name = 'picker glyph'; X = $glyphX; Y = $glyphY; Color = '220,225,232' }
            $idlePoints = @(@{ Name = 'background'; X = $padX; Y = $midY; Color = '0,0,0' }, $glyph)
            $hoverPoints = @(@{ Name = 'background'; X = $padX; Y = $midY; Color = '20,20,22' }, $glyph)
        }
        Assert-Pixels $surface.Window "$($surface.Name)-idle.png" '0,0,0' $awayX $awayY $idlePoints
        Assert-Pixels $surface.Window "$($surface.Name)-hover.png" '0,0,0' $hoverX $hoverY $hoverPoints $settle
        if ($surface.Name -eq 'notch') {
            # Only the body and ears own input; the shadow is a separate transparent HWND.
            $openEars = & $px 16
            $openRegion = "$($expandedLeft - $openEars),0,$((& $px 320) + 2 * $openEars),$(& $px 80)"
            Assert-NoMedia
            $observedRegion = [NativeProbe]::RegionBox($surface.Window)
            Assert-Check ($observedRegion -eq $openRegion) "Open notch input excludes its shadow (expected $openRegion, observed $observedRegion)"
            $shadowPoint = New-Object NativeProbe+Point
            $shadowPoint.X = $hoverX; $shadowPoint.Y = $rect.Top + (& $px 100)
            $underShadow = [NativeProbe]::WindowFromPoint($shadowPoint)
            Assert-Check ($underShadow -ne $surface.Window -and $underShadow -ne (Surface-Window 'ROQA Notch Shadow')) 'Open shadow band resolves to an underlying window'
            $sizeNow = New-Object NativeProbe+Rect
            [void][NativeProbe]::GetWindowRect($surface.Window, [ref]$sizeNow)
            Assert-Check ($sizeNow.Left -eq $rect.Left -and $sizeNow.Top -eq $rect.Top -and $sizeNow.Width -eq $rect.Width -and $sizeNow.Height -eq $rect.Height) 'Hover opens the notch in place without moving or resizing its HWND'
        }
        Assert-Foreground $focusBaseline 'Hover does not steal focus'
        Assert-Pixels $surface.Window "$($surface.Name)-leave.png" '0,0,0' $awayX $awayY $idlePoints $settle
        if ($surface.Name -eq 'notch') {
            $underShadow = [NativeProbe]::WindowFromPoint($shadowPoint)
            Assert-Check ($underShadow -ne $surface.Window -and $underShadow -ne (Surface-Window 'ROQA Notch Shadow')) 'Collapsed shadow band resolves to an underlying window'
            Assert-NoMedia
            $observedRegion = [NativeProbe]::RegionBox($surface.Window)
            Assert-Check ($observedRegion -eq $restRegion) "Pointer leave collapses the notch and restores its rest region (observed $observedRegion)"
            # Hover intent: touching the pill and leaving sideways within 320 ms must never open the panel.
            Assert-Check ([NativeProbe]::SetCursorPos($hoverX, $hoverY)) 'Touch the collapsed pill'
            Start-Sleep -Milliseconds 100
            Assert-Check ([NativeProbe]::SetCursorPos($rect.Left + (& $px 6), $rect.Top + (& $px 10))) 'Leave sideways along the screen edge'
            Start-Sleep -Milliseconds 800
            Assert-NoMedia
            $observedRegion = [NativeProbe]::RegionBox($surface.Window)
            Assert-Check ($observedRegion -eq $restRegion) "A pointer passing over the pill does not open the panel (observed $observedRegion)"
        }
        if ($surface.Name -eq 'notch') {
            $anchorRect = New-Object NativeProbe+Rect
            [void][NativeProbe]::GetWindowRect($anchor, [ref]$anchorRect)
            try {
                Assert-Check ([NativeProbe]::SetWindowPos($anchor, [IntPtr]::Zero, $rect.Left - 20, $rect.Top - 40, $rect.Width + 40, $rect.Height + 80, 0x0014)) 'Place underlying fixture behind the notch'
                # Each click must be received by this PowerShell process's fixture, not ROQA.
                foreach ($case in @(
                    @{ Name = 'rest rounded corner'; Wait = 0; X = $bodyLeft + (& $px 1); Y = & $px 29 },
                    @{ Name = 'hover intent side margin'; Wait = 120; X = & $px 50; Y = & $px 40 },
                    @{ Name = 'expanding side shadow'; Wait = 1; Expansion = $true },
                    @{ Name = 'open side shadow'; Wait = 1100; X = & $px 50; Y = & $px 40 },
                    @{ Name = 'open rounded corner'; Wait = 1100; X = $expandedLeft + (& $px 1); Y = & $px 79 },
                    @{ Name = 'closing side shadow'; Wait = 1100; Close = $true; X = & $px 50; Y = & $px 40 },
                    @{ Name = 'closing rounded corner'; Wait = 1100; Close = $true; X = $expandedLeft + (& $px 1); Y = & $px 79 }
                )) {
                    [void][NativeProbe]::SetCursorPos($awayX, $awayY)
                    Start-Sleep -Milliseconds 650
                    if ($case.Wait -gt 0) {
                        [void][NativeProbe]::SetCursorPos($hoverX, $hoverY)
                        Start-Sleep -Milliseconds $case.Wait
                    }
                    if ($case.ContainsKey('Close')) {
                        [void][NativeProbe]::SetCursorPos($awayX, $awayY)
                        Start-Sleep -Milliseconds 60
                    }
                    $point = New-Object NativeProbe+Point
                    $phaseClock = [Diagnostics.Stopwatch]::StartNew()
                    $waitToExpansionMs = 0
                    if ($case.ContainsKey('Expansion')) {
                        # Observe actual growth beyond the 33.5-DIP intent swell. Do not infer
                        # expansion from a sleep: fail if no intermediate geometry is observed.
                        do {
                            $regionBefore = [NativeProbe]::RegionBox($surface.Window)
                            $boundsBefore = @($regionBefore.Split(',') | ForEach-Object { [int]$_ })
                            if ($boundsBefore.Count -ne 4) { throw 'No notch region while waiting for expansion.' }
                            if ($boundsBefore[3] -ge (& $px 48) -and $boundsBefore[3] -le (& $px 64)) { break }
                            Start-Sleep -Milliseconds 2
                        } while ($phaseClock.ElapsedMilliseconds -lt 2000)
                        Assert-Check ($boundsBefore[3] -ge (& $px 48) -and $boundsBefore[3] -le (& $px 64)) 'Observe intermediate panel expansion after hover intent'
                        $waitToExpansionMs = $phaseClock.ElapsedMilliseconds
                        $phaseClock.Restart()
                        # Just outside the contemporaneous silhouette bounds, halfway down
                        # the growing panel: inside the shadow, outside all body-region rows.
                        $point.X = $rect.Left + $boundsBefore[0] - (& $px 2)
                        $point.Y = $rect.Top + [int][Math]::Floor($boundsBefore[3] / 2)
                    } else {
                        $regionBefore = [NativeProbe]::RegionBox($surface.Window)
                        $point.X = $rect.Left + $case.X; $point.Y = $rect.Top + $case.Y
                    }
                    Assert-Check ([NativeProbe]::WindowFromPoint($point) -eq $anchor) "$($case.Name) resolves to the underlying fixture"
                    $beforeClicks = [NativeProbe]::AnchorClicks()
                    [PickerProbe]::Click($point.X, $point.Y)
                    $regionAfter = [NativeProbe]::RegionBox($surface.Window)
                    $clickElapsedMs = $phaseClock.ElapsedMilliseconds
                    if ($case.ContainsKey('Expansion')) {
                        $boundsAfter = @($regionAfter.Split(',') | ForEach-Object { [int]$_ })
                        Assert-Check ($boundsAfter.Count -eq 4 -and $boundsAfter[3] -ge $boundsBefore[3] -and $boundsAfter[3] -lt (& $px 76)) 'Expansion click is bracketed by growing intermediate geometry before settling'
                    }
                    Start-Sleep -Milliseconds 80
                    $receivedClicks = [NativeProbe]::AnchorClicks() - $beforeClicks
                    $observations.ClickProbes += [ordered]@{ Phase = $case.Name; RegionBefore = $regionBefore;
                        RegionAfter = $regionAfter; WindowX = $point.X - $rect.Left; WindowY = $point.Y - $rect.Top;
                        WaitToExpansionMs = $waitToExpansionMs; ObservationToInjectionMs = $clickElapsedMs; ReceivedClicks = $receivedClicks }
                    Assert-Check ($receivedClicks -eq 1) "$($case.Name) delivers a real click to the underlying process"
                }
            } finally {
                [void][NativeProbe]::SetWindowPos($anchor, [IntPtr]::Zero, $anchorRect.Left, $anchorRect.Top, $anchorRect.Width, $anchorRect.Height, 0x0014)
                [void][NativeProbe]::SetCursorPos($awayX, $awayY)
                Start-Sleep -Milliseconds 650
            }
        }
        Assert-Check ([NativeProbe]::SetWindowPos($surface.Window, [IntPtr](-1), $originalRect.Left, $originalRect.Top, $originalRect.Width, $originalRect.Height, 0x0010)) 'Restore test-owned window to intended geometry'
    }
    Invoke-Control '--hide-dock'; [void](Wait-Surface 'ROQA Dock' $false)
    Assert-Check (!$script:hostProcess.HasExited) 'Closing one surface preserves the other'
    Invoke-Control '--show-dock'; [void](Wait-Surface 'ROQA Dock' $true)
    $notch = Wait-Surface 'ROQA Notch' $true
    $realDpi = [NativeProbe]::GetDpiForWindow($notch)
    # M3: WM_DPICHANGED only triggers a re-read of the real topology; its DPI value is not trusted.
    [NativeProbe]::DpiChange($notch, $realDpi + 48)
    Start-Sleep -Milliseconds 250
    $afterDpi = Wait-Surface 'ROQA Notch' $true
    $scaledRect = New-Object NativeProbe+Rect
    [void][NativeProbe]::GetWindowRect($afterDpi, [ref]$scaledRect)
    Assert-Check ($afterDpi -eq $notch -and $scaledRect.Width -eq [int][Math]::Floor((440 * $realDpi + 48) / 96)) "Synthetic WM_DPICHANGED keeps the unchanged notch HWND at the real DPI (dpi=$realDpi, width=$($scaledRect.Width))"
    # Query a separate PMv2 HWND on the primary monitor, never GetDpiForSystem/sign-in scale.
    $currentPrimaryDpi = [NativeProbe]::GetDpiForWindow($anchor)
    Assert-Check ($currentPrimaryDpi -gt 0) 'Independent primary-monitor DPI query succeeds'
    foreach ($broadcast in @(0x001A, 0x007E)) {
        [void][NativeProbe]::SendMessage($notch, $broadcast, [UIntPtr]::Zero, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 250
        $notch = Wait-Surface 'ROQA Notch' $true
        $dock = Wait-Surface 'ROQA Dock' $true
        foreach ($entry in @(@{ Window = $notch; Width = 440; Height = 164 }, @{ Window = $dock; Width = 0; Height = 64 })) {
            [void][NativeProbe]::GetWindowRect($entry.Window, [ref]$scaledRect)
            $widthOk = if ($entry.Width -eq 0) { Test-DockWidth $scaledRect.Width $currentPrimaryDpi } else { $scaledRect.Width -eq [int][Math]::Floor(($entry.Width * $currentPrimaryDpi + 48) / 96) }
            Assert-Check ($widthOk -and $scaledRect.Height -eq [int][Math]::Floor(($entry.Height * $currentPrimaryDpi + 48) / 96)) "Broadcast $broadcast reconciles both dimensions to current primary-monitor DPI"
        }
    }
    $unchangedNotch = $notch; $unchangedDock = $dock
    foreach ($repeat in 1..5) {
        [void][NativeProbe]::SendMessage($notch, 0x001A, [UIntPtr]::Zero, [IntPtr]::Zero)
        [void][NativeProbe]::SendMessage($dock, 0x001A, [UIntPtr]::Zero, [IntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 250
    Assert-Check ((Wait-Surface 'ROQA Notch' $true) -eq $unchangedNotch -and (Wait-Surface 'ROQA Dock' $true) -eq $unchangedDock) 'Unchanged settings preserve surface HWNDs'
    # The dock remains alive, so each cycle destroys real notch HWND/composition objects without exiting the host.
    1..10 | ForEach-Object { Cycle-Notch }
    Start-Sleep -Seconds 1
    $baseline = Resource-Sample 'warm'
    $observations.ResourceSamples += $baseline
    foreach ($block in 1..5) {
        1..20 | ForEach-Object { Cycle-Notch }
        Start-Sleep -Seconds 1
        $sample = Resource-Sample "block-$block"
        $observations.ResourceSamples += $sample
        Assert-Check (($sample.PrivateBytes - $baseline.PrivateBytes) -le 5MB) "Block $block private-byte plateau"
        Assert-Check (($sample.Handles - $baseline.Handles) -le 10) "Block $block handle plateau"
        Assert-Check (($sample.Gdi - $baseline.Gdi) -le 5 -and ($sample.User - $baseline.User) -le 5) "Block $block USER/GDI plateau"
    }
    foreach ($counter in @('PrivateBytes', 'Handles', 'Gdi', 'User')) {
        $samples = @($observations.ResourceSamples)
        $strictlyGrowing = $true
        for ($index = 1; $index -lt $samples.Count; $index++) {
            if ($samples[$index][$counter] -le $samples[$index - 1][$counter]) { $strictlyGrowing = $false }
        }
        Assert-Check (!$strictlyGrowing) "No sustained monotonic $counter growth across resource blocks"
    }
    $observations.WorkBefore = $workBefore
    $observations.WorkAfter = [NativeProbe]::WorkArea()
    Assert-Check (($observations.WorkAfter | ConvertTo-Json -Compress) -eq ($workBefore | ConvertTo-Json -Compress)) 'Work area unchanged'
    Invoke-Control '--quit'
    Assert-Check ($script:hostProcess.WaitForExit(5000)) 'Focus-independent quit exits promptly'
    Assert-Check ($script:hostProcess.ExitCode -eq 0) 'Graceful quit returns zero'
    $script:hostProcess.Dispose(); $script:hostProcess = $null
    $script:hostProcess = Start-Process -FilePath $executable -PassThru -WindowStyle Hidden
    $notch = Wait-Surface 'ROQA Notch' $true
    $dock = Wait-Surface 'ROQA Dock' $true
    Assert-Check ([NativeProbe]::PostMessage($notch, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)) 'Post native close to notch'
    [void](Wait-Surface 'ROQA Notch' $false)
    Assert-Check (!$script:hostProcess.HasExited) 'Native close of one surface preserves host'
    Invoke-Control '--show-notch'; $notch = Wait-Surface 'ROQA Notch' $true
    Assert-Check ([NativeProbe]::PostMessage($dock, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)) 'Post native close to dock'
    [void](Wait-Surface 'ROQA Dock' $false)
    Assert-Check (!$script:hostProcess.HasExited) 'Closing dock preserves notch'
    [void][NativeProbe]::PostMessage($notch, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    Assert-Check ($script:hostProcess.WaitForExit(5000)) 'Native close of last surface exits host'
    Assert-Check ($script:hostProcess.ExitCode -eq 0) 'Last-surface shutdown returns zero'
    $script:hostProcess.Dispose(); $script:hostProcess = $null
    foreach ($probe in @('--probe-failure', '--probe-panic')) {
        $failedHost = Start-Process -FilePath $executable -ArgumentList $probe -PassThru -WindowStyle Hidden
        if (!$failedHost.WaitForExit(5000)) { $failedHost.Kill(); throw "Failure probe hung: $probe" }
        Assert-Check ($failedHost.ExitCode -eq 1) "Failure takes expected orderly exit code 1: $probe"
        $failedHost.Dispose()
        Assert-Check ([NativeProbe]::Surfaces('ROQA Notch').Count -eq 0 -and [NativeProbe]::Surfaces('ROQA Dock').Count -eq 0) "No orphan surface after exited failure probe (OS cleanup also applies): $probe"
    }
    $observations.Status = 'PASS'
}
catch {
    $observations.Status = if ($_.Exception.Message.StartsWith('INTERFERENCE:')) { 'INCONCLUSIVE' } else { 'FAIL' }
    $observations.Failure = $_.Exception.Message
    throw
}
finally {
    if ($null -ne $script:hostProcess) {
        if (!$script:hostProcess.HasExited) {
            try { Invoke-Control '--quit' } catch { Write-Warning $_ }
            if (!$script:hostProcess.WaitForExit(2000)) { $script:hostProcess.Kill(); Write-Warning 'Forced cleanup of the test-owned process; this is not a test pass.' }
        }
        $script:hostProcess.Dispose()
    }
    if ($anchor -ne [IntPtr]::Zero) { [NativeProbe]::StopAnchor($anchor) }
    [void][NativeProbe]::SetCursorPos($originalCursor.X, $originalCursor.Y)
    [void][NativeProbe]::SetForegroundWindow($originalForeground)
    $observations | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $evidence 'smoke.json')
}
Write-Host "Native smoke passed. Evidence: $evidence"
