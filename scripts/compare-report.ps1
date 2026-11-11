# Compares saved runs from measure-idle.ps1: ROQA runs against pinned competitor runs of one scenario on one machine. Reads JSON only; launches nothing.
# -SelfTest checks the verdict logic and the run validation on fixed inputs.
param([string[]]$RoqaRuns = @(), [string[]]$CompetitorRuns = @(), [string]$Output = '', [switch]$SelfTest)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/verdict.ps1"
function New-Report($Roqa, $Competitor) {
    $Roqa = @($Roqa); $Competitor = @($Competitor)
    if (!$Roqa.Count -or !$Competitor.Count) { throw 'Both -RoqaRuns and -CompetitorRuns are required.' }
    foreach ($run in $Roqa) { if ($run.Competitor) { throw "ROQA side holds a competitor run ($($run.Build))." } }
    foreach ($run in $Competitor) { if (!$run.Competitor -or !$run.Competitor.PinnedSha256 -or $run.Competitor.PinnedSha256 -ne $run.BinarySha256) { throw "Competitor run is not pinned to its measured SHA-256 ($($run.Build))." } }
    # Verdicts only mean something for one pinned build per side, one scenario and one machine.
    foreach ($side in $Roqa, $Competitor) {
        if (@($side | ForEach-Object { $_.BinarySha256 } | Select-Object -Unique).Count -ne 1) { throw 'A side mixes builds (different BinarySha256).' }
        if (@($side | ForEach-Object { $_.Build } | Select-Object -Unique).Count -ne 1) { throw 'A side mixes build labels.' }
    }
    $all = $Roqa + $Competitor
    if (@($all | ForEach-Object { $_.Scenario } | Select-Object -Unique).Count -ne 1) { throw 'Runs mix scenarios.' }
    $machines = @($all | ForEach-Object { "$($_.Machine.Os.BuildNumber)|$($_.Machine.Cpu.Name)|$(($_.Machine.Gpu | ForEach-Object { $_.Name }) -join ',')|$($_.Machine.MonitorCount)" } | Select-Object -Unique)
    if ($machines.Count -ne 1) { throw 'Runs come from different machine configurations (OS build, CPU, GPU or monitor count).' }
    $complete = @($all | Where-Object { !$_.Complete }).Count -eq 0
    $cpuMeasurable = @($all | Where-Object { $_.CpuVerdict -eq 'INCONCLUSIVE' }).Count -eq 0
    $metrics = [ordered]@{}
    foreach ($metric in 'PrivateBytesEnd', 'CpuCyclesPerSecond') {
        $metrics[$metric] = Get-RatioVerdict ([double[]]@($Roqa | ForEach-Object { $_.$metric })) ([double[]]@($Competitor | ForEach-Object { $_.$metric })) ($complete -and ($metric -ne 'CpuCyclesPerSecond' -or $cpuMeasurable))
    }
    return [ordered]@{
        Label = 'Pinned builds on one machine; MET needs 3 runs a side and a 30% lower median; INCONCLUSIVE below counter floors. DWM, GPU, power and latency are separate manual measurements; churn boundedness is read from the Start/End handle and object counts, not judged here.'
        Scenario = $all[0].Scenario
        Roqa = [ordered]@{ Build = $Roqa[0].Build; BinarySha256 = $Roqa[0].BinarySha256; Runs = $Roqa.Count }
        Competitor = [ordered]@{ Name = $Competitor[0].Competitor.Name; PinnedSha256 = $Competitor[0].Competitor.PinnedSha256; Runs = $Competitor.Count }
        Machine = $all[0].Machine
        Metrics = $metrics
    }
}
function Read-Runs([string[]]$Paths) { return @($Paths | ForEach-Object { Get-Content -Raw -LiteralPath $_ | ConvertFrom-Json }) }
if ($SelfTest) {
    function Assert-Equal($Actual, $Expected, [string]$Name) { if ($Actual -ne $Expected) { throw "SELF-TEST FAIL $Name : got '$Actual', want '$Expected'" } }
    function Assert-Throws([scriptblock]$Block, [string]$Pattern, [string]$Name) {
        try { & $Block } catch { if ($_.Exception.Message -match $Pattern) { return }; throw "SELF-TEST FAIL $Name : wrong error '$($_.Exception.Message)'" }
        throw "SELF-TEST FAIL $Name : no error"
    }
    Assert-Equal (Get-CpuVerdict 0.5 900 1 $true $true) 'WITHIN' 'cpu within'
    Assert-Equal (Get-CpuVerdict 1.0 900 1 $true $true) 'WITHIN' 'cpu at the gate'
    Assert-Equal (Get-CpuVerdict 2.0 900 1 $true $true) 'ABOVE' 'cpu above'
    Assert-Equal (Get-CpuVerdict 0.5 30 1 $true $true) 'INCONCLUSIVE' 'one tick in 30 s is 0.52 ms/s'
    Assert-Equal (Get-CpuVerdict 0.5 900 10 $true $true) 'INCONCLUSIVE' 'ten processes at 900 s: 0.17 ms/s'
    Assert-Equal (Get-CpuVerdict 0.5 900 1 $false $true) 'INCONCLUSIVE' 'incomplete run'
    Assert-Equal (Get-CpuVerdict 0.5 900 1 $true $false) 'MEASURABLE' 'not gated'
    Assert-Equal (Get-Median ([double[]](1, 2, 3, 4))) 2.5 'median even'
    Assert-Equal (Get-Median ([double[]](9, 1, 5))) 5 'median odd'
    Assert-Equal (Get-RatioVerdict ([double[]](60, 60, 60)) ([double[]](100, 100, 100)) $true).Verdict 'MET' 'ratio met'
    Assert-Equal (Get-RatioVerdict ([double[]](70, 70, 70)) ([double[]](100, 100, 100)) $true).Verdict 'MET' 'ratio at exactly 30%'
    Assert-Equal (Get-RatioVerdict ([double[]](75, 75, 75)) ([double[]](100, 100, 100)) $true).Verdict 'NOT MET' 'ratio not met'
    Assert-Equal (Get-RatioVerdict ([double[]](60, 60)) ([double[]](100, 100, 100)) $true).Verdict 'INCONCLUSIVE' 'two runs'
    Assert-Equal (Get-RatioVerdict ([double[]](60, 60, 60)) ([double[]](100, 100, 100)) $false).Verdict 'INCONCLUSIVE' 'below floor'
    Assert-Equal (Get-RatioVerdict ([double[]](1, 1, 1)) ([double[]](0, 0, 0)) $true).Verdict 'INCONCLUSIVE' 'zero competitor'
    function New-Run($Build, $Sha, $Value, $Pinned) {
        [pscustomobject]@{ Scenario = 'idle'; Build = $Build; BinarySha256 = $Sha; Complete = $true; CpuVerdict = 'WITHIN'; PrivateBytesEnd = $Value; CpuCyclesPerSecond = $Value
            Competitor = $(if ($Pinned -ne 'none') { [pscustomobject]@{ Name = $Build; PinnedSha256 = $Pinned } } else { $null })
            Machine = [pscustomobject]@{ Os = [pscustomobject]@{ BuildNumber = '26200' }; Cpu = [pscustomobject]@{ Name = 'cpu' }; Gpu = @([pscustomobject]@{ Name = 'gpu' }); MonitorCount = 1 } }
    }
    $roqa = 1..3 | ForEach-Object { New-Run 'roqa' 'AA' 50 'none' }
    $rival = 1..3 | ForEach-Object { New-Run 'rival' 'BB' 100 'BB' }
    Assert-Equal (New-Report $roqa $rival).Metrics.PrivateBytesEnd.Verdict 'MET' 'report'
    Assert-Throws { New-Report $roqa @(New-Run 'rival' 'BB' 100 'CC') } 'not pinned' 'pin mismatch'
    Assert-Throws { New-Report $roqa @(New-Run 'rival' 'BB' 100 'none') } 'not pinned' 'no pin'
    Assert-Throws { New-Report $roqa @() } 'required' 'no competitor runs'
    Assert-Throws { New-Report ($roqa + (New-Run 'roqa' 'ZZ' 50 'none')) $rival } 'mixes builds' 'mixed builds'
    $other = New-Run 'rival' 'BB' 100 'BB'; $other.Machine.MonitorCount = 3
    Assert-Throws { New-Report $roqa @($other) } 'different machine' 'machine mismatch'
    Write-Host 'SELF-TEST PASS'
    return
}
$report = New-Report (Read-Runs $RoqaRuns) (Read-Runs $CompetitorRuns) | ConvertTo-Json -Depth 8
if ($Output) { $report | Set-Content -Encoding UTF8 $Output; Write-Host "Report saved to $Output." } else { $report }
