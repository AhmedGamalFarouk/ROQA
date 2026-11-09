param([string]$EvidenceDirectory = 'artifacts/m2-packaged-hung')
# Evidence only (no input injection): a hung real window must not stall the catalog, and two packaged apps
# hosted by the same ApplicationFrameHost must get separate dock groups (AUMID) with real icons.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes
Add-Type -Path "$projectRoot/tests/NativeProbe.cs" -ReferencedAssemblies System.Drawing
Add-Type -Namespace RoqaCheck -Name Hung -MemberDefinition '[DllImport("user32.dll")] public static extern bool IsHungAppWindow(System.IntPtr window); [DllImport("user32.dll")] public static extern bool IsWindow(System.IntPtr window); [DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr window, uint message, System.IntPtr w, System.IntPtr l);'
[NativeProbe]::EnablePhysicalPixels()
$evidence = [IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
$probe = Join-Path $projectRoot 'target/release/roqa-uia-probe.exe'
$result = [ordered]@{ Status = 'RUNNING'; Utc = [DateTime]::UtcNow.ToString('o'); BinarySha256 = (Get-FileHash $executable -Algorithm SHA256).Hash; Checks = @(); FrameTitles = @(); DockButtons = @(); IconSpread = @(); QuitMilliseconds = $null; Limits = 'One machine, one run. Hung = IsHungAppWindow on a WinForms window whose UI thread sleeps; elevated targets untested. Packaged apps: Calculator and Settings only. Icon presence is a pixel-spread heuristic plus the saved capture, not identity proof.' }
$resident = $null; $hung = $null
$before = @(Get-Process CalculatorApp, SystemSettings -ErrorAction SilentlyContinue | ForEach-Object Id)
function Check([bool]$Condition, [string]$Name) { if (!$Condition) { throw "FAIL: $Name" }; $result.Checks += $Name }
function Wait-Until([scriptblock]$Predicate, [string]$Name, [int]$Seconds = 15) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ($null -ne $resident -and $resident.HasExited) { throw "FAIL: host exited $($resident.ExitCode) during $Name" }
        if (& $Predicate) { Check $true $Name; return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "FAIL: Timed out: $Name"
}
function Dock { [NativeProbe]::FindWindow('ROQA.NativeSurface.1', 'ROQA Dock') }
function Dock-Buttons {
    $json = & $probe buttons (Dock).ToInt64()
    if ($LASTEXITCODE -ne 0) { throw 'Native UIA dock query failed.' }
    $buttons = ConvertFrom-Json -InputObject $json
    return ,$buttons
}
function Frame-Titles {
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'ApplicationFrameWindow')
    $frames = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
    # Callers wrap the result in @(); a leading comma here would make that a one-element array.
    return @($frames | ForEach-Object { $_.Current.Name } | Where-Object { $_ })
}
function Has-Button([string]$Title) { @((Dock-Buttons) | Where-Object { $_.name -eq $Title -or $_.name -like "$Title, *" }).Count -ge 1 }
try {
    Check ((Dock) -eq [IntPtr]::Zero) 'No pre-existing resident surface'

    # 1. A real hung top-level window (kernel32 Sleep: CLR STA waits such as Start-Sleep pump messages) exists before ROQA's first full scan.
    $hungScript = "Add-Type -AssemblyName System.Windows.Forms; `$f = New-Object System.Windows.Forms.Form; `$f.Text = 'ROQA hung fixture'; `$f.Show(); [System.Windows.Forms.Application]::DoEvents(); Add-Type -Name K -Namespace H -MemberDefinition '[DllImport(""kernel32.dll"")] public static extern void Sleep(uint ms);'; [H.K]::Sleep(180000)"
    # EncodedCommand: Start-Process does not quote array arguments in Windows PowerShell 5.1.
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($hungScript))
    $hung = Start-Process powershell.exe -ArgumentList '-NoProfile', '-EncodedCommand', $encoded -PassThru
    $hungWindow = [IntPtr]::Zero
    Wait-Until { $script:hungWindow = [NativeProbe]::FindWindow([NullString]::Value, 'ROQA hung fixture'); $script:hungWindow -ne [IntPtr]::Zero } 'Hung fixture window exists'
    # Windows reports hung only while queued messages go unread; WM_NULL is a harmless pending message.
    [void][RoqaCheck.Hung]::PostMessage($hungWindow, 0, [IntPtr]::Zero, [IntPtr]::Zero)
    Wait-Until { [RoqaCheck.Hung]::IsHungAppWindow($hungWindow) } 'Windows reports the fixture as hung' 20

    $resident = Start-Process $executable -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $evidence 'runtime.log')
    [void]$resident.Handle
    Wait-Until { (Dock) -ne [IntPtr]::Zero } 'Dock appears with a hung window present'
    Wait-Until { Has-Button 'ROQA hung fixture' } 'Catalog publishes the hung window as a dock group'

    # 2. Two packaged apps launched after the hung window: publication continues and they group separately.
    Start-Process 'calculator:'
    Start-Process 'ms-settings:about'
    Wait-Until { @(Frame-Titles).Count -ge 2 } 'At least two ApplicationFrameHost windows exist' 30
    Start-Sleep -Seconds 2 # let titles settle after app startup
    $titles = @(Frame-Titles)
    $result.FrameTitles = $titles
    foreach ($title in $titles) { Wait-Until { Has-Button $title } "Packaged app '$title' has its own dock group" }
    Check ([RoqaCheck.Hung]::IsWindow($hungWindow) -and [RoqaCheck.Hung]::IsHungAppWindow($hungWindow)) 'Fixture still hung after packaged publication'
    Wait-Until { [NativeProbe]::FindWindow('Ghost', [NullString]::Value) -ne [IntPtr]::Zero } 'Windows created a ghost window for the hung app' 20
    Start-Sleep -Seconds 2 # allow a catalog generation that would include the ghost
    $buttons = Dock-Buttons
    $result.DockButtons = @($buttons | Select-Object name, invoke, rect)
    # The taskbar shows one button for a hung app; Windows' ghost window must not add a second group.
    Check (@($buttons | Where-Object { $_.name -like 'ROQA hung fixture*' }).Count -eq 1) 'Hung app has exactly one dock group (no ghost duplicate)'

    # 3. Icons: capture the dock and measure pixel spread inside each slot's icon square.
    Start-Sleep -Seconds 1
    $dock = Dock
    $rect = New-Object NativeProbe+Rect; [void][NativeProbe]::GetWindowRect($dock, [ref]$rect)
    $dpi = [NativeProbe]::GetDpiForWindow($dock)
    $px = { param([int]$dips) [int][Math]::Floor(($dips * $dpi + 48) / 96) }
    $bitmap = New-Object System.Drawing.Bitmap $rect.Width, $rect.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $evidence 'dock-packaged.png'), [System.Drawing.Imaging.ImageFormat]::Png)
        $icon = & $px 40; $top = [int](($rect.Height - $icon) / 2)
        for ($slot = 0; $slot -lt $buttons.Count; $slot++) {
            $left = (& $px 8) + (& $px 56) * $slot + (& $px 8)
            if ($left + $icon -gt $rect.Width) { break }
            $values = for ($y = $top; $y -lt $top + $icon; $y += 2) { for ($x = $left; $x -lt $left + $icon; $x += 2) { $c = $bitmap.GetPixel($x, $y); 0.299 * $c.R + 0.587 * $c.G + 0.114 * $c.B } }
            $mean = ($values | Measure-Object -Average).Average
            $spread = [Math]::Sqrt((($values | ForEach-Object { ($_ - $mean) * ($_ - $mean) }) | Measure-Object -Average).Average)
            $result.IconSpread += [ordered]@{ Slot = $slot; Name = $buttons[$slot].name; LuminanceStdDev = [Math]::Round($spread, 2) }
        }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    foreach ($title in $titles) {
        $entry = @($result.IconSpread | Where-Object { $_.Name -eq $title -or $_.Name -like "$title, *" }) | Select-Object -First 1
        Check ($null -ne $entry -and $entry.LuminanceStdDev -gt 10) "Packaged app '$title' slot draws a non-blank icon"
    }

    # 4. Quit stays prompt while the hung window exists.
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $client = Start-Process $executable -ArgumentList '--quit' -WindowStyle Hidden -PassThru
    Check ($client.WaitForExit(5000) -and $client.ExitCode -eq 0) 'Control --quit exits zero'
    Check ($resident.WaitForExit(5000)) 'Resident exits within 5 s with a hung window present'
    $result.QuitMilliseconds = $watch.ElapsedMilliseconds
    Check ($resident.ExitCode -eq 0) 'Resident exit code 0'
    $result.Status = 'PASS'
} catch {
    $result.Status = if ("$_" -like 'FAIL:*') { 'FAIL' } else { 'ERROR' }
    $result.Error = "$_"
} finally {
    if ($null -ne $resident -and !$resident.HasExited) { $resident.Kill(); $result.Cleanup = 'Resident force-killed; not a pass.' }
    if ($null -ne $hung -and !$hung.HasExited) { $hung.Kill() }
    # Close only app instances this check started.
    Get-Process CalculatorApp, SystemSettings -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id } | Stop-Process -Force -ErrorAction SilentlyContinue
    $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidence 'packaged-hung.json') -Encoding UTF8
}
Write-Host "Packaged/hung check $($result.Status). Evidence: $evidence"
if ($result.Status -ne 'PASS') { exit 1 }
