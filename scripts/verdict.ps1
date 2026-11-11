# Pure verdict logic, dot-sourced by measure-idle.ps1 and compare-report.ps1 (self-test: compare-report.ps1 -SelfTest).
# Process.TotalProcessorTime moves in scheduler ticks per process; a verdict needs one tick per process to be under a tenth of the gate.
function Get-CpuVerdict([double]$MsPerSecond, [double]$DurationSeconds, [int]$ProcessCount, [bool]$Complete, [bool]$Gated, [double]$GateMsPerSecond = 1.0, [double]$TickMilliseconds = 15.625) {
    if (!$Complete -or $DurationSeconds -le 0 -or $TickMilliseconds * $ProcessCount / $DurationSeconds -gt $GateMsPerSecond / 10) { return 'INCONCLUSIVE' }
    if (!$Gated) { return 'MEASURABLE' }
    return $(if ($MsPerSecond -le $GateMsPerSecond) { 'WITHIN' } else { 'ABOVE' })
}
function Get-Median([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $mid = [int][Math]::Floor($sorted.Count / 2)
    return $(if ($sorted.Count % 2) { $sorted[$mid] } else { ($sorted[$mid - 1] + $sorted[$mid]) / 2 })
}
# Lower is better. MET needs at least $MinRuns per side, a measurable counter and a median reduction of $RequiredReduction.
function Get-RatioVerdict([double[]]$Roqa, [double[]]$Competitor, [bool]$Measurable, [int]$MinRuns = 3, [double]$RequiredReduction = 0.30) {
    $result = [ordered]@{ Verdict = 'INCONCLUSIVE'; Reason = ''; RoqaMedian = $null; CompetitorMedian = $null; Reduction = $null }
    if (!$Measurable) { $result.Reason = 'a run is incomplete or its counter is below the measurement floor'; return $result }
    if ($Roqa.Count -lt $MinRuns -or $Competitor.Count -lt $MinRuns) { $result.Reason = "fewer than $MinRuns runs on a side"; return $result }
    $result.RoqaMedian = Get-Median $Roqa
    $result.CompetitorMedian = Get-Median $Competitor
    if ($result.CompetitorMedian -le 0) { $result.Reason = 'competitor median is zero'; return $result }
    $result.Reduction = 1 - $result.RoqaMedian / $result.CompetitorMedian
    $result.Verdict = $(if ($result.Reduction -ge $RequiredReduction) { 'MET' } else { 'NOT MET' })
    return $result
}
