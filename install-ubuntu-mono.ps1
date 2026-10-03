[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$fontUri = 'https://raw.githubusercontent.com/google/fonts/main/ufl/ubuntumono/UbuntuMono-Regular.ttf'
$fontDirectory = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
$fontPath = Join-Path $fontDirectory 'UbuntuMono-Regular.ttf'
$fontRegistry = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts'
$fontRegistryName = 'Ubuntu Mono Regular (TrueType)'

New-Item -ItemType Directory -Force -Path $fontDirectory | Out-Null

if (-not (Test-Path -LiteralPath $fontPath)) {
    Invoke-WebRequest -Uri $fontUri -OutFile $fontPath -UseBasicParsing
}

$fontBytes = [System.IO.File]::ReadAllBytes($fontPath)
if ($fontBytes.Length -lt 100000 -or
    $fontBytes[0] -ne 0 -or
    $fontBytes[1] -ne 1 -or
    $fontBytes[2] -ne 0 -or
    $fontBytes[3] -ne 0) {
    throw 'The downloaded Ubuntu Mono file is not a valid TrueType font.'
}

New-Item -Path $fontRegistry -Force | Out-Null
New-ItemProperty -Path $fontRegistry -Name $fontRegistryName -Value $fontPath -PropertyType String -Force | Out-Null

if (-not ('NtivFontInstaller.NativeMethods' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;

namespace NtivFontInstaller {
    public static class NativeMethods {
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
        public static extern int AddFontResourceEx(string fileName, uint flags, IntPtr reserved);

        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr SendMessageTimeout(
            IntPtr window, uint message, IntPtr wParam, string lParam,
            uint flags, uint timeout, out IntPtr result);
    }
}
'@
}

[void][NtivFontInstaller.NativeMethods]::AddFontResourceEx($fontPath, 0, [IntPtr]::Zero)
$broadcast = [IntPtr]0xffff
$fontChange = 0x001d
$sendAbortIfHung = 0x0002
$result = [IntPtr]::Zero
[void][NtivFontInstaller.NativeMethods]::SendMessageTimeout(
    $broadcast, $fontChange, [IntPtr]::Zero, $null,
    $sendAbortIfHung, 1000, [ref]$result)

Write-Host "Installed Ubuntu Mono for the current user: $fontPath"
Write-Host 'Close and reopen Windows Terminal so the new font is detected.'
