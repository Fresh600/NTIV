mod app;
mod archives;
mod cli;
mod content_search;
mod filesystem;
mod jobs;
mod launcher;
mod operations;
mod plugins;
mod preview;
mod search;
mod state;
mod theme;
mod tree;
mod tui;
mod ui;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;

use app::{App, Screen};
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let launch = resolve_launch(cli.path)?;
    let app = App::new(launch.directory, launch.screen, launch.selected)?;

    tui::install_panic_hook();
    let mut terminal = tui::TerminalSession::enter().context("failed to initialize terminal")?;
    tui::set_background(app.theme().background);
    app::run(&mut terminal.terminal, app)
}

struct Launch {
    screen: Screen,
    directory: PathBuf,
    selected: Option<PathBuf>,
}

fn resolve_launch(path: Option<PathBuf>) -> Result<Launch> {
    let cwd = std::env::current_dir().context("could not determine the current directory")?;

    let Some(path) = path else {
        let directory = dirs::desktop_dir()
            .filter(|path| path.is_dir())
            .unwrap_or(cwd);
        return Ok(Launch {
            screen: Screen::Dashboard,
            directory,
            selected: None,
        });
    };

    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };

    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }

    if path.is_file() {
        let parent = path
            .parent()
            .map(PathBuf::from)
            .context("the selected file has no parent directory")?;
        Ok(Launch {
            screen: Screen::Explorer,
            directory: parent,
            selected: Some(path),
        })
    } else if path.is_dir() {
        Ok(Launch {
            screen: Screen::Explorer,
            directory: path,
            selected: None,
        })
    } else {
        bail!("unsupported path type: {}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn no_path_opens_dashboard() {
        let launch = resolve_launch(None).unwrap();
        assert_eq!(launch.screen, Screen::Dashboard);
        if let Some(desktop) = dirs::desktop_dir().filter(|path| path.is_dir()) {
            assert_eq!(launch.directory, desktop);
        }
    }

    #[test]
    fn file_path_opens_its_parent_and_selects_the_file() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("selected file.txt");
        File::create(&file).unwrap();

        let launch = resolve_launch(Some(file.clone())).unwrap();
        assert_eq!(launch.screen, Screen::Explorer);
        assert_eq!(launch.directory, temp.path());
        assert_eq!(launch.selected, Some(file));
    }
}
