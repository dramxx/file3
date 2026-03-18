use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(frame.area());

    render_left_panel(app, frame, chunks[0]);
    render_right_panel(app, frame, chunks[1]);
    render_status_bar(frame, frame.area());
}

fn render_left_panel(app: &App, frame: &mut Frame, area: Rect) {
    let title = truncate(&app.current_dir.to_string_lossy(), area.width as usize - 2);

    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| {
            let prefix = if entry.is_dir { "📁 " } else { "📄 " };
            let suffix = if entry.is_dir { "/" } else { "" };
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::raw(format!("{}{}", entry.name, suffix)),
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

    let paragraph = Paragraph::new(content.as_str())
        .block(Block::bordered().title(title).borders(Borders::ALL))
        .scroll((app.scroll, 0))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect) {
    let status = "[q] Quit  [↑↓] Navigate  [Enter] Open  [Backspace] Up";
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
