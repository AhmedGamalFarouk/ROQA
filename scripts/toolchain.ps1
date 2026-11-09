Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:RoqaRoot = Split-Path -Parent $PSScriptRoot

function Invoke-RoqaToolchain {
    param([scriptblock]$Command)
    $priorCargo = $env:CARGO_HOME
    $priorRustup = $env:RUSTUP_HOME
    $priorPath = $env:PATH
    try {
        $env:CARGO_HOME = Join-Path $script:RoqaRoot '.tools/cargo'
        $env:RUSTUP_HOME = Join-Path $script:RoqaRoot '.tools/rustup'
        $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $priorPath
        Push-Location $script:RoqaRoot
        try { & $Command } finally { Pop-Location }
    }
    finally {
        $env:CARGO_HOME = $priorCargo
        $env:RUSTUP_HOME = $priorRustup
        $env:PATH = $priorPath
    }
}
