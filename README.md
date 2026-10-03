# NTIV

**Navigate Windows at the speed of keys.**

NTIV 0.5 is a keyboard-first file workspace for Windows Terminal. It combines a
LazyVim-inspired dashboard with Vim navigation, tabs, split browsing, file and content
search, background file jobs, Visual selection, previews, favorites, an application launcher,
Git status, text comparison, themes, plugins, and an optional mouse learning mode.

The default Sunset theme uses a neutral charcoal background (`#18171C`), a
lime-to-orange banner, warm text, and `#FFA770` for the active selection. NTIV paints
the entire compatible terminal background while it is open and restores it on exit.

## Start

```powershell
ntiv                 # dashboard; Open Desktop is the default action
ntiv .               # browse the current directory
ntiv C:\Projects     # browse a directory
ntiv .\README.md     # reveal and preview a file
```

The live clock appears at the lower-right of the status line, like a Neovim statusline.
The current location is shown as a readable breadcrumb such as
`C: › Users › Name › Desktop`.

## Essential keyboard controls

| Key | Action |
| --- | --- |
| `j` / `k` or arrows | Move down / up |
| `h` / Backspace | Go to the parent directory |
| `l` / Enter | Enter a directory or open a file |
| Tab / Shift+Tab | Move between tree, files, and split panels |
| `/` | Live filter in the current directory |
| `f` | Recursive fuzzy file search below the current directory |
| `F` | Search text inside files; Enter opens the match at its line |
| `e` | Open in `NTIV_EDITOR`, then `EDITOR`, or Notepad |
| `?` | Open the full in-app help; scroll with `j`/`k` or Page Up/Down |
| Esc | Close a popup; from Explorer return to the dashboard |
| `q` | Quit NTIV from normal mode |

## File operations

| Key | Action |
| --- | --- |
| Space | Mark or unmark multiple files; in the tree, expand/collapse |
| `v`, then `j` / `k` | Select a continuous range in Visual mode |
| `y` | Copy marked files, or the selected item |
| `x` | Cut marked files, or the selected item |
| `p` | Paste into the active directory |
| `u` | Undo the last copy, move, rename, or create operation |
| `n` | Create an item, then choose `F` for file or `D` for directory |
| `r` | Rename the selected item |
| `d` | Compare exactly two marked text files |
| `D` | Move the selection to the Windows Recycle Bin after confirmation |

Copy, move, undo, and Recycle Bin jobs run in the background. Their progress appears
in the status line and `Esc` requests a safe cancellation. Paste never overwrites an existing item: NTIV creates a safe name such as
`report (copy 1).txt`. Deletion is available from the ADMIN right-click menu, always
requires confirmation, and uses the Windows Recycle Bin. Recycle Bin deletion is not
part of the `u` undo history.

## Workspace tools

| Key | Action |
| --- | --- |
| Ctrl+P | Search the command palette |
| Ctrl+T / Ctrl+W | Open / close a tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| `[` / `]` | Alternative previous / next tab keys |
| `s` | Toggle a side-by-side folder split |
| `b` | Add/remove the current folder from Favorites |
| `a` | Search and launch Start Menu applications |
| `g` | Show Git status for the current directory |
| `t` | Cycle the active theme |
| Ctrl+Shift+T | Open the visual theme picker |
| `z` | Open configured plugins |
| `:` | Commands: `q`, `help`, `cd <path>`, `open`, `grep <text>`, `admin` |

Recent folders and Favorites can also be opened directly from the dashboard. State is
saved for the current user in `%APPDATA%\NTIV\state.json`.

## Mouse learning mode

Press `:`, type `admin`, press Enter, and enter the NTIV password. The password is
masked. Once unlocked, click to select, double-click to open, use the wheel to move,
and right-click a file for Open, Edit, New File, New Folder, Rename, Copy, Cut, Paste,
Copy Path, Reveal in Explorer, Open Terminal, and Recycle Bin.

This is an NTIV learning mode, not Windows elevation: it does not grant Administrator
privileges. Hebrew and mixed right-to-left names are reordered only for terminal
display; their original names remain intact for search and filesystem operations.

## Preview

Text and Markdown previews are size-limited and syntax-colored. Images show type,
dimensions, and metadata; PDFs show metadata and an estimated page count. Directories
show basic information. Preview errors stay inside the preview panel and do not close
NTIV.

ZIP files open with Enter like read-only folders. Their contents are extracted into a
safe NTIV cache with path-traversal and expansion-size limits. Press Backspace at the
archive root to return to the original ZIP.

## Custom themes and plugins

Copy `examples\themes.json` to `%APPDATA%\NTIV\themes.json`, edit its hex colors,
restart NTIV, then press `t` to reach the added theme.

The six built-in themes are Sunset, Midnight, Mono, Candy, Meadow, and Lavender.
Candy uses a white background with `#FF7FBA`, Meadow uses `#FFEEAD` with `#96CEB4`,
and Lavender uses `#EBE1EF` with `#8A5BD6`. Press Ctrl+Shift+T to preview their
banner colors and apply one with Enter.

Copy `examples\plugins.json` to `%APPDATA%\NTIV\plugins.json`. Each entry has a
display name, executable command, and optional argument list. Press `z` to run one in
the current folder. Plugin commands are trusted local configuration and run with the
same Windows permissions as NTIV.

## Build and install

### WinGet

After the package is accepted into the Windows Package Manager community repository,
install NTIV for the current user with:

```powershell
winget install --id Fresh600.NTIV --exact
```

Upgrade later releases with:

```powershell
winget upgrade --id Fresh600.NTIV --exact
```

Until the community submission is merged, use the release ZIP from the
[GitHub Releases](https://github.com/Fresh600/NTIV/releases) page and follow
`INSTALL-WINDOWS.md`.

### Build from source

Requirements: Windows 11 x64, current stable Rust, and Microsoft Visual C++ Build
Tools.

```powershell
.\build-release.ps1
.\install.ps1
```

The portable executable is created at `dist\ntiv.exe`. The installer copies it to
`%LOCALAPPDATA%\Programs\NTIV`, adds that directory to the current user's `PATH`, and
installs the optional **NTIV** Windows Terminal profile with zero padding and Ubuntu
Mono. Open a new terminal after installation.

Ubuntu Mono can be installed for the current user from the official Google Fonts
repository with:

```powershell
.\install-ubuntu-mono.ps1
```

To remove NTIV:

```powershell
.\uninstall.ps1
```

## Safety notes

- Junctions and symbolic links are not recursively copied.
- Name conflicts are resolved without overwriting existing files.
- File-changing keyboard commands are available in normal mode; ADMIN only enables
  mouse interaction and the right-click learning menu.
- Search stays below the current directory and skips inaccessible paths.
