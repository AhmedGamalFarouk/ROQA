# -Executable measures another build (for example a saved baseline) instead of target/release.
# -SampleSeconds lengthens the sample (for example 900 for a 15-minute run); default 30 keeps the M1 sample length.
param([switch]$SkipBuild, [string]$EvidenceDirectory = 'artifacts/m1', [string]$Executable = '', [ValidateRange(1, 86400)][int]$SampleSeconds = 30)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (!$SkipBuild) { & "$PSScriptRoot/cargo.ps1" build --locked --release }
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$executable = if ($Executable) { (Resolve-Path -LiteralPath $Executable).Path } else { Join-Path $projectRoot 'target/release/roqa.exe' }
Add-Type -Namespace RoqaIdle -Name Native -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError = true)] static extern bool QueryProcessCycleTime(System.IntPtr process, out ulong cycles);
[DllImport("user32.dll", SetLastError = true)] public static extern uint GetGuiResources(System.IntPtr process, uint flags);
[StructLayout(LayoutKind.Sequential)] struct LastInputInfo { public uint Size; public uint Time; }
[DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LastInputInfo info);
[StructLayout(LayoutKind.Sequential)] struct PowerStatus { public byte AcLine, BatteryFlag, BatteryPercent, SystemFlag; public uint LifeTime, FullLifeTime; }
[DllImport("kernel32.dll", SetLastError = true)] static extern bool GetSystemPowerStatus(out PowerStatus status);
public static ulong Cycles(System.IntPtr process) {
    ulong cycles;
    if (!QueryProcessCycleTime(process, out cycles)) throw new System.ComponentModel.Win32Exception();
    return cycles;
}
// 0 means failure or no objects; every windowed ROQA build owns USER objects, so 0 is treated as failure.
// GDI is recorded as returned: M1 to M2a builds legitimately owned 0 GDI objects.
public static uint UserObjects(System.IntPtr process) {
    uint count = GetGuiResources(process, 1);
    if (count == 0) throw new System.ComponentModel.Win32Exception();
    return count;
}
public static uint LastInputTick() {
    LastInputInfo info = new LastInputInfo();
    info.Size = (uint)Marshal.SizeOf(info);
    if (!GetLastInputInfo(ref info)) throw new System.InvalidOperationException("GetLastInputInfo failed.");
    return info.Time;
}
public static uint IdleMilliseconds() { return unchecked((uint)System.Environment.TickCount - LastInputTick()); }
public static bool OnBattery() {
    PowerStatus status;
    if (!GetSystemPowerStatus(out status)) throw new System.ComponentModel.Win32Exception();
    return status.AcLine == 0;
}
'@
$machine = [ordered]@{
    Os = (Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber)
    Cpu = (Get-CimInstance Win32_Processor | Select-Object Name, NumberOfLogicalProcessors, MaxClockSpeed)
    Gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate)
}
$nominalMegahertz = [double](@($machine.Cpu)[0].MaxClockSpeed)
if ($nominalMegahertz -le 0) { throw 'Win32_Processor.MaxClockSpeed is missing; cannot label cycles.' }
# The display must not turn off and the screen saver must not start inside the run: either would change what is measured.
$videoIdle = [regex]::Matches(((powercfg /q SCHEME_CURRENT SUB_VIDEO VIDEOIDLE) -join "`n"), '0x[0-9a-fA-F]{8}')
if ($LASTEXITCODE -ne 0 -or $videoIdle.Count -lt 2) { throw 'Could not read the display-off timeout from powercfg.' }
$onBattery = [RoqaIdle.Native]::OnBattery()
$desktopKey = Get-Item 'HKCU:\Control Panel\Desktop'
$screenSaverSeconds = if ($desktopKey.GetValue('SCRNSAVE.EXE')) { [int]$desktopKey.GetValue('ScreenSaveTimeOut', 0) } else { 0 }
$machine.IdleTimeouts = [ordered]@{
    OnBattery = $onBattery
    # powercfg lists the AC index before the DC index, after the setting's range; 0 means never.
    DisplayOffSeconds = [Convert]::ToUInt32($videoIdle[$videoIdle.Count - $(if ($onBattery) { 1 } else { 2 })].Value, 16)
    ScreenSaverSeconds = $screenSaverSeconds
}
$idleLimitSeconds = ($machine.IdleTimeouts.DisplayOffSeconds, $screenSaverSeconds | Where-Object { $_ -gt 0 } | Measure-Object -Minimum).Minimum
# shortcut: 30 s covers two WMI reads (about 5 s each when measured); the post-run check below catches a slower machine.
if ($idleLimitSeconds -and [RoqaIdle.Native]::IdleMilliseconds() / 1000 + 5 + $SampleSeconds + 30 -ge $idleLimitSeconds) {
    throw "STOP: an idle timeout of $idleLimitSeconds s (display off or screen saver) would fall inside the run; shorten -SampleSeconds or start sooner after input."
}
# Wake proxy: raw ContextSwitchesPersec is each live thread's cumulative switch count. One WMI read takes seconds,
# so reads sit outside the CPU window; each is timed by its midpoint because the snapshot instant inside it is unknown.
function Read-ContextSwitches([int]$ProcessId) {
    $begin = [System.Diagnostics.Stopwatch]::GetTimestamp()
    $rows = @(Get-CimInstance Win32_PerfRawData_PerfProc_Thread -Filter "IDProcess = $ProcessId" -Property IDThread, ContextSwitchesPersec)
    $end = [System.Diagnostics.Stopwatch]::GetTimestamp()
    # A live ROQA always has threads, so no rows means the counter source failed, not zero wakes.
    if (!$rows.Count) { throw "WMI returned no threads for process $ProcessId; performance counters may be disabled." }
    $counts = @{}
    foreach ($row in $rows) { $counts[[long]$row.IDThread] = [long]$row.ContextSwitchesPersec }
    return @{ Counts = $counts; Midpoint = ($begin + $end) / 2; ReadSeconds = ($end - $begin) / [System.Diagnostics.Stopwatch]::Frequency }
}
# Threads alive at both reads add their difference; threads born between reads add their whole count; exited threads are only counted.
function Measure-SwitchDelta([hashtable]$Start, [hashtable]$End) {
    $switches = [long]0
    foreach ($id in $End.Keys) {
        $before = $Start[$id]
        # A smaller count under the same ID means the ID was reused by a new thread.
        $switches += if ($null -ne $before -and $End[$id] -ge $before) { $End[$id] - $before } else { $End[$id] }
    }
    return @{ Switches = $switches; Exited = @($Start.Keys | Where-Object { !$End.ContainsKey($_) }).Count; Born = @($End.Keys | Where-Object { !$Start.ContainsKey($_) }).Count }
}
$inputBeforeLaunch = [RoqaIdle.Native]::LastInputTick()
$idleSecondsAtLaunch = [RoqaIdle.Native]::IdleMilliseconds() / 1000
$launchClock = [System.Diagnostics.Stopwatch]::StartNew()
$measuredHost = Start-Process -FilePath $executable -PassThru -WindowStyle Hidden
try {
    if ($measuredHost.WaitForExit(5000)) { throw 'ROQA exited during the five-second settle period.' }
    $switchesStart = Read-ContextSwitches $measuredHost.Id
    $measuredHost.Refresh()
    $startCpu = $measuredHost.TotalProcessorTime.TotalMilliseconds
    $startPrivate = $measuredHost.PrivateMemorySize64
    $startHandles = $measuredHost.HandleCount
    $startGdi = [RoqaIdle.Native]::GetGuiResources($measuredHost.Handle, 0)
    $startUser = [RoqaIdle.Native]::UserObjects($measuredHost.Handle)
    $startCycles = [RoqaIdle.Native]::Cycles($measuredHost.Handle)
    $sampleClock = [System.Diagnostics.Stopwatch]::StartNew()
    $settleSeconds = $launchClock.Elapsed.TotalSeconds
    # Polls the sampler (not ROQA) once a second so user input stops a long run early.
    while (($remaining = $SampleSeconds * 1000 - $sampleClock.ElapsedMilliseconds) -gt 0) {
        if ($measuredHost.WaitForExit([int][Math]::Min(1000, $remaining))) { throw "ROQA exited during the $SampleSeconds-second sample." }
        if ([RoqaIdle.Native]::LastInputTick() -ne $inputBeforeLaunch) { throw 'STOP: user input arrived during the settle or sample; nothing was recorded.' }
    }
    $endCycles = [RoqaIdle.Native]::Cycles($measuredHost.Handle)
    $sampleClock.Stop()
    $measuredHost.Refresh()
    $cpuMilliseconds = $measuredHost.TotalProcessorTime.TotalMilliseconds - $startCpu
    $endPrivate = $measuredHost.PrivateMemorySize64
    $endHandles = $measuredHost.HandleCount
    $endGdi = [RoqaIdle.Native]::GetGuiResources($measuredHost.Handle, 0)
    $endUser = [RoqaIdle.Native]::UserObjects($measuredHost.Handle)
    $switchesEnd = Read-ContextSwitches $measuredHost.Id
    if ([RoqaIdle.Native]::LastInputTick() -ne $inputBeforeLaunch) { throw 'STOP: user input arrived during the final reads; nothing was recorded.' }
    if ($idleLimitSeconds -and $idleSecondsAtLaunch + $launchClock.Elapsed.TotalSeconds -ge $idleLimitSeconds) { throw "STOP: the $idleLimitSeconds s idle timeout passed during the run; nothing was recorded." }
    $seconds = $sampleClock.Elapsed.TotalSeconds
    $cycles = $endCycles - $startCycles
    $wake = Measure-SwitchDelta $switchesStart.Counts $switchesEnd.Counts
    $wakeSeconds = ($switchesEnd.Midpoint - $switchesStart.Midpoint) / [System.Diagnostics.Stopwatch]::Frequency
    $observation = [ordered]@{
        Label = 'Idle observation only; no product-budget or competitor verdict'
        Utc = [DateTime]::UtcNow.ToString('o')
        BinarySha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash
        Method = "Release build; five-second settle plus one WMI read (SettleSeconds, about 10 s; M1 used 5 s); $SampleSeconds-second wait; System.Diagnostics.Process start/end counters; QueryProcessCycleTime start/end; GetGuiResources start/end; per-thread WMI context switches read just outside the CPU window; refuses to run into the display-off or screen-saver timeout; stops on any user input (GetLastInputInfo) from launch to the final read"
        Machine = $machine
        DurationSeconds = $seconds
        CpuMilliseconds = $cpuMilliseconds
        CpuMillisecondsPerSecond = $cpuMilliseconds / $seconds
        PrivateBytesStart = $startPrivate
        PrivateBytesEnd = $endPrivate
        HandlesStart = $startHandles
        HandlesEnd = $endHandles
        SettleSeconds = $settleSeconds
        IdleSecondsAtLaunch = $idleSecondsAtLaunch
        CpuCycles = $cycles
        CpuCyclesPerSecond = $cycles / $seconds
        NominalClockMegahertz = $nominalMegahertz
        ApproxCpuMillisecondsPerSecondFromCycles = $cycles / ($nominalMegahertz * 1000) / $seconds
        GdiObjectsStart = $startGdi
        GdiObjectsEnd = $endGdi
        UserObjectsStart = $startUser
        UserObjectsEnd = $endUser
        WakeProxyContextSwitches = $wake.Switches
        WakeProxySeconds = $wakeSeconds
        WakeProxyContextSwitchesPerSecond = $wake.Switches / $wakeSeconds
        WakeProxyReadSeconds = @($switchesStart.ReadSeconds, $switchesEnd.ReadSeconds)
        ThreadsStart = $switchesStart.Counts.Count
        ThreadsEnd = $switchesEnd.Counts.Count
        ThreadsExited = $wake.Exited
        ThreadsBorn = $wake.Born
        Limits = 'CpuMilliseconds moves in scheduler ticks (15.625 ms by default), so it is coarse at idle. CpuCycles has no tick quantization; ApproxCpuMillisecondsPerSecondFromCycles assumes cycles tick at NominalClockMegahertz (WMI MaxClockSpeed), which is unverified; the QueryThreadCycleTime docs advise against converting cycles to time, so it is approximate; compare builds by CpuCyclesPerSecond. WakeProxyContextSwitches is a wake proxy, not a wake count: it is matched per thread ID, threads that exit between reads lose their in-window switches (so it can understate; see ThreadsExited, and a reused thread ID can hide an exit and understate further), and its window is WakeProxySeconds give or take half the sum of WakeProxyReadSeconds because the snapshot instant inside each WMI read is unknown. SettleSeconds is longer than the M1 five seconds. Group-policy screen-saver and lock timeouts and the system sleep timeout are not read; a screen saver without ScreenSaveTimeOut is treated as never. Two endpoint queries per counter; sampler outside target; DWM/GPU/power/shared allocations not attributed; other desktop applications remain running.'
    }
    $observation | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $evidence 'idle.json')
}
finally {
    if (!$measuredHost.HasExited) {
        $quit = Start-Process -FilePath $executable -ArgumentList '--quit' -WindowStyle Hidden -PassThru
        if (!$quit.WaitForExit(5000) -or $quit.ExitCode -ne 0) { Write-Warning 'Idle sample quit signaling failed.' }
        $quit.Dispose()
        if (!$measuredHost.WaitForExit(5000)) { $measuredHost.Kill(); throw 'Forced cleanup after idle sample; graceful shutdown failed.' }
        if ($measuredHost.ExitCode -ne 0) { throw "Measured host exited with $($measuredHost.ExitCode)." }
    }
    $measuredHost.Dispose()
}
Write-Host "Idle observation saved to $evidence/idle.json."
