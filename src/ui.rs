use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, ViewMode};
use crate::markdown::{is_markdown_file, render_markdown};
use crate::syntax::highlight_code;

const BG: Color = Color::Rgb(30, 30, 46);
const PANEL_BG: Color = Color::Rgb(40, 40, 60);
const SELECTION: Color = Color::Rgb(75, 75, 120);
const TEXT: Color = Color::Rgb(220, 220, 240);
const TEXT_DIM: Color = Color::Rgb(120, 120, 150);
const ACCENT: Color = Color::Rgb(97, 175, 239);
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

    render_header(app, frame, chunks[0]);

    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Min(0)])
        .split(chunks[1]);

    frame.render_widget(Clear, main_area[0]);
    frame.render_widget(Clear, main_area[1]);
    render_left_panel(app, frame, main_area[0]);
    render_right_panel(app, frame, main_area[1]);
    render_status_bar(app, frame, chunks[2]);
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mode_indicator = if app.show_dirty_only {
        Span::styled(" [dirty]", Style::default().fg(DIRTY_COLOR))
    } else {
        Span::raw("")
    };

    let paragraph = Paragraph::new(
        Line::from(vec![
            Span::raw("  "),
            Span::styled("file3", Style::default().fg(ACCENT).bold()),
            Span::raw("  ·  TUI File Explorer"),
            mode_indicator,
        ])
        .style(Style::default().bg(PANEL_BG).fg(TEXT)),
    )
    .style(Style::default().bg(BG));

    frame.render_widget(paragraph, area);
}

fn render_left_panel(app: &App, frame: &mut Frame, area: Rect) {
    let title = if app.show_dirty_only {
        format!(" {} dirty files ", app.dirty_entries.len())
    } else {
        format!(
            " {} ",
            truncate(&app.current_dir.to_string_lossy(), area.width as usize - 4)
        )
    };

    let items: Vec<ListItem> =
        if app.show_dirty_only {
            if app.dirty_entries.is_empty() {
                vec![ListItem::new(
                    Line::from(vec![Span::raw("  No dirty files")])
                        .style(Style::default().fg(TEXT_DIM)),
                )]
            } else {
                app.dirty_entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| {
                        let is_selected = app.selected == i;
                        let cursor = if is_selected { "▌" } else { " " };
                        ListItem::new(
                            Line::from(vec![
                                Span::styled(cursor, Style::default().fg(ACCENT)),
                                Span::raw(" "),
                                Span::raw("●").style(Style::default().fg(DIRTY_COLOR)),
                                Span::raw("  "),
                                Span::raw(&entry.name).style(Style::default().fg(TEXT)),
                            ])
                            .style(
                                Style::default().bg(if is_selected { SELECTION } else { PANEL_BG }),
                            ),
                        )
                    })
                    .collect()
            }
        } else {
            let mut items: Vec<ListItem> = Vec::new();
            let not_at_root = !app.is_at_root();

            if not_at_root {
                let is_selected = app.selected == 0;
                let cursor = if is_selected { "▌" } else { " " };
                items.push(ListItem::new(
                    Line::from(vec![
                        Span::styled(cursor, Style::default().fg(ACCENT)),
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
                let is_dirty = app.is_dirty(&entry.path);
                let (icon, color) = if entry.is_dir {
                    ("▶", DIRTY_COLOR)
                } else if is_dirty {
                    ("●", DIRTY_COLOR)
                } else {
                    (" ", TEXT_DIM)
                };

                let name_style = if entry.is_dir {
                    Style::default().fg(DIRTY_COLOR)
                } else if is_dirty {
                    Style::default().fg(TEXT)
                } else {
                    Style::default().fg(TEXT_DIM)
                };

                let list_index = if not_at_root { i + 1 } else { i };
                let is_selected = app.selected == list_index;
                let cursor = if is_selected { "▌" } else { " " };

                ListItem::new(
                    Line::from(vec![
                        Span::styled(cursor, Style::default().fg(ACCENT)),
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

            items
        };

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title.as_str())
                .border_style(Style::default().fg(BORDER))
                .title_style(Style::default().fg(if app.show_dirty_only {
                    DIRTY_COLOR
                } else {
                    TEXT_DIM
                }))
                .borders(Borders::ALL),
        )
        .style(Style::default().bg(PANEL_BG));

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_right_panel(app: &App, frame: &mut Frame, area: Rect) {
    if app.selected_is_parent() && !app.show_dirty_only {
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

    if app.show_dirty_only && app.dirty_entries.is_empty() {
        let content_block = Block::bordered()
            .title(" No dirty files ")
            .border_style(Style::default().fg(BORDER))
            .title_style(Style::default().fg(DIRTY_COLOR))
            .borders(Borders::ALL)
            .style(Style::default().bg(BG));

        frame.render_widget(
            Paragraph::new(
                Line::from(Span::raw("  No files with uncommitted changes"))
                    .style(Style::default().fg(TEXT_DIM)),
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

    let inner = content_block.inner(area);
    let wrap_width = ((inner.width.saturating_sub(2)) as f32 * 0.8) as usize;
    let paragraph = match app.view_mode {
        ViewMode::Diff => {
            let diff = app.diff_content.clone().unwrap_or_default();
            if diff.is_empty() {
                Paragraph::new(
                    Line::from(Span::raw("  No changes vs HEAD"))
                        .style(Style::default().fg(TEXT_DIM)),
                )
            } else {
                Paragraph::new(render_diff(&diff, wrap_width))
            }
        }
        ViewMode::Content => {
            if let Some(ref content) = app.file_content {
                if let Some(entry) = app.selected_entry() {
                    if is_markdown_file(&entry.name) {
                        Paragraph::new(render_markdown(content))
                    } else {
                        Paragraph::new(highlight_code(content, &entry.name))
                    }
                } else {
                    Paragraph::new(
                        wrap_text_at_width(content, wrap_width).style(Style::default().fg(TEXT)),
                    )
                }
            } else {
                Paragraph::new(Line::from(Span::raw("  ")).style(Style::default().fg(TEXT)))
            }
        }
    };

    frame.render_widget(paragraph.scroll((app.scroll, 0)).block(content_block), area);
}

fn render_diff(diff: &str, wrap_width: usize) -> Text<'static> {
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

        let full_line = format!("{}{}", prefix, line);
        if full_line.chars().count() <= wrap_width {
            lines.push(Line::from(vec![Span::raw(full_line).style(style)]));
        } else {
            let chars: Vec<char> = full_line.chars().collect();
            let mut pos = 0;
            while pos < chars.len() {
                let end = std::cmp::min(pos + wrap_width, chars.len());
                let mut break_pos = end;
                for i in (pos..end).rev() {
                    if chars[i].is_whitespace() {
                        break_pos = i;
                        break;
                    }
                }
                if break_pos == pos {
                    break_pos = end;
                }
                let wrapped: String = chars[pos..break_pos].iter().collect();
                lines.push(Line::from(vec![Span::raw(wrapped).style(style)]));
                pos = if break_pos < chars.len()
                    && chars[break_pos..].iter().any(|c| !c.is_whitespace())
                {
                    let next_start = chars[break_pos..]
                        .iter()
                        .position(|c| !c.is_whitespace())
                        .unwrap_or(0)
                        + break_pos;
                    next_start
                } else {
                    break;
                };
            }
        }
    }

    Text::from(lines)
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let key_hints: Vec<(&str, &str)> = if app.show_dirty_only && app.is_git_repo {
        vec![
            ("q", "quit"),
            ("↑↓/kj", "navigate"),
            ("↵", "open"),
            ("f", "show all"),
            ("d", "diff"),
            ("h", "hidden"),
            ("PgUp/PgDn", "scroll"),
        ]
    } else if app.show_dirty_only {
        vec![
            ("q", "quit"),
            ("↑↓/kj", "navigate"),
            ("↵", "open"),
            ("f", "show all"),
            ("h", "hidden"),
            ("PgUp/PgDn", "scroll"),
        ]
    } else if app.is_git_repo {
        vec![
            ("q", "quit"),
            ("↑↓/kj", "navigate"),
            ("↵", "open"),
            ("f", "dirty"),
            ("d", "diff"),
            ("h", "hidden"),
            ("PgUp/PgDn", "scroll"),
        ]
    } else {
        vec![
            ("q", "quit"),
            ("↑↓/kj", "navigate"),
            ("↵", "open"),
            ("h", "hidden"),
            ("PgUp/PgDn", "scroll"),
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

fn wrap_text_at_width(text: &str, width: usize) -> Text<'_> {
    let mut result_lines: Vec<Line<'_>> = Vec::new();

    for line in text.lines() {
        if line.chars().count() <= width {
            result_lines.push(Line::from(line));
        } else {
            let chars: Vec<char> = line.chars().collect();
            let mut pos = 0;
            while pos < chars.len() {
                let end = std::cmp::min(pos + width, chars.len());
                let mut break_pos = end;

                for i in (pos..end).rev() {
                    if chars[i].is_whitespace() {
                        break_pos = i;
                        break;
                    }
                }

                if break_pos == pos {
                    break_pos = end;
                }

                let wrapped: String = chars[pos..break_pos].iter().collect();
                result_lines.push(Line::from(wrapped));
                pos = if chars[break_pos..].iter().any(|c| !c.is_whitespace()) {
                    let next_start = chars[break_pos..]
                        .iter()
                        .position(|c| !c.is_whitespace())
                        .unwrap_or(0)
                        + break_pos;
                    next_start
                } else {
                    break_pos + 1
                };
            }
        }
    }

    Text::from(result_lines)
}
