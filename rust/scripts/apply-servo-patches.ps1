param(
    [switch]$CheckOnly,
    [switch]$NoClean
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RustRoot = Resolve-Path (Join-Path $ScriptDir "..")
$RepoRoot = Resolve-Path (Join-Path $RustRoot "..")
$PatchPath = Join-Path $RepoRoot "docs\patches\servo-nodelist-bing-panic.patch"
$CargoLock = Join-Path $RustRoot "Cargo.lock"

if (-not (Test-Path -LiteralPath $PatchPath)) {
    throw "Servo patch file not found: $PatchPath"
}
if (-not (Test-Path -LiteralPath $CargoLock)) {
    throw "Cargo.lock not found: $CargoLock"
}

$lockText = Get-Content -LiteralPath $CargoLock -Raw
$match = [regex]::Match($lockText, 'git\+https://github\.com/servo/servo\?branch=main#([0-9a-f]+)')
if (-not $match.Success) {
    throw "Could not find locked Servo git revision in $CargoLock"
}
$LockedRevision = $match.Groups[1].Value

$CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
$CheckoutRoot = Join-Path $CargoHome "git\checkouts"
if (-not (Test-Path -LiteralPath $CheckoutRoot)) {
    throw "Cargo git checkout root not found: $CheckoutRoot. Run cargo fetch/build first."
}

$ServoCheckout = Get-ChildItem -LiteralPath $CheckoutRoot -Directory -Filter "servo-*" |
    ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -Directory } |
    Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "Cargo.toml") } |
    Where-Object {
        $head = (& git -C $_.FullName rev-parse HEAD 2>$null).Trim()
        $LASTEXITCODE -eq 0 -and $head -eq $LockedRevision
    } |
    Select-Object -First 1

if (-not $ServoCheckout) {
    throw "Could not find Servo checkout for locked revision $LockedRevision under $CheckoutRoot"
}

$ServoPath = $ServoCheckout.FullName
Write-Host "Servo checkout: $ServoPath"
Write-Host "Servo revision: $LockedRevision"

$AppliedNow = $false
& git -C $ServoPath apply --check --reverse $PatchPath 2>$null
if ($LASTEXITCODE -eq 0) {
    Write-Host "Servo NodeList patch is already applied."
} else {
    & git -C $ServoPath apply --check $PatchPath
    if ($LASTEXITCODE -ne 0) {
        throw "Servo NodeList patch does not apply cleanly to $ServoPath"
    }
    if ($CheckOnly) {
        Write-Host "Servo NodeList patch is not applied, but it applies cleanly."
        exit 0
    }
    & git -C $ServoPath apply $PatchPath
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to apply Servo NodeList patch."
    }
    $AppliedNow = $true
    Write-Host "Applied Servo NodeList patch."
}

if ($AppliedNow -and -not $NoClean -and -not $CheckOnly) {
    Write-Host "Cleaning servo-script so the next build uses the patched source..."
    & cargo clean -p servo-script --manifest-path (Join-Path $RustRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0) {
        throw "cargo clean -p servo-script failed."
    }
}
