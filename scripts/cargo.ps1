param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArguments)
. "$PSScriptRoot/toolchain.ps1"
Invoke-RoqaToolchain {
    $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    if (!(Test-Path -LiteralPath $cargo)) { throw 'Run scripts/bootstrap.ps1 first.' }
    & $cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE." }
}
