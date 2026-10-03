use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use sha2::{Digest, Sha256};

use crate::{
    archives,
    content_search::{self, ContentMatch, ContentSearchMessage, ContentSearchTask},
    filesystem::{FileEntry, read_entries},
    jobs::{self, FileJobTask, JobMessage, JobProgress, JobResult},
    launcher::{self, LauncherApp},
    operations::{self, ClipboardKind, FileClipboard, UndoAction},
    plugins::{self, PluginDefinition},
    preview::{self, PreviewBody, PreviewResponse},
    search::{self, SearchMessage, SearchTask},
    state::{self, SavedState},
    theme::Theme,
    tree::TreeState,
    tui::NtivTerminal,
    ui,
};

const SEARCH_RESULT_LIMIT: usize = 500;
const ADMIN_PASSWORD_HASH: [u8; 32] = [
    0xD3, 0xAC, 0xBD, 0x0F, 0x29, 0x0B, 0xFD, 0x94, 0x16, 0x83, 0xFD, 0xD1, 0xBD, 0x67, 0x2E, 0x8C,
    0x96, 0x58, 0x44, 0xCB, 0x67, 0x1F, 0x3B, 0xFA, 0x47, 0xBC, 0x1B, 0x02, 0x00, 0xEB, 0xE0, 0x38,
];
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);
const DASHBOARD_ITEM_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Explorer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
    Filter,
    Find,
    ContentSearch,
    Command,
    AdminPassword,
    ContextMenu,
    Rename,
    ConfirmDelete,
    CreateKind,
    CreateName,
    Locations,
    Palette,
    Launcher,
    GitStatus,
    Compare,
    Plugins,
    Themes,
    Help,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Visual => "VISUAL",
            Self::Filter => "FILTER",
            Self::Find => "FIND",
            Self::ContentSearch => "GREP",
            Self::Command => "COMMAND",
            Self::AdminPassword => "ADMIN",
            Self::ContextMenu => "ACTIONS",
            Self::Rename => "RENAME",
            Self::ConfirmDelete => "CONFIRM",
            Self::CreateKind => "NEW",
            Self::CreateName => "CREATE",
            Self::Locations => "LOCATIONS",
            Self::Palette => "PALETTE",
            Self::Launcher => "LAUNCHER",
            Self::GitStatus => "GIT",
            Self::Compare => "COMPARE",
            Self::Plugins => "PLUGINS",
            Self::Themes => "THEMES",
            Self::Help => "HELP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Files,
    Preview,
    Split,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Tree => Self::Files,
            Self::Files => Self::Preview,
            Self::Preview => Self::Tree,
            Self::Split => Self::Tree,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Tree => Self::Preview,
            Self::Files => Self::Tree,
            Self::Preview => Self::Files,
            Self::Split => Self::Files,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandDefinition {
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    Open,
    Edit,
    NewFile,
    NewFolder,
    Rename,
    Copy,
    Cut,
    Paste,
    CopyPath,
    Reveal,
    OpenTerminal,
    Delete,
    Cancel,
}

impl ContextAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Edit => "Open in editor",
            Self::NewFile => "New file",
            Self::NewFolder => "New folder",
            Self::Rename => "Rename",
            Self::Copy => "Copy",
            Self::Cut => "Cut",
            Self::Paste => "Paste here",
            Self::CopyPath => "Copy full path",
            Self::Reveal => "Show in File Explorer",
            Self::OpenTerminal => "Open terminal here",
            Self::Delete => "Move to Recycle Bin",
            Self::Cancel => "Cancel",
        }
    }
}

pub const CONTEXT_ACTIONS: [ContextAction; 13] = [
    ContextAction::Open,
    ContextAction::Edit,
    ContextAction::NewFile,
    ContextAction::NewFolder,
    ContextAction::Rename,
    ContextAction::Copy,
    ContextAction::Cut,
    ContextAction::Paste,
    ContextAction::CopyPath,
    ContextAction::Reveal,
    ContextAction::OpenTerminal,
    ContextAction::Delete,
    ContextAction::Cancel,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    OpenDesktop,
    Recent,
    Favorites,
    NewFile,
    NewFolder,
    ToggleFavorite,
    FindFiles,
    LaunchApplication,
    CycleTheme,
    Plugins,
    Help,
    Quit,
}

impl PaletteAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenDesktop => "Open Desktop",
            Self::Recent => "Open recent folders",
            Self::Favorites => "Open favorites",
            Self::NewFile => "Create a new file",
            Self::NewFolder => "Create a new folder",
            Self::ToggleFavorite => "Toggle current folder favorite",
            Self::FindFiles => "Find files recursively",
            Self::LaunchApplication => "Find and launch an application",
            Self::CycleTheme => "Switch color theme",
            Self::Plugins => "Run a configured plugin",
            Self::Help => "Show keyboard help",
            Self::Quit => "Quit NTIV",
        }
    }
}

pub const PALETTE_ACTIONS: [PaletteAction; 12] = [
    PaletteAction::OpenDesktop,
    PaletteAction::Recent,
    PaletteAction::Favorites,
    PaletteAction::NewFile,
    PaletteAction::NewFolder,
    PaletteAction::ToggleFavorite,
    PaletteAction::FindFiles,
    PaletteAction::LaunchApplication,
    PaletteAction::CycleTheme,
    PaletteAction::Plugins,
    PaletteAction::Help,
    PaletteAction::Quit,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewItemKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Copy)]
pub struct ContextMenuState {
    pub x: u16,
    pub y: u16,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy)]
struct MouseClick {
    at: Instant,
    focus: Focus,
    index: usize,
}

#[derive(Debug, Clone)]
struct ArchiveSession {
    source: PathBuf,
    root: PathBuf,
}

pub struct App {
    pub screen: Screen,
    pub mode: Mode,
    resume_mode: Mode,
    pub focus: Focus,
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub filter_input: String,
    pub command_input: String,
    pub password_input: String,
    pub rename_input: String,
    pub mouse_enabled: bool,
    pub marked: HashSet<PathBuf>,
    visual_anchor: Option<usize>,
    pub file_clipboard: Option<FileClipboard>,
    undo_stack: Vec<UndoAction>,
    file_job: Option<FileJobTask>,
    pub job_progress: Option<JobProgress>,
    job_is_cut: bool,
    pending_delete: Vec<PathBuf>,
    pub create_input: String,
    pub new_item_kind: Option<NewItemKind>,
    pub saved_state: SavedState,
    pub location_title: String,
    pub location_items: Vec<PathBuf>,
    pub location_selected: usize,
    pub palette_input: String,
    pub palette_selected: usize,
    pub launcher_input: String,
    pub launcher_apps: Vec<LauncherApp>,
    pub launcher_selected: usize,
    pub tabs: Vec<PathBuf>,
    pub active_tab: usize,
    archive_sessions: Vec<ArchiveSession>,
    pub split_dir: Option<PathBuf>,
    pub split_entries: Vec<FileEntry>,
    pub split_selected: usize,
    pub git_lines: Vec<String>,
    pub compare_lines: Vec<String>,
    pub overlay_scroll: u16,
    pub help_scroll: u16,
    pub plugins: Vec<PluginDefinition>,
    pub plugin_selected: usize,
    pub themes: Vec<(String, Theme)>,
    pub theme_selected: usize,
    pub context_menu: Option<ContextMenuState>,
    pub context_target: Option<PathBuf>,
    pub tree: TreeState,
    pub preview_title: String,
    pub preview_body: PreviewBody,
    pub preview_scroll: u16,
    pub preview_path: Option<PathBuf>,
    preview_generation: u64,
    preview_sender: Sender<PreviewResponse>,
    preview_receiver: Receiver<PreviewResponse>,
    search_task: Option<SearchTask>,
    pub search_index: Vec<PathBuf>,
    pub search_results: Vec<PathBuf>,
    pub search_selected: usize,
    pub search_done: bool,
    pub search_skipped: usize,
    content_search_task: Option<ContentSearchTask>,
    pub content_query: String,
    pub content_results: Vec<ContentMatch>,
    pub content_selected: usize,
    pub content_done: bool,
    pub content_skipped: usize,
    pub content_limited: bool,
    pub status: String,
    pub dashboard_selected: usize,
    pub should_quit: bool,
    pub commands: Vec<CommandDefinition>,
    last_click: Option<MouseClick>,
}

impl App {
    pub fn new(directory: PathBuf, screen: Screen, initial: Option<PathBuf>) -> Result<Self> {
        let directory = dunce::canonicalize(&directory).unwrap_or(directory);
        let initial = initial.map(|path| dunce::canonicalize(&path).unwrap_or(path));
        let entries = read_entries(&directory)
            .with_context(|| format!("could not read directory: {}", directory.display()))?;
        let selected = initial
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| entry.path == *path))
            .unwrap_or(0);
        let tree = TreeState::new(&directory);
        let (preview_sender, preview_receiver) = mpsc::channel();
        let mut saved_state = if cfg!(test) {
            SavedState::default()
        } else {
            state::load()
        };
        if screen == Screen::Explorer {
            saved_state.record_recent(directory.clone());
        }

        let themes = if cfg!(test) {
            Theme::builtins()
        } else {
            Theme::load_all()
        };
        let theme_selected = saved_state.theme_index.min(themes.len().saturating_sub(1));
        let mut app = Self {
            screen,
            mode: Mode::Normal,
            resume_mode: Mode::Normal,
            focus: Focus::Files,
            current_dir: directory.clone(),
            entries,
            selected,
            filter_input: String::new(),
            command_input: String::new(),
            password_input: String::new(),
            rename_input: String::new(),
            mouse_enabled: false,
            marked: HashSet::new(),
            visual_anchor: None,
            file_clipboard: None,
            undo_stack: Vec::new(),
            file_job: None,
            job_progress: None,
            job_is_cut: false,
            pending_delete: Vec::new(),
            create_input: String::new(),
            new_item_kind: None,
            saved_state,
            location_title: String::new(),
            location_items: Vec::new(),
            location_selected: 0,
            palette_input: String::new(),
            palette_selected: 0,
            launcher_input: String::new(),
            launcher_apps: Vec::new(),
            launcher_selected: 0,
            tabs: vec![directory.clone()],
            active_tab: 0,
            archive_sessions: Vec::new(),
            split_dir: None,
            split_entries: Vec::new(),
            split_selected: 0,
            git_lines: Vec::new(),
            compare_lines: Vec::new(),
            overlay_scroll: 0,
            help_scroll: 0,
            plugins: if cfg!(test) {
                Vec::new()
            } else {
                plugins::load()
            },
            plugin_selected: 0,
            themes,
            theme_selected,
            context_menu: None,
            context_target: None,
            tree,
            preview_title: "Preview".into(),
            preview_body: PreviewBody::Metadata(vec!["Select an item to preview it.".into()]),
            preview_scroll: 0,
            preview_path: None,
            preview_generation: 0,
            preview_sender,
            preview_receiver,
            search_task: None,
            search_index: Vec::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_done: false,
            search_skipped: 0,
            content_search_task: None,
            content_query: String::new(),
            content_results: Vec::new(),
            content_selected: 0,
            content_done: false,
            content_skipped: 0,
            content_limited: false,
            status: "Ready".into(),
            dashboard_selected: 0,
            should_quit: false,
            commands: vec![
                CommandDefinition {
                    name: "q",
                    description: "quit",
                },
                CommandDefinition {
                    name: "help",
                    description: "show help",
                },
                CommandDefinition {
                    name: "cd <path>",
                    description: "change folder",
                },
                CommandDefinition {
                    name: "open",
                    description: "open selected",
                },
                CommandDefinition {
                    name: "grep <text>",
                    description: "search inside files",
                },
                CommandDefinition {
                    name: "admin",
                    description: "enable mouse",
                },
            ],
            last_click: None,
        };
        if app.screen == Screen::Explorer {
            app.request_preview_for_selection();
        }
        Ok(app)
    }

    pub fn filtered_entries(&self) -> Vec<&FileEntry> {
        if self.filter_input.is_empty() {
            return self.entries.iter().collect();
        }
        let mut ranked: Vec<(i64, &FileEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                search::fuzzy_score(&entry.name, &self.filter_input).map(|score| (score, entry))
            })
            .collect();
        ranked.sort_by(|left, right| {
            right
                .1
                .is_dir
                .cmp(&left.1.is_dir)
                .then_with(|| right.0.cmp(&left.0))
                .then_with(|| left.1.name.cmp(&right.1.name))
        });
        ranked.into_iter().map(|(_, entry)| entry).collect()
    }

    pub fn visible_entry_count(&self) -> usize {
        self.filtered_entries().len()
    }

    pub fn pending_delete_count(&self) -> usize {
        self.pending_delete.len()
    }

    pub fn active_path(&self) -> Option<PathBuf> {
        if self.mode == Mode::Find {
            return self.search_results.get(self.search_selected).cloned();
        }
        if self.mode == Mode::ContentSearch {
            return self
                .content_results
                .get(self.content_selected)
                .map(|result| result.path.clone());
        }
        match self.focus {
            Focus::Tree => self.tree.selected_path().map(PathBuf::from),
            Focus::Files => self
                .filtered_entries()
                .get(self.selected)
                .map(|entry| entry.path.clone()),
            Focus::Preview => self.preview_path.clone(),
            Focus::Split => self
                .split_entries
                .get(self.split_selected)
                .map(|entry| entry.path.clone()),
        }
    }

    pub fn display_location(&self) -> String {
        if let Some(session) = self
            .archive_sessions
            .iter()
            .find(|session| self.current_dir.starts_with(&session.root))
        {
            let archive = session.source.display().to_string();
            let relative = self
                .current_dir
                .strip_prefix(&session.root)
                .ok()
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| {
                    path.components()
                        .map(|component| component.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" › ")
                });
            return relative.map_or_else(
                || format!("{archive} (ZIP)"),
                |relative| format!("{archive} (ZIP) › {relative}"),
            );
        }
        self.current_dir
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join(" › ")
    }

    pub fn poll_background(&mut self) {
        while let Ok(response) = self.preview_receiver.try_recv() {
            if response.generation == self.preview_generation {
                self.preview_title = response.title;
                self.preview_body = response.body;
                self.preview_path = Some(response.path);
            }
        }

        let mut changed = false;
        let mut finished = None;
        if let Some(task) = &self.search_task {
            while let Ok(message) = task.receiver.try_recv() {
                match message {
                    SearchMessage::Batch(paths) => {
                        self.search_index.extend(paths);
                        changed = true;
                    }
                    SearchMessage::Done { skipped } => finished = Some(skipped),
                }
            }
        }
        if let Some(skipped) = finished {
            self.search_done = true;
            self.search_skipped = skipped;
            self.search_task = None;
            changed = true;
        }
        if changed {
            self.refresh_search_results();
        }

        let mut content_finished = None;
        if let Some(task) = &self.content_search_task {
            while let Ok(message) = task.receiver.try_recv() {
                match message {
                    ContentSearchMessage::Batch(matches) => {
                        self.content_results.extend(matches);
                    }
                    ContentSearchMessage::Done { skipped, limited } => {
                        content_finished = Some((skipped, limited));
                    }
                }
            }
        }
        if let Some((skipped, limited)) = content_finished {
            self.content_done = true;
            self.content_skipped = skipped;
            self.content_limited = limited;
            self.content_search_task = None;
            self.status = if limited {
                format!(
                    "Content search reached {} results",
                    self.content_results.len()
                )
            } else {
                format!(
                    "Content search found {} matches",
                    self.content_results.len()
                )
            };
        }

        let mut job_finished = None;
        if let Some(task) = &self.file_job {
            while let Ok(message) = task.receiver.try_recv() {
                match message {
                    JobMessage::Progress(progress) => self.job_progress = Some(progress),
                    JobMessage::Finished(result) => job_finished = Some(result),
                }
            }
        }
        if let Some(result) = job_finished {
            self.file_job = None;
            self.job_progress = None;
            match result {
                Ok(JobResult::Paste(action)) => {
                    self.undo_stack.push(action);
                    if self.job_is_cut {
                        self.file_clipboard = None;
                    }
                    self.marked.clear();
                    self.visual_anchor = None;
                    self.status = "File operation complete — press u to undo".into();
                }
                Ok(JobResult::Undo) => self.status = "Last file action undone".into(),
                Ok(JobResult::Delete(count)) => {
                    self.marked.clear();
                    self.pending_delete.clear();
                    self.status = format!("Moved {count} item(s) to the Recycle Bin");
                }
                Ok(JobResult::Cancelled) => self.status = "File operation cancelled safely".into(),
                Err(error) => self.status = format!("File operation failed: {error}"),
            }
            self.job_is_cut = false;
            self.reload_entries(None);
            self.reload_split_entries();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc
            && let Some(task) = &self.file_job
        {
            task.cancel();
            self.status = "Cancelling file operation safely…".into();
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if let Some(task) = &self.file_job {
                task.cancel();
                self.status = "Cancelling file operation safely…".into();
                return;
            }
            self.should_quit = true;
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('p')
            && matches!(self.mode, Mode::Normal)
        {
            self.palette_input.clear();
            self.palette_selected = 0;
            self.mode = Mode::Palette;
            return;
        }
        if key
            .modifiers
            .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
            && matches!(self.mode, Mode::Normal)
        {
            self.open_theme_picker();
            return;
        }

        match self.mode {
            Mode::Help => self.handle_help_key(key),
            Mode::Visual => self.handle_visual_key(key),
            Mode::Filter => self.handle_filter_key(key),
            Mode::Find => self.handle_find_key(key),
            Mode::ContentSearch => self.handle_content_search_key(key),
            Mode::Command => self.handle_command_key(key),
            Mode::AdminPassword => self.handle_admin_password_key(key),
            Mode::ContextMenu => self.handle_context_menu_key(key),
            Mode::Rename => self.handle_rename_key(key),
            Mode::ConfirmDelete => self.handle_delete_confirmation_key(key),
            Mode::CreateKind => self.handle_create_kind_key(key),
            Mode::CreateName => self.handle_create_name_key(key),
            Mode::Locations => self.handle_locations_key(key),
            Mode::Palette => self.handle_palette_key(key),
            Mode::Launcher => self.handle_launcher_key(key),
            Mode::GitStatus | Mode::Compare => self.handle_text_overlay_key(key),
            Mode::Plugins => self.handle_plugins_key(key),
            Mode::Themes => self.handle_themes_key(key),
            Mode::Normal if self.screen == Screen::Dashboard => self.handle_dashboard_key(key),
            Mode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_visual_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                self.visual_anchor = None;
                self.mode = Mode::Normal;
                self.status = format!("{} item(s) selected", self.marked.len());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                self.update_visual_range();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                self.update_visual_range();
            }
            KeyCode::Char('y') => {
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                self.set_file_clipboard(ClipboardKind::Copy);
            }
            KeyCode::Char('x') => {
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                self.set_file_clipboard(ClipboardKind::Cut);
            }
            KeyCode::Char('D') => self.begin_delete_selected_or_marked(),
            _ => {}
        }
    }

    fn handle_content_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_content_search(),
            KeyCode::Enter => {
                if let Some(result) = self.content_results.get(self.content_selected).cloned() {
                    self.cancel_content_search_task();
                    self.mode = Mode::Normal;
                    self.edit_path_at_line(&result.path, result.line);
                }
            }
            KeyCode::Backspace => {
                self.content_query.pop();
                self.restart_content_search();
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_content_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_content_selection(1),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.content_query.push(character);
                self.restart_content_search();
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => self.mode = self.resume_mode,
            KeyCode::Up | KeyCode::Char('k') => {
                self.help_scroll = self.help_scroll.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.help_scroll = self.help_scroll.saturating_add(1)
            }
            KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
            KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
            _ => {}
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.dashboard_selected = self.dashboard_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.dashboard_selected =
                    (self.dashboard_selected + 1).min(DASHBOARD_ITEM_COUNT - 1)
            }
            KeyCode::Enter => self.activate_dashboard_item(),
            KeyCode::Char('o') => self.open_explorer(),
            KeyCode::Char('f') => {
                self.open_explorer();
                self.enter_find();
            }
            KeyCode::Char('r') => self.open_recent(),
            KeyCode::Char('b') => self.open_favorites(),
            KeyCode::Char('a') => self.enter_launcher(),
            KeyCode::Char('?') => self.show_help(),
            KeyCode::Char(':') => {
                self.command_input.clear();
                self.mode = Mode::Command;
            }
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }

    fn activate_dashboard_item(&mut self) {
        match self.dashboard_selected {
            0 => self.open_explorer(),
            1 => self.open_recent(),
            2 => self.open_favorites(),
            3 => {
                self.open_explorer();
                self.enter_find();
            }
            4 => self.show_help(),
            5 => self.should_quit = true,
            _ => {}
        }
    }

    fn open_explorer(&mut self) {
        self.screen = Screen::Explorer;
        self.mode = Mode::Normal;
        self.status = "Explorer ready".into();
        self.record_recent(self.current_dir.clone());
        self.request_preview_for_selection();
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter_input.clear();
                self.mode = Mode::Normal;
                self.selected = self.selected.min(self.entries.len().saturating_sub(1));
                self.request_preview_for_selection();
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.activate_selected();
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
                self.selection_after_query_change();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.filter_input.push(character);
                self.selection_after_query_change();
            }
            _ => {}
        }
    }

    fn handle_find_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.cancel_search();
                self.mode = Mode::Normal;
                self.request_preview_for_selection();
            }
            KeyCode::Enter => {
                let path = self.search_results.get(self.search_selected).cloned();
                self.cancel_search();
                self.mode = Mode::Normal;
                if let Some(path) = path {
                    self.open_path(path);
                }
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
                self.refresh_search_results();
            }
            KeyCode::Up => self.move_search_selection(-1),
            KeyCode::Down => self.move_search_selection(1),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.filter_input.push(character);
                self.refresh_search_results();
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.command_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let command = std::mem::take(&mut self.command_input);
                self.mode = Mode::Normal;
                self.execute_command(&command);
            }
            KeyCode::Backspace => {
                self.command_input.pop();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.command_input.push(character)
            }
            _ => {}
        }
    }

    fn handle_admin_password_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.password_input.clear();
                self.mode = Mode::Normal;
                self.status = "Admin unlock cancelled".into();
            }
            KeyCode::Enter => {
                let password = std::mem::take(&mut self.password_input);
                if Sha256::digest(password.as_bytes()).as_slice() == ADMIN_PASSWORD_HASH {
                    self.mouse_enabled = true;
                    self.mode = Mode::Normal;
                    self.status = "Mouse learning mode enabled".into();
                } else {
                    self.status = "Incorrect password — try again or press Esc".into();
                }
            }
            KeyCode::Backspace => {
                self.password_input.pop();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.password_input.push(character)
            }
            _ => {}
        }
    }

    fn handle_context_menu_key(&mut self, key: KeyEvent) {
        let Some(menu) = self.context_menu.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_context_menu(),
            KeyCode::Up | KeyCode::Char('k') => {
                menu.selected = menu.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                menu.selected = (menu.selected + 1).min(CONTEXT_ACTIONS.len() - 1);
            }
            KeyCode::Enter => self.activate_context_action(),
            _ => {}
        }
    }

    fn handle_rename_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.close_context_menu(),
            KeyCode::Enter => self.commit_rename(),
            KeyCode::Backspace => {
                self.rename_input.pop();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.rename_input.push(character);
            }
            _ => {}
        }
    }

    fn handle_delete_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.commit_delete(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.close_context_menu(),
            _ => {}
        }
    }

    fn handle_create_kind_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('f') | KeyCode::Char('F') => self.begin_create(NewItemKind::File),
            KeyCode::Char('d') | KeyCode::Char('D') => self.begin_create(NewItemKind::Folder),
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "Create cancelled".into();
            }
            _ => {}
        }
    }

    fn handle_create_name_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.create_input.clear();
                self.new_item_kind = None;
                self.mode = Mode::Normal;
                self.status = "Create cancelled".into();
            }
            KeyCode::Enter => self.commit_create(),
            KeyCode::Backspace => {
                self.create_input.pop();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.create_input.push(character);
            }
            _ => {}
        }
    }

    fn handle_locations_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.location_items.clear();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.location_selected = self.location_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.location_selected =
                    (self.location_selected + 1).min(self.location_items.len().saturating_sub(1))
            }
            KeyCode::Enter => {
                if let Some(path) = self.location_items.get(self.location_selected).cloned() {
                    self.mode = Mode::Normal;
                    self.open_explorer();
                    self.navigate_to(path);
                    self.location_items.clear();
                }
            }
            _ => {}
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.palette_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Up => self.palette_selected = self.palette_selected.saturating_sub(1),
            KeyCode::Down => {
                self.palette_selected =
                    (self.palette_selected + 1).min(self.palette_matches().len().saturating_sub(1))
            }
            KeyCode::Backspace => {
                self.palette_input.pop();
                self.palette_selected = 0;
            }
            KeyCode::Enter => {
                if let Some(action) = self.palette_matches().get(self.palette_selected).copied() {
                    self.palette_input.clear();
                    self.mode = Mode::Normal;
                    self.execute_palette_action(action);
                }
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.palette_input.push(character);
                self.palette_selected = 0;
            }
            _ => {}
        }
    }

    fn handle_launcher_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.launcher_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Up => self.launcher_selected = self.launcher_selected.saturating_sub(1),
            KeyCode::Down => {
                self.launcher_selected = (self.launcher_selected + 1)
                    .min(self.launcher_matches().len().saturating_sub(1))
            }
            KeyCode::Backspace => {
                self.launcher_input.pop();
                self.launcher_selected = 0;
            }
            KeyCode::Enter => {
                let app = self
                    .launcher_matches()
                    .get(self.launcher_selected)
                    .map(|app| (*app).clone());
                if let Some(app) = app {
                    match open_default_silently(&app.path) {
                        Ok(()) => {
                            self.status = format!("Launched {}", app.name);
                            self.mode = Mode::Normal;
                        }
                        Err(error) => self.status = format!("Could not launch app: {error}"),
                    }
                }
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.launcher_input.push(character);
                self.launcher_selected = 0;
            }
            _ => {}
        }
    }

    fn handle_text_overlay_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.overlay_scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.overlay_scroll = self.overlay_scroll.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.overlay_scroll = self.overlay_scroll.saturating_add(1)
            }
            _ => {}
        }
    }

    fn handle_plugins_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Up | KeyCode::Char('k') => {
                self.plugin_selected = self.plugin_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.plugin_selected =
                    (self.plugin_selected + 1).min(self.plugins.len().saturating_sub(1))
            }
            KeyCode::Enter => self.run_selected_plugin(),
            _ => {}
        }
    }

    fn handle_themes_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Up | KeyCode::Char('k') => {
                self.theme_selected = self.theme_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.theme_selected =
                    (self.theme_selected + 1).min(self.themes.len().saturating_sub(1))
            }
            KeyCode::Enter => self.select_theme(self.theme_selected),
            _ => {}
        }
    }

    pub fn launcher_matches(&self) -> Vec<&LauncherApp> {
        let query = self.launcher_input.trim();
        let mut matches = self
            .launcher_apps
            .iter()
            .filter_map(|app| {
                if query.is_empty() {
                    Some((0, app))
                } else {
                    search::fuzzy_score(&app.name, query).map(|score| (score, app))
                }
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|item| std::cmp::Reverse(item.0));
        matches.into_iter().map(|(_, app)| app).collect()
    }

    fn enter_launcher(&mut self) {
        if self.launcher_apps.is_empty() {
            self.launcher_apps = launcher::discover();
        }
        self.launcher_input.clear();
        self.launcher_selected = 0;
        if self.launcher_apps.is_empty() {
            self.status = "No Start Menu applications were found".into();
        } else {
            self.mode = Mode::Launcher;
        }
    }

    pub fn palette_matches(&self) -> Vec<PaletteAction> {
        let query = self.palette_input.trim();
        let mut matches = PALETTE_ACTIONS
            .iter()
            .filter_map(|action| {
                if query.is_empty() {
                    Some((0, *action))
                } else {
                    search::fuzzy_score(action.label(), query).map(|score| (score, *action))
                }
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|item| std::cmp::Reverse(item.0));
        matches.into_iter().map(|(_, action)| action).collect()
    }

    fn execute_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::OpenDesktop => {
                if let Some(desktop) = dirs::desktop_dir() {
                    self.open_explorer();
                    self.navigate_to(desktop);
                }
            }
            PaletteAction::Recent => self.open_recent(),
            PaletteAction::Favorites => self.open_favorites(),
            PaletteAction::NewFile => {
                self.open_explorer();
                self.begin_create(NewItemKind::File);
            }
            PaletteAction::NewFolder => {
                self.open_explorer();
                self.begin_create(NewItemKind::Folder);
            }
            PaletteAction::ToggleFavorite => self.toggle_favorite(),
            PaletteAction::FindFiles => {
                self.open_explorer();
                self.enter_find();
            }
            PaletteAction::LaunchApplication => self.enter_launcher(),
            PaletteAction::CycleTheme => self.cycle_theme(),
            PaletteAction::Plugins => self.open_plugins(),
            PaletteAction::Help => self.show_help(),
            PaletteAction::Quit => self.should_quit = true,
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('t') => self.new_tab(),
                KeyCode::Char('w') => self.close_tab(),
                KeyCode::Tab => self.switch_tab(1),
                KeyCode::BackTab => self.switch_tab(-1),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Tab => {
                self.focus = if self.split_dir.is_some() {
                    match self.focus {
                        Focus::Tree => Focus::Files,
                        Focus::Files | Focus::Preview => Focus::Split,
                        Focus::Split => Focus::Tree,
                    }
                } else {
                    self.focus.next()
                };
                self.request_preview_for_selection();
            }
            KeyCode::BackTab => {
                self.focus = if self.split_dir.is_some() {
                    match self.focus {
                        Focus::Tree => Focus::Split,
                        Focus::Files => Focus::Tree,
                        Focus::Split | Focus::Preview => Focus::Files,
                    }
                } else {
                    self.focus.previous()
                };
                self.request_preview_for_selection();
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Backspace => self.navigate_parent(),
            KeyCode::Left | KeyCode::Char('h') => match self.focus {
                Focus::Tree => {
                    self.tree.collapse_or_parent();
                    self.request_preview_for_selection();
                }
                Focus::Files => self.navigate_parent(),
                Focus::Preview => self.preview_scroll = self.preview_scroll.saturating_sub(1),
                Focus::Split => self.navigate_split_parent(),
            },
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.activate_selected(),
            KeyCode::Char(' ') => match self.focus {
                Focus::Tree => {
                    self.tree.toggle_selected();
                    self.request_preview_for_selection();
                }
                Focus::Files => self.toggle_marked(),
                Focus::Preview => {}
                Focus::Split => self.toggle_marked(),
            },
            KeyCode::Char('/') => {
                self.filter_input.clear();
                self.selected = 0;
                self.mode = Mode::Filter;
            }
            KeyCode::Char('f') => self.enter_find(),
            KeyCode::Char('F') => self.enter_content_search(String::new()),
            KeyCode::Char('v') | KeyCode::Char('V') => self.enter_visual_mode(),
            KeyCode::Char('e') => self.edit_selected(),
            KeyCode::Char('y') => self.set_file_clipboard(ClipboardKind::Copy),
            KeyCode::Char('x') => self.set_file_clipboard(ClipboardKind::Cut),
            KeyCode::Char('p') => self.paste_file_clipboard(),
            KeyCode::Char('u') => self.undo_last_action(),
            KeyCode::Char('n') => self.mode = Mode::CreateKind,
            KeyCode::Char('r') => self.begin_rename_selected(),
            KeyCode::Char('b') => self.toggle_favorite(),
            KeyCode::Char('a') => self.enter_launcher(),
            KeyCode::Char('t') => self.cycle_theme(),
            KeyCode::Char('z') => self.open_plugins(),
            KeyCode::Char('s') => self.toggle_split(),
            KeyCode::Char('g') => self.show_git_status(),
            KeyCode::Char('d') => self.compare_marked_files(),
            KeyCode::Char('D') => self.begin_delete_selected_or_marked(),
            KeyCode::Char('[') => self.switch_tab(-1),
            KeyCode::Char(']') => self.switch_tab(1),
            KeyCode::Char(':') => {
                self.command_input.clear();
                self.mode = Mode::Command;
            }
            KeyCode::Char('?') => self.show_help(),
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                self.screen = Screen::Dashboard;
                self.focus = Focus::Files;
                self.status = "Dashboard".into();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Tree => self.tree.move_selection(delta),
            Focus::Files => {
                let count = self.visible_entry_count();
                if count == 0 {
                    self.selected = 0;
                } else {
                    self.selected = self.selected.saturating_add_signed(delta).min(count - 1);
                }
            }
            Focus::Preview => {
                self.preview_scroll = if delta.is_negative() {
                    self.preview_scroll
                        .saturating_sub(delta.unsigned_abs() as u16)
                } else {
                    self.preview_scroll.saturating_add(delta as u16)
                }
            }
            Focus::Split => {
                let count = self.split_entries.len();
                if count == 0 {
                    self.split_selected = 0;
                } else {
                    self.split_selected = self
                        .split_selected
                        .saturating_add_signed(delta)
                        .min(count - 1);
                }
            }
        }
        self.request_preview_for_selection();
    }

    fn move_search_selection(&mut self, delta: isize) {
        if self.search_results.is_empty() {
            self.search_selected = 0;
        } else {
            self.search_selected = self
                .search_selected
                .saturating_add_signed(delta)
                .min(self.search_results.len() - 1);
        }
        self.request_preview_for_selection();
    }

    fn move_content_selection(&mut self, delta: isize) {
        if self.content_results.is_empty() {
            self.content_selected = 0;
        } else {
            self.content_selected = self
                .content_selected
                .saturating_add_signed(delta)
                .min(self.content_results.len() - 1);
        }
        self.request_preview_for_selection();
    }

    fn enter_visual_mode(&mut self) {
        if self.focus != Focus::Files || self.visible_entry_count() == 0 {
            self.status = "Visual selection is available in the Files panel".into();
            return;
        }
        self.marked.clear();
        self.visual_anchor = Some(self.selected);
        self.mode = Mode::Visual;
        self.update_visual_range();
    }

    fn update_visual_range(&mut self) {
        let Some(anchor) = self.visual_anchor else {
            return;
        };
        let start = anchor.min(self.selected);
        let end = anchor.max(self.selected);
        self.marked = self
            .filtered_entries()
            .into_iter()
            .enumerate()
            .filter(|(index, _)| *index >= start && *index <= end)
            .map(|(_, entry)| entry.path.clone())
            .collect();
        self.status = format!("VISUAL · {} item(s)", self.marked.len());
    }

    fn selection_after_query_change(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_entry_count().saturating_sub(1));
        self.request_preview_for_selection();
    }

    fn activate_selected(&mut self) {
        if self.focus == Focus::Tree {
            if let Some(path) = self.tree.selected_path().map(PathBuf::from) {
                self.navigate_to(path);
            }
            return;
        }
        if self.focus == Focus::Preview {
            return;
        }
        if self.focus == Focus::Split {
            if let Some(path) = self.active_path() {
                if path.is_dir() {
                    self.navigate_split_to(path);
                } else {
                    self.open_path(path);
                }
            }
            return;
        }
        if let Some(path) = self.active_path() {
            self.open_path(path);
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.navigate_to(path);
            return;
        }
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        {
            match archives::extract_for_browsing(&path) {
                Ok(root) => {
                    let root = dunce::canonicalize(&root).unwrap_or(root);
                    self.archive_sessions.push(ArchiveSession {
                        source: path.clone(),
                        root: root.clone(),
                    });
                    self.navigate_to(root);
                    self.saved_state
                        .recent
                        .retain(|recent| recent != &self.current_dir);
                    self.save_state();
                    self.status = format!(
                        "Browsing {} as a read-only folder",
                        path.file_name()
                            .map(|name| name.to_string_lossy())
                            .unwrap_or_default()
                    );
                }
                Err(error) => self.status = format!("Could not open ZIP: {error}"),
            }
            return;
        }
        match open_default_silently(&path) {
            Ok(()) => self.status = format!("Opened {}", path.display()),
            Err(error) => self.status = format!("Could not open file: {error}"),
        }
    }

    fn edit_selected(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = "Nothing selected".into();
            return;
        };
        if path.is_dir() {
            self.status = "Select a file to edit".into();
            return;
        }
        let editor = std::env::var("NTIV_EDITOR")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "notepad.exe".into());
        let mut command = Command::new(&editor);
        command.arg(&path);
        match spawn_silently(&mut command) {
            Ok(_) => self.status = format!("Opened in {editor}"),
            Err(error) => self.status = format!("Could not start {editor}: {error}"),
        }
    }

    fn navigate_parent(&mut self) {
        if let Some(session) = self
            .archive_sessions
            .iter()
            .find(|session| self.current_dir == session.root)
            .cloned()
        {
            if let Some(parent) = session.source.parent().map(PathBuf::from) {
                self.navigate_to(parent);
                self.reload_entries(Some(session.source));
                self.status = "Closed ZIP view".into();
            }
            return;
        }
        if let Some(parent) = self.current_dir.parent().map(PathBuf::from) {
            self.navigate_to(parent);
        } else {
            self.status = "Already at the filesystem root".into();
        }
    }

    fn navigate_to(&mut self, directory: PathBuf) {
        let directory = dunce::canonicalize(&directory).unwrap_or(directory);
        match read_entries(&directory) {
            Ok(entries) => {
                self.current_dir = directory;
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    *tab = self.current_dir.clone();
                }
                self.entries = entries;
                self.selected = 0;
                self.filter_input.clear();
                self.tree.sync_to_path(&self.current_dir);
                self.status = format!("{} items", self.entries.len());
                self.marked.clear();
                self.record_recent(self.current_dir.clone());
                self.request_preview_for_selection();
            }
            Err(error) => {
                self.status = format!("Cannot read {}: {error}", directory.display());
            }
        }
    }

    fn execute_command(&mut self, input: &str) {
        let input = input.trim();
        let normalized = input.to_ascii_lowercase();
        if matches!(normalized.as_str(), "q" | "quit") {
            self.should_quit = true;
        } else if normalized == "help" {
            self.show_help();
        } else if normalized == "open" {
            self.activate_selected();
        } else if normalized == "grep" {
            self.enter_content_search(String::new());
        } else if let Some(query) = input.strip_prefix("grep ") {
            self.enter_content_search(query.trim().to_string());
        } else if normalized == "admin" {
            if self.mouse_enabled {
                self.status = "Mouse learning mode is already enabled".into();
            } else {
                self.password_input.clear();
                self.mode = Mode::AdminPassword;
                self.status = "Enter the NTIV admin password".into();
            }
        } else if let Some(path) = input.strip_prefix("cd ") {
            let path = path.trim().trim_matches('"');
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                self.current_dir.join(path)
            };
            if path.is_dir() {
                self.navigate_to(path);
            } else {
                self.status = "cd expects an existing directory".into();
            }
        } else if input.is_empty() {
            self.status = "Command cancelled".into();
        } else {
            self.status = format!("Unknown command: {input}");
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, width: u16, height: u16) {
        if !self.mouse_enabled || width < 60 || height < 18 {
            return;
        }

        if self.mode == Mode::ContextMenu {
            self.handle_context_menu_mouse(mouse, width, height);
            return;
        }
        if self.mode != Mode::Normal {
            return;
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => self.mouse_scroll(-3),
            MouseEventKind::ScrollDown => self.mouse_scroll(3),
            MouseEventKind::Down(MouseButton::Left) => {
                if self.screen == Screen::Dashboard {
                    self.dashboard_click(mouse.row, height);
                } else {
                    self.explorer_click(mouse.column, mouse.row, width, height);
                }
            }
            MouseEventKind::Down(MouseButton::Right) if self.screen == Screen::Explorer => {
                self.open_context_menu_at(mouse.column, mouse.row, width, height)
            }
            _ => {}
        }
    }

    fn mouse_scroll(&mut self, delta: isize) {
        if self.screen == Screen::Dashboard {
            let next = self
                .dashboard_selected
                .saturating_add_signed(delta.signum());
            self.dashboard_selected = next.min(DASHBOARD_ITEM_COUNT - 1);
        } else {
            self.move_selection(delta);
        }
    }

    fn dashboard_click(&mut self, row: u16, height: u16) {
        let start_y = height.saturating_sub(18) / 2;
        let start_y = start_y.saturating_sub(2);
        let first_item_row = start_y + 9;
        if (first_item_row..first_item_row + DASHBOARD_ITEM_COUNT as u16).contains(&row) {
            self.dashboard_selected = usize::from(row - first_item_row);
            self.activate_dashboard_item();
        }
    }

    fn explorer_click(&mut self, column: u16, row: u16, width: u16, height: u16) {
        let main_height = height.saturating_sub(2);
        if row == 0 || row >= main_height.saturating_sub(1) {
            return;
        }

        let tree_width = if width >= 100 {
            width.saturating_mul(25) / 100
        } else {
            width.saturating_mul(35) / 100
        };
        let file_end = if width >= 100 {
            width.saturating_mul(70) / 100
        } else {
            width
        };
        let visible_rows = usize::from(main_height.saturating_sub(2));

        if column < tree_width {
            self.focus = Focus::Tree;
            if let Some(index) =
                clicked_list_index(row, self.tree.items.len(), self.tree.selected, visible_rows)
            {
                self.tree.selected = index;
                if self.is_double_click(Focus::Tree, index) {
                    self.activate_selected();
                } else {
                    self.request_preview_for_selection();
                }
            }
        } else if column < file_end {
            self.focus = Focus::Files;
            let count = self.visible_entry_count();
            if let Some(index) = clicked_list_index(row, count, self.selected, visible_rows) {
                self.selected = index;
                if self.is_double_click(Focus::Files, index) {
                    self.activate_selected();
                } else {
                    self.request_preview_for_selection();
                }
            }
        } else if width >= 100 {
            self.focus = Focus::Preview;
        }
    }

    fn is_double_click(&mut self, focus: Focus, index: usize) -> bool {
        let now = Instant::now();
        let double = self.last_click.is_some_and(|click| {
            click.focus == focus
                && click.index == index
                && now.duration_since(click.at) <= DOUBLE_CLICK_WINDOW
        });
        self.last_click = if double {
            None
        } else {
            Some(MouseClick {
                at: now,
                focus,
                index,
            })
        };
        double
    }

    fn open_context_menu_at(&mut self, column: u16, row: u16, width: u16, height: u16) {
        let main_height = height.saturating_sub(2);
        if row == 0 || row >= main_height.saturating_sub(1) {
            return;
        }
        let tree_width = if width >= 100 {
            width.saturating_mul(25) / 100
        } else {
            width.saturating_mul(35) / 100
        };
        let file_end = if width >= 100 {
            width.saturating_mul(70) / 100
        } else {
            width
        };
        if column < tree_width || column >= file_end {
            return;
        }
        self.focus = Focus::Files;
        let count = self.visible_entry_count();
        let visible_rows = usize::from(main_height.saturating_sub(2));
        let Some(index) = clicked_list_index(row, count, self.selected, visible_rows) else {
            return;
        };
        self.selected = index;
        self.request_preview_for_selection();
        self.context_target = self.active_path();
        self.context_menu = Some(ContextMenuState {
            x: column,
            y: row,
            selected: 0,
        });
        self.mode = Mode::ContextMenu;
    }

    fn handle_context_menu_mouse(&mut self, mouse: MouseEvent, width: u16, height: u16) {
        let MouseEventKind::Down(MouseButton::Left) = mouse.kind else {
            return;
        };
        let Some(menu) = self.context_menu else {
            self.mode = Mode::Normal;
            return;
        };
        let menu_width = 30_u16.min(width);
        let menu_height = CONTEXT_ACTIONS.len() as u16 + 2;
        let x = menu.x.min(width.saturating_sub(menu_width));
        let y = menu.y.min(height.saturating_sub(menu_height));
        if mouse.column > x
            && mouse.column < x + menu_width - 1
            && mouse.row > y
            && mouse.row < y + menu_height - 1
        {
            if let Some(menu) = self.context_menu.as_mut() {
                menu.selected = usize::from(mouse.row - y - 1);
            }
            self.activate_context_action();
        } else {
            self.close_context_menu();
        }
    }

    fn activate_context_action(&mut self) {
        let Some(menu) = self.context_menu else {
            self.mode = Mode::Normal;
            return;
        };
        match CONTEXT_ACTIONS[menu.selected] {
            ContextAction::Open => {
                if let Some(path) = self.context_target.clone() {
                    self.close_context_menu();
                    self.open_path(path);
                }
            }
            ContextAction::Edit => {
                if let Some(path) = self.context_target.clone() {
                    self.close_context_menu();
                    self.edit_path(&path);
                }
            }
            ContextAction::NewFile => self.begin_create(NewItemKind::File),
            ContextAction::NewFolder => self.begin_create(NewItemKind::Folder),
            ContextAction::Rename => {
                let Some(path) = self.context_target.as_ref() else {
                    self.close_context_menu();
                    return;
                };
                if !self.target_can_be_changed(path) {
                    self.status = "The current folder or its parent cannot be renamed here".into();
                    self.close_context_menu();
                    return;
                }
                self.rename_input = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.mode = Mode::Rename;
            }
            ContextAction::Copy => {
                self.set_file_clipboard(ClipboardKind::Copy);
                self.close_context_menu_preserving_status();
            }
            ContextAction::Cut => {
                self.set_file_clipboard(ClipboardKind::Cut);
                self.close_context_menu_preserving_status();
            }
            ContextAction::Paste => {
                self.close_context_menu_preserving_status();
                self.paste_file_clipboard();
            }
            ContextAction::CopyPath => self.copy_target_path(),
            ContextAction::Reveal => self.reveal_target(),
            ContextAction::OpenTerminal => self.open_terminal_for_target(),
            ContextAction::Delete => {
                let Some(path) = self.context_target.as_ref() else {
                    self.close_context_menu();
                    return;
                };
                if self.target_can_be_changed(path) {
                    self.pending_delete = vec![path.clone()];
                    self.mode = Mode::ConfirmDelete;
                } else {
                    self.status = "The current folder or its parent cannot be deleted here".into();
                    self.close_context_menu();
                }
            }
            ContextAction::Cancel => self.close_context_menu(),
        }
    }

    fn edit_path(&mut self, path: &Path) {
        if path.is_dir() {
            self.status = "Select a file to edit".into();
            return;
        }
        let editor = std::env::var("NTIV_EDITOR")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "notepad.exe".into());
        let mut command = Command::new(&editor);
        command.arg(path);
        match spawn_silently(&mut command) {
            Ok(_) => self.status = format!("Opened in {editor}"),
            Err(error) => self.status = format!("Could not start {editor}: {error}"),
        }
    }

    fn edit_path_at_line(&mut self, path: &Path, line: usize) {
        if let Some(code) = vscode_executable() {
            let mut command = Command::new(code);
            command
                .arg("--goto")
                .arg(format!("{}:{line}", path.display()));
            match spawn_silently(&mut command) {
                Ok(_) => self.status = format!("Opened match at line {line}"),
                Err(error) => self.status = format!("Could not open match: {error}"),
            }
        } else {
            self.edit_path(path);
        }
    }

    fn copy_target_path(&mut self) {
        let Some(path) = self.context_target.clone() else {
            self.close_context_menu();
            return;
        };
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Set-Clipboard -Value $args[0]",
            ])
            .arg(path.as_os_str());
        let result = silent_command(&mut command).status();
        self.status = match result {
            Ok(status) if status.success() => "Full path copied to clipboard".into(),
            Ok(status) => format!("Clipboard command failed with {status}"),
            Err(error) => format!("Could not copy path: {error}"),
        };
        self.close_context_menu_preserving_status();
    }

    fn reveal_target(&mut self) {
        let Some(path) = self.context_target.clone() else {
            self.close_context_menu();
            return;
        };
        let mut command = Command::new("explorer.exe");
        command.arg(format!("/select,{}", path.display()));
        self.status = match spawn_silently(&mut command) {
            Ok(_) => "Opened File Explorer".into(),
            Err(error) => format!("Could not open File Explorer: {error}"),
        };
        self.close_context_menu_preserving_status();
    }

    fn open_terminal_for_target(&mut self) {
        let Some(path) = self.context_target.clone() else {
            self.close_context_menu();
            return;
        };
        let directory = if path.is_dir() {
            path
        } else {
            path.parent()
                .map(PathBuf::from)
                .unwrap_or(self.current_dir.clone())
        };
        let mut command = Command::new("wt.exe");
        command.arg("-d").arg(directory);
        self.status = match spawn_silently(&mut command) {
            Ok(_) => "Opened a terminal in the selected location".into(),
            Err(error) => format!("Could not open Windows Terminal: {error}"),
        };
        self.close_context_menu_preserving_status();
    }

    fn commit_rename(&mut self) {
        let Some(source) = self.context_target.clone() else {
            self.close_context_menu();
            return;
        };
        let name = self.rename_input.trim();
        if name.is_empty()
            || matches!(name, "." | "..")
            || Path::new(name).components().count() != 1
        {
            self.status = "Enter a single valid file or folder name".into();
            return;
        }
        let Some(parent) = source.parent() else {
            self.status = "This item cannot be renamed".into();
            return;
        };
        let destination = parent.join(name);
        if destination.exists() && destination != source {
            self.status = "An item with that name already exists".into();
            return;
        }
        match fs::rename(&source, &destination) {
            Ok(()) => {
                self.undo_stack.push(UndoAction::Rename {
                    from: destination.clone(),
                    to: source,
                });
                self.status = format!("Renamed to {name}");
                self.close_context_menu_preserving_status();
                if self.focus == Focus::Split {
                    self.reload_split_entries();
                } else {
                    self.reload_entries(Some(destination));
                }
            }
            Err(error) => self.status = format!("Could not rename item: {error}"),
        }
    }

    fn commit_delete(&mut self) {
        if self.pending_delete.is_empty() {
            self.close_context_menu();
            return;
        }
        if self.file_job.is_some() {
            self.status = "A file operation is already running".into();
            return;
        }
        let paths = std::mem::take(&mut self.pending_delete);
        self.context_menu = None;
        self.context_target = None;
        self.mode = Mode::Normal;
        self.job_progress = Some(JobProgress {
            label: "TRASH",
            completed: 0,
            total: paths.len(),
            current: "Preparing…".into(),
        });
        self.file_job = Some(jobs::spawn_delete(paths));
        self.status = "Moving selection to the Recycle Bin…".into();
    }

    fn begin_delete_selected_or_marked(&mut self) {
        let paths = self.selected_or_marked();
        if paths.is_empty() {
            self.status = "Nothing selected".into();
            return;
        }
        if paths.iter().any(|path| !self.target_can_be_changed(path)) {
            self.status = "One or more selected items cannot be deleted here".into();
            return;
        }
        self.pending_delete = paths;
        self.context_target = self.pending_delete.first().cloned();
        self.visual_anchor = None;
        self.mode = Mode::ConfirmDelete;
    }

    fn reload_entries(&mut self, select: Option<PathBuf>) {
        match read_entries(&self.current_dir) {
            Ok(entries) => {
                self.entries = entries;
                self.selected = select
                    .as_ref()
                    .and_then(|path| {
                        self.filtered_entries()
                            .iter()
                            .position(|entry| &entry.path == path)
                    })
                    .unwrap_or_else(|| {
                        self.selected
                            .min(self.visible_entry_count().saturating_sub(1))
                    });
                self.request_preview_for_selection();
            }
            Err(error) => self.status = format!("Could not refresh folder: {error}"),
        }
    }

    fn toggle_marked(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.marked.remove(&path) {
            self.marked.insert(path);
        }
        self.status = format!("{} item(s) selected", self.marked.len());
    }

    fn selected_or_marked(&self) -> Vec<PathBuf> {
        if self.marked.is_empty() {
            self.active_path().into_iter().collect()
        } else {
            self.marked.iter().cloned().collect()
        }
    }

    fn set_file_clipboard(&mut self, kind: ClipboardKind) {
        let paths = self.selected_or_marked();
        if paths.is_empty() {
            self.status = "Nothing selected".into();
            return;
        }
        if kind == ClipboardKind::Cut && paths.iter().any(|path| self.path_is_in_archive(path)) {
            self.status = "ZIP browsing is read-only; copy is available, cut is not".into();
            return;
        }
        let count = paths.len();
        self.file_clipboard = Some(FileClipboard { kind, paths });
        self.status = match kind {
            ClipboardKind::Copy => format!("{count} item(s) ready to copy — press p to paste"),
            ClipboardKind::Cut => format!("{count} item(s) ready to move — press p to paste"),
        };
    }

    fn paste_file_clipboard(&mut self) {
        if self.file_job.is_some() {
            self.status = "A file operation is already running — Esc cancels it".into();
            return;
        }
        let Some(clipboard) = self.file_clipboard.clone() else {
            self.status = "The NTIV clipboard is empty".into();
            return;
        };
        let paste_into_split = self.focus == Focus::Split;
        let destination = if paste_into_split {
            self.split_dir.as_ref().unwrap_or(&self.current_dir)
        } else {
            &self.current_dir
        }
        .clone();
        if self.path_is_in_archive(&destination) {
            self.status = "ZIP browsing is read-only".into();
            return;
        }
        self.job_is_cut = clipboard.kind == ClipboardKind::Cut;
        let label = if self.job_is_cut { "MOVE" } else { "COPY" };
        self.job_progress = Some(JobProgress {
            label,
            completed: 0,
            total: clipboard.paths.len().max(1),
            current: "Scanning selection…".into(),
        });
        self.file_job = Some(jobs::spawn_paste(clipboard, destination));
        self.status = format!("{label} started in background · Esc cancels");
    }

    fn undo_last_action(&mut self) {
        if self.file_job.is_some() {
            self.status = "A file operation is already running — Esc cancels it".into();
            return;
        }
        let Some(action) = self.undo_stack.pop() else {
            self.status = "Nothing to undo".into();
            return;
        };
        self.job_progress = Some(JobProgress {
            label: "UNDO",
            completed: 0,
            total: 1,
            current: "Restoring last action".into(),
        });
        self.file_job = Some(jobs::spawn_undo(action));
        self.status = "Undo running in background · Esc cancels".into();
    }

    fn begin_create(&mut self, kind: NewItemKind) {
        if self.current_location_is_archive() {
            self.status = "ZIP browsing is read-only".into();
            self.mode = Mode::Normal;
            return;
        }
        self.context_menu = None;
        self.context_target = None;
        self.create_input.clear();
        self.new_item_kind = Some(kind);
        self.mode = Mode::CreateName;
        self.status = match kind {
            NewItemKind::File => "Enter a name for the new file".into(),
            NewItemKind::Folder => "Enter a name for the new folder".into(),
        };
    }

    fn commit_create(&mut self) {
        let name = self.create_input.trim();
        if name.is_empty()
            || matches!(name, "." | "..")
            || Path::new(name).components().count() != 1
        {
            self.status = "Enter a single valid name".into();
            return;
        }
        let Some(kind) = self.new_item_kind else {
            self.mode = Mode::Normal;
            return;
        };
        let directory = if self.focus == Focus::Split {
            self.split_dir.as_ref().unwrap_or(&self.current_dir)
        } else {
            &self.current_dir
        };
        match operations::create(directory, name, kind == NewItemKind::Folder) {
            Ok((path, undo)) => {
                self.undo_stack.push(undo);
                self.status = format!("Created {name} — press u to undo");
                self.create_input.clear();
                self.new_item_kind = None;
                self.mode = Mode::Normal;
                if self.focus == Focus::Split {
                    self.reload_split_entries();
                } else {
                    self.reload_entries(Some(path));
                }
            }
            Err(error) => self.status = format!("Could not create item: {error}"),
        }
    }

    fn begin_rename_selected(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = "Nothing selected".into();
            return;
        };
        if !self.target_can_be_changed(&path) {
            self.status = "This folder cannot be renamed here".into();
            return;
        }
        self.context_target = Some(path.clone());
        self.rename_input = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.mode = Mode::Rename;
    }

    fn new_tab(&mut self) {
        let path = dirs::desktop_dir().unwrap_or_else(|| self.current_dir.clone());
        self.tabs.push(path.clone());
        self.active_tab = self.tabs.len() - 1;
        self.navigate_to(path);
        self.status = format!("Opened tab {}", self.active_tab + 1);
    }

    fn close_tab(&mut self) {
        if self.tabs.len() == 1 {
            self.status = "The last tab cannot be closed".into();
            return;
        }
        self.tabs.remove(self.active_tab);
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        let path = self.tabs[self.active_tab].clone();
        self.navigate_to(path);
        self.status = format!("Closed tab · {} tab(s) open", self.tabs.len());
    }

    fn switch_tab(&mut self, delta: isize) {
        if self.tabs.len() < 2 {
            self.status = "Only one tab is open".into();
            return;
        }
        let next = if delta.is_negative() {
            self.active_tab
                .checked_sub(1)
                .unwrap_or(self.tabs.len() - 1)
        } else {
            (self.active_tab + 1) % self.tabs.len()
        };
        self.active_tab = next;
        self.navigate_to(self.tabs[next].clone());
        self.status = format!("Tab {} of {}", next + 1, self.tabs.len());
    }

    fn toggle_split(&mut self) {
        if self.split_dir.is_some() {
            self.split_dir = None;
            self.split_entries.clear();
            self.focus = Focus::Files;
            self.status = "Split view closed".into();
            return;
        }
        let directory = self
            .active_path()
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.current_dir.clone());
        self.navigate_split_to(directory);
    }

    fn navigate_split_to(&mut self, directory: PathBuf) {
        match read_entries(&directory) {
            Ok(entries) => {
                self.split_dir = Some(directory);
                self.split_entries = entries;
                self.split_selected = 0;
                self.focus = Focus::Split;
                self.status = "Split view ready · Tab switches panels".into();
            }
            Err(error) => self.status = format!("Could not open split: {error}"),
        }
    }

    fn reload_split_entries(&mut self) {
        let Some(directory) = self.split_dir.clone() else {
            return;
        };
        match read_entries(&directory) {
            Ok(entries) => {
                self.split_entries = entries;
                self.split_selected = self
                    .split_selected
                    .min(self.split_entries.len().saturating_sub(1));
            }
            Err(error) => self.status = format!("Could not refresh split: {error}"),
        }
    }

    fn navigate_split_parent(&mut self) {
        let parent = self
            .split_dir
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from);
        if let Some(parent) = parent {
            self.navigate_split_to(parent);
        }
    }

    fn show_git_status(&mut self) {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.current_dir)
            .args(["status", "--short", "--branch"])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout);
                self.git_lines = if text.trim().is_empty() {
                    vec!["Working tree clean".into()]
                } else {
                    text.lines().map(str::to_string).collect()
                };
                self.overlay_scroll = 0;
                self.mode = Mode::GitStatus;
            }
            Ok(_) => self.status = "The current folder is not a Git repository".into(),
            Err(error) => self.status = format!("Could not run Git: {error}"),
        }
    }

    fn compare_marked_files(&mut self) {
        let mut paths = self
            .marked
            .iter()
            .filter(|path| path.is_file())
            .cloned()
            .collect::<Vec<_>>();
        paths.sort_by_key(|path| path.to_string_lossy().to_lowercase());
        let mut paths = paths.into_iter();
        let Some(left_path) = paths.next() else {
            self.status = "Select exactly two text files with Space".into();
            return;
        };
        let Some(right_path) = paths.next() else {
            self.status = "Select exactly two text files with Space".into();
            return;
        };
        if paths.next().is_some() {
            self.status = "Select exactly two text files with Space".into();
            return;
        }
        let result = fs::read_to_string(&left_path)
            .and_then(|left| fs::read_to_string(&right_path).map(|right| (left, right)));
        match result {
            Ok((left, right)) => {
                self.compare_lines = build_text_diff(&left_path, &left, &right_path, &right);
                self.overlay_scroll = 0;
                self.mode = Mode::Compare;
            }
            Err(error) => self.status = format!("Could not compare text files: {error}"),
        }
    }

    fn open_recent(&mut self) {
        self.open_locations("RECENT FOLDERS", self.saved_state.recent.clone());
    }

    fn open_favorites(&mut self) {
        self.open_locations("FAVORITES", self.saved_state.favorites.clone());
    }

    fn open_locations(&mut self, title: &str, paths: Vec<PathBuf>) {
        self.location_items = paths.into_iter().filter(|path| path.is_dir()).collect();
        self.location_selected = 0;
        self.location_title = title.into();
        if self.location_items.is_empty() {
            self.status = format!("No {} yet", title.to_ascii_lowercase());
        } else {
            self.mode = Mode::Locations;
        }
    }

    fn toggle_favorite(&mut self) {
        let added = self.saved_state.toggle_favorite(self.current_dir.clone());
        self.status = if added {
            "Current folder added to favorites".into()
        } else {
            "Current folder removed from favorites".into()
        };
        self.save_state();
    }

    fn record_recent(&mut self, path: PathBuf) {
        self.saved_state.record_recent(path);
        self.save_state();
    }

    fn save_state(&mut self) {
        if !cfg!(test)
            && let Err(error) = state::save(&self.saved_state)
        {
            self.status = format!("Could not save NTIV state: {error}");
        }
    }

    pub fn theme(&self) -> Theme {
        self.themes
            .get(self.saved_state.theme_index % self.themes.len().max(1))
            .map(|(_, theme)| *theme)
            .unwrap_or(Theme::SUNSET)
    }

    fn cycle_theme(&mut self) {
        self.saved_state.theme_index =
            (self.saved_state.theme_index + 1) % self.themes.len().max(1);
        crate::tui::set_background(self.theme().background);
        let name = self
            .themes
            .get(self.saved_state.theme_index)
            .map(|(name, _)| name.as_str())
            .unwrap_or("Sunset");
        self.status = format!("Theme: {name}");
        self.save_state();
    }

    fn open_theme_picker(&mut self) {
        self.theme_selected = self
            .saved_state
            .theme_index
            .min(self.themes.len().saturating_sub(1));
        self.mode = Mode::Themes;
    }

    fn select_theme(&mut self, index: usize) {
        if index >= self.themes.len() {
            return;
        }
        self.saved_state.theme_index = index;
        self.theme_selected = index;
        crate::tui::set_background(self.theme().background);
        let name = self.themes[index].0.clone();
        self.status = format!("Theme: {name}");
        self.mode = Mode::Normal;
        self.save_state();
    }

    fn open_plugins(&mut self) {
        self.plugins = plugins::load();
        self.plugin_selected = 0;
        if self.plugins.is_empty() {
            self.status = plugins::config_path()
                .map(|path| format!("No plugins configured · {}", path.display()))
                .unwrap_or_else(|| "No plugins configured".into());
        } else {
            self.mode = Mode::Plugins;
        }
    }

    fn run_selected_plugin(&mut self) {
        let Some(plugin) = self.plugins.get(self.plugin_selected).cloned() else {
            return;
        };
        let mut command = Command::new(&plugin.command);
        command.args(&plugin.args).current_dir(&self.current_dir);
        match spawn_silently(&mut command) {
            Ok(_) => {
                self.status = format!("Started plugin: {}", plugin.name);
                self.mode = Mode::Normal;
            }
            Err(error) => self.status = format!("Plugin failed to start: {error}"),
        }
    }

    fn target_can_be_changed(&self, target: &Path) -> bool {
        !self.path_is_in_archive(target)
            && target != self.current_dir
            && !self.current_dir.starts_with(target)
    }

    fn current_location_is_archive(&self) -> bool {
        self.path_is_in_archive(&self.current_dir)
    }

    fn path_is_in_archive(&self, path: &Path) -> bool {
        self.archive_sessions
            .iter()
            .any(|session| path.starts_with(&session.root))
    }

    fn close_context_menu(&mut self) {
        self.status = "Action cancelled".into();
        self.pending_delete.clear();
        self.close_context_menu_preserving_status();
    }

    fn close_context_menu_preserving_status(&mut self) {
        self.context_menu = None;
        self.context_target = None;
        self.rename_input.clear();
        self.mode = Mode::Normal;
    }

    fn show_help(&mut self) {
        self.resume_mode = self.mode;
        self.help_scroll = 0;
        self.mode = Mode::Help;
    }

    fn enter_find(&mut self) {
        self.cancel_search();
        self.mode = Mode::Find;
        self.focus = Focus::Files;
        self.filter_input.clear();
        self.search_index.clear();
        self.search_results.clear();
        self.search_selected = 0;
        self.search_done = false;
        self.search_skipped = 0;
        self.search_task = Some(search::spawn_search(self.current_dir.clone()));
        self.status = format!("Searching below {}", self.current_dir.display());
    }

    fn enter_content_search(&mut self, query: String) {
        self.cancel_search();
        self.cancel_content_search_task();
        self.mode = Mode::ContentSearch;
        self.focus = Focus::Files;
        self.content_query = query;
        self.content_results.clear();
        self.content_selected = 0;
        self.content_done = false;
        self.content_skipped = 0;
        self.content_limited = false;
        self.restart_content_search();
    }

    fn restart_content_search(&mut self) {
        self.cancel_content_search_task();
        self.content_results.clear();
        self.content_selected = 0;
        self.content_done = false;
        self.content_skipped = 0;
        self.content_limited = false;
        if self.content_query.trim().is_empty() {
            self.status = "Type text to search inside files".into();
            return;
        }
        self.content_search_task = Some(content_search::spawn_content_search(
            self.current_dir.clone(),
            self.content_query.clone(),
        ));
        self.status = format!("Searching file contents for ‘{}’…", self.content_query);
    }

    fn cancel_content_search_task(&mut self) {
        if let Some(task) = self.content_search_task.take() {
            task.cancel();
        }
    }

    fn cancel_content_search(&mut self) {
        self.cancel_content_search_task();
        self.content_query.clear();
        self.content_results.clear();
        self.content_selected = 0;
        self.content_done = false;
        self.mode = Mode::Normal;
        self.status = "Content search closed".into();
        self.request_preview_for_selection();
    }

    fn cancel_search(&mut self) {
        if let Some(task) = self.search_task.take() {
            task.cancel();
        }
        self.search_index.clear();
        self.search_results.clear();
        self.search_selected = 0;
        self.search_done = false;
        self.filter_input.clear();
    }

    fn refresh_search_results(&mut self) {
        self.search_results = search::rank_paths(
            &self.search_index,
            &self.current_dir,
            &self.filter_input,
            SEARCH_RESULT_LIMIT,
        );
        self.search_selected = self
            .search_selected
            .min(self.search_results.len().saturating_sub(1));
        self.request_preview_for_selection();
    }

    fn request_preview_for_selection(&mut self) {
        if self.screen != Screen::Explorer {
            return;
        }
        let Some(path) = self.active_path() else {
            self.preview_path = None;
            self.preview_title = "Preview".into();
            self.preview_body = PreviewBody::Metadata(vec!["No matching item.".into()]);
            return;
        };
        if self.preview_path.as_ref() == Some(&path)
            && !matches!(self.preview_body, PreviewBody::Loading)
        {
            return;
        }
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.preview_scroll = 0;
        self.preview_path = Some(path.clone());
        self.preview_title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.preview_body = PreviewBody::Loading;
        preview::spawn_preview(path, self.preview_generation, self.preview_sender.clone());
    }
}

pub fn run(terminal: &mut NtivTerminal, mut app: App) -> Result<()> {
    loop {
        app.poll_background();
        terminal.draw(|frame| ui::render(frame, &app))?;
        if app.should_quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(40))? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    app.handle_key(key)
                }
                Event::Resize(_, _) => {}
                Event::Mouse(mouse) => {
                    let size = terminal.size()?;
                    app.handle_mouse(mouse, size.width, size.height);
                }
                _ => {}
            }
        }
    }
}

fn clicked_list_index(
    row: u16,
    total: usize,
    selected: usize,
    visible_rows: usize,
) -> Option<usize> {
    if total == 0 || visible_rows == 0 {
        return None;
    }
    let start = selected.saturating_sub(visible_rows.saturating_sub(1));
    let index = start + usize::from(row.saturating_sub(1));
    (index < total).then_some(index)
}

fn build_text_diff(left_path: &Path, left: &str, right_path: &Path, right: &str) -> Vec<String> {
    let left_lines = left.lines().collect::<Vec<_>>();
    let right_lines = right.lines().collect::<Vec<_>>();
    let mut output = vec![
        format!("--- {}", left_path.display()),
        format!("+++ {}", right_path.display()),
        String::new(),
    ];
    for index in 0..left_lines.len().max(right_lines.len()).min(5_000) {
        let left = left_lines.get(index).copied();
        let right = right_lines.get(index).copied();
        if left == right {
            if let Some(line) = left {
                output.push(format!("  {line}"));
            }
        } else {
            if let Some(line) = left {
                output.push(format!("- {line}"));
            }
            if let Some(line) = right {
                output.push(format!("+ {line}"));
            }
        }
    }
    output
}

fn open_default_silently(path: &Path) -> std::io::Result<()> {
    if is_source_file(path)
        && let Some(code) = vscode_executable()
    {
        let mut command = Command::new(code);
        command.arg(path);
        spawn_silently(&mut command)?;
        return Ok(());
    }

    // `start` asks the Windows shell to resolve shortcuts and file associations.
    // Running it through a silent cmd process keeps external application output
    // away from NTIV's alternate screen while still launching GUI applications.
    let mut command = Command::new("cmd.exe");
    command.args(["/D", "/S", "/C", "start", ""]).arg(path);
    let status = silent_command(&mut command).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "Windows shell returned {status}"
        )))
    }
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "py" | "rs"
                    | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "html"
                    | "css"
                    | "json"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "md"
            )
        })
}

fn vscode_executable() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("Microsoft VS Code")
                .join("Code.exe"),
        );
    }
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Microsoft VS Code")
                .join("Code.exe"),
        );
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn spawn_silently(command: &mut Command) -> std::io::Result<std::process::Child> {
    silent_command(command).spawn()
}

fn silent_command(command: &mut Command) -> &mut Command {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::Write,
    };

    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    use super::*;

    #[test]
    fn filtering_clamps_selection_and_matches_fuzzily() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("alpha.txt")).unwrap();
        File::create(temp.path().join("beta.txt")).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.selected = 1;
        app.filter_input = "alp".into();
        app.selection_after_query_change();
        assert_eq!(app.selected, 0);
        assert_eq!(app.filtered_entries()[0].name, "alpha.txt");
    }

    #[test]
    fn visual_mode_selects_a_contiguous_range() {
        let temp = tempdir().unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            File::create(temp.path().join(name)).unwrap();
        }
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

        assert_eq!(app.mode, Mode::Visual);
        assert_eq!(app.marked.len(), 3);
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.file_clipboard.as_ref().unwrap().paths.len(), 3);
    }

    #[test]
    fn uppercase_f_opens_content_search_and_grep_command_starts_it() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::SHIFT));
        assert_eq!(app.mode, Mode::ContentSearch);

        app.cancel_content_search();
        app.execute_command("grep hello");
        assert_eq!(app.mode, Mode::ContentSearch);
        assert_eq!(app.content_query, "hello");
        assert!(app.content_search_task.is_some());
    }

    #[test]
    fn failed_navigation_keeps_current_directory() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        let original = app.current_dir.clone();
        app.navigate_to(temp.path().join("missing"));
        assert_eq!(app.current_dir, original);
        assert!(app.status.starts_with("Cannot read"));
    }

    #[test]
    fn command_cd_supports_spaces() {
        let temp = tempdir().unwrap();
        let child = temp.path().join("folder with spaces");
        fs::create_dir(&child).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.execute_command(&format!("cd \"{}\"", child.display()));
        assert_eq!(app.current_dir, dunce::canonicalize(child).unwrap());
    }

    #[test]
    fn admin_command_unlocks_mouse_with_the_configured_password() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();

        app.execute_command("ADMIN");
        assert_eq!(app.mode, Mode::AdminPassword);
        app.password_input = "omrischapira".into();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.mouse_enabled);
        assert_eq!(app.status, "Mouse learning mode enabled");
    }

    #[test]
    fn wrong_admin_password_stays_in_the_password_prompt() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();

        app.execute_command("admin");
        app.password_input = "wrong".into();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.mode, Mode::AdminPassword);
        assert!(!app.mouse_enabled);
        assert!(app.password_input.is_empty());
    }

    #[test]
    fn dashboard_click_only_works_after_mouse_unlock() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40,
            row: 13,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(click, 80, 30);
        assert_eq!(app.screen, Screen::Dashboard);

        app.mouse_enabled = true;
        app.handle_mouse(click, 80, 30);
        assert_eq!(app.screen, Screen::Explorer);
    }

    #[test]
    fn right_click_opens_actions_for_the_clicked_file() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("notes.txt");
        File::create(&file).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.mouse_enabled = true;

        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: 40,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
            120,
            35,
        );

        assert_eq!(app.mode, Mode::ContextMenu);
        assert_eq!(app.context_target, Some(dunce::canonicalize(file).unwrap()));
        assert!(app.context_menu.is_some());
    }

    #[test]
    fn rename_supports_hebrew_names() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("ישן.txt");
        File::create(&source).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.mode = Mode::Rename;
        app.context_target = Some(source);
        app.context_menu = Some(ContextMenuState {
            x: 0,
            y: 0,
            selected: 2,
        });
        app.rename_input = "חדש.txt".into();

        app.commit_rename();

        assert!(temp.path().join("חדש.txt").exists());
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status.starts_with("Renamed"));
    }

    #[test]
    fn tabs_switch_between_folders() {
        let temp = tempdir().unwrap();
        let second = temp.path().join("second");
        fs::create_dir(&second).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.tabs.push(second.clone());

        app.switch_tab(1);

        assert_eq!(app.active_tab, 1);
        assert_eq!(app.current_dir, dunce::canonicalize(second).unwrap());
    }

    #[test]
    fn control_tab_switches_to_the_next_tab() {
        let temp = tempdir().unwrap();
        let second = temp.path().join("second");
        fs::create_dir(&second).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.tabs.push(second.clone());

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));

        assert_eq!(app.active_tab, 1);
        assert_eq!(app.current_dir, dunce::canonicalize(second).unwrap());
    }

    #[test]
    fn escape_from_explorer_returns_to_dashboard() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(app.screen, Screen::Dashboard);
        assert!(!app.should_quit);
    }

    #[test]
    fn control_shift_t_opens_the_theme_picker() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();

        app.handle_key(KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));

        assert_eq!(app.mode, Mode::Themes);
        assert_eq!(app.themes.len(), 6);
    }

    #[test]
    fn zip_files_open_as_read_only_folders() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("project.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("src/main.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"inside").unwrap();
        writer.finish().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();

        app.open_path(archive_path.clone());

        assert!(app.current_location_is_archive());
        assert!(app.current_dir.join("src/main.txt").is_file());
        app.navigate_parent();
        assert_eq!(app.current_dir, dunce::canonicalize(temp.path()).unwrap());
        assert_eq!(
            app.active_path(),
            Some(dunce::canonicalize(archive_path).unwrap())
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_shell_launcher_starts_shortcut_targets_without_terminal_output() {
        let temp = tempdir().unwrap();
        let marker = temp.path().join("launch result.txt");
        let script = temp.path().join("test launcher.cmd");
        fs::write(
            &script,
            format!("@echo launched>\"{}\"\r\n", marker.display()),
        )
        .unwrap();

        open_default_silently(&script).unwrap();
        for _ in 0..40 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        assert_eq!(fs::read_to_string(marker).unwrap().trim(), "launched");
    }

    #[test]
    fn split_view_opens_the_selected_directory() {
        let temp = tempdir().unwrap();
        let child = temp.path().join("child");
        fs::create_dir(&child).unwrap();
        File::create(child.join("inside.txt")).unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();

        app.toggle_split();

        assert_eq!(
            app.split_dir,
            Some(dunce::canonicalize(child).unwrap_or_else(|_| temp.path().join("child")))
        );
        assert_eq!(app.focus, Focus::Split);
        assert_eq!(app.split_entries.len(), 1);
    }

    #[test]
    fn compare_builds_a_readable_diff_for_two_marked_files() {
        let temp = tempdir().unwrap();
        let left = temp.path().join("left.txt");
        let right = temp.path().join("right.txt");
        fs::write(&left, "same\nold\n").unwrap();
        fs::write(&right, "same\nnew\n").unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        app.marked.insert(left);
        app.marked.insert(right);

        app.compare_marked_files();

        assert_eq!(app.mode, Mode::Compare);
        assert!(app.compare_lines.iter().any(|line| line == "- old"));
        assert!(app.compare_lines.iter().any(|line| line == "+ new"));
    }

    #[test]
    fn command_palette_fuzzy_finds_the_app_launcher() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        app.palette_input = "launch app".into();
        assert_eq!(
            app.palette_matches().first(),
            Some(&PaletteAction::LaunchApplication)
        );
    }

    #[test]
    fn theme_switch_cycles_and_persists_the_index_in_state() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        app.cycle_theme();
        assert_eq!(app.saved_state.theme_index, 1);
        assert_eq!(app.status, "Theme: Midnight");
    }
}
