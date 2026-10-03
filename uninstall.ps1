[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$installDirectory = Join-Path $env:LOCALAPPDATA 'Programs\NTIV'
$resolvedInstall = [System.IO.Path]::GetFullPath($installDirectory)
$expectedRoot = [System.IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs'))

if (-not $resolvedInstall.StartsWith($expectedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove unexpected path: $resolvedInstall"
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = @($userPath -split ';' | Where-Object {
    -not [string]::IsNullOrWhiteSpace($_) -and
    $_.TrimEnd('\') -ine $installDirectory.TrimEnd('\')
})
[Environment]::SetEnvironmentVariable('Path', ($pathEntries -join ';'), 'User')

if (Test-Path -LiteralPath $resolvedInstall) {
    Remove-Item -LiteralPath $resolvedInstall -Recurse -Force
}

$fragmentDirectory = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows Terminal\Fragments\NTIV'
$fragmentFile = Join-Path $fragmentDirectory 'ntiv.json'
if (Test-Path -LiteralPath $fragmentFile) {
    Remove-Item -LiteralPath $fragmentFile -Force
}
if ((Test-Path -LiteralPath $fragmentDirectory) -and
    -not (Get-ChildItem -LiteralPath $fragmentDirectory -Force | Select-Object -First 1)) {
    Remove-Item -LiteralPath $fragmentDirectory
}

$terminalSettingsCandidates = @(
    (Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'),
    (Join-Path $env:LOCALAPPDATA 'Microsoft\Windows Terminal\settings.json')
)
foreach ($terminalSettings in $terminalSettingsCandidates) {
    if (-not (Test-Path -LiteralPath $terminalSettings)) { continue }
    $settings = Get-Content -LiteralPath $terminalSettings -Raw | ConvertFrom-Json
    if ($settings.PSObject.Properties['keybindings']) {
        $settings.keybindings = @($settings.keybindings | Where-Object {
            -not (($_.id -eq 'unbound' -and $_.keys -in @('ctrl+shift+t', 'ctrl+tab', 'ctrl+shift+tab')) -or
                  ($_.id -eq 'Terminal.NewTab' -and $_.keys -eq 'ctrl+alt+t'))
        })
        $settings | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $terminalSettings -Encoding utf8
    }
}

Write-Host 'NTIV was removed from the current user account.'
