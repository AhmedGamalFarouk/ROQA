param([Parameter(Mandatory = $true)][string]$ExpectedSha256, [string]$EvidenceDirectory = 'artifacts/m3-topology-1/native-1')
# Passive per-monitor placement check: launches the release build once, reads every ROQA surface
# against its own monitor, then quits gracefully. No input injection, cursor or foreground changes.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Add-Type -AssemblyName System.Drawing
Add-Type -Path "$projectRoot/tests/NativeProbe.cs" -ReferencedAssemblies System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class TopologyProbe {
    delegate bool WindowVisitor(IntPtr window, IntPtr data);
    delegate bool MonitorVisitor(IntPtr monitor, IntPtr dc, IntPtr rect, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(WindowVisitor visit, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, MonitorVisitor visit, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder name, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfoEx info);
    [StructLayout(LayoutKind.Sequential)] public struct Rect {
        public int Left, Top, Right, Bottom;
        public int Width { get { return Right - Left; } }
        public int Height { get { return Bottom - Top; } }
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct MonitorInfoEx {
        public int Size; public Rect Monitor; public Rect Work; public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string Device;
    }
    public static MonitorInfoEx Info(IntPtr monitor) {
        MonitorInfoEx info = new MonitorInfoEx(); info.Size = Marshal.SizeOf(typeof(MonitorInfoEx));
        if (!GetMonitorInfo(monitor, ref info)) throw new InvalidOperationException("Cannot inspect monitor.");
        return info;
    }
    public static List<MonitorInfoEx> Monitors() {
        List<IntPtr> handles = new List<IntPtr>();
        if (!EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (m, dc, r, d) => { handles.Add(m); return handles.Count < 16; }, IntPtr.Zero) && handles.Count == 0)
            throw new InvalidOperationException("Cannot enumerate monitors.");
        return handles.ConvertAll(Info);
    }
    public static string MonitorOf(IntPtr window) { return Info(MonitorFromWindow(window, 2)).Device; }
    public static List<IntPtr> Surfaces(uint pid) {
        List<IntPtr> found = new List<IntPtr>();
        EnumWindows((w, d) => {
            System.Text.StringBuilder name = new System.Text.StringBuilder(64);
            uint owner;
            GetWindowThreadProcessId(w, out owner);
            if (GetClassName(w, name, 64) > 0 && name.ToString() == "ROQA.NativeSurface.1" && (pid == 0 || owner == pid) && IsWindowVisible(w)) found.Add(w);
            return found.Count < 256;
        }, IntPtr.Zero);
        return found;
    }
    public static string Title(IntPtr window) {
        System.Text.StringBuilder text = new System.Text.StringBuilder(64);
        GetWindowText(window, text, 64);
        return text.ToString();
    }
}
'@
[NativeProbe]::EnablePhysicalPixels()
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
$record = [ordered]@{ Label = 'M3 topology candidate 1 passive placement check; author verification, not approval';
    Utc = [DateTime]::UtcNow.ToString('o'); BinarySha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash;
    Monitors = @(); Surfaces = @(); Checks = @(); Resources = $null; Quit = $null; Result = 'FAIL' }
function Assert-Check([bool]$Condition, [string]$Name) {
    if (!$Condition) { throw "FAIL: $Name" }
    $record.Checks += $Name
}
function Near([int]$Actual, [int]$Expected) { return [Math]::Abs($Actual - $Expected) -le 1 }
function Px([int]$Dips, [uint32]$Dpi) { return [int][Math]::Floor(($Dips * $Dpi + 48) / 96) }

$hostProcess = $null
try {
    Assert-Check ($record.BinarySha256 -eq $ExpectedSha256.ToUpperInvariant()) "Binary hash is the reviewed candidate $ExpectedSha256"
    Assert-Check (@(Get-Process -Name roqa -ErrorAction SilentlyContinue).Count -eq 0) 'No pre-existing ROQA process'
    Assert-Check ([TopologyProbe]::Surfaces(0).Count -eq 0) 'No pre-existing ROQA surfaces'
    $monitors = [TopologyProbe]::Monitors()
    foreach ($m in $monitors) {
        $record.Monitors += [ordered]@{ Device = $m.Device; Primary = (($m.Flags -band 1) -ne 0);
            Monitor = "$($m.Monitor.Left),$($m.Monitor.Top),$($m.Monitor.Width)x$($m.Monitor.Height)";
            Work = "$($m.Work.Left),$($m.Work.Top),$($m.Work.Width)x$($m.Work.Height)" }
    }
    $foreground = [NativeProbe]::GetForegroundWindow()
    Assert-Check ($foreground -ne [IntPtr]::Zero) 'Existing foreground window recorded before launch'
    $hostProcess = Start-Process -FilePath $executable -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $evidence 'runtime.log')
    $expected = 3 * $monitors.Count
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $surfaces = [TopologyProbe]::Surfaces([uint32]$hostProcess.Id)
        if ($surfaces.Count -ge $expected) { break }
        if ($hostProcess.HasExited) { throw "Host exited with $($hostProcess.ExitCode) before placing surfaces." }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    Start-Sleep -Milliseconds 400
    $surfaces = [TopologyProbe]::Surfaces([uint32]$hostProcess.Id)
    Assert-Check ($surfaces.Count -eq $expected) "Exactly one notch, shadow and dock per monitor ($($surfaces.Count) of $expected)"
    Assert-Check ([NativeProbe]::GetForegroundWindow() -eq $foreground) 'Launch does not change the foreground window'
    foreach ($m in $monitors) {
        $mine = @($surfaces | Where-Object { [TopologyProbe]::MonitorOf($_) -eq $m.Device })
        $work = $m.Work
        $byTitle = @{}
        foreach ($window in $mine) {
            $rect = New-Object NativeProbe+Rect
            [void][NativeProbe]::GetWindowRect($window, [ref]$rect)
            $title = [TopologyProbe]::Title($window)
            $byTitle[$title] = $rect
            $dpi = [NativeProbe]::GetDpiForWindow($window)
            $record.Surfaces += [ordered]@{ Monitor = $m.Device; Title = $title; Dpi = $dpi; Rect = "$($rect.Left),$($rect.Top),$($rect.Width)x$($rect.Height)" }
            if ($title -ne 'ROQA Notch Shadow') {
                $name = ($title -replace 'ROQA ', '').ToLowerInvariant()
                $record.Surfaces[-1].CenterPixel = [NativeProbe]::Capture($window, (Join-Path $evidence "$($m.Device -replace '[\\.]', '')-$name.png"))
            }
        }
        Assert-Check ($mine.Count -eq 3 -and $byTitle.Count -eq 3) "$($m.Device) owns one notch, shadow and dock"
        $dpi = [NativeProbe]::GetDpiForWindow($mine[0])
        $notch = $byTitle['ROQA Notch']; $dock = $byTitle['ROQA Dock']; $shadow = $byTitle['ROQA Notch Shadow']
        foreach ($pair in @(@('notch', $notch), @('dock', $dock))) {
            $r = $pair[1]
            Assert-Check ($r.Left -ge $work.Left -and $r.Right -le $work.Right -and $r.Top -ge $work.Top -and $r.Bottom -le $work.Bottom) "$($m.Device) $($pair[0]) stays inside its work area"
            Assert-Check (Near ($r.Left - $work.Left) ([int][Math]::Floor(($work.Width - $r.Width) / 2))) "$($m.Device) $($pair[0]) is centred on its monitor"
        }
        # Text factor 1.0 was the last recorded setting; a mismatch here would expose a changed factor.
        Assert-Check ($notch.Width -eq (Px 440 $dpi) -and $notch.Height -eq (Px 164 $dpi) -and $notch.Top -eq $work.Top) "$($m.Device) notch is flush with the top at $dpi DPI"
        Assert-Check ($dock.Height -eq (Px 64 $dpi) -and $dock.Bottom -eq ($work.Bottom - (Px 12 $dpi))) "$($m.Device) dock sits above the work-area bottom at $dpi DPI"
        Assert-Check ($shadow.Left -eq $notch.Left -and $shadow.Top -eq $notch.Top -and $shadow.Width -eq $notch.Width -and $shadow.Height -eq $notch.Height) "$($m.Device) shadow follows its notch"
    }
    $hostProcess.Refresh()
    $record.Resources = [ordered]@{ PrivateBytes = $hostProcess.PrivateMemorySize64; Handles = $hostProcess.HandleCount;
        Gdi = [NativeProbe]::GetGuiResources($hostProcess.Handle, 0); User = [NativeProbe]::GetGuiResources($hostProcess.Handle, 1) }
    $started = [DateTime]::UtcNow
    $client = Start-Process -FilePath $executable -ArgumentList '--quit' -PassThru -WindowStyle Hidden
    if (!$client.WaitForExit(5000)) { $client.Kill(); throw 'Quit client hung.' }
    $clientExit = $client.ExitCode; $client.Dispose()
    $exited = $hostProcess.WaitForExit(5000)
    $record.Quit = [ordered]@{ ClientExit = $clientExit; HostExited = $exited; HostExit = $(if ($exited) { $hostProcess.ExitCode } else { $null });
        Milliseconds = [int]([DateTime]::UtcNow - $started).TotalMilliseconds }
    Assert-Check ($clientExit -eq 0 -and $exited -and $hostProcess.ExitCode -eq 0) 'Graceful --quit exits the host with code 0'
    Assert-Check ([TopologyProbe]::Surfaces(0).Count -eq 0) 'No ROQA surface remains after quit'
    Assert-Check ([NativeProbe]::GetForegroundWindow() -eq $foreground) 'Foreground unchanged after quit'
    $record.Result = 'PASS'
}
catch { $record.Failure = "$_"; throw }
finally {
    if ($null -ne $hostProcess) {
        if (!$hostProcess.HasExited) {
            try { & $executable --quit } catch { Write-Warning $_ }
            if (!$hostProcess.WaitForExit(3000)) { $hostProcess.Kill(); $record.ForcedCleanup = 'Forced cleanup of the test-owned process; not a pass.' }
        }
        $hostProcess.Dispose()
    }
    $record | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 (Join-Path $evidence 'topology.json')
}
