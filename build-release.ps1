[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'

if (-not (Test-Path -LiteralPath $cargo)) {
    throw 'Rust was not found. Install rustup before building NTIV.'
}

Push-Location $projectRoot
try {
    & $cargo test
    if ($LASTEXITCODE -ne 0) { throw 'NTIV tests failed.' }

    & $cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'NTIV release build failed.' }

    $dist = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\ntiv.exe') -Destination (Join-Path $dist 'ntiv.exe') -Force
    Write-Host "Built: $(Join-Path $dist 'ntiv.exe')"
}
finally {
    Pop-Location
}

