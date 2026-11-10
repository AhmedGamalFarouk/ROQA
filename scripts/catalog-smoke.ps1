param([switch]$SkipBuild, [string]$EvidenceDirectory = 'artifacts/m2a')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (!$SkipBuild) { & "$PSScriptRoot/cargo.ps1" build --locked --release }
Add-Type -AssemblyName System.Drawing
Add-Type -Path "$projectRoot/tests/NativeProbe.cs" -ReferencedAssemblies System.Drawing
[NativeProbe]::EnablePhysicalPixels()
$evidence = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $EvidenceDirectory))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$diagnostics = Join-Path $evidence 'catalog.json'
$executable = Join-Path $projectRoot 'target/release/roqa.exe'
$observations = [ordered]@{ BinarySha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash; Utc = [DateTime]::UtcNow.ToString('o'); Checks = @(); Samples = @() }
function Check([bool]$Condition, [string]$Name) {
    if (!$Condition) { throw "FAIL: $Name" }
    $observations.Checks += $Name
}
function Read-Catalog {
    if (!(Test-Path -LiteralPath $diagnostics)) { return $null }
    # Allow atomic replacement while reading; never block a diagnostic writer with a deny-delete handle.
    $stream = [System.IO.File]::Open($diagnostics, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete))
    $reader = New-Object System.IO.StreamReader($stream)
    try { return ($reader.ReadToEnd() | ConvertFrom-Json) } finally { $reader.Dispose() }
}
function Wait-Catalog([scriptblock]$Predicate, [string]$Name, [int]$TimeoutSeconds = 8) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($null -ne $resident -and $resident.HasExited) { throw "Host exited $($resident.ExitCode) during $Name." }
        $state = Read-Catalog
        if ($null -ne $state -and (& $Predicate $state)) { Check $true $Name; return $state }
        Start-Sleep -Milliseconds 30
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "FAIL: Catalog did not converge: $Name"
}
function Fixture-Row($State, [IntPtr]$Window) { return ,@($State.windows | Where-Object { $_.handle -eq $Window.ToInt64() -and $_.process -eq $PID }) }
function Sample-Host([string]$Name) {
    $resident.Refresh()
    return [ordered]@{ Name = $Name; PrivateBytes = $resident.PrivateMemorySize64; Handles = $resident.HandleCount; User = [NativeProbe]::GetGuiResources($resident.Handle, 1); Gdi = [NativeProbe]::GetGuiResources($resident.Handle, 0) }
}
function Quit-Host {
    $quit = Start-Process -FilePath $executable -ArgumentList '--quit' -WindowStyle Hidden -PassThru
    try { Check ($quit.WaitForExit(5000) -and $quit.ExitCode -eq 0) 'Quit control delivered' } finally { $quit.Dispose() }
    Check ($resident.WaitForExit(5000)) 'Worker/hooks shutdown completes promptly'
    Check ($resident.ExitCode -eq 0) "Worker/hooks shutdown exits zero (observed $($resident.ExitCode))"
}
$stormThread = $null
$queueTraffic = $null
$resident = $null; $fixture = [IntPtr]::Zero
$originalForeground = [NativeProbe]::GetForegroundWindow()
try {
    Check ([NativeProbe]::Surfaces('ROQA Dock').Count -eq 0) 'No pre-existing resident surface'
    $fixture = [NativeProbe]::StartAnchor()
    Check ([NativeProbe]::SetWindowText($fixture, 'ROQA catalog fixture')) 'Set real fixture title'
    Start-Sleep -Milliseconds 150
    $foreground = [NativeProbe]::GetForegroundWindow()
    if ($foreground -eq [IntPtr]::Zero) { throw 'INTERFERENCE: No foreground before launch.' }
    $resident = Start-Process -FilePath $executable -ArgumentList @('--diagnostics', ('"' + $diagnostics + '"')) -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $evidence 'runtime.log')
    [void]$resident.Handle
    $state = Wait-Catalog { param($s) $s.complete -and (Fixture-Row $s $fixture).Count -eq 1 } 'Initial enumeration includes real fixture'
    $row = (Fixture-Row $state $fixture)[0]
    Check ($row.title_utf16_length -eq 'ROQA catalog fixture'.Length -and $row.identity_available) 'Real title and process identity read'
    Check ([NativeProbe]::GetForegroundWindow() -eq $foreground) 'Passive startup keeps foreground'
    $arabic = 'ROQA تجربة نافذة عربية'
    Check ([NativeProbe]::SetWindowText($fixture, $arabic)) 'Rename fixture to real Arabic title'
    $state = Wait-Catalog { param($s) $r = Fixture-Row $s $fixture; $r.Count -eq 1 -and $r[0].title_utf16_length -eq $arabic.Length } 'Name-change subscription refreshes UTF-16 title metadata'
    # Metadata only: this is not Arabic rendering/IME evidence; M2b must test those separately.
    [NativeProbe]::ShowFixture($fixture, 6)
    $state = Wait-Catalog { param($s) $r = Fixture-Row $s $fixture; $r.Count -eq 1 -and $r[0].minimized } 'Minimize event converges'
    [NativeProbe]::ShowFixture($fixture, 9)
    $state = Wait-Catalog { param($s) $r = Fixture-Row $s $fixture; $r.Count -eq 1 -and !$r[0].minimized } 'Restore event converges'
    [void][NativeProbe]::SetForegroundWindow($fixture)
    if ([NativeProbe]::GetForegroundWindow() -ne $fixture) { throw 'INTERFERENCE: Fixture could not obtain foreground.' }
    $state = Wait-Catalog { param($s) $r = Fixture-Row $s $fixture; $r.Count -eq 1 -and $r[0].foreground } 'Foreground event converges'
    [NativeProbe]::ShowFixture($fixture, 0)
    $state = Wait-Catalog { param($s) (Fixture-Row $s $fixture).Count -eq 0 } 'Hidden window removed'
    [NativeProbe]::ShowFixture($fixture, 4)
    $state = Wait-Catalog { param($s) (Fixture-Row $s $fixture).Count -eq 1 } 'Shown window reappears without polling delay'
    $beforeStorm = $state.scans
    $beforeFull = $state.full_scans
    $beforeDelta = $state.delta_batches
    $beforeInspected = $state.inspected
    $stormClock = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($index in 1..500) { [void][NativeProbe]::SetWindowText($fixture, "ROQA event storm $index") }
    $finalTitle = 'ROQA event storm complete - newest metadata wins'
    [void][NativeProbe]::SetWindowText($fixture, $finalTitle)
    $state = Wait-Catalog { param($s) $r = Fixture-Row $s $fixture; $r.Count -eq 1 -and $r[0].title_utf16_length -eq $finalTitle.Length } 'Event storm converges to final title'
    $stormClock.Stop()
    $observations.Storm = [ordered]@{ Changes = 501; Scans = $state.scans - $beforeStorm; FullScans = $state.full_scans - $beforeFull; DeltaBatches = $state.delta_batches - $beforeDelta; Inspected = $state.inspected - $beforeInspected; ElapsedMilliseconds = $stormClock.Elapsed.TotalMilliseconds }
    Check (($state.scans - $beforeStorm) -lt 50) '501 metadata changes require fewer than 50 worker batches'
    Check (($state.full_scans - $beforeFull) -le 1 -and ($state.delta_batches - $beforeDelta) -ge 1) 'Ordinary title events use targeted refresh with at most one overflow repair'
    [NativeProbe]::StopAnchor($fixture); $closed = $fixture; $fixture = [IntPtr]::Zero
    $state = Wait-Catalog { param($s) (Fixture-Row $s $closed).Count -eq 0 } 'Destroyed window removed'
    $baseline = Sample-Host 'warm'; $observations.Samples += $baseline
    foreach ($block in 1..3) {
        foreach ($cycle in 1..5) {
            $fixture = [NativeProbe]::StartAnchor()
            $state = Wait-Catalog { param($s) (Fixture-Row $s $fixture).Count -eq 1 } 'New window create/show converges'
            [NativeProbe]::StopAnchor($fixture); $closed = $fixture; $fixture = [IntPtr]::Zero
            $state = Wait-Catalog { param($s) (Fixture-Row $s $closed).Count -eq 0 } 'Window destruction converges'
        }
        Start-Sleep -Milliseconds 200
        $sample = Sample-Host "block-$block"; $observations.Samples += $sample
        Check (($sample.PrivateBytes - $baseline.PrivateBytes) -le 8MB) "Block $block private memory bounded"
        Check (($sample.Handles - $baseline.Handles) -le 10 -and ($sample.User - $baseline.User) -le 3 -and ($sample.Gdi - $baseline.Gdi) -le 3) "Block $block kernel/USER/GDI bounded"
    }
    $dock = [NativeProbe]::PrimarySurface('ROQA Dock')
    $fixture = [NativeProbe]::StartAnchor()
    $state = Wait-Catalog { param($s) (Fixture-Row $s $fixture).Count -eq 1 } 'Fixture present before queue saturation'
    $beforeQueueGeneration = $state.generation
    $beforeRepair = $state.reconciliations
    $queueTraffic = [NativeProbe]::StartQueueTraffic($dock, 36000)
    $queueTitle = 'ROQA queue saturation metadata changes - final'
    Check ([NativeProbe]::SetWindowText($fixture, $queueTitle)) 'Change fixture metadata while queue traffic runs'
    $state = Wait-Catalog { param($s) $r = Fixture-Row $s $fixture; $s.generation -gt $beforeQueueGeneration -and $queueTraffic.Worker.IsAlive -and $r.Count -eq 1 -and $r[0].title_utf16_length -eq $queueTitle.Length } 'Catalog publishes new metadata during sustained posted-message traffic' 35
    $state = Wait-Catalog { param($s) $s.reconciliations -gt $beforeRepair -and $s.complete -and $queueTraffic.Worker.IsAlive } 'Monotonic repair runs while posted-message traffic continues' 35
    Check ($queueTraffic.Worker.Join(40000)) 'Owned queue traffic producer stops'
    $observations.QueueTraffic = [ordered]@{ Posted = $queueTraffic.Posted; Rejected = $queueTraffic.Rejected; Generation = $state.generation; Reconciliations = $state.reconciliations }
    Check ($queueTraffic.Posted -gt 10000 -and $queueTraffic.Rejected -gt 0) 'Probe reached posted-message queue capacity during the sustained workload'
    $queueTraffic = $null
    [NativeProbe]::StopAnchor($fixture); $fixture = [IntPtr]::Zero
    $state = Wait-Catalog { param($s) $s.reconciliations -ge 1 -and $s.complete } 'Thirty-second fallback reconciliation actually runs' 35
    $fixture = [NativeProbe]::StartAnchor()
    $state = Wait-Catalog { param($s) (Fixture-Row $s $fixture).Count -eq 1 } 'Fixture present before shutdown storm'
    $beforeContinuous = $state.generation
    $stormThread = [NativeProbe]::StartTitleStorm($fixture, 3000)
    $state = Wait-Catalog { param($s) $s.generation -gt $beforeContinuous -and $stormThread.IsAlive } 'Catalog keeps publishing while title events continue'
    Start-Sleep -Milliseconds 150
    $quitClock = [System.Diagnostics.Stopwatch]::StartNew()
    Quit-Host
    $quitClock.Stop()
    $observations.ShutdownDuringStormMilliseconds = $quitClock.Elapsed.TotalMilliseconds
    Check ($quitClock.Elapsed.TotalMilliseconds -lt 1500) 'Quit remains responsive during ongoing title events'
    Check ($stormThread.Join(5000)) 'Owned fixture storm thread stops'
    $stormThread = $null
    $state = Read-Catalog
    Check ($state.stopped -and $state.hooks_live -eq 0) 'In-process explicit unhook and worker join recorded before process exit'
    $observations.Final = $state
    $observations.Status = 'PASS'
} catch {
    $observations.Status = if ($_.Exception.Message.StartsWith('INTERFERENCE:')) { 'INCONCLUSIVE' } else { 'FAIL' }
    $observations.Failure = $_.Exception.Message
    throw
} finally {
    if ($null -ne $resident) {
        if (!$resident.HasExited) {
            try { Quit-Host } catch { Write-Warning $_ }
            if (!$resident.WaitForExit(2000)) { $resident.Kill(); Write-Warning 'Forced cleanup of owned host; not a test pass.' }
        }
        $resident.Dispose()
    }
    if ($null -ne $queueTraffic -and !$queueTraffic.Worker.Join(40000)) { Write-Warning 'Queue producer did not stop.' }
    if ($null -ne $stormThread -and !$stormThread.Join(5000)) { Write-Warning 'Fixture storm thread did not stop.' }
    if ($fixture -ne [IntPtr]::Zero) { [NativeProbe]::StopAnchor($fixture) }
    [void][NativeProbe]::SetForegroundWindow($originalForeground)
    $observations | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence 'catalog-smoke.json') -Encoding UTF8
}
Write-Host "Window catalog smoke passed. Evidence: $evidence"
