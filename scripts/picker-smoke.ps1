param([switch]$SkipBuild, [string]$EvidenceDirectory = 'artifacts/m2b1')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (!$SkipBuild) { & "$PSScriptRoot/cargo.ps1" build --locked --release }
Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes
Add-Type -Path "$projectRoot/tests/NativeProbe.cs" -ReferencedAssemblies System.Drawing
Add-Type -Path "$projectRoot/tests/PickerProbe.cs"
[NativeProbe]::EnablePhysicalPixels()
$evidence = [IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
$result = [ordered]@{ Status = 'RUNNING'; Utc = [DateTime]::UtcNow.ToString('o'); PowerShellPid = $PID; BinarySha256 = (Get-FileHash $executable -Algorithm SHA256).Hash; Checks = @(); Samples = @(); Limits = 'Native standard-control and UIA proof only. Actual Narrator speech, OS text-scale setting changes and IME composition remain untested; an IME context is not composition evidence.' }
$resident = $null; $fixture = [IntPtr]::Zero; $picker = [IntPtr]::Zero
$originalForeground = [NativeProbe]::GetForegroundWindow()
function Check([bool]$Condition, [string]$Name) { if (!$Condition) { throw "FAIL: $Name" }; $result.Checks += $Name }
function Wait-Until([scriptblock]$Predicate, [string]$Name) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if ($null -ne $resident -and $resident.HasExited) { throw "Host exited $($resident.ExitCode) during $Name" }
        if (& $Predicate) { Check $true $Name; return }
        Start-Sleep -Milliseconds 30
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "FAIL: Timed out: $Name"
}
function Signal([string]$Command) {
    $client = Start-Process $executable -ArgumentList $Command -WindowStyle Hidden -PassThru
    try { Check ($client.WaitForExit(5000) -and $client.ExitCode -eq 0) "Control $Command exits zero" } finally { $client.Dispose() }
}
function Assert-ForegroundOrClassify([IntPtr]$Expected, [string]$Name) {
    $foreground = [NativeProbe]::GetForegroundWindow()
    if ($foreground -eq $Expected) { Check $true $Name; return }
    $owner = 0; [void][NativeProbe]::GetWindowThreadProcessId($foreground, [ref]$owner)
    $detail = "foreground HWND=$($foreground.ToInt64()) pid=$owner title='$([PickerProbe]::Text($foreground))' expected=$($Expected.ToInt64()) resident=$($resident.Id)"
    # A ROQA-owned foreground window is a product failure; any other owner is outside interference and never counts as a pass.
    if ($owner -eq $resident.Id) { throw "FAIL: $Name; $detail" }
    throw "INTERFERENCE: $Name; $detail"
}
function Find-Picker { return [NativeProbe]::FindWindow('#32770', 'ROQA Windows') }
function Open-Picker {
    if ([NativeProbe]::GetForegroundWindow() -ne $fixture) {
        if (![NativeProbe]::SetForegroundWindow($fixture)) { throw 'INTERFERENCE: Cannot establish owned foreground fixture.' }
    }
    # Real keyboard route: the harness grants nothing; roqa --show-picker must hand over foreground itself.
    $before = [NativeProbe]::GetForegroundWindow()
    Signal '--show-picker'
    Wait-Until { (Find-Picker) -ne [IntPtr]::Zero } 'Explicit picker open creates native dialog'
    $window = Find-Picker
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ([NativeProbe]::GetForegroundWindow() -ne $window -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 30 }
    # Foreground left on the requesting app means ROQA failed the hand-over: a product failure, not interference.
    if ([NativeProbe]::GetForegroundWindow() -eq $before) { throw 'FAIL: --show-picker opened without foreground; requesting app kept focus.' }
    Assert-ForegroundOrClassify $window 'Explicit picker receives focus through the real --show-picker route'
    return $window
}
function Assert-Foreground {
    if ([NativeProbe]::GetForegroundWindow() -ne $picker) { throw 'INTERFERENCE: Foreground changed before owned keyboard input.' }
}
function Inspect([IntPtr]$Window) {
    $json = & "$projectRoot/target/release/roqa-uia-probe.exe" inspect $Window.ToInt64()
    if ($LASTEXITCODE -ne 0) { throw 'Native UIA inspect failed.' }
    return ($json | ConvertFrom-Json)
}
function Set-Filter([string]$Value) {
    $handle = [PickerProbe]::GetDlgItem($picker,100)
    $answer = & "$projectRoot/target/release/roqa-uia-probe.exe" set $handle.ToInt64() $Value
    if ($LASTEXITCODE -ne 0 -or $answer -ne 'true') { throw 'Native UIA ValuePattern failed.' }
}
function List-Names {
    $list = [PickerProbe]::GetDlgItem($picker,101)
    $json = & "$projectRoot/target/release/roqa-uia-probe.exe" list $list.ToInt64()
    if ($LASTEXITCODE -ne 0) { throw 'Native UIA list query failed.' }
    $names = ConvertFrom-Json -InputObject $json
    return ,$names
}
function Dock-Buttons {
    $json = & "$projectRoot/target/release/roqa-uia-probe.exe" buttons $dock.ToInt64()
    if ($LASTEXITCODE -ne 0) { throw 'Native UIA dock query failed.' }
    $buttons = ConvertFrom-Json -InputObject $json
    return ,$buttons
}
function Click-Slot([int]$Slot) {
    # Real mouse input: the click itself is what lets ROQA take foreground for the target.
    $rect = New-Object NativeProbe+Rect; [void][NativeProbe]::GetWindowRect($dock, [ref]$rect)
    $dpi = [NativeProbe]::GetDpiForWindow($dock)
    $px = { param([int]$dips) [int][Math]::Floor(($dips * $dpi + 48) / 96) }
    [PickerProbe]::Click($rect.Left + (& $px 8) + (& $px 56) * $Slot + (& $px 28), $rect.Top + [int]($rect.Height / 2))
}
function Close-Keyboard {
    Assert-Foreground
    [PickerProbe]::Key(27)
    Wait-Until { (Find-Picker) -eq [IntPtr]::Zero } 'Escape destroys picker without closing resident'
    Check (!$resident.HasExited) 'Resident survives picker dismissal'
}
function Sample([string]$Name) {
    $resident.Refresh()
    return [ordered]@{ Name = $Name; PrivateBytes = $resident.PrivateMemorySize64; Handles = $resident.HandleCount; User = [NativeProbe]::GetGuiResources($resident.Handle,1); Gdi = [NativeProbe]::GetGuiResources($resident.Handle,0) }
}
try {
    Check ([NativeProbe]::Surfaces('ROQA Dock').Count -eq 0) 'No pre-existing resident surface'
    $fixture = [NativeProbe]::StartAnchor()
    $title = 'ROQA fixture — مرحباً بالعالم واختبار العربية'
    Check ([NativeProbe]::SetWindowText($fixture,$title)) 'Real fixture uses mixed Latin and Arabic title'
    if (![NativeProbe]::SetForegroundWindow($fixture)) { throw 'INTERFERENCE: Windows denied initial fixture foreground.' }
    $resident = Start-Process $executable -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $evidence 'runtime.log')
    [void]$resident.Handle
    Wait-Until { [NativeProbe]::PrimarySurface('ROQA Dock') -ne [IntPtr]::Zero } 'Native dock appears'
    Assert-ForegroundOrClassify $fixture 'Passive launch keeps foreground fixture'
    $picker = Open-Picker
    Wait-Until { (List-Names) -contains $title } 'Picker contains actual catalog title through UIA'
    $edit = [PickerProbe]::GetDlgItem($picker,100)
    Check ([PickerProbe]::Focused($picker) -eq $edit) 'Keyboard focus starts in native filter edit'
    $editElement = Inspect $edit
    Check ($editElement.role -eq 50004) 'UIA exposes native edit role'
    Check ($editElement.name -eq 'Filter windows') 'UIA exposes filter label'
    Check ($editElement.focusable) 'UIA filter is keyboard focusable'
    $result.EditFontHeight = [PickerProbe]::FontHeight($edit)
    Set-Filter 'مرحباً'
    Wait-Until { $names = List-Names; $names.Count -eq 1 -and $names[0] -eq $title } 'Arabic filter retains real Arabic title'
    $valueJson = & "$projectRoot/target/release/roqa-uia-probe.exe" value $edit.ToInt64()
    $value = ConvertFrom-Json -InputObject $valueJson
    $result.EditValueCodepoints = @($value.ToCharArray() | ForEach-Object { [int]$_ })
    Check ($value -eq $title.Split(' ')[3]) 'Native edit retains Unicode through UIA ValuePattern'
    [void][NativeProbe]::Capture($picker,(Join-Path $evidence 'arabic-picker.png'))
    Assert-Foreground
    [PickerProbe]::Key(9)
    Wait-Until { [PickerProbe]::Focused($picker) -eq [PickerProbe]::GetDlgItem($picker,101) } 'Tab moves from filter to list'
    [PickerProbe]::Key(13)
    Wait-Until { (Find-Picker) -eq [IntPtr]::Zero } 'Enter activates selected real window and closes picker'
    Wait-Until { [NativeProbe]::GetForegroundWindow() -eq $fixture } 'Selected fixture is actually foreground'
    $picker = Open-Picker
    Set-Filter 'ROQA fixture'
    $renamed = 'ROQA fixture — updated while picker is open'
    [void][NativeProbe]::SetWindowText($fixture,$renamed)
    Wait-Until { (List-Names) -contains $renamed } 'Live title update reaches open picker'
    Check ([NativeProbe]::GetForegroundWindow() -eq $picker) 'Passive catalog update does not steal picker focus'
    Set-Filter 'ROQA no matching window 9DA7C14D'
    Wait-Until { (List-Names).Count -eq 0 } 'No-match filter exposes an empty result'
    $button = Inspect ([PickerProbe]::GetDlgItem($picker,1))
    Check (!$button.enabled) 'Open action is disabled with no results'
    Check ((Inspect ([PickerProbe]::GetDlgItem($picker,102))).name.StartsWith('0 windows.')) 'Empty-result status describes actual state'
    Set-Filter 'ROQA fixture'
    Wait-Until { (List-Names).Count -eq 1 } 'Clearing no-match query restores actual result'
    Check ((Inspect ([PickerProbe]::GetDlgItem($picker,1))).enabled) 'Open action re-enables when a result exists'
    Close-Keyboard
    Wait-Until { [NativeProbe]::GetForegroundWindow() -eq $fixture } 'Escape returns focus to prior real app'
    foreach ($cycle in 1..5) { $picker = Open-Picker; Close-Keyboard }
    $baseline = Sample 'warm'; $result.Samples += $baseline
    foreach ($block in 1..3) {
        foreach ($cycle in 1..10) { $picker = Open-Picker; Close-Keyboard }
        $sample = Sample "block-$block"; $result.Samples += $sample
        Check (($sample.PrivateBytes-$baseline.PrivateBytes) -le 8MB) "Block $block picker memory bounded"
        Check (($sample.Handles-$baseline.Handles) -le 5 -and ($sample.User-$baseline.User) -le 3 -and ($sample.Gdi-$baseline.Gdi) -le 3) "Block $block picker handle USER GDI counts bounded"
    }
    # Dock: the real fixture group is a named UIA button, and clicks behave like the taskbar.
    $dock = [NativeProbe]::PrimarySurface('ROQA Dock')
    Wait-Until { @((Dock-Buttons) | Where-Object { $_.name -like "$renamed*" }).Count -eq 1 } 'Dock exposes the real fixture group as a named UIA button'
    $buttons = Dock-Buttons
    Check ($buttons[0].name -eq 'All windows' -and $buttons[0].invoke) 'Dock slot 0 is the invokable picker button'
    $slot = 0..($buttons.Count - 1) | Where-Object { $buttons[$_].name -like "$renamed*" } | Select-Object -First 1
    Check ($buttons[$slot].invoke) 'Fixture dock button supports Invoke'
    if ([NativeProbe]::GetForegroundWindow() -ne $fixture) { throw 'INTERFERENCE: Fixture lost foreground before dock clicks.' }
    Click-Slot $slot
    Wait-Until { [PickerProbe]::IsIconic($fixture) } 'Clicking the focused lone window minimizes it'
    Click-Slot $slot
    Wait-Until { !([PickerProbe]::IsIconic($fixture)) -and [NativeProbe]::GetForegroundWindow() -eq $fixture } 'Clicking a minimized group restores and activates it'
    Click-Slot 0
    Wait-Until { (Find-Picker) -ne [IntPtr]::Zero } 'Dock picker slot opens the picker'
    $picker = Find-Picker
    Wait-Until { [NativeProbe]::GetForegroundWindow() -eq $picker } 'Dock click gives the picker focus'
    Close-Keyboard
    $picker = Open-Picker
    Signal '--quit'
    Check ($resident.WaitForExit(5000) -and $resident.ExitCode -eq 0) 'Quit with picker open releases resident cleanly'
    Check ((Find-Picker) -eq [IntPtr]::Zero) 'Picker HWND is gone after graceful quit'
    $result.Status = 'PASS'
} catch {
    $result.Status = if ($_.Exception.Message.StartsWith('INTERFERENCE:')) { 'INCONCLUSIVE' } else { 'FAIL' }
    $result.Failure = $_.Exception.Message
    try {
        if ($picker -ne [IntPtr]::Zero -and (Find-Picker) -ne [IntPtr]::Zero) {
            [void][NativeProbe]::Capture($picker,(Join-Path $evidence 'failure-picker.png'))
            foreach ($id in @(100,101,1)) {
                $handle = [PickerProbe]::GetDlgItem($picker,$id)
                & "$projectRoot/target/release/roqa-uia-probe.exe" inspect $handle.ToInt64() | Set-Content (Join-Path $evidence "native-uia-$id.json") -Encoding UTF8
            }
            $handle = [PickerProbe]::GetDlgItem($picker,101)
            & "$projectRoot/target/release/roqa-uia-probe.exe" list $handle.ToInt64() | Set-Content (Join-Path $evidence 'native-uia-list.json') -Encoding UTF8
        }
    } catch { $result.FailureEvidenceError = $_.Exception.Message }
    throw
} finally {
    if ($null -ne $resident) {
        if (!$resident.HasExited) { try { Signal '--quit' } catch { Write-Warning $_ }; if (!$resident.WaitForExit(5000)) { $resident.Kill(); Write-Warning 'Forced owned-host cleanup; not a test pass.' } }
        $resident.Dispose()
    }
    if ($fixture -ne [IntPtr]::Zero) { [NativeProbe]::StopAnchor($fixture) }
    [void][NativeProbe]::SetForegroundWindow($originalForeground)
    $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence 'picker-smoke.json') -Encoding UTF8
}
Write-Host "Picker smoke passed. Evidence: $evidence"
