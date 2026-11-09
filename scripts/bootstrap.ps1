. "$PSScriptRoot/toolchain.ps1"

Invoke-RoqaToolchain {
    $downloadDirectory = Join-Path $script:RoqaRoot '.tools/downloads'
    New-Item -ItemType Directory -Force -Path $downloadDirectory | Out-Null
    $installer = Join-Path $downloadDirectory 'rustup-init-1.28.2.exe'
    $installerUrl = 'https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-pc-windows-msvc/rustup-init.exe'
    $checksumReply = Invoke-WebRequest -UseBasicParsing -Uri ($installerUrl + '.sha256')
    $checksumText = if ($checksumReply.Content -is [byte[]]) {
        [System.Text.Encoding]::UTF8.GetString($checksumReply.Content)
    } else { [string]$checksumReply.Content }
    $expectedHash = ($checksumText.Trim() -split '\s+')[0]
    $pinnedInstallerHash = '88d8258dcf6ae4f7a80c7d1088e1f36fa7025a1cfd1343731b4ee6f385121fc0'
    if ($expectedHash -ne $pinnedInstallerHash) { throw 'Published checksum differs from the pinned installer checksum.' }
    if (!(Test-Path -LiteralPath $installer)) {
        Invoke-WebRequest -UseBasicParsing -Uri $installerUrl -OutFile $installer
    }
    if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $expectedHash) {
        throw 'Rust installer checksum mismatch; the installer was not executed.'
    }
    $installerHelp = & $installer --help
    if ($LASTEXITCODE -ne 0 -or !($installerHelp -match '--no-modify-path')) {
        throw 'Pinned rustup installer does not expose the required --no-modify-path flag.'
    }
    $rustup = Join-Path $env:CARGO_HOME 'bin/rustup.exe'
    if (!(Test-Path -LiteralPath $rustup)) {
        & $installer -y --no-modify-path --profile minimal --default-toolchain none
        if ($LASTEXITCODE -ne 0) { throw 'Project-local rustup installation failed.' }
    }
    & $rustup set auto-self-update disable
    if ($LASTEXITCODE -ne 0) { throw 'Could not disable project-local rustup self-updates.' }
    & $rustup toolchain install 1.91.1 --profile minimal --component rustfmt --component clippy --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Pinned Rust toolchain installation failed.' }
    & (Join-Path $env:CARGO_HOME 'bin/rustc.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'Pinned Rust verification failed.' }
    Write-Host "Verified installer SHA256: $expectedHash"
}
