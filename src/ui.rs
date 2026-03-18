use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, ViewMode};
use crate::syntax::highlight_code;

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(frame.area());

    render_left_panel(app, frame, chunks[0]);
    render_right_panel(app, frame, chunks[1]);
    render_status_bar(app, frame, frame.area());
}

fn render_left_panel(app: &App, frame: &mut Frame, area: Rect) {
    let title = truncate(&app.current_dir.to_string_lossy(), area.width as usize - 2);

    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| {
            let prefix = if entry.is_dir { "📁 " } else { "📄 " };
            let suffix = if entry.is_dir { "/" } else { "" };
            let dirty_marker = if !entry.is_dir && app.is_dirty(&entry.path) {
                " ~"
            } else {
                ""
            };

            let style = if !entry.is_dir && app.is_dirty(&entry.path) {
                Style::default().fg(ratatui::style::Color::Yellow)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::raw(format!("{}{}{}", entry.name, suffix, dirty_marker)).style(style),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let list = List::new(items)
        .block(Block::bordered().title(title).borders(Borders::ALL))
        .highlight_style(Style::new().reversed())
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_right_panel(app: &App, frame: &mut Frame, area: Rect) {
    let (content, title) = if let Some(entry) = app.selected_entry() {
        if entry.is_dir {
            (
                "  Select a file to preview".to_string(),
                "Preview".to_string(),
            )
        } else if app.view_mode == ViewMode::Diff {
            let diff = app.diff_content.clone().unwrap_or_default();
            if diff.is_empty() {
                (
                    "  No changes vs HEAD".to_string(),
                    format!("{} [diff]", entry.name),
                )
            } else {
                (diff, format!("{} [diff]", entry.name))
            }
        } else if let Some(ref content) = app.file_content {
            (content.clone(), entry.name.clone())
        } else {
            (
                "  [binary file or too large]".to_string(),
                entry.name.clone(),
            )
        }
    } else {
        ("  No entries".to_string(), "Preview".to_string())
    };

    let title = truncate(&title, area.width as usize - 2);

    if app.view_mode == ViewMode::Diff {
        let text = render_diff(&content);
        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title(title).borders(Borders::ALL))
            .scroll((app.scroll, 0))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    } else if let Some(entry) = app.selected_entry() {
        if entry.is_dir {
            let paragraph = Paragraph::new(content.as_str())
                .block(Block::bordered().title(title).borders(Borders::ALL))
                .scroll((app.scroll, 0))
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
        } else if app.file_content.is_some() {
            let highlighted = highlight_code(&content, &entry.name);
            let paragraph = Paragraph::new(highlighted)
                .block(Block::bordered().title(title).borders(Borders::ALL))
                .scroll((app.scroll, 0))
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new(content.as_str())
                .block(Block::bordered().title(title).borders(Borders::ALL))
                .scroll((app.scroll, 0))
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
        }
    } else {
        let paragraph = Paragraph::new(content.as_str())
            .block(Block::bordered().title(title).borders(Borders::ALL))
            .scroll((app.scroll, 0))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }
}

fn render_diff(diff: &str) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for line in diff.lines() {
        let style = if line.starts_with("diff ") {
            Style::default().fg(ratatui::style::Color::Magenta).dim()
        } else if line.starts_with("index ") {
            Style::default().fg(ratatui::style::Color::Blue).dim()
        } else if line.starts_with("---") {
            Style::default().fg(ratatui::style::Color::Red)
        } else if line.starts_with("+++") {
            Style::default().fg(ratatui::style::Color::Green)
        } else if line.starts_with("@@") {
            Style::default().fg(ratatui::style::Color::Cyan).dim()
        } else if line.starts_with('+') && !line.starts_with("+++") {
            Style::default().fg(ratatui::style::Color::Green)
        } else if line.starts_with('-') && !line.starts_with("---") {
            Style::default().fg(ratatui::style::Color::Red)
        } else {
            Style::default().fg(ratatui::style::Color::DarkGray)
        };

        lines.push(Line::from(Span::raw(line.to_string()).style(style)));
    }

    Text::from(lines)
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let base_status = "[q] Quit  [↑↓] Navigate  [Enter] Open  [Backspace] Up";
    let status = if app.is_git_repo {
        format!("{}  [d] Diff", base_status)
    } else {
        base_status.to_string()
    };

    let paragraph = Paragraph::new(Line::from(Span::raw(status)))
        .style(Style::default().dim())
        .block(Block::bordered().borders(Borders::TOP));

    let status_height = 1;
    let status_area = Rect::new(
        area.x,
        area.bottom() - status_height,
        area.width,
        status_height,
    );
    frame.render_widget(paragraph, status_area);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        let start_len = max_len.saturating_sub(3);
        format!("...{}", &s[s.len() - start_len..])
    } else {
        s.to_string()
    }
}
