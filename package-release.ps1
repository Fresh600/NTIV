[CmdletBinding()]
param(
    [string]$Version = '0.5.0'
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$binary = Join-Path $projectRoot 'dist\ntiv.exe'
if (-not (Test-Path -LiteralPath $binary)) {
    throw 'dist\ntiv.exe was not found. Run .\build-release.ps1 first.'
}

$dist = Join-Path $projectRoot 'dist'
$archive = Join-Path $dist "NTIV-$Version-windows-x64.zip"
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ntiv-package-" + [guid]::NewGuid().ToString('N'))
$resolvedTempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$resolvedTemporaryRoot = [System.IO.Path]::GetFullPath($temporaryRoot)
if (-not $resolvedTemporaryRoot.StartsWith($resolvedTempBase, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use unexpected temporary path: $resolvedTemporaryRoot"
}

New-Item -ItemType Directory -Path $resolvedTemporaryRoot | Out-Null
try {
    Copy-Item -LiteralPath $binary -Destination (Join-Path $resolvedTemporaryRoot 'ntiv.exe')
    foreach ($file in @('install.ps1', 'uninstall.ps1', 'install-ubuntu-mono.ps1', 'README.md', 'INSTALL-WINDOWS.md', 'CHANGELOG.md', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination (Join-Path $resolvedTemporaryRoot $file)
    }
    $terminalDirectory = Join-Path $resolvedTemporaryRoot 'windows-terminal'
    New-Item -ItemType Directory -Path $terminalDirectory | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'windows-terminal\ntiv.json') -Destination (Join-Path $terminalDirectory 'ntiv.json')

    if (Test-Path -LiteralPath $archive) {
        Remove-Item -LiteralPath $archive -Force
    }
    Compress-Archive -Path (Join-Path $resolvedTemporaryRoot '*') -DestinationPath $archive -CompressionLevel Optimal
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    Set-Content -LiteralPath "$archive.sha256.txt" -Value "$hash  $(Split-Path -Leaf $archive)" -Encoding ascii
    Write-Host "Package: $archive"
    Write-Host "SHA256:  $hash"
}
finally {
    if (Test-Path -LiteralPath $resolvedTemporaryRoot) {
        Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force
    }
}
