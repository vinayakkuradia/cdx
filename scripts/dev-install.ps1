# Developer installer: build & install cdx-bin from source, then wire up PowerShell (and cmd.exe).
$ErrorActionPreference = 'Stop'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust toolchain not found. Install it from https://rustup.rs and re-run."
}

$root = Split-Path -Parent $PSScriptRoot
cargo install --path $root --locked
if ($LASTEXITCODE -ne 0) {
    throw "cargo install failed. If the message above mentions link.exe, install Build Tools for Visual Studio with the C++ (MSVC) workload: https://visualstudio.microsoft.com/downloads/"
}

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
$bin = Join-Path $cargoHome 'bin'

# PowerShell profile
if (-not (Test-Path $PROFILE)) { New-Item -ItemType File -Path $PROFILE -Force | Out-Null }
if (Select-String -Path $PROFILE -SimpleMatch 'cdx-bin init' -Quiet) {
    Write-Host "Already configured: $PROFILE"
} else {
    Add-Content -Path $PROFILE -Value "`n# cdx`nInvoke-Expression (& cdx-bin init powershell | Out-String)"
    Write-Host "Updated: $PROFILE"
}

# cmd.exe support: a cdx.cmd next to the binary (already on PATH via cargo)
& (Join-Path $bin 'cdx-bin.exe') init cmd | Set-Content -Path (Join-Path $bin 'cdx.cmd') -Encoding ascii
Write-Host "Wrote: $(Join-Path $bin 'cdx.cmd')"

Write-Host "`nDone. Open a new terminal and type: cdx"
