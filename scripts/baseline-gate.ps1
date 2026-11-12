# Size-baseline raise gate (M5 gate 1, I3). Prints an error when the baseline in -Current is higher than
# the one in -Committed (HEAD) without a fresh sign-off; prints nothing when it is fine.
# A raise needs two lines that HEAD's file does not have:  raise-reason: <why>   approved-by: <name> <YYYY-MM-DD>
param([Parameter(Mandatory)][string[]]$Current, [string[]]$Committed = @())
Set-StrictMode -Version Latest
$new = [int64]$Current[0]
$old = if ($Committed.Count -gt 0) { [int64]$Committed[0] } else { $new }
if ($new -le $old) { return }
function Test-Fresh([string]$Pattern) { $null -ne ($Current | Where-Object { $_ -match $Pattern -and $_ -notin $Committed } | Select-Object -First 1) }
if ((Test-Fresh '^raise-reason:\s*\S') -and (Test-Fresh '^approved-by:\s*\S.*\d{4}-\d{2}-\d{2}\s*$')) { return }
"Baseline raised from $old B to $new B without sign-off. A gate reviewer or the user must approve the growth; add two new lines to scripts/size-baseline.txt in the same commit: 'raise-reason: <why>' and 'approved-by: <name> <YYYY-MM-DD>'."
