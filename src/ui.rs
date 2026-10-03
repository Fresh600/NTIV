use std::path::Path;

use chrono::Local;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{App, CONTEXT_ACTIONS, Focus, Mode, NewItemKind, Screen},
    filesystem::{display_name, human_size},
    preview::PreviewBody,
    theme::Theme,
};
use unicode_bidi::BidiInfo;

const BANNER: [&str; 6] = [
    "███╗   ██╗████████╗██╗██╗   ██╗",
    "████╗  ██║╚══██╔══╝██║██║   ██║",
    "██╔██╗ ██║   ██║   ██║██║   ██║",
    "██║╚██╗██║   ██║   ██║╚██╗ ██╔╝",
    "██║ ╚████║   ██║   ██║ ╚████╔╝ ",
    "╚═╝  ╚═══╝   ╚═╝   ╚═╝  ╚═══╝  ",
];

const DASHBOARD_ITEMS: [(&str, &str); 6] = [
    ("Open Desktop", "o"),
    ("Recent folders", "r"),
    ("Favorites", "b"),
    ("Find file", "f"),
    ("Keyboard help", "?"),
    ("Quit", "q"),
];

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let theme = app.theme();
    let area = frame.area();
    frame.render_widget(Block::default().style(theme.base()), area);

    if area.width < 60 || area.height < 18 {
        render_too_small(frame, area, theme);
        return;
    }

    match app.screen {
        Screen::Dashboard => render_dashboard(frame, area, app, theme),
        Screen::Explorer => render_explorer(frame, area, app, theme),
    }

    match app.mode {
        Mode::Help => render_help(frame, area, app, theme),
        Mode::ContextMenu => render_context_menu(frame, area, app, theme),
        Mode::Rename => render_rename_dialog(frame, area, app, theme),
        Mode::ConfirmDelete => render_delete_confirmation(frame, area, app, theme),
        Mode::CreateKind => render_create_kind_dialog(frame, area, theme),
        Mode::CreateName => render_create_name_dialog(frame, area, app, theme),
        Mode::Locations => render_locations(frame, area, app, theme),
        Mode::Palette => render_palette(frame, area, app, theme),
        Mode::Launcher => render_launcher(frame, area, app, theme),
        Mode::GitStatus => {
            render_text_overlay(frame, area, " GIT STATUS ", &app.git_lines, app, theme)
        }
        Mode::Compare => render_text_overlay(
            frame,
            area,
            " FILE COMPARE ",
            &app.compare_lines,
            app,
            theme,
        ),
        Mode::Plugins => render_plugins(frame, area, app, theme),
        Mode::Themes => render_themes(frame, area, app, theme),
        _ => {}
    }
}

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    // Every dashboard element is positioned from the same centered canvas.
    // Keeping the menu exactly as wide as its rows prevents it from appearing
    // left-shifted in a non-maximized terminal window.
    let content_height = 18;
    let start_y = area.y
        + area
            .height
            .saturating_sub(content_height)
            .saturating_div(2)
            .saturating_sub(2);

    let banner_lines: Vec<Line<'_>> = BANNER
        .iter()
        .map(|line| gradient_line(line, theme.banner, theme.highlight))
        .collect();
    frame.render_widget(
        Paragraph::new(banner_lines).alignment(Alignment::Center),
        Rect::new(area.x, start_y, area.width, 6),
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Navigate Windows at the speed of keys",
            Style::default().fg(theme.muted),
        )))
        .alignment(Alignment::Center),
        Rect::new(area.x, start_y + 7, area.width, 1),
    );

    let menu_width = area.width.min(36);
    let menu_x = area.x + (area.width - menu_width) / 2;
    let menu_lines: Vec<Line<'_>> = DASHBOARD_ITEMS
        .iter()
        .enumerate()
        .map(|(index, (label, key))| {
            let style = if index == app.dashboard_selected {
                theme.selected()
            } else {
                theme.base()
            };
            Line::from(vec![
                Span::styled("  ◆  ", Style::default().fg(theme.highlight)),
                Span::styled(format!("{label:<28}"), style),
                Span::styled(format!(" {key} "), Style::default().fg(theme.hotkey)),
            ])
            .style(style)
        })
        .collect();
    frame.render_widget(
        Paragraph::new(menu_lines),
        Rect::new(menu_x, start_y + 9, menu_width, 6),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  NTIV 0.5  ", Style::default().fg(theme.active)),
            Span::styled("● ", Style::default().fg(theme.highlight)),
            Span::styled(&app.status, Style::default().fg(theme.muted)),
            Span::styled(
                if app.mouse_enabled {
                    "  ·  MOUSE ON"
                } else {
                    ""
                },
                Style::default().fg(theme.highlight),
            ),
        ]))
        .alignment(Alignment::Center),
        Rect::new(area.x, start_y + 17, area.width, 1),
    );

    render_clock(frame, bottom_right(area, 10), theme);
}

fn render_explorer(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    render_breadcrumb(frame, rows[0], app, theme);
    let main = rows[1];

    if app.split_dir.is_some() && area.width >= 100 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(40),
                Constraint::Percentage(40),
            ])
            .split(main);
        render_tree(frame, columns[0], app, theme);
        render_files(frame, columns[1], app, theme);
        render_split(frame, columns[2], app, theme);
    } else if area.width >= 100 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(45),
                Constraint::Percentage(30),
            ])
            .split(main);
        render_tree(frame, columns[0], app, theme);
        render_files(frame, columns[1], app, theme);
        render_preview(frame, columns[2], app, theme);
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(main);
        render_tree(frame, columns[0], app, theme);
        render_files(frame, columns[1], app, theme);
    }

    render_status(frame, rows[2], app, theme);
    render_input(frame, rows[3], app, theme);
}

fn render_breadcrumb(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let path = terminal_text(&app.display_location());
    let tabs = app
        .tabs
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let name = path
                .file_name()
                .map(|name| terminal_text(&name.to_string_lossy()))
                .unwrap_or_else(|| path.display().to_string());
            if index == app.active_tab {
                format!("[{}:{name}]", index + 1)
            } else {
                format!(" {}:{name} ", index + 1)
            }
        })
        .collect::<Vec<_>>()
        .join("");
    let line = Line::from(vec![
        Span::styled(format!(" {path}"), Style::default().fg(theme.foreground)),
        Span::styled(format!("  {tabs}"), Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.base()), area);
}

fn render_tree(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let items: Vec<ListItem<'_>> = app
        .tree
        .items
        .iter()
        .map(|item| {
            let marker = if item.path == app.current_dir {
                "▰"
            } else if item.expanded {
                "▾"
            } else {
                "▸"
            };
            let indent = "  ".repeat(item.depth);
            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(format!("{marker} "), Style::default().fg(theme.highlight)),
                Span::styled(terminal_text(&display_name(&item.path)), theme.base()),
            ]))
        })
        .collect();
    let block = panel_block(" ▰ FOLDERS ", app.focus == Focus::Tree, theme);
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected())
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(Some(app.tree.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_files(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let (title, items, selected) = if app.mode == Mode::ContentSearch {
        let suffix = if app.content_done {
            if app.content_limited {
                " · limit reached"
            } else {
                ""
            }
        } else {
            " · searching…"
        };
        let title = format!(" ⌕ CONTENT · {}{suffix} ", app.content_results.len());
        let items = app
            .content_results
            .iter()
            .map(|result| {
                let relative = result
                    .path
                    .strip_prefix(&app.current_dir)
                    .unwrap_or(&result.path);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(
                            "{}:{}:{} ",
                            terminal_text(&relative.display().to_string()),
                            result.line,
                            result.column
                        ),
                        Style::default().fg(theme.highlight),
                    ),
                    Span::styled(terminal_text(&result.text), theme.base()),
                ]))
            })
            .collect();
        (title, items, app.content_selected)
    } else if app.mode == Mode::Find {
        let title = if app.search_done {
            format!(" ⌕ FIND · {} matches ", app.search_results.len())
        } else {
            format!(" ⌕ FIND · scanning {} files… ", app.search_index.len())
        };
        let items: Vec<ListItem<'_>> = app
            .search_results
            .iter()
            .map(|path| {
                let relative = path.strip_prefix(&app.current_dir).unwrap_or(path);
                ListItem::new(Line::from(vec![
                    Span::styled("• ", Style::default().fg(theme.highlight)),
                    Span::styled(terminal_text(&relative.display().to_string()), theme.base()),
                ]))
            })
            .collect();
        (title, items, app.search_selected)
    } else {
        let entries = app.filtered_entries();
        let title = if app.filter_input.is_empty() {
            format!(" ≡ FILES · {} ", entries.len())
        } else {
            format!(" ≡ FILES · {}/{} ", entries.len(), app.entries.len())
        };
        let items: Vec<ListItem<'_>> = entries
            .into_iter()
            .map(|entry| {
                let icon = if app.marked.contains(&entry.path) {
                    "✓"
                } else if entry.is_dir {
                    "▰"
                } else {
                    "•"
                };
                let size = if entry.is_dir {
                    "<DIR>".into()
                } else {
                    human_size(entry.len)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{icon} "),
                        Style::default().fg(if entry.is_dir {
                            if app.marked.contains(&entry.path) {
                                theme.active
                            } else {
                                theme.highlight
                            }
                        } else {
                            if app.marked.contains(&entry.path) {
                                theme.active
                            } else {
                                theme.muted
                            }
                        }),
                    ),
                    Span::styled(format!("{:<38}", terminal_text(&entry.name)), theme.base()),
                    Span::styled(size, Style::default().fg(theme.muted)),
                ]))
            })
            .collect();
        (title, items, app.selected)
    };

    let list = List::new(items)
        .block(panel_block(&title, app.focus == Focus::Files, theme))
        .highlight_style(theme.selected())
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_preview(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let title = format!(" ◫ PREVIEW · {} ", terminal_text(&app.preview_title));
    let text = match &app.preview_body {
        PreviewBody::Loading => Text::from(Line::from(Span::styled(
            "Loading preview…",
            Style::default().fg(theme.muted),
        ))),
        PreviewBody::Text(lines) => Text::from(
            lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:>4} ", index + 1),
                            Style::default().fg(theme.muted),
                        ),
                        Span::styled(
                            terminal_text(line),
                            preview_text_style(app.preview_path.as_deref(), line, theme),
                        ),
                    ])
                })
                .collect::<Vec<_>>(),
        ),
        PreviewBody::Metadata(lines) => Text::from(
            lines
                .iter()
                .map(|line| Line::from(Span::styled(terminal_text(line), theme.base())))
                .collect::<Vec<_>>(),
        ),
        PreviewBody::Error(error) => Text::from(Line::from(Span::styled(
            terminal_text(error),
            Style::default().fg(theme.hotkey),
        ))),
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(panel_block(&title, app.focus == Focus::Preview, theme))
            .scroll((app.preview_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_split(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let title = app
        .split_dir
        .as_ref()
        .map(|path| format!(" SPLIT · {} ", terminal_text(&path.display().to_string())))
        .unwrap_or_else(|| " SPLIT ".into());
    let items = app
        .split_entries
        .iter()
        .map(|entry| {
            let icon = if app.marked.contains(&entry.path) {
                "✓"
            } else if entry.is_dir {
                "▰"
            } else {
                "•"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(theme.highlight)),
                Span::styled(terminal_text(&entry.name), theme.base()),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.split_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(panel_block(&title, app.focus == Focus::Split, theme))
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        area,
        &mut state,
    );
}

fn render_status(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let (position, total) = selection_progress(app);
    let position_text = format!(" {position}/{total} ");
    let progress_text = if total == 0 {
        " -- ".to_string()
    } else if position <= 1 {
        " TOP ".to_string()
    } else if position >= total {
        " BOT ".to_string()
    } else {
        format!(" {}% ", position.saturating_mul(100) / total)
    };
    let progress_width = progress_text.chars().count() as u16;
    let position_width = position_text.chars().count() as u16;
    let clock_width = area.width.min(10);
    let metrics_width = if area.width >= 54 {
        progress_width.saturating_add(position_width)
    } else {
        0
    };
    let right_width = clock_width.saturating_add(metrics_width);
    let status_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(right_width),
        area.height,
    );
    let metrics_area = Rect::new(
        area.right().saturating_sub(right_width),
        area.y,
        metrics_width,
        area.height,
    );
    let clock_area = Rect::new(
        area.right().saturating_sub(clock_width),
        area.y,
        clock_width,
        area.height,
    );
    let mode_style = Style::default()
        .fg(theme.background)
        .bg(theme.active)
        .add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::styled(format!(" {} ", app.mode.label()), mode_style),
        Span::styled("  ", theme.base()),
        Span::styled(
            terminal_text(&shorten_path(
                &app.current_dir,
                status_area.width.saturating_sub(28) as usize,
            )),
            Style::default().fg(theme.foreground).bg(theme.background),
        ),
        Span::styled("  │  ", Style::default().fg(theme.muted)),
        Span::styled(
            terminal_text(&job_status(app)),
            Style::default().fg(if app.job_progress.is_some() {
                theme.highlight
            } else {
                theme.muted
            }),
        ),
        Span::styled(
            if app.marked.is_empty() {
                String::new()
            } else {
                format!("  │  {} SELECTED", app.marked.len())
            },
            Style::default().fg(theme.active),
        ),
        Span::styled(
            if app.mouse_enabled {
                "  │  MOUSE ON"
            } else {
                ""
            },
            Style::default().fg(theme.highlight),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.base()), status_area);
    if metrics_width > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    progress_text,
                    Style::default()
                        .fg(theme.background)
                        .bg(theme.foreground)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    position_text,
                    Style::default()
                        .fg(theme.background)
                        .bg(theme.active)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            metrics_area,
        );
    }
    render_clock(frame, clock_area, theme);
}

fn selection_progress(app: &App) -> (usize, usize) {
    let (selected, total) = if app.mode == Mode::ContentSearch {
        (app.content_selected, app.content_results.len())
    } else if app.mode == Mode::Find {
        (app.search_selected, app.search_results.len())
    } else if app.focus == Focus::Split {
        (app.split_selected, app.split_entries.len())
    } else {
        (app.selected, app.filtered_entries().len())
    };
    if total == 0 {
        (0, 0)
    } else {
        (selected.min(total - 1) + 1, total)
    }
}

fn job_status(app: &App) -> String {
    let Some(progress) = &app.job_progress else {
        return app.status.clone();
    };
    let percent = progress
        .completed
        .saturating_mul(100)
        .checked_div(progress.total)
        .unwrap_or(0);
    let filled = percent.min(100) / 10;
    let bar = format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(10_usize.saturating_sub(filled))
    );
    format!(
        "{} [{bar}] {percent}% {}/{} · {} · Esc cancel",
        progress.label, progress.completed, progress.total, progress.current
    )
}

fn bottom_right(area: Rect, width: u16) -> Rect {
    let width = area.width.min(width);
    Rect::new(
        area.right().saturating_sub(width),
        area.bottom().saturating_sub(1),
        width,
        1,
    )
}

fn render_clock(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let clock = format!(" {} ", Local::now().format("%H:%M:%S"));
    frame.render_widget(
        Paragraph::new(clock).alignment(Alignment::Right).style(
            Style::default()
                .fg(theme.background)
                .bg(theme.active)
                .add_modifier(Modifier::BOLD),
        ),
        area,
    );
}

fn render_input(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let (prefix, value, hint) = match app.mode {
        Mode::Filter => ("/", app.filter_input.as_str(), "type to filter · Esc clear"),
        Mode::Find => (
            "find› ",
            app.filter_input.as_str(),
            "Enter open · Esc cancel",
        ),
        Mode::ContentSearch => (
            "grep› ",
            app.content_query.as_str(),
            "j/k results · Enter open at line · Esc cancel",
        ),
        Mode::Command => (
            ":",
            app.command_input.as_str(),
            "q · help · cd <path> · open · grep <text> · admin",
        ),
        Mode::AdminPassword => ("ADMIN password› ", "", "Enter unlock · Esc cancel"),
        _ => ("", "", "Ctrl+P commands  ? help"),
    };
    let displayed_value = if app.mode == Mode::AdminPassword {
        "•".repeat(app.password_input.chars().count())
    } else {
        terminal_text(value)
    };
    let line = if matches!(
        app.mode,
        Mode::Filter | Mode::Find | Mode::ContentSearch | Mode::Command | Mode::AdminPassword
    ) {
        Line::from(vec![
            Span::styled(prefix, Style::default().fg(theme.hotkey)),
            Span::styled(
                displayed_value.as_str(),
                theme.base().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {hint}"), Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from(Span::styled(hint, Style::default().fg(theme.muted)))
    };
    frame.render_widget(Paragraph::new(line).style(theme.base()), area);

    if matches!(
        app.mode,
        Mode::Filter | Mode::Find | Mode::ContentSearch | Mode::Command | Mode::AdminPassword
    ) {
        let x = area.x + prefix.chars().count() as u16 + displayed_value.chars().count() as u16;
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
    }
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered_rect(area, 76, 96);
    frame.render_widget(Clear, popup);

    let mut command_spans = vec![Span::styled(
        "Commands  ",
        Style::default().fg(theme.highlight),
    )];
    for (index, command) in app.commands.iter().enumerate() {
        if index > 0 {
            command_spans.push(Span::styled("  ·  ", Style::default().fg(theme.muted)));
        }
        command_spans.push(Span::styled(
            command.name,
            Style::default().fg(theme.hotkey),
        ));
        command_spans.push(Span::styled(
            format!(" {}", command.description),
            Style::default().fg(theme.foreground),
        ));
    }

    let lines = vec![
        Line::from(Span::styled(
            "Keyboard navigation",
            Style::default()
                .fg(theme.highlight)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Start with ↑/↓ and Enter. Press Esc whenever you want to go back.",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        help_line("j/k or ↑/↓", "Move the highlighted selection", theme),
        help_line("l or Enter", "Open the highlighted item", theme),
        help_line("h or Backspace", "Go back to the parent folder", theme),
        help_line("Tab / Shift+Tab", "Move between panels", theme),
        help_line("Space", "Expand or collapse a folder in the tree", theme),
        help_line("/  ·  f", "Filter this folder · search below it", theme),
        help_line("F  ·  :grep", "Search text inside files", theme),
        help_line("v  ·  j/k", "Visual range selection", theme),
        help_line("y / x / p / D", "Copy · cut · paste · Recycle Bin", theme),
        help_line("e  ·  :  ·  q", "Edit · command mode · quit", theme),
        help_line("b", "Add/remove current folder favorite", theme),
        help_line("Ctrl+P", "Open the command palette", theme),
        help_line("a", "Find and launch a Start Menu app", theme),
        help_line("Ctrl+T / Ctrl+W", "Open / close a tab", theme),
        help_line("Ctrl+Tab", "Switch to the next tab", theme),
        help_line("Ctrl+Shift+Tab", "Switch to the previous tab", theme),
        help_line("[ / ]", "Alternative previous / next tab keys", theme),
        help_line("s", "Toggle split-folder view", theme),
        help_line("g", "Show Git status", theme),
        help_line("d", "Compare two selected text files", theme),
        help_line("t", "Quickly cycle color themes", theme),
        help_line("Ctrl+Shift+T", "Open the theme picker", theme),
        help_line("z", "Run a configured plugin", theme),
        Line::from(""),
        Line::from(Span::styled(
            "Mouse learning mode",
            Style::default()
                .fg(theme.highlight)
                .add_modifier(Modifier::BOLD),
        )),
        help_line(":admin", "Unlock mouse learning mode", theme),
        help_line("Mouse click", "Select an item or panel", theme),
        help_line("Mouse double-click", "Open a folder or file", theme),
        help_line("Mouse right-click", "Open file actions (ADMIN mode)", theme),
        help_line("Mouse wheel", "Move through items", theme),
        Line::from(""),
        Line::from(command_spans),
        Line::from(Span::styled(
            "Press Esc, ? or q to close",
            Style::default().fg(theme.muted),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" NTIV HELP ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.active))
                    .style(theme.base()),
            )
            .scroll((app.help_scroll, 0))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn render_context_menu(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let Some(menu) = app.context_menu else {
        return;
    };
    let width = 30_u16.min(area.width);
    let height = CONTEXT_ACTIONS.len() as u16 + 2;
    let x = menu.x.min(area.right().saturating_sub(width));
    let y = menu.y.min(area.bottom().saturating_sub(height));
    let popup = Rect::new(x, y, width, height);
    frame.render_widget(Clear, popup);
    let items = CONTEXT_ACTIONS
        .iter()
        .map(|action| ListItem::new(action.label()))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(menu.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" ACTIONS ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.highlight))
                    .style(theme.base()),
            )
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        popup,
        &mut state,
    );
}

fn render_rename_dialog(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered_fixed_rect(area, 60, 5);
    frame.render_widget(Clear, popup);
    let value = terminal_text(&app.rename_input);
    let lines = vec![
        Line::from(Span::styled(
            value.as_str(),
            theme.base().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter rename · Esc cancel",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" RENAME ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.active))
                .style(theme.base()),
        ),
        popup,
    );
    let cursor_x = popup.x + 1 + value.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(popup.right().saturating_sub(2)), popup.y + 1));
}

fn render_delete_confirmation(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered_fixed_rect(area, 64, 6);
    frame.render_widget(Clear, popup);
    let target = app
        .context_target
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| terminal_text(&name.to_string_lossy()))
        .unwrap_or_else(|| "selected item".into());
    let question = if app.pending_delete_count() > 1 {
        format!(
            "Move {} selected items to the Recycle Bin?",
            app.pending_delete_count()
        )
    } else {
        format!("Move {target} to the Recycle Bin?")
    };
    let lines = vec![
        Line::from(Span::styled(question, theme.base())),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter / Y", Style::default().fg(theme.hotkey)),
            Span::styled(" confirm     ", theme.base()),
            Span::styled("N / Esc", Style::default().fg(theme.highlight)),
            Span::styled(" cancel", theme.base()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" CONFIRM DELETE ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.hotkey))
                .style(theme.base()),
        ),
        popup,
    );
}

fn render_create_kind_dialog(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let popup = centered_fixed_rect(area, 48, 7);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(
            "What would you like to create?",
            theme.base().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("F", Style::default().fg(theme.hotkey)),
            Span::styled("  New file      ", theme.base()),
            Span::styled("D", Style::default().fg(theme.hotkey)),
            Span::styled("  New folder", theme.base()),
        ]),
        Line::from(Span::styled("Esc cancel", Style::default().fg(theme.muted))),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" NEW ITEM ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.active))
                .style(theme.base()),
        ),
        popup,
    );
}

fn render_create_name_dialog(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered_fixed_rect(area, 60, 5);
    frame.render_widget(Clear, popup);
    let value = terminal_text(&app.create_input);
    let title = match app.new_item_kind {
        Some(NewItemKind::Folder) => " NEW FOLDER ",
        _ => " NEW FILE ",
    };
    let lines = vec![
        Line::from(Span::styled(
            value.as_str(),
            theme.base().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter create · Esc cancel",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.active))
                .style(theme.base()),
        ),
        popup,
    );
    let cursor_x = popup.x + 1 + value.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(popup.right().saturating_sub(2)), popup.y + 1));
}

fn render_locations(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let width = area.width.saturating_sub(8).min(84);
    let height = (app.location_items.len() as u16 + 2)
        .min(area.height.saturating_sub(4))
        .max(5);
    let popup = centered_fixed_rect(area, width, height);
    frame.render_widget(Clear, popup);
    let items = app
        .location_items
        .iter()
        .map(|path| ListItem::new(terminal_text(&path.display().to_string())))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.location_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!(" {} ", app.location_title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.highlight))
                    .style(theme.base()),
            )
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        popup,
        &mut state,
    );
}

fn render_palette(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered_fixed_rect(area, 70, 16);
    frame.render_widget(Clear, popup);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(popup);
    let input = terminal_text(&app.palette_input);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.hotkey)),
            Span::styled(input.as_str(), theme.base().add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::default()
                .title(" COMMAND PALETTE ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.active))
                .style(theme.base()),
        ),
        chunks[0],
    );
    let actions = app.palette_matches();
    let items = actions
        .iter()
        .map(|action| ListItem::new(action.label()))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.palette_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.muted))
                    .style(theme.base()),
            )
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        chunks[1],
        &mut state,
    );
    let cursor_x = chunks[0].x + 3 + input.chars().count() as u16;
    frame.set_cursor_position((
        cursor_x.min(chunks[0].right().saturating_sub(2)),
        chunks[0].y + 1,
    ));
}

fn render_launcher(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered_fixed_rect(area, 72, 18);
    frame.render_widget(Clear, popup);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(popup);
    let input = terminal_text(&app.launcher_input);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("app› ", Style::default().fg(theme.hotkey)),
            Span::styled(input.as_str(), theme.base().add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::default()
                .title(" APPLICATION LAUNCHER ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.active))
                .style(theme.base()),
        ),
        chunks[0],
    );
    let matches = app.launcher_matches();
    let items = matches
        .iter()
        .map(|entry| ListItem::new(terminal_text(&entry.name)))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.launcher_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.muted))
                    .style(theme.base()),
            )
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        chunks[1],
        &mut state,
    );
    let cursor_x = chunks[0].x + 6 + input.chars().count() as u16;
    frame.set_cursor_position((
        cursor_x.min(chunks[0].right().saturating_sub(2)),
        chunks[0].y + 1,
    ));
}

fn render_text_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    lines: &[String],
    app: &App,
    theme: Theme,
) {
    let popup = centered_fixed_rect(
        area,
        area.width.saturating_sub(8).min(110),
        area.height.saturating_sub(4),
    );
    frame.render_widget(Clear, popup);
    let text = lines
        .iter()
        .map(|line| {
            let style = if line.starts_with('+') {
                Style::default().fg(theme.banner)
            } else if line.starts_with('-') {
                Style::default().fg(theme.hotkey)
            } else {
                theme.base()
            };
            Line::from(Span::styled(terminal_text(line), style))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.active))
                    .style(theme.base()),
            )
            .scroll((app.overlay_scroll, 0))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn render_plugins(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let height = (app.plugins.len() as u16 + 2)
        .min(area.height.saturating_sub(4))
        .max(6);
    let popup = centered_fixed_rect(area, 64, height);
    frame.render_widget(Clear, popup);
    let items = app
        .plugins
        .iter()
        .map(|plugin| ListItem::new(terminal_text(&plugin.name)))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.plugin_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" PLUGINS ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.active))
                    .style(theme.base()),
            )
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        popup,
        &mut state,
    );
}

fn render_themes(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let height = (app.themes.len() as u16 + 4)
        .min(area.height.saturating_sub(4))
        .max(8);
    let popup = centered_fixed_rect(area, 58, height);
    frame.render_widget(Clear, popup);
    let items = app
        .themes
        .iter()
        .enumerate()
        .map(|(index, (name, preview))| {
            let current = if index == app.saved_state.theme_index {
                "  ACTIVE"
            } else {
                ""
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    "██",
                    Style::default().fg(preview.banner).bg(theme.background),
                ),
                Span::styled(
                    "██",
                    Style::default().fg(preview.highlight).bg(theme.background),
                ),
                Span::styled("  ", theme.base()),
                Span::styled(name.as_str(), theme.base()),
                Span::styled(
                    current,
                    Style::default().fg(theme.hotkey).bg(theme.background),
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.theme_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" THEMES · Enter apply · Esc cancel ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.active))
                    .style(theme.base()),
            )
            .highlight_symbol("› ")
            .highlight_style(theme.selected()),
        popup,
        &mut state,
    );
}

fn help_line<'a>(key: &'a str, description: &'a str, theme: Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{key:<20}"), Style::default().fg(theme.hotkey)),
        Span::styled(description, theme.base()),
    ])
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let text = vec![
        Line::from(Span::styled(
            "NTIV",
            Style::default()
                .fg(theme.banner)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Terminal is too small.",
            Style::default().fg(theme.foreground),
        )),
        Line::from(Span::styled(
            "Resize to at least 60 × 18.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(theme.base()),
        area,
    );
}

fn panel_block<'a>(title: &'a str, active: bool, theme: Theme) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme.border(active))
        .style(theme.base())
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let width = area.width.saturating_mul(width_percent) / 100;
    let height = area.height.saturating_mul(height_percent) / 100;
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn centered_fixed_rect(area: Rect, requested_width: u16, requested_height: u16) -> Rect {
    let width = requested_width.min(area.width.saturating_sub(2)).max(1);
    let height = requested_height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn terminal_text(input: &str) -> String {
    let bidi = BidiInfo::new(input, None);
    if bidi.paragraphs.is_empty() {
        return input.to_string();
    }
    bidi.paragraphs
        .iter()
        .map(|paragraph| bidi.reorder_line(paragraph, paragraph.range.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn preview_text_style(path: Option<&Path>, line: &str, theme: Theme) -> Style {
    let extension = path
        .and_then(Path::extension)
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let trimmed = line.trim_start();
    if matches!(extension.as_str(), "md" | "markdown") {
        if trimmed.starts_with('#') {
            return Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD);
        }
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            return Style::default().fg(theme.highlight);
        }
    }
    if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("<!--") {
        return Style::default().fg(theme.muted);
    }
    if [
        "fn ",
        "pub ",
        "use ",
        "struct ",
        "enum ",
        "impl ",
        "let ",
        "const ",
        "class ",
        "def ",
        "function ",
        "import ",
    ]
    .iter()
    .any(|keyword| trimmed.starts_with(keyword))
    {
        return Style::default().fg(theme.banner);
    }
    theme.base()
}

fn shorten_path(path: &Path, max_chars: usize) -> String {
    let display = path.display().to_string();
    if display.chars().count() <= max_chars {
        return display;
    }
    let keep = max_chars.saturating_sub(1);
    let tail: String = display
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

fn gradient_line(text: &str, start: Color, end: Color) -> Line<'static> {
    let (Color::Rgb(start_r, start_g, start_b), Color::Rgb(end_r, end_g, end_b)) = (start, end)
    else {
        return Line::from(text.to_string());
    };
    let width = text.chars().count().saturating_sub(1).max(1) as u32;
    let spans = text
        .chars()
        .enumerate()
        .map(|(index, character)| {
            let index = index as u32;
            let mix = |from: u8, to: u8| {
                let from = from as i32;
                let delta = to as i32 - from;
                (from + delta * index as i32 / width as i32) as u8
            };
            Span::styled(
                character.to_string(),
                Style::default().fg(Color::Rgb(
                    mix(start_r, end_r),
                    mix(start_g, end_g),
                    mix(start_b, end_b),
                )),
            )
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::tempdir;

    use super::*;

    fn buffer_text(backend: &TestBackend) -> String {
        backend
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn dashboard_renders_at_eighty_columns() {
        let temp = tempdir().unwrap();
        let app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("Navigate Windows"));
        assert!(text.contains("Open Desktop"));
    }

    #[test]
    fn theme_picker_lists_all_built_in_themes() {
        let temp = tempdir().unwrap();
        let mut app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        app.mode = Mode::Themes;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());

        for name in ["Sunset", "Midnight", "Mono", "Candy", "Meadow", "Lavender"] {
            assert!(text.contains(name));
        }
    }

    #[test]
    fn dashboard_canvas_is_centered_in_a_windowed_terminal() {
        let temp = tempdir().unwrap();
        let app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal.backend().buffer().content();
        let row = |y: usize| {
            content[y * 80..(y + 1) * 80]
                .iter()
                .map(|cell| cell.symbol())
                .collect::<Vec<_>>()
                .join("")
        };

        assert!(row(1).contains("███╗"));
        assert_eq!(row(10).find("  ◆  Open Desktop"), Some(22));
        assert!(row(18).contains("NTIV 0.5"));
        assert_eq!(row(23).matches(':').count(), 2);
        assert_eq!(content[23 * 80 + 79].bg, Theme::SUNSET.active);
        assert_eq!(
            content[0].bg,
            Color::Rgb(0x18, 0x17, 0x1C),
            "the entire terminal cell grid must use the theme background"
        );
    }

    #[test]
    fn hebrew_runs_are_reordered_for_left_to_right_terminals() {
        assert_eq!(terminal_text("שלום"), "םולש");
        assert_eq!(terminal_text("file שלום.txt"), "file םולש.txt");
    }

    #[test]
    fn narrow_terminal_hides_preview_but_keeps_files() {
        let temp = tempdir().unwrap();
        let app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("FOLDERS"));
        assert!(text.contains("FILES"));
        assert!(!text.contains("PREVIEW"));
    }

    #[test]
    fn wide_terminal_renders_all_three_panels() {
        let temp = tempdir().unwrap();
        let app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("FOLDERS"));
        assert!(text.contains("FILES"));
        assert!(text.contains("PREVIEW"));
    }

    #[test]
    fn explorer_status_is_clean_and_reports_selection_progress() {
        let temp = tempdir().unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(temp.path().join(name), name).unwrap();
        }
        let mut app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("TOP"));
        assert!(text.contains("1/3"));
        assert!(!text.contains("Ctrl+Tab tabs"));
        assert!(!text.contains("Ctrl+Shift+T themes"));
        assert!(!text.contains("Space select"));

        app.selected = 1;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("66%"));
        assert!(text.contains("2/3"));

        app.selected = 2;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("BOT"));
        assert!(text.contains("3/3"));
    }

    #[test]
    fn explorer_uses_compact_panel_and_folder_icons() {
        let temp = tempdir().unwrap();
        std::fs::create_dir(temp.path().join("folder")).unwrap();
        let app = App::new(temp.path().to_path_buf(), Screen::Explorer, None).unwrap();
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("▰ FOLDERS"));
        assert!(text.contains("≡ FILES"));
        assert!(text.contains("◫ PREVIEW"));
        assert!(text.contains("▰ folder"));
    }

    #[test]
    fn very_small_terminal_shows_resize_message() {
        let temp = tempdir().unwrap();
        let app = App::new(temp.path().to_path_buf(), Screen::Dashboard, None).unwrap();
        let backend = TestBackend::new(50, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        assert!(buffer_text(terminal.backend()).contains("too small"));
    }

    #[test]
    fn banner_gradient_reaches_both_palette_ends() {
        let line = gradient_line("NTIV", Theme::SUNSET.banner, Theme::SUNSET.highlight);
        assert_eq!(
            line.spans.first().unwrap().style.fg,
            Some(Theme::SUNSET.banner)
        );
        assert_eq!(
            line.spans.last().unwrap().style.fg,
            Some(Theme::SUNSET.highlight)
        );
    }
}
