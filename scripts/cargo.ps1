param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArguments)
. "$PSScriptRoot/toolchain.ps1"
Invoke-RoqaToolchain {
    $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    if (!(Test-Path -LiteralPath $cargo)) { throw 'Run scripts/bootstrap.ps1 first.' }
    # Cargo prints progress ("Finished") on stderr; under 'Stop' PowerShell turns that into an error. The exit code check below is the real test.
    $ErrorActionPreference = 'Continue'
    & $cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE." }
}
