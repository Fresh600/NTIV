[CmdletBinding()]
param(
    [string]$BinaryPath
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot

if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
    $portable = Join-Path $projectRoot 'ntiv.exe'
    $built = Join-Path $projectRoot 'dist\ntiv.exe'
    if (Test-Path -LiteralPath $portable) {
        $BinaryPath = $portable
    }
    elseif (Test-Path -LiteralPath $built) {
        $BinaryPath = $built
    }
    else {
        throw 'ntiv.exe was not found. Run .\build-release.ps1 first.'
    }
}

$resolvedBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
$installDirectory = Join-Path $env:LOCALAPPDATA 'Programs\NTIV'
$installedBinary = Join-Path $installDirectory 'ntiv.exe'
New-Item -ItemType Directory -Force -Path $installDirectory | Out-Null
Copy-Item -LiteralPath $resolvedBinary -Destination $installedBinary -Force

# Install a dedicated Windows Terminal profile without modifying existing profiles.
$fragmentSource = Join-Path $projectRoot 'windows-terminal\ntiv.json'
if (Test-Path -LiteralPath $fragmentSource) {
    $fragmentDirectory = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows Terminal\Fragments\NTIV'
    New-Item -ItemType Directory -Force -Path $fragmentDirectory | Out-Null
    Copy-Item -LiteralPath $fragmentSource -Destination (Join-Path $fragmentDirectory 'ntiv.json') -Force
}

# Windows Terminal owns these shortcuts by default. Release them so NTIV can
# receive its internal tab and theme-picker keys. Keep a one-time backup.
$terminalSettingsCandidates = @(
    (Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'),
    (Join-Path $env:LOCALAPPDATA 'Microsoft\Windows Terminal\settings.json')
)
foreach ($terminalSettings in $terminalSettingsCandidates) {
    if (-not (Test-Path -LiteralPath $terminalSettings)) { continue }
    $backup = "$terminalSettings.ntiv-backup"
    if (-not (Test-Path -LiteralPath $backup)) {
        Copy-Item -LiteralPath $terminalSettings -Destination $backup
    }
    $settings = Get-Content -LiteralPath $terminalSettings -Raw | ConvertFrom-Json
    if (-not $settings.PSObject.Properties['keybindings']) {
        $settings | Add-Member -NotePropertyName keybindings -NotePropertyValue @()
    }
    $reservedKeys = @('ctrl+shift+t', 'ctrl+tab', 'ctrl+shift+tab', 'ctrl+alt+t')
    $bindings = @($settings.keybindings | Where-Object { $_.keys -notin $reservedKeys })
    $bindings += [pscustomobject]@{ id = 'unbound'; keys = 'ctrl+shift+t' }
    $bindings += [pscustomobject]@{ id = 'unbound'; keys = 'ctrl+tab' }
    $bindings += [pscustomobject]@{ id = 'unbound'; keys = 'ctrl+shift+tab' }
    $bindings += [pscustomobject]@{ id = 'Terminal.NewTab'; keys = 'ctrl+alt+t' }
    $settings.keybindings = $bindings
    $settings | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $terminalSettings -Encoding utf8
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
$alreadyPresent = $pathEntries | Where-Object { $_.TrimEnd('\') -ieq $installDirectory.TrimEnd('\') }
if (-not $alreadyPresent) {
    $newPath = (($pathEntries + $installDirectory) -join ';')
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
}

if (-not (($env:Path -split ';') -contains $installDirectory)) {
    $env:Path = "$env:Path;$installDirectory"
}

Write-Host "Installed NTIV to $installedBinary"
Write-Host 'Added an optional NTIV profile to Windows Terminal.'
Write-Host 'Configured Ctrl+Tab for NTIV tabs and Ctrl+Shift+T for NTIV themes.'
Write-Host 'Open a new terminal and run: ntiv'
