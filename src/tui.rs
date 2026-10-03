use std::io::{self, Stdout, Write};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, style::Color};

pub type NtivTerminal = Terminal<CrosstermBackend<Stdout>>;

const APP_BACKGROUND: &str = "\x1b]11;rgb:18/17/1c\x1b\\";
const RESET_BACKGROUND: &str = "\x1b]111\x1b\\";

pub struct TerminalSession {
    pub terminal: NtivTerminal,
}

impl TerminalSession {
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        write!(stdout, "{APP_BACKGROUND}")?;
        stdout.flush()?;
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        terminal.clear()?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let _ = write!(self.terminal.backend_mut(), "{RESET_BACKGROUND}");
        let _ = self.terminal.backend_mut().flush();
        let _ = self.terminal.show_cursor();
    }
}

pub fn restore_terminal() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen, Show);
    let _ = write!(stdout, "{RESET_BACKGROUND}");
    let _ = stdout.flush();
}

pub fn set_background(color: Color) {
    let Color::Rgb(red, green, blue) = color else {
        return;
    };
    let mut stdout = io::stdout();
    let _ = write!(stdout, "\x1b]11;rgb:{red:02x}/{green:02x}/{blue:02x}\x1b\\");
    let _ = stdout.flush();
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();
        previous(panic_info);
    }));
}
