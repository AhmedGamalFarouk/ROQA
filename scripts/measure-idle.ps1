# Scenario runner; one invocation is one repetition and needs its own fresh user confirmation. See docs/development/m5-harness.md.
# -Executable measures another build (for example a saved baseline) instead of target/release; -Executable never triggers a build.
# -Competitor <name> measures a competitor build: it needs -Executable and -ExpectedSha256 (the pinned SHA-256) or it fails before launching.
# -Scenario: idle (default), start-warm, start-cold (counters from launch), churn and media (a person drives the stimulus; input abort is off, -Note is required).
# -SampleSeconds lengthens the sample (for example 900 for the 15-minute static gate); default 30 keeps the M1 sample length.
# While sampling the harness holds SetThreadExecutionState(ES_CONTINUOUS|ES_DISPLAY_REQUIRED|ES_SYSTEM_REQUIRED), cleared in finally, so a sample may outlast the display-off timeout (power settings are never changed); if the call fails the old timeout refusal applies.
# The idle scenario passes --diagnostics to ROQA and records the counters' start and end values and delta (paints, wakes). A user-input abort writes <scenario>-<build>-r<N>.aborted.json (Status ABORTED, elapsed seconds) instead of nothing.
# -DryRun validates arguments and prints the plan without launching, building or reading counters.
param(
    [switch]$SkipBuild, [switch]$DryRun, [string]$EvidenceDirectory = 'artifacts/m1', [string]$Executable = '',
    [ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedSha256 = '', [string]$Competitor = '',
    [ValidateSet('idle', 'start-warm', 'start-cold', 'churn', 'media')][string]$Scenario = 'idle',
    [string]$Note = '', [ValidateRange(1, 99)][int]$Repetition = 1, [ValidateRange(1, 86400)][int]$SampleSeconds = 30
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
. "$PSScriptRoot/verdict.ps1"
if ($Competitor -and ($Competitor -notmatch '^[A-Za-z0-9._-]+$' -or !$Executable -or !$ExpectedSha256)) { throw 'Competitor mode needs a file-name-safe -Competitor plus both -Executable (a pinned path) and -ExpectedSha256; nothing was launched.' }
if ($Scenario -in 'churn', 'media', 'start-cold' -and !$Note) { throw "-Scenario $Scenario needs -Note (the stimulus performed, or the attested cold-start state)." }
$isStart = $Scenario -like 'start-*'
$inputGuard = $Scenario -notin 'churn', 'media'
$label = if ($Competitor) { $Competitor } else { 'roqa' }
if (!$SkipBuild -and !$DryRun -and !$Executable) { & "$PSScriptRoot/cargo.ps1" build --locked --release }
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
$executable = if ($Executable) { (Resolve-Path -LiteralPath $Executable).Path } elseif ($DryRun) { Join-Path $projectRoot 'target/release/roqa.exe' } else { (Resolve-Path -LiteralPath (Join-Path $projectRoot 'target/release/roqa.exe')).Path }
$binarySha = if (Test-Path -LiteralPath $executable) { (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash } else { $null }
if ($ExpectedSha256 -and $binarySha -ne $ExpectedSha256) { throw "Pinned SHA-256 mismatch for $executable (got $binarySha); nothing was launched." }
$outputPath = Join-Path $evidence "$Scenario-$label-r$Repetition.json"
$abortedPath = [System.IO.Path]::ChangeExtension($outputPath, 'aborted.json')
# Only ROQA's idle scenario reads its diagnostics report; a competitor has none.
$diagnosticsPath = if ($Scenario -eq 'idle' -and !$Competitor) { [System.IO.Path]::ChangeExtension($outputPath, 'diagnostics.json') } else { '' }
# A second instance of the same executable would be counted as a descendant or make a single-instance build exit.
$running = @(Get-Process -Name ([System.IO.Path]::GetFileNameWithoutExtension($executable)) -ErrorAction SilentlyContinue)
if ($running.Count) { if ($DryRun) { Write-Warning "$($running.Count) process(es) with this name are already running." } else { throw "$($running.Count) process(es) with this name are already running; stop them first." } }
if ($DryRun) {
    [ordered]@{ DryRun = $true; Scenario = $Scenario; Build = $label; Executable = $executable; BinarySha256 = $binarySha; SampleSeconds = $SampleSeconds; InputGuard = $inputGuard; KeepAwake = $true; Diagnostics = $diagnosticsPath; Output = $outputPath; AbortedOutput = $abortedPath } | ConvertTo-Json
    return
}
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
Add-Type -AssemblyName System.Windows.Forms
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
[DllImport("kernel32.dll")] static extern uint SetThreadExecutionState(uint flags);
// ES_CONTINUOUS (0x80000000) with ES_SYSTEM_REQUIRED (1) and ES_DISPLAY_REQUIRED (2) holds until cleared with ES_CONTINUOUS alone; both are per calling thread.
public static bool KeepAwake(bool hold) { return SetThreadExecutionState(hold ? 0x80000003u : 0x80000000u) != 0; }
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
    MonitorCount = [System.Windows.Forms.Screen]::AllScreens.Count
    # shortcut: AppliedDPI is the system scale at sign-in; per-monitor scales are not read, record them by hand.
    AppliedDpi = (Get-Item 'HKCU:\Control Panel\Desktop\WindowMetrics').GetValue('AppliedDPI', $null)
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
# Held for the whole sample and cleared in the final finally; while it is held the display-off and screen-saver timeouts cannot fall inside the run.
$held = [RoqaIdle.Native]::KeepAwake($true)
$idleLimitSeconds = ($machine.IdleTimeouts.DisplayOffSeconds, $screenSaverSeconds | Where-Object { $_ -gt 0 } | Measure-Object -Minimum).Minimum
# shortcut: 30 s covers two WMI reads (about 5 s each when measured); the post-run check below catches a slower machine.
if (!$held -and $idleLimitSeconds -and [RoqaIdle.Native]::IdleMilliseconds() / 1000 + 5 + $SampleSeconds + 30 -ge $idleLimitSeconds) {
    throw "STOP: an idle timeout of $idleLimitSeconds s (display off or screen saver) would fall inside the run; shorten -SampleSeconds or start sooner after input."
}
# The measured scope is the launched process plus its descendants (WebView2, helpers), found by one Win32_Process read.
# shortcut: a grandchild whose parent already exited is missed and a reused PID can mislink; both are rare for a launcher tree.
function Get-TreeIds([int]$RootId) {
    $rows = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId)
    $ids = @($RootId)
    for ($i = 0; $i -lt $ids.Count; $i++) { $ids += @($rows | Where-Object { $_.ParentProcessId -eq $ids[$i] -and [int]$_.ProcessId -notin $ids } | ForEach-Object { [int]$_.ProcessId }) }
    return $ids
}
# A descendant that cannot be read (exited, elevated) is counted, and the run is then reported incomplete instead of understated.
function Read-Tree([int]$RootId) {
    $procs = @{}
    $unreadable = 0
    foreach ($id in Get-TreeIds $RootId) {
        try {
            $p = [System.Diagnostics.Process]::GetProcessById($id)
            try {
                $procs[$id] = @{
                    Cycles = [RoqaIdle.Native]::Cycles($p.Handle); CpuMs = $p.TotalProcessorTime.TotalMilliseconds
                    Private = $p.PrivateMemorySize64; Handles = $p.HandleCount; Threads = $p.Threads.Count
                    Gdi = [RoqaIdle.Native]::GetGuiResources($p.Handle, 0)
                    User = if ($id -eq $RootId) { [RoqaIdle.Native]::UserObjects($p.Handle) } else { [RoqaIdle.Native]::GetGuiResources($p.Handle, 1) }
                }
            }
            finally { $p.Dispose() }
        }
        catch { if ($id -eq $RootId) { throw }; $unreadable++ }
    }
    return @{ Procs = $procs; Unreadable = $unreadable }
}
function Sum-Tree($Tree, [string]$Key) { return [double](@($Tree.Procs.Values | ForEach-Object { $_[$Key] } | Measure-Object -Sum).Sum) }
# Processes alive at both reads add their difference; processes born between reads add their whole value (a start scenario has no start read).
function Measure-TreeDelta($Start, $End, [string]$Key) {
    $sum = 0.0
    foreach ($id in $End.Procs.Keys) {
        $before = $Start.Procs[$id]
        $sum += if ($null -ne $before -and $End.Procs[$id][$Key] -ge $before[$Key]) { $End.Procs[$id][$Key] - $before[$Key] } else { $End.Procs[$id][$Key] }
    }
    return $sum
}
# Wake proxy: raw ContextSwitchesPersec is each live thread's cumulative switch count. One WMI read takes seconds,
# so reads sit outside the CPU window; each is timed by its midpoint because the snapshot instant inside it is unknown.
function Read-ContextSwitches([int[]]$ProcessIds) {
    $begin = [System.Diagnostics.Stopwatch]::GetTimestamp()
    $rows = @(Get-CimInstance Win32_PerfRawData_PerfProc_Thread -Filter (($ProcessIds | ForEach-Object { "IDProcess = $_" }) -join ' OR ') -Property IDThread, ContextSwitchesPersec)
    $end = [System.Diagnostics.Stopwatch]::GetTimestamp()
    # A live ROQA always has threads, so no rows means the counter source failed, not zero wakes.
    if (!$rows.Count) { throw "WMI returned no threads for process $($ProcessIds[0]); performance counters may be disabled." }
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
# The counters of ROQA's last --diagnostics report (written after each catalog scan, so up to one reconcile period old; the age is recorded).
function Read-Diagnostics {
    if (!$diagnosticsPath -or !(Test-Path -LiteralPath $diagnosticsPath)) { return $null }
    try { return @{ Counters = (Get-Content -Raw -LiteralPath $diagnosticsPath | ConvertFrom-Json).counters; AgeSeconds = ((Get-Date) - (Get-Item -LiteralPath $diagnosticsPath).LastWriteTime).TotalSeconds } }
    catch { return $null }
}
function Measure-DiagnosticsDelta($Start, $End) {
    $delta = [ordered]@{}
    if ($Start -and $End) { foreach ($name in 'media_wakes', 'media_refreshes', 'paints', 'store_writes') { if ($null -ne $Start.Counters.$name -and $null -ne $End.Counters.$name) { $delta[$name] = $End.Counters.$name - $Start.Counters.$name } } }
    return $delta
}
$inputBeforeLaunch = [RoqaIdle.Native]::LastInputTick()
function Assert-NoInput([string]$When) { if ($inputGuard -and [RoqaIdle.Native]::LastInputTick() -ne $inputBeforeLaunch) { throw "STOP: user input arrived during $When; nothing was recorded." } }
$idleSecondsAtLaunch = [RoqaIdle.Native]::IdleMilliseconds() / 1000
$sampler = [System.Diagnostics.Process]::GetCurrentProcess()
$launchClock = [System.Diagnostics.Stopwatch]::StartNew()
$launchTimestamp = [System.Diagnostics.Stopwatch]::GetTimestamp()
# Hidden is ROQA's own launch mode; a competitor starts with its default window state, whose effect is unverified.
$launchArguments = @{ FilePath = $executable; PassThru = $true; WindowStyle = 'Hidden' }
if ($diagnosticsPath) { Remove-Item -LiteralPath $diagnosticsPath -ErrorAction SilentlyContinue; $launchArguments.ArgumentList = @('--diagnostics', ('"' + $diagnosticsPath + '"')) }
$measuredHost = if ($Competitor) { Start-Process -FilePath $executable -PassThru } else { Start-Process @launchArguments }
$sampleClock = $null
$diagnosticsStart = $null
try {
    $empty = @{ Procs = @{}; Unreadable = 0 }
    if ($isStart) {
        # A start scenario counts from process creation, so the start counters are empty by construction.
        $treeStart = $empty
        $switchesStart = @{ Counts = @{}; Midpoint = $launchTimestamp; ReadSeconds = 0 }
        $sampleClock = $launchClock
    }
    else {
        if ($measuredHost.WaitForExit(5000)) { throw 'The measured build exited during the five-second settle period.' }
        $treeStart = Read-Tree $measuredHost.Id
        $switchesStart = Read-ContextSwitches @($treeStart.Procs.Keys)
        $sampleClock = [System.Diagnostics.Stopwatch]::StartNew()
    }
    $diagnosticsStart = Read-Diagnostics
    $startSamplerCycles = [RoqaIdle.Native]::Cycles($sampler.Handle)
    $settleSeconds = $launchClock.Elapsed.TotalSeconds
    # Polls the sampler (not the build) once a second so user input stops a long run early.
    while (($remaining = $SampleSeconds * 1000 - $sampleClock.ElapsedMilliseconds) -gt 0) {
        if ($measuredHost.WaitForExit([int][Math]::Min(1000, $remaining))) { throw "The measured build exited during the $SampleSeconds-second sample." }
        Assert-NoInput 'the settle or sample'
    }
    $treeEnd = Read-Tree $measuredHost.Id
    $endSamplerCycles = [RoqaIdle.Native]::Cycles($sampler.Handle)
    $sampleClock.Stop()
    $switchesEnd = Read-ContextSwitches @($treeEnd.Procs.Keys)
    $diagnosticsEnd = Read-Diagnostics
    Assert-NoInput 'the final reads'
    if (!$held -and $idleLimitSeconds -and $idleSecondsAtLaunch + $launchClock.Elapsed.TotalSeconds -ge $idleLimitSeconds) { throw "STOP: the $idleLimitSeconds s idle timeout passed during the run; nothing was recorded." }
    $seconds = $sampleClock.Elapsed.TotalSeconds
    $cycles = Measure-TreeDelta $treeStart $treeEnd 'Cycles'
    $cpuMilliseconds = Measure-TreeDelta $treeStart $treeEnd 'CpuMs'
    $wake = Measure-SwitchDelta $switchesStart.Counts $switchesEnd.Counts
    $wakeSeconds = ($switchesEnd.Midpoint - $switchesStart.Midpoint) / [System.Diagnostics.Stopwatch]::Frequency
    $processesExited = @($treeStart.Procs.Keys | Where-Object { !$treeEnd.Procs.ContainsKey($_) }).Count
    $unreadable = [Math]::Max($treeStart.Unreadable, $treeEnd.Unreadable)
    $complete = $processesExited -eq 0 -and $unreadable -eq 0
    $processCount = [Math]::Max($treeStart.Procs.Count, $treeEnd.Procs.Count)
    $observation = [ordered]@{
        Label = 'Observation only; CpuVerdict is INCONCLUSIVE below the counter floor and, for idle, compares CPU with the proposed 1 ms/s gate; no product-budget or comparison verdict'
        Status = 'COMPLETE'
        Utc = [DateTime]::UtcNow.ToString('o')
        Scenario = $Scenario
        Build = $label
        Repetition = $Repetition
        Note = $Note
        Competitor = if ($Competitor) { [ordered]@{ Name = $Competitor; Executable = $executable; PinnedSha256 = $ExpectedSha256.ToUpperInvariant() } } else { $null }
        BinarySha256 = $binarySha
        Method = "Scope is the launched process plus descendants (one Win32_Process read); $(if ($isStart) { 'counters run from process creation, so every *Start value is 0 by construction' } else { 'five-second settle plus one WMI read (SettleSeconds, about 10 s; M1 used 5 s)' }); $SampleSeconds-second wait; System.Diagnostics.Process counters summed over the tree; QueryProcessCycleTime; GetGuiResources; per-thread WMI context switches read just outside the CPU window; sampler cycles recorded; holds SetThreadExecutionState (display and system required) so no display-off or screen-saver timeout falls inside the run, and refuses to run into one if that call fails; $(if ($inputGuard) { 'stops on any user input (GetLastInputInfo) from launch to the final read' } else { 'user input is expected (churn or media stimulus) and does not stop the run' }); no working-set trimming"
        Machine = $machine
        Complete = $complete
        ProcessesStart = $treeStart.Procs.Count
        ProcessesEnd = $treeEnd.Procs.Count
        ProcessesExited = $processesExited
        UnreadableProcesses = $unreadable
        CpuVerdict = Get-CpuVerdict ($cpuMilliseconds / $seconds) $seconds $processCount $complete ($Scenario -eq 'idle')
        DurationSeconds = $seconds
        CpuMilliseconds = $cpuMilliseconds
        CpuMillisecondsPerSecond = $cpuMilliseconds / $seconds
        PrivateBytesStart = Sum-Tree $treeStart 'Private'
        PrivateBytesEnd = Sum-Tree $treeEnd 'Private'
        HandlesStart = Sum-Tree $treeStart 'Handles'
        HandlesEnd = Sum-Tree $treeEnd 'Handles'
        SettleSeconds = $settleSeconds
        IdleSecondsAtLaunch = $idleSecondsAtLaunch
        CpuCycles = $cycles
        CpuCyclesPerSecond = $cycles / $seconds
        NominalClockMegahertz = $nominalMegahertz
        ApproxCpuMillisecondsPerSecondFromCycles = $cycles / ($nominalMegahertz * 1000) / $seconds
        GdiObjectsStart = Sum-Tree $treeStart 'Gdi'
        GdiObjectsEnd = Sum-Tree $treeEnd 'Gdi'
        UserObjectsStart = Sum-Tree $treeStart 'User'
        UserObjectsEnd = Sum-Tree $treeEnd 'User'
        SamplerCyclesPerSecond = ($endSamplerCycles - $startSamplerCycles) / $seconds
        WakeProxyContextSwitches = $wake.Switches
        WakeProxySeconds = $wakeSeconds
        WakeProxyContextSwitchesPerSecond = $wake.Switches / $wakeSeconds
        WakeProxyReadSeconds = @($switchesStart.ReadSeconds, $switchesEnd.ReadSeconds)
        ThreadsStart = $switchesStart.Counts.Count
        ThreadsEnd = $switchesEnd.Counts.Count
        ThreadsExited = $wake.Exited
        ThreadsBorn = $wake.Born
        KeepAwakeHeld = $held
        Diagnostics = if ($diagnosticsPath) { [ordered]@{ Start = if ($diagnosticsStart) { $diagnosticsStart.Counters } else { $null }; End = if ($diagnosticsEnd) { $diagnosticsEnd.Counters } else { $null }; StartReportAgeSeconds = if ($diagnosticsStart) { $diagnosticsStart.AgeSeconds } else { $null }; EndReportAgeSeconds = if ($diagnosticsEnd) { $diagnosticsEnd.AgeSeconds } else { $null }; Delta = Measure-DiagnosticsDelta $diagnosticsStart $diagnosticsEnd } } else { $null }
        Limits = 'CpuMilliseconds moves in scheduler ticks (15.625 ms by default) per process, so it is coarse at idle. CpuCycles has no tick quantization; ApproxCpuMillisecondsPerSecondFromCycles assumes cycles tick at NominalClockMegahertz (WMI MaxClockSpeed), which is unverified; the QueryThreadCycleTime docs advise against converting cycles to time, so it is approximate; compare builds by CpuCyclesPerSecond. WakeProxyContextSwitches is a wake proxy, not a wake count: it is matched per thread ID, threads that exit between reads lose their in-window switches (so it can understate; see ThreadsExited, and a reused thread ID can hide an exit and understate further), and its window is WakeProxySeconds give or take half the sum of WakeProxyReadSeconds because the snapshot instant inside each WMI read is unknown. SettleSeconds is longer than the M1 five seconds. Group-policy screen-saver and lock timeouts and the system sleep timeout are not read; a screen saver without ScreenSaveTimeOut is treated as never. Two endpoint queries per counter; SamplerCyclesPerSecond covers only the wait loop, not the WMI reads; DWM/GPU/power/shared allocations not attributed; other desktop applications remain running. Diagnostics counters are those of the last ROQA report, up to one catalog reconcile period older than the read (see the report ages); null means no report was readable, never zero. A held execution state does not stop group-policy lock or sleep timeouts. Complete is false when a descendant exited or could not be read between the reads; its CPU is then lost and the verdict is INCONCLUSIVE.'
    }
    $observation | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $outputPath
}
catch {
    # Keep the partial evidence of an abort (user input, idle timeout, early exit) and still fail the run.
    $failure = $_
    [ordered]@{ Status = 'ABORTED'; Reason = $failure.Exception.Message; Utc = [DateTime]::UtcNow.ToString('o'); Scenario = $Scenario; Build = $label; Repetition = $Repetition; BinarySha256 = $binarySha; ElapsedSeconds = $launchClock.Elapsed.TotalSeconds; SampleElapsedSeconds = if ($sampleClock) { $sampleClock.Elapsed.TotalSeconds } else { $null }; SampleSeconds = $SampleSeconds; KeepAwakeHeld = $held; DiagnosticsStart = if ($diagnosticsStart) { $diagnosticsStart.Counters } else { $null } } | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 $abortedPath
    Write-Warning "Aborted after $([int]$launchClock.Elapsed.TotalSeconds) s; partial record in $abortedPath."
    throw
}
finally {
    [void][RoqaIdle.Native]::KeepAwake($false)
    if (!$measuredHost.HasExited) {
        if ($Competitor) {
            # A competitor has no known --quit: ask its main window to close, then end only the owned process tree.
            [void]$measuredHost.CloseMainWindow()
            if (!$measuredHost.WaitForExit(5000)) { & taskkill.exe /PID $measuredHost.Id /T /F | Out-Null; [void]$measuredHost.WaitForExit(5000) }
        }
        else {
            $quit = Start-Process -FilePath $executable -ArgumentList '--quit' -WindowStyle Hidden -PassThru
            if (!$quit.WaitForExit(5000) -or $quit.ExitCode -ne 0) { Write-Warning 'Idle sample quit signaling failed.' }
            $quit.Dispose()
            if (!$measuredHost.WaitForExit(5000)) { $measuredHost.Kill(); throw 'Forced cleanup after idle sample; graceful shutdown failed.' }
            if ($measuredHost.ExitCode -ne 0) { throw "Measured host exited with $($measuredHost.ExitCode)." }
        }
    }
    $measuredHost.Dispose()
}
Write-Host "Observation saved to $outputPath."
