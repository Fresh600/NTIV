# Install NTIV on another Windows 11 computer

NTIV 0.5.0 supports 64-bit Windows 11 and Windows Terminal.

1. Extract the complete NTIV ZIP file.
2. Open PowerShell inside the extracted folder.
3. Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

4. Close and reopen Windows Terminal.
5. Run `ntiv`, or choose the **NTIV** profile in Windows Terminal.

The installation is per-user, does not require Administrator privileges, installs to
`%LOCALAPPDATA%\Programs\NTIV`, and adds that directory to the user's `PATH`.

Ubuntu Mono is optional. To install it for the current user:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\install-ubuntu-mono.ps1
```

To uninstall:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\uninstall.ps1
```

For security, compare the ZIP SHA-256 value with the value supplied by the publisher
before running the installer.
