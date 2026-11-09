Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
& "$PSScriptRoot/cargo.ps1" -CargoArguments @('fmt', '--check')
& "$PSScriptRoot/cargo.ps1" -CargoArguments @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
& "$PSScriptRoot/cargo.ps1" -CargoArguments @('test', '--locked')
