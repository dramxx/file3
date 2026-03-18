use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, ViewMode};
use crate::syntax::highlight_code;

const BG: Color = Color::Rgb(30, 30, 46);
const PANEL_BG: Color = Color::Rgb(40, 40, 60);
const SELECTION: Color = Color::Rgb(75, 75, 120);
const TEXT: Color = Color::Rgb(220, 220, 240);
const TEXT_DIM: Color = Color::Rgb(120, 120, 150);
const ACCENT: Color = Color::Rgb(97, 175, 239);
const DIR_COLOR: Color = Color::Rgb(229, 192, 109);
const DIRTY_COLOR: Color = Color::Rgb(229, 192, 109);
const BORDER: Color = Color::Rgb(60, 60, 80);

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, chunks[0]);

    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Min(0)])
        .split(chunks[1]);

    render_left_panel(app, frame, main_area[0]);
    render_right_panel(app, frame, main_area[1]);
    render_status_bar(app, frame, chunks[2]);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let paragraph = Paragraph::new(
        Line::from(vec![
            Span::raw("  "),
            Span::styled("file3", Style::default().fg(ACCENT).bold()),
            Span::raw("  ·  TUI File Explorer"),
        ])
        .style(Style::default().bg(PANEL_BG).fg(TEXT)),
    )
    .style(Style::default().bg(BG));

    frame.render_widget(paragraph, area);
}

fn render_left_panel(app: &App, frame: &mut Frame, area: Rect) {
    let title = truncate(&app.current_dir.to_string_lossy(), area.width as usize - 4);

    let mut items: Vec<ListItem> = Vec::new();

    let not_at_root = !app.is_at_root();

    if not_at_root {
        let is_selected = app.selected == 0;
        items.push(ListItem::new(
            Line::from(vec![
                Span::raw(" "),
                Span::raw("◀").style(Style::default().fg(TEXT_DIM)),
                Span::raw("  "),
                Span::raw("..").style(Style::default().fg(TEXT_DIM)),
            ])
            .style(Style::default().bg(if is_selected {
                SELECTION
            } else {
                PANEL_BG
            })),
        ));
    }

    items.extend(app.entries.iter().enumerate().map(|(i, entry)| {
        let (icon, color) = if entry.is_dir {
            ("▶", DIR_COLOR)
        } else if app.is_dirty(&entry.path) {
            ("●", DIRTY_COLOR)
        } else {
            (" ", TEXT_DIM)
        };

        let name_style = if entry.is_dir {
            Style::default().fg(DIR_COLOR)
        } else if app.is_dirty(&entry.path) {
            Style::default().fg(TEXT)
        } else {
            Style::default().fg(TEXT_DIM)
        };

        let list_index = if not_at_root { i + 1 } else { i };
        let is_selected = app.selected == list_index;

        ListItem::new(
            Line::from(vec![
                Span::raw(" "),
                Span::raw(icon).style(Style::default().fg(color)),
                Span::raw(" "),
                Span::raw(format!(
                    "{}{}",
                    entry.name,
                    if entry.is_dir { "/" } else { "" }
                ))
                .style(name_style),
            ])
            .style(Style::default().bg(if is_selected {
                SELECTION
            } else {
                PANEL_BG
            })),
        )
    }));

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(format!(" {} ", title))
                .border_style(Style::default().fg(BORDER))
                .title_style(Style::default().fg(TEXT_DIM))
                .borders(Borders::ALL),
        )
        .style(Style::default().bg(PANEL_BG));

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_right_panel(app: &App, frame: &mut Frame, area: Rect) {
    if app.selected_is_parent() {
        let content_block = Block::bordered()
            .title(" .. ")
            .border_style(Style::default().fg(BORDER))
            .title_style(Style::default().fg(TEXT_DIM))
            .borders(Borders::ALL)
            .style(Style::default().bg(BG));

        frame.render_widget(
            Paragraph::new(
                Line::from(Span::raw("  Go up one directory")).style(Style::default().fg(TEXT_DIM)),
            )
            .scroll((0, 0))
            .block(content_block),
            area,
        );
        return;
    }

    let title = if let Some(entry) = app.selected_entry() {
        if entry.is_dir {
            return;
        }

        let title = match app.view_mode {
            ViewMode::Diff => format!(" {} [diff] ", entry.name),
            ViewMode::Content => format!(" {} ", entry.name),
        };

        if app.view_mode == ViewMode::Diff && app.diff_content.is_some() {
            let content = app.diff_content.as_ref().unwrap();
            let additions = content
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count();
            let deletions = content
                .lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                .count();
            format!("{} +{} -{}", title, additions, deletions)
        } else {
            title
        }
    } else {
        " Preview ".to_string()
    };

    let content_block = Block::bordered()
        .title(title.as_str())
        .border_style(Style::default().fg(BORDER))
        .title_style(Style::default().fg(if app.view_mode == ViewMode::Diff {
            Color::Rgb(86, 182, 194)
        } else {
            TEXT
        }))
        .borders(Borders::ALL)
        .style(Style::default().bg(BG));

    let _inner = content_block.inner(area);
    let paragraph = match app.view_mode {
        ViewMode::Diff => {
            let diff = app.diff_content.clone().unwrap_or_default();
            if diff.is_empty() {
                Paragraph::new(
                    Line::from(Span::raw("  No changes vs HEAD"))
                        .style(Style::default().fg(TEXT_DIM)),
                )
            } else {
                Paragraph::new(render_diff(&diff))
            }
        }
        ViewMode::Content => {
            if let Some(entry) = app.selected_entry() {
                if entry.is_dir {
                    return;
                }
                if let Some(ref content) = app.file_content {
                    Paragraph::new(highlight_code(content, &entry.name))
                } else {
                    Paragraph::new(
                        Line::from(Span::raw("[binary file or too large]"))
                            .style(Style::default().fg(TEXT_DIM)),
                    )
                }
            } else {
                Paragraph::new(
                    Line::from(Span::raw("  Select a file to preview"))
                        .style(Style::default().fg(TEXT_DIM)),
                )
            }
        }
    };

    frame.render_widget(paragraph.scroll((app.scroll, 0)).block(content_block), area);
}

fn render_diff(diff: &str) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for line in diff.lines() {
        let (style, prefix) = if line.starts_with("diff ") {
            (Style::default().fg(Color::Rgb(140, 140, 170)), "")
        } else if line.starts_with("index ") {
            (Style::default().fg(Color::Rgb(100, 100, 130)), "")
        } else if line.starts_with("@@") {
            (Style::default().fg(Color::Rgb(86, 182, 194)).bold(), "")
        } else if line.starts_with('+') && !line.starts_with("+++") {
            (Style::default().fg(Color::Rgb(152, 195, 121)), "")
        } else if line.starts_with('-') && !line.starts_with("---") {
            (Style::default().fg(Color::Rgb(224, 108, 117)), "")
        } else if line.starts_with("---") {
            (Style::default().fg(Color::Rgb(224, 108, 117)), "")
        } else if line.starts_with("+++") {
            (Style::default().fg(Color::Rgb(152, 195, 121)), "")
        } else {
            (Style::default().fg(TEXT_DIM), "")
        };

        lines.push(Line::from(vec![
            Span::raw(format!("{}{}", prefix, line)).style(style)
        ]));
    }

    Text::from(lines)
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let key_hints = if app.is_git_repo {
        vec![
            ("q", "quit"),
            ("↑↓", "navigate"),
            ("↵", "open"),
            ("⌫", "up"),
            ("d", "diff"),
        ]
    } else {
        vec![
            ("q", "quit"),
            ("↑↓", "navigate"),
            ("↵", "open"),
            ("⌫", "up"),
        ]
    };

    let items: Vec<Span> = key_hints
        .iter()
        .enumerate()
        .map(|(i, (key, desc))| {
            let mut spans = vec![
                Span::styled(format!("[{}]", key), Style::default().fg(ACCENT)),
                Span::raw(" "),
                Span::raw(*desc),
            ];
            if i < key_hints.len() - 1 {
                spans.push(Span::raw("  ·  "));
            }
            spans
        })
        .flatten()
        .collect();

    let paragraph = Paragraph::new(Line::from(items)).style(Style::default().bg(PANEL_BG).fg(TEXT));

    frame.render_widget(paragraph, area);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        let start_len = max_len.saturating_sub(3);
        format!("...{}", &s[s.len() - start_len..])
    } else {
        s.to_string()
    }
}
