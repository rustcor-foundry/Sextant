# Build the Sextant Browser installer end to end.
# Usage:  powershell -ExecutionPolicy Bypass -File installer\build.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

Write-Host '==> Release build (sextant-browser)...' -ForegroundColor Cyan
cargo build --release -p sextant-browser --manifest-path (Join-Path $root 'rust\Cargo.toml')

# Locate ISCC (Inno Setup compiler).
$iscc = (Get-Command ISCC -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    foreach ($p in @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe")) {
        if (Test-Path $p) { $iscc = $p; break }
    }
}
if (-not $iscc) { throw 'ISCC (Inno Setup compiler) not found on PATH.' }

Write-Host "==> Compiling installer with $iscc ..." -ForegroundColor Cyan
& $iscc (Join-Path $PSScriptRoot 'sextant.iss')

Write-Host '==> Done. Setup written to dist\' -ForegroundColor Green
Get-ChildItem (Join-Path $root 'dist') -Filter 'sextant-browser-setup-*.exe' |
    Select-Object Name, Length, LastWriteTime
