use std::path::Path;

use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::app::{App, Focus, Overlay, SideMode};
use crate::palette::PaletteMode;
use crate::finder::Finder;
use crate::icons;
use crate::search::{Field, Row as SearchRow};
use crate::syntax::syntax_name;
use crate::text::{
    col_of_byte, expand_tabs, overlay, slice_line, str_width, truncate_left, truncate_right,
    wrap_line,
};
use crate::theme::{self, Theme};
use crate::viewer::{ViewMode, Viewer};

fn t() -> &'static Theme {
    theme::current()
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::default().bg(t().editor_bg)), area);

    let [header, main, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    let max_side = main.width.saturating_sub(30).max(20);
    let side_width = app
        .side_width
        .unwrap_or((main.width / 5).clamp(30, 48))
        .min(max_side);
    let [side, viewer] =
        Layout::horizontal([Constraint::Length(side_width), Constraint::Min(0)]).areas(main);
    app.areas.main = main;
    app.areas.side = side;
    app.areas.viewer = viewer;

    render_header(app, frame, header);
    match app.side_mode {
        SideMode::Tree => render_tree(app, frame, side),
        SideMode::Changes => render_changes(app, frame, side),
        SideMode::Search => render_search(app, frame, side),
    }
    render_viewer(app, frame, viewer);
    render_status_bar(app, frame, status);

    match &app.overlay {
        Overlay::None => {
            app.areas.popup = Rect::default();
            app.areas.popup_list = Rect::default();
        }
        Overlay::Finder(_) => render_finder(app, frame, area),
        Overlay::Palette(_) => render_palette(app, frame, area),
        Overlay::Symbols(_) => render_symbols(app, frame, area),
        Overlay::GotoLine(input) => {
            let input = input.clone();
            render_goto_line(app, frame, area, &input)
        }
        Overlay::Help => render_help(app, frame, area),
    }
}

// ---------------------------------------------------------------- shared pieces

fn panel(title: Line<'static>, focused: bool, bg: Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { t().border_focus } else { t().border }))
        .title(title)
        .style(Style::default().bg(bg))
}

fn panel_title(text: &str, focused: bool) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {} ", text),
        Style::default()
            .fg(if focused { t().text_bright } else { t().text_dim })
            .bold(),
    ))
}

fn row_rect(area: Rect, i: usize) -> Rect {
    Rect::new(area.x, area.y + i as u16, area.width, 1)
}

fn selection_bg(focused: bool) -> Color {
    if focused {
        t().selection_focus
    } else {
        t().selection
    }
}

/// A centered hint for empty states.
fn render_placeholder(frame: &mut Frame, area: Rect, text: &str) {
    if area.height == 0 {
        return;
    }
    let y = area.y + area.height / 3;
    let line = Line::from(Span::styled(text.to_string(), Style::default().fg(t().text_faint).italic()))
        .centered();
    frame.render_widget(line, Rect::new(area.x, y, area.width, 1));
}

/// Scrollbar drawn on the panel's right border, alongside the `list` rows.
fn render_list_scrollbar(frame: &mut Frame, panel_area: Rect, list: Rect, len: usize, offset: usize) {
    let height = list.height as usize;
    if len <= height {
        return;
    }
    let track = Rect::new(panel_area.right().saturating_sub(1), list.y, 1, list.height);
    let mut state = ScrollbarState::new(len.saturating_sub(height))
        .position(offset)
        .viewport_content_length(height);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol("▐")
            .thumb_style(Style::default().fg(t().text_faint)),
        track,
        &mut state,
    );
}

/// Icon (with trailing space) and chevron for a tree entry.
fn file_icon_span(app: &App, name: &str, is_dir: bool, open: bool) -> Option<Span<'static>> {
    if !app.nerd_font {
        return None;
    }
    if is_dir {
        Some(Span::styled(
            format!("{} ", icons::folder_icon(open)),
            Style::default().fg(t().folder),
        ))
    } else {
        let (icon, color) = icons::file_icon(name);
        Some(Span::styled(format!("{} ", icon), Style::default().fg(color)))
    }
}

fn rel_path(app: &App, path: &Path) -> String {
    path.strip_prefix(&app.root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn root_name(app: &App) -> String {
    app.root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| app.root.to_string_lossy().into_owned())
}

/// Lays out `left` spans followed by `right` spans flush right, shortening the name span
/// (index `shrink`) of `left` when they don't fit.
fn fit_row(mut left: Vec<Span<'static>>, shrink: usize, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let right_w: usize = right.iter().map(|s| str_width(&s.content)).sum();
    let left_w: usize = left.iter().map(|s| str_width(&s.content)).sum();
    let avail = width.saturating_sub(right_w);
    if left_w > avail {
        if let Some(span) = left.get_mut(shrink) {
            let other = left_w - str_width(&span.content);
            let room = avail.saturating_sub(other);
            span.content = truncate_right(&span.content, room).into();
        }
    }
    let left_w: usize = left.iter().map(|s| str_width(&s.content)).sum();
    let pad = width.saturating_sub(left_w + right_w);
    left.push(Span::raw(" ".repeat(pad)));
    left.extend(right);
    Line::from(left)
}

// ---------------------------------------------------------------- header & status

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let sep = Span::styled(" › ", Style::default().fg(t().text_faint));
    let mut left = vec![
        Span::raw(" "),
        Span::styled("■ ", Style::default().fg(t().border_focus)),
        Span::styled(root_name(app), Style::default().fg(t().accent).bold()),
    ];

    let shown = app
        .viewer
        .path
        .clone()
        .or_else(|| app.tree.selected_path());
    if let Some(path) = shown {
        let rel = rel_path(app, &path);
        let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        for (i, part) in parts.iter().enumerate() {
            left.push(sep.clone());
            let style = if i + 1 == parts.len() {
                Style::default().fg(t().text_bright)
            } else {
                Style::default().fg(t().text_dim)
            };
            left.push(Span::styled(part.to_string(), style));
        }
    }

    let mut right = Vec::new();
    if app.tree.show_all {
        right.push(Span::styled("all files ", Style::default().fg(t().text_faint)));
    }
    if app.dirty_count() > 0 {
        right.push(Span::styled(
            format!("● {} changed ", app.dirty_count()),
            Style::default().fg(t().modified),
        ));
    }

    // Long breadcrumbs lose their middle parts first.
    let width = area.width as usize;
    let right_w: usize = right.iter().map(|s| str_width(&s.content)).sum();
    while left.len() > 5 && left.iter().map(|s| str_width(&s.content)).sum::<usize>() + right_w > width {
        left.remove(3);
        left.remove(3);
        if left.get(3).is_none_or(|s| s.content != "…") {
            left.insert(3, sep.clone());
            left.insert(4, Span::styled("…", Style::default().fg(t().text_faint)));
        }
    }

    let line = fit_row(left, 2, right, width).style(Style::default().bg(t().bar_bg));
    frame.render_widget(line, area);
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let git = app.is_git_repo();
    let hints: Vec<(&str, &str)> = match (&app.overlay, app.focus, app.side_mode) {
        (Overlay::Finder(_), _, _) => vec![("↵", "open"), ("↑↓", "select"), ("Esc", "close")],
        (Overlay::Symbols(_), _, _) => vec![("↵", "go"), ("↑↓", "preview"), ("Esc", "back")],
        (Overlay::Palette(p), _, _) => match p.mode {
            PaletteMode::Commands(_) => vec![("↵", "run"), ("↑↓", "select"), ("Esc", "close")],
            PaletteMode::Themes { .. } => vec![("↑↓", "preview"), ("↵", "keep"), ("Esc", "cancel")],
        },
        (Overlay::GotoLine(_), _, _) => vec![("↵", "go"), ("Esc", "cancel")],
        (Overlay::Help, _, _) => vec![("any key", "close")],
        _ if app.viewer.find.as_ref().is_some_and(|f| f.editing) => {
            vec![("↵", "done"), ("↑↓", "prev/next"), ("Esc", "cancel")]
        }
        (_, Focus::Side, SideMode::Search) => vec![
            ("↵", "open"),
            ("↑↓", "results"),
            ("Alt+C/W/R", "case/word/regex"),
            ("Alt+F", "filters"),
            ("Tab", "to file"),
            ("Esc", "close"),
        ],
        (_, Focus::Side, SideMode::Changes) => {
            vec![("↵", "open"), ("d", "diff"), ("f", "tree"), ("^P", "files"), ("^F", "search"), ("?", "help")]
        }
        (_, Focus::Side, SideMode::Tree) => {
            let mut h = vec![("^P", "files"), ("^F", "search"), ("↵", "open"), ("←→", "fold"), ("h", "hidden")];
            if git {
                h.push(("f", "changes"));
            }
            h.extend([("?", "help"), ("q", "quit")]);
            h
        }
        (_, Focus::Viewer, _) => {
            let mut h = vec![("/", "find"), ("n/N", "next"), ("^G", "line"), ("w", "wrap")];
            if app.viewer.is_markdown() {
                h.push(("m", "markdown"));
            }
            if git {
                h.push(("d", "diff"));
            }
            h.extend([("Esc", "back"), ("?", "help")]);
            h
        }
    };

    let mut left: Vec<Span<'static>> = vec![Span::raw(" ")];
    if let Some((message, _)) = &app.toast {
        left.push(Span::styled(format!("{}   ", message), Style::default().fg(t().accent).bold()));
    } else {
        for (key, desc) in hints {
            left.push(Span::styled(key.to_string(), Style::default().fg(t().accent).bold()));
            left.push(Span::styled(format!(" {}   ", desc), Style::default().fg(t().text_dim)));
        }
    }

    let v = &app.viewer;
    let mut info = Vec::new();
    if v.wrap {
        info.push("wrap".to_string());
    }
    if let Some(path) = &v.path {
        if v.mode == ViewMode::Diff {
            info.push("diff".to_string());
        } else if v.is_markdown() && !v.markdown_source {
            info.push("Markdown preview".to_string());
        } else {
            info.push(syntax_name(&path.to_string_lossy()).to_string());
        }
        if !v.lines.is_empty() {
            info.push(format!("Ln {}/{}", v.scroll + 1, v.lines.len()));
        }
    }
    let right = vec![Span::styled(
        format!("{} ", info.join("  ·  ")),
        Style::default().fg(t().text_dim),
    )];

    // Drop hints that don't fit rather than cutting one in half.
    let right_w = str_width(&right[0].content);
    while left.len() > 1 && left.iter().map(|s| str_width(&s.content)).sum::<usize>() + right_w > area.width as usize {
        left.truncate(left.len() - 2);
    }
    let line = fit_row(left, usize::MAX, right, area.width as usize).style(Style::default().bg(t().bar_bg));
    frame.render_widget(line, area);
}

// ---------------------------------------------------------------- side panel

fn render_tree(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Side && matches!(app.overlay, Overlay::None);
    let title = panel_title(&format!("EXPLORER · {}", root_name(app)), focused);
    let block = panel(title, focused, t().sidebar_bg);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.areas.side_list = inner;
    app.areas.search_toggles = Default::default();

    let height = inner.height as usize;
    let tree = &mut app.tree;
    tree.offset = tree.offset.min(tree.rows.len().saturating_sub(height));

    if tree.rows.is_empty() {
        render_placeholder(frame, inner, "Folder is empty");
        return;
    }

    let width = inner.width as usize;
    let app = &*app;
    let tree = &app.tree;
    for (i, row) in tree.rows.iter().skip(tree.offset).take(height).enumerate() {
        let index = tree.offset + i;
        let entry = &row.entry;
        let open = entry.is_dir && tree.is_expanded(&entry.path);

        let mut spans = vec![Span::raw(" ")];
        for _ in 0..row.depth {
            spans.push(Span::styled("│ ", Style::default().fg(t().guide)));
        }
        let chevron = match (entry.is_dir, open) {
            (true, true) => "▾ ",
            (true, false) => "▸ ",
            _ => "  ",
        };
        spans.push(Span::styled(chevron, Style::default().fg(t().text_dim)));
        if let Some(icon) = file_icon_span(app, &entry.name, entry.is_dir, open) {
            spans.push(icon);
        }

        let dirty = app.is_dirty(&entry.path);
        let dirty_dir = entry.is_dir && app.has_dirty_children(&entry.path);
        let name_color = if entry.ignored {
            t().text_faint
        } else if dirty || dirty_dir {
            t().modified
        } else {
            t().text
        };
        let name_index = spans.len();
        spans.push(Span::styled(entry.name.clone(), Style::default().fg(name_color)));

        let right = if dirty {
            vec![Span::styled("M ", Style::default().fg(t().modified))]
        } else if dirty_dir {
            vec![Span::styled("● ", Style::default().fg(t().modified))]
        } else {
            vec![]
        };

        let mut line = fit_row(spans, name_index, right, width);
        if index == tree.selected {
            line = line.style(Style::default().bg(selection_bg(focused)));
        }
        frame.render_widget(line, row_rect(inner, i));
    }
    render_list_scrollbar(frame, area, inner, tree.rows.len(), tree.offset);
}

fn render_changes(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Side && matches!(app.overlay, Overlay::None);
    let title = panel_title(&format!("CHANGES · {}", app.changes.entries.len()), focused);
    let block = panel(title, focused, t().sidebar_bg);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.areas.side_list = inner;
    app.areas.search_toggles = Default::default();

    let height = inner.height as usize;
    let changes = &mut app.changes;
    changes.offset = changes.offset.min(changes.entries.len().saturating_sub(height));

    let app = &*app;
    if app.changes.entries.is_empty() {
        render_placeholder(frame, inner, "No uncommitted changes");
        return;
    }
    let width = inner.width as usize;
    for (i, entry) in app
        .changes
        .entries
        .iter()
        .enumerate()
        .skip(app.changes.offset)
        .take(height)
    {
        let (dir, file) = match entry.name.rsplit_once('/') {
            Some((d, f)) => (d.to_string(), f.to_string()),
            None => (String::new(), entry.name.clone()),
        };
        let mut spans = vec![Span::raw(" ")];
        if let Some(icon) = file_icon_span(app, &file, false, false) {
            spans.push(icon);
        }
        spans.push(Span::styled(file, Style::default().fg(t().modified)));
        spans.push(Span::raw(" "));
        let dir_index = spans.len();
        spans.push(Span::styled(dir, Style::default().fg(t().text_faint)));

        let mut line = fit_row(spans, dir_index, vec![Span::styled("M ", Style::default().fg(t().modified))], width);
        if i == app.changes.selected {
            line = line.style(Style::default().bg(selection_bg(focused)));
        }
        frame.render_widget(line, row_rect(inner, i - app.changes.offset));
    }
    render_list_scrollbar(frame, area, inner, app.changes.entries.len(), app.changes.offset);
}

fn render_search(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Side && matches!(app.overlay, Overlay::None);
    let block = panel(panel_title("SEARCH", focused), focused, t().sidebar_bg);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    // Query row with the option toggles on the right, then the optional filter rows.
    let s = &app.search;
    let row = |i: u16| Rect::new(inner.x, inner.y + i, inner.width, 1);
    let opts = s.options;
    let toggles = [
        ("Aa", opts.case_sensitive),
        ("ab", opts.whole_word),
        (".*", opts.regex),
        ("⋯", s.show_filters),
    ];
    let toggles_w: u16 = 3 * 4 + 1;
    let mut toggle_rects = [Rect::default(); 4];
    let mut toggle_spans = Vec::new();
    for (i, (label, on)) in toggles.iter().enumerate() {
        let x = inner.right().saturating_sub(toggles_w) + (i as u16) * 3;
        toggle_rects[i] = Rect::new(x, inner.y, 2, 1);
        let style = if *on {
            Style::default().fg(t().on_accent).bg(t().border_focus)
        } else {
            Style::default().fg(t().text_faint)
        };
        toggle_spans.push(Span::styled(format!("{:<2}", label), style));
        toggle_spans.push(Span::raw(" "));
    }

    let mut fields = vec![(Field::Query, " › ", s.input.as_str(), "Search")];
    if s.show_filters {
        fields.push((Field::Include, " include › ", s.include.as_str(), "e.g. src, *.ts"));
        fields.push((Field::Exclude, " exclude › ", s.exclude.as_str(), "e.g. *.test.*, docs"));
    }
    let mut field_rects = [Rect::default(); 3];
    for (i, (field, prompt, text, placeholder)) in fields.iter().enumerate() {
        let area = row(i as u16);
        field_rects[i] = area;
        let active = focused && s.field == *field;
        let right = if i == 0 { toggle_spans.clone() } else { Vec::new() };
        let right_w: usize = right.iter().map(|sp| str_width(&sp.content)).sum();
        let room = (area.width as usize).saturating_sub(str_width(prompt) + right_w + 2);
        let mut spans = vec![Span::styled(prompt.to_string(), Style::default().fg(t().accent))];
        if text.is_empty() && !active {
            spans.push(Span::styled(placeholder.to_string(), Style::default().fg(t().text_faint)));
        } else {
            spans.push(Span::styled(truncate_left(text, room), Style::default().fg(t().text_bright)));
            if active {
                spans.push(Span::styled("▏", Style::default().fg(t().accent)));
            }
        }
        let line = fit_row(spans, usize::MAX, right, area.width as usize)
            .style(Style::default().bg(t().input_bg));
        frame.render_widget(line, area);
    }
    let first_free = fields.len() as u16;
    app.areas.search_toggles = toggle_rects;
    app.areas.search_fields = field_rects;
    if inner.height < first_free + 2 {
        return;
    }

    // Summary row.
    let s = &app.search;
    let (summary, color) = if let Some(err) = &s.error {
        (err.clone(), t().error)
    } else if s.input.is_empty() {
        ("Type to search in all files".to_string(), t().text_faint)
    } else if s.running || s.is_pending() {
        (format!("Searching…  {} results", s.hit_count()), t().text_dim)
    } else if s.files.is_empty() {
        ("No results".to_string(), t().text_dim)
    } else {
        let more = if s.truncated { "+" } else { "" };
        (
            format!(
                "{}{} results in {} file{}",
                s.hit_count(),
                more,
                s.files.len(),
                if s.files.len() == 1 { "" } else { "s" }
            ),
            t().text_dim,
        )
    };
    frame.render_widget(
        Line::from(Span::styled(
            format!(" {}", truncate_right(&summary, inner.width as usize - 1)),
            Style::default().fg(color),
        )),
        row(first_free),
    );

    let list = Rect::new(inner.x, inner.y + first_free + 1, inner.width, inner.height - first_free - 1);
    app.areas.side_list = list;
    let height = list.height as usize;
    let search = &mut app.search;
    search.offset = search.offset.min(search.rows.len().saturating_sub(height));

    let app = &*app;
    let s = &app.search;
    let width = list.width as usize;
    for (i, row) in s.rows.iter().enumerate().skip(s.offset).take(height) {
        let line = match *row {
            SearchRow::File(f) => {
                let file = &s.files[f];
                let (dir, name) = match file.rel.rsplit_once('/') {
                    Some((d, n)) => (d.to_string(), n.to_string()),
                    None => (String::new(), file.rel.clone()),
                };
                let mut spans = vec![Span::raw(" ")];
                if let Some(icon) = file_icon_span(app, &name, false, false) {
                    spans.push(icon);
                }
                spans.push(Span::styled(name, Style::default().fg(t().text_bright).bold()));
                spans.push(Span::raw(" "));
                let dir_index = spans.len();
                spans.push(Span::styled(dir, Style::default().fg(t().text_faint)));
                fit_row(
                    spans,
                    dir_index,
                    vec![Span::styled(
                        format!(" {} ", file.hits.len()),
                        Style::default().fg(t().text_dim).bg(t().input_bg),
                    ), Span::raw(" ")],
                    width,
                )
            }
            SearchRow::Hit(f, h) => {
                let hit = &s.files[f].hits[h];
                let number = format!("  {:>4}  ", hit.line_number);
                let avail = width.saturating_sub(str_width(&number));
                let mut line = hit_line(&hit.text, &hit.ranges, avail);
                line.spans.insert(0, Span::styled(number, Style::default().fg(t().text_faint)));
                if i == s.selected {
                    line = line.style(Style::default().bg(selection_bg(focused)));
                }
                line
            }
        };
        frame.render_widget(line, row_rect(list, i - s.offset));
    }
    render_list_scrollbar(frame, area, list, s.rows.len(), s.offset);
}

/// One search hit: leading whitespace trimmed, matches highlighted, scrolled so the first
/// match is visible.
fn hit_line(text: &str, ranges: &[(usize, usize)], width: usize) -> Line<'static> {
    let expanded = expand_tabs(text);
    let map = |b: usize| crate::text::expanded_offset(text, b);
    let lead = expanded.len() - expanded.trim_start().len();
    let body = &expanded[lead..];
    let styled: Vec<(usize, usize, Style)> = ranges
        .iter()
        .map(|&(a, b)| {
            (
                map(a).saturating_sub(lead),
                map(b).saturating_sub(lead),
                Style::default().bg(t().match_bg).fg(t().text_bright),
            )
        })
        .collect();
    let line = overlay(&Line::styled(body.to_string(), Style::default().fg(t().text_dim)), &styled);

    // Scroll right only when the first match would not fit, keeping a little context before it.
    let (first_col, end_col) = styled
        .first()
        .map(|r| (col_of_byte(body, r.0), col_of_byte(body, r.1)))
        .unwrap_or((0, 0));
    let start = if end_col > width {
        first_col.saturating_sub((width / 4).min(10))
    } else {
        0
    };
    let mut out = slice_line(&line, start, width.saturating_sub(usize::from(start > 0)));
    if start > 0 {
        out.spans.insert(0, Span::styled("…", Style::default().fg(t().text_faint)));
    }
    out
}

// ---------------------------------------------------------------- viewer

fn render_viewer(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Viewer && matches!(app.overlay, Overlay::None);
    let v = &app.viewer;

    let mut title = vec![Span::raw(" ")];
    if v.path.is_some() {
        if let Some(icon) = file_icon_span(app, &v.file_name(), false, false) {
            title.push(icon);
        }
        title.push(Span::styled(
            v.file_name(),
            Style::default().fg(if focused { t().text_bright } else { t().text }).bold(),
        ));
        if app.is_dirty(v.path.as_deref().unwrap_or(Path::new(""))) {
            title.push(Span::styled(" M", Style::default().fg(t().modified)));
        }
        if v.mode == ViewMode::Diff {
            let (added, removed) = v.diff_stats();
            title.push(Span::styled("  diff ", Style::default().fg(t().hunk)));
            title.push(Span::styled(format!("+{} ", added), Style::default().fg(t().added)));
            title.push(Span::styled(format!("−{}", removed), Style::default().fg(t().removed)));
        }
    } else {
        title.push(Span::styled("Preview", Style::default().fg(t().text_dim)));
    }
    title.push(Span::raw(" "));

    let block = panel(Line::from(title), focused, t().editor_bg);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Find bar takes the bottom row while active.
    let (text_area, find_area) = if v.find.is_some() && inner.height > 2 {
        (
            Rect::new(inner.x, inner.y, inner.width, inner.height - 1),
            Some(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1)),
        )
    } else {
        (inner, None)
    };
    app.areas.viewer_text = text_area;

    let v = &mut app.viewer;
    let gutter = if v.numbered && !v.lines.is_empty() {
        v.lines.len().to_string().len() + 3
    } else {
        1
    };
    v.height = text_area.height as usize;
    v.width = (text_area.width as usize).saturating_sub(gutter + 1);
    v.scroll = v.scroll.min(v.max_scroll());

    let v = &app.viewer;
    if let Some(message) = &v.message {
        render_placeholder(frame, text_area, message);
    } else if v.lines.is_empty() && v.path.is_some() {
        render_placeholder(frame, text_area, "Empty file");
    } else {
        render_text(v, frame, text_area, gutter);
        let content = v.lines.len();
        if content > v.height {
            let mut state = ScrollbarState::new(v.max_scroll())
                .position(v.scroll)
                .viewport_content_length(v.height);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_symbol(None)
                    .thumb_symbol("▐")
                    .thumb_style(Style::default().fg(t().text_faint)),
                area.inner(Margin::new(0, 1)),
                &mut state,
            );
        }
    }

    if let (Some(find), Some(find_area)) = (&v.find, find_area) {
        let mut spans = vec![
            Span::styled(" Find ", Style::default().fg(t().text_dim)),
            Span::styled(find.query.clone(), Style::default().fg(t().text_bright)),
        ];
        if find.editing {
            spans.push(Span::styled("▏", Style::default().fg(t().accent)));
        }
        let count = if find.query.is_empty() {
            String::new()
        } else if find.matches.is_empty() {
            "No results ".to_string()
        } else {
            format!("{} of {} ", find.current + 1, find.matches.len())
        };
        let color = if !find.query.is_empty() && find.matches.is_empty() { t().error } else { t().text_dim };
        let line = fit_row(spans, 1, vec![Span::styled(count, Style::default().fg(color))], find_area.width as usize)
            .style(Style::default().bg(t().input_bg));
        frame.render_widget(line, find_area);
    }
}

fn render_text(v: &Viewer, frame: &mut Frame, area: Rect, gutter: usize) {
    let width = (area.width as usize).saturating_sub(gutter + 1);
    let mut y = 0usize;
    let mut index = v.scroll;
    while y < area.height as usize && index < v.lines.len() {
        let marked = v.mark.as_ref().is_some_and(|m| m.line == index);
        let line = overlay(&v.styled[index], &v.highlights(index));
        let pieces = if v.wraps() {
            wrap_line(&line, width)
        } else {
            vec![slice_line(&line, v.hscroll, width)]
        };

        for (p, piece) in pieces.into_iter().enumerate() {
            if y >= area.height as usize {
                break;
            }
            let number = if !v.numbered {
                " ".to_string()
            } else if p == 0 {
                format!(" {:>w$}  ", index + 1, w = gutter - 3)
            } else {
                " ".repeat(gutter)
            };
            let number_color = if marked { t().text_bright } else { t().text_faint };
            let mut spans = vec![Span::styled(number, Style::default().fg(number_color))];
            spans.extend(piece.spans);
            let mut row = Line::from(spans);
            if marked {
                row = row.style(Style::default().bg(t().current_line_bg));
            }
            frame.render_widget(row, row_rect(area, y));
            y += 1;
        }
        index += 1;
    }
}

// ---------------------------------------------------------------- overlays

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 4,
        width,
        height,
    )
}

fn render_finder(app: &mut App, frame: &mut Frame, area: Rect) {
    let popup = centered(area, (area.width * 7 / 10).clamp(40, 100), (area.height * 6 / 10).max(10));
    frame.render_widget(Clear, popup);

    let Overlay::Finder(finder) = &app.overlay else { return };
    let count = if app.files_loading {
        " indexing… ".to_string()
    } else {
        format!(" {} of {} files ", finder.results.len(), app.files.len())
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t().border_focus))
        .title(panel_title("Go to File", true))
        .title_bottom(Line::from(Span::styled(count, Style::default().fg(t().text_faint))).right_aligned())
        .style(Style::default().bg(t().popup_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 3 {
        return;
    }

    let input = Line::from(vec![
        Span::styled(" › ", Style::default().fg(t().accent)),
        Span::styled(
            truncate_left(&finder.input, inner.width as usize - 5),
            Style::default().fg(t().text_bright),
        ),
        Span::styled("▏", Style::default().fg(t().accent)),
    ])
    .style(Style::default().bg(t().input_bg));
    frame.render_widget(input, Rect::new(inner.x, inner.y, inner.width, 1));

    let list = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2);
    app.areas.popup = popup;
    app.areas.popup_list = list;

    let Overlay::Finder(finder) = &app.overlay else { return };
    if finder.results.is_empty() {
        let text = if app.files_loading { "Indexing files…" } else { "No matching files" };
        render_placeholder(frame, list, text);
        return;
    }
    render_finder_rows(app, finder, frame, list);
}

fn render_finder_rows(app: &App, finder: &Finder, frame: &mut Frame, list: Rect) {
    let height = list.height as usize;
    for (i, item) in finder.results.iter().enumerate().skip(finder.offset).take(height) {
        let label = if item.recent { "recently opened " } else { "" };
        let width = (list.width as usize).saturating_sub(label.len());
        let chars: Vec<char> = item.path.chars().collect();
        let name_start = item.path.rfind('/').map(|b| item.path[..b].chars().count() + 1).unwrap_or(0);
        let highlighted = |range: std::ops::Range<usize>, base: Style| -> Vec<Span<'static>> {
            range
                .map(|c| {
                    let style = if item.indices.binary_search(&c).is_ok() {
                        base.fg(t().match_fg).bold()
                    } else {
                        base
                    };
                    Span::styled(chars[c].to_string(), style)
                })
                .collect()
        };

        let name: String = chars[name_start..].iter().collect();
        let mut spans = vec![Span::raw(" ")];
        if let Some(icon) = file_icon_span(app, &name, false, false) {
            spans.push(icon);
        }
        spans.extend(highlighted(name_start..chars.len(), Style::default().fg(t().text_bright)));
        if name_start > 0 {
            spans.push(Span::raw("  "));
            let dir_end = name_start - 1;
            let dir_width: usize = chars[..dir_end].iter().map(|c| crate::text::char_width(*c)).sum();
            let used: usize = spans.iter().map(|s| str_width(&s.content)).sum();
            if used + dir_width < width {
                spans.extend(highlighted(0..dir_end, Style::default().fg(t().text_faint)));
            } else {
                let dir: String = chars[..dir_end].iter().collect();
                spans.push(Span::styled(
                    truncate_left(&dir, width.saturating_sub(used + 1)),
                    Style::default().fg(t().text_faint),
                ));
            }
        }
        let mut line = slice_line(&Line::from(spans), 0, width);
        if !label.is_empty() {
            line.spans.push(Span::raw(" ".repeat(width.saturating_sub(line.width()))));
            line.spans.push(Span::styled(label, Style::default().fg(t().text_faint)));
        }
        if i == finder.selected {
            line = line.style(Style::default().bg(t().selection_focus));
        }
        frame.render_widget(line, row_rect(list, i - finder.offset));
    }
}

/// Spans for `text` with the chars at `indices` (sorted char positions) emphasized.
fn highlight_chars(text: &str, indices: &[usize], base: Style) -> Vec<Span<'static>> {
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            let style = if indices.binary_search(&i).is_ok() {
                base.fg(t().match_fg).bold()
            } else {
                base
            };
            Span::styled(c.to_string(), style)
        })
        .collect()
}

fn render_palette(app: &mut App, frame: &mut Frame, area: Rect) {
    let Overlay::Palette(palette) = &app.overlay else { return };
    let picker = &palette.picker;
    let rows = (picker.results.len() as u16).clamp(1, 14);
    let popup = centered(area, 72, rows + 4);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t().border_focus))
        .title(panel_title(palette.title(), true))
        .style(Style::default().bg(t().popup_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 3 {
        return;
    }

    let placeholder = match palette.mode {
        PaletteMode::Commands(_) => "Type a command",
        PaletteMode::Themes { .. } => "Select a color theme",
    };
    let mut input = vec![Span::styled(" › ", Style::default().fg(t().accent))];
    if picker.input.is_empty() {
        input.push(Span::styled("▏", Style::default().fg(t().accent)));
        input.push(Span::styled(placeholder, Style::default().fg(t().text_faint)));
    } else {
        input.push(Span::styled(
            truncate_left(&picker.input, inner.width as usize - 5),
            Style::default().fg(t().text_bright),
        ));
        input.push(Span::styled("▏", Style::default().fg(t().accent)));
    }
    frame.render_widget(
        Line::from(input).style(Style::default().bg(t().input_bg)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let list = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2);
    if picker.results.is_empty() {
        render_placeholder(frame, list, "No matching commands");
    }
    let original = match palette.mode {
        PaletteMode::Themes { original } => Some(original),
        PaletteMode::Commands(_) => None,
    };
    for (i, item) in picker
        .results
        .iter()
        .enumerate()
        .skip(picker.offset)
        .take(list.height as usize)
    {
        let mut spans = vec![Span::raw(" ")];
        let label_index = spans.len();
        spans.extend(highlight_chars(&item.path, &item.indices, Style::default().fg(t().text)));
        let right = if original == Some(item.index) {
            "current ".to_string()
        } else {
            let key = palette.key_for(item.index);
            if key.is_empty() { String::new() } else { format!("{} ", key) }
        };
        let mut line = fit_row(
            spans,
            label_index,
            vec![Span::styled(right, Style::default().fg(t().text_faint))],
            list.width as usize,
        );
        if i == picker.selected {
            line = line.style(Style::default().bg(t().selection_focus));
        }
        frame.render_widget(line, row_rect(list, i - picker.offset));
    }
    app.areas.popup = popup;
    app.areas.popup_list = list;
}

fn render_symbols(app: &mut App, frame: &mut Frame, area: Rect) {
    let Overlay::Symbols(symbols) = &app.overlay else { return };
    let picker = &symbols.picker;
    let rows = (picker.results.len() as u16).clamp(1, 16);
    // Kept narrow and high so the file stays visible below it.
    let width = (area.width / 2).clamp(40, 70);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + 1,
        width.min(area.width),
        (rows + 4).min(area.height.saturating_sub(2)),
    );
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t().border_focus))
        .title(panel_title("Go to Symbol", true))
        .title_bottom(
            Line::from(Span::styled(
                format!(" {} of {} ", picker.results.len(), symbols.symbols.len()),
                Style::default().fg(t().text_faint),
            ))
            .right_aligned(),
        )
        .style(Style::default().bg(t().popup_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 3 {
        return;
    }

    let mut input = vec![
        Span::styled(" @ ", Style::default().fg(t().accent)),
        Span::styled(picker.input.clone(), Style::default().fg(t().text_bright)),
        Span::styled("▏", Style::default().fg(t().accent)),
    ];
    if picker.input.is_empty() {
        input.push(Span::styled("Type to filter", Style::default().fg(t().text_faint)));
    }
    frame.render_widget(
        Line::from(input).style(Style::default().bg(t().input_bg)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let list = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2);
    if picker.results.is_empty() {
        render_placeholder(frame, list, "No matching symbols");
    }
    for (i, item) in picker
        .results
        .iter()
        .enumerate()
        .skip(picker.offset)
        .take(list.height as usize)
    {
        let symbol = &symbols.symbols[item.index];
        let badge_color = match symbol.kind {
            crate::symbols::Kind::Function => t().hunk,
            crate::symbols::Kind::Type => t().modified,
            crate::symbols::Kind::Module => t().folder,
            crate::symbols::Kind::Heading => t().accent,
        };
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(format!("{} ", symbol.kind.badge()), Style::default().fg(badge_color).bold()),
        ];
        let name_index = spans.len();
        spans.extend(highlight_chars(&symbol.name, &item.indices, Style::default().fg(t().text)));
        let mut line = fit_row(
            spans,
            name_index,
            vec![Span::styled(format!("{} ", symbol.line + 1), Style::default().fg(t().text_faint))],
            list.width as usize,
        );
        if i == picker.selected {
            line = line.style(Style::default().bg(t().selection_focus));
        }
        frame.render_widget(line, row_rect(list, i - picker.offset));
    }
    app.areas.popup = popup;
    app.areas.popup_list = list;
}

fn render_goto_line(app: &mut App, frame: &mut Frame, area: Rect, input: &str) {
    let popup = centered(area, 44, 3);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t().border_focus))
        .title(panel_title("Go to Line", true))
        .style(Style::default().bg(t().popup_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let line = Line::from(vec![
        Span::styled(" › ", Style::default().fg(t().accent)),
        Span::styled(input.to_string(), Style::default().fg(t().text_bright)),
        Span::styled("▏", Style::default().fg(t().accent)),
        Span::styled(
            format!("   line 1 – {}", app.viewer.lines.len()),
            Style::default().fg(t().text_faint),
        ),
    ])
    .style(Style::default().bg(t().input_bg));
    frame.render_widget(line, inner);
    app.areas.popup = popup;
    app.areas.popup_list = Rect::default();
}

type HelpSection = (&'static str, &'static [(&'static str, &'static str)]);

const HELP_LEFT: &[HelpSection] = &[
    (
        "Anywhere",
        &[
            ("F1", "all commands, themes"),
            ("Ctrl+P", "go to file"),
            ("Ctrl+F", "search in all files"),
            ("Ctrl+R", "go to symbol"),
            ("Ctrl+G", "go to line"),
            ("Alt+← →", "back / forward"),
            ("Tab", "switch panel"),
            ("PgUp PgDn", "scroll the file"),
            ("h", "show ignored files"),
            ("q", "quit"),
        ],
    ),
    (
        "Explorer",
        &[
            ("↑ ↓", "move and preview"),
            ("→ ←", "expand / collapse"),
            ("Enter", "open"),
            ("f", "changed files (git)"),
        ],
    ),
];

const HELP_RIGHT: &[HelpSection] = &[
    (
        "File",
        &[
            ("/  n  N", "find in file"),
            ("← →", "scroll sideways"),
            ("w", "wrap long lines"),
            ("m", "markdown source"),
            ("d", "diff vs HEAD (git)"),
            ("y  Y", "copy path (+ line)"),
            ("e", "open in editor"),
            ("Esc", "clear / back"),
        ],
    ),
    (
        "Search panel",
        &[
            ("Alt+C W R", "case, word, regex"),
            ("Alt+F", "include / exclude"),
            ("Enter", "open result"),
        ],
    ),
    (
        "Mouse",
        &[
            ("click", "select, expand"),
            ("double-click", "focus the file"),
            ("wheel", "scroll"),
            ("drag border", "resize explorer"),
        ],
    ),
];

fn help_lines(sections: &[HelpSection]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (i, (section, keys)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(
            format!(" {}", section),
            Style::default().fg(t().accent).bold(),
        )));
        for (key, desc) in *keys {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<13}", key), Style::default().fg(t().text_bright)),
                Span::styled(desc.to_string(), Style::default().fg(t().text_dim)),
            ]));
        }
    }
    lines
}

fn render_help(app: &mut App, frame: &mut Frame, area: Rect) {
    let left = help_lines(HELP_LEFT);
    let right = help_lines(HELP_RIGHT);
    let rows = left.len().max(right.len()) as u16;
    let popup = centered(area, 76, rows + 2);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t().border_focus))
        .title(panel_title("Keyboard & Mouse", true))
        .style(Style::default().bg(t().popup_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(inner);
    frame.render_widget(Paragraph::new(left), l);
    frame.render_widget(Paragraph::new(right), r);
    app.areas.popup = popup;
    app.areas.popup_list = Rect::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::{Duration, Instant};

    fn draw(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(app, f)).unwrap();
        // Draw twice: the first frame records panel sizes the second one uses.
        terminal.draw(|f| render(app, f)).unwrap();
        let buf = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
    }

    fn settle(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while (app.files_loading || app.search.running || app.search.is_pending())
            && Instant::now() < deadline
        {
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.tick();
    }

    /// Renders every screen of the app on this repository at a narrow and a wide size.
    /// Run with `--nocapture` to look at the output.
    #[test]
    fn test_render_all_screens() {
        let root = std::env::current_dir().unwrap();
        for (w, h) in [(80, 24), (160, 40)] {
            let mut app = App::with_root(root.clone());
            settle(&mut app);
            let mut shots = vec![("tree", draw(&mut app, w, h))];

            press(&mut app, KeyCode::Enter, KeyModifiers::NONE); // expand first folder
            press(&mut app, KeyCode::Down, KeyModifiers::NONE);
            press(&mut app, KeyCode::Down, KeyModifiers::NONE);
            shots.push(("tree expanded + file", draw(&mut app, w, h)));

            press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
            for c in "self".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
            }
            shots.push(("find in file", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            press(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
            for c in "vwr".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
            }
            shots.push(("finder", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            shots.push(("finder opened", draw(&mut app, w, h)));

            press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
            for c in "highlight".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
            }
            settle(&mut app);
            shots.push(("search", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
            press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
            for c in "src".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
            }
            settle(&mut app);
            shots.push(("search with filter", draw(&mut app, w, h)));

            press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
            shots.push(("help", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);
            press(&mut app, KeyCode::Char('9'), KeyModifiers::NONE);
            shots.push(("goto line", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            press(&mut app, KeyCode::Char('w'), KeyModifiers::NONE);
            shots.push(("goto done + wrap", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Char('w'), KeyModifiers::NONE);

            press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
            shots.push(("go to symbol", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            press(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
            for c in "readme".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
            }
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            shots.push(("markdown preview", draw(&mut app, w, h)));

            press(&mut app, KeyCode::F(1), KeyModifiers::NONE);
            shots.push(("command palette", draw(&mut app, w, h)));
            for c in "theme".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
            }
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            press(&mut app, KeyCode::Down, KeyModifiers::NONE);
            shots.push(("theme picker", draw(&mut app, w, h)));
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            if app.is_git_repo() {
                press(&mut app, KeyCode::Char('f'), KeyModifiers::NONE);
                shots.push(("changes", draw(&mut app, w, h)));
                press(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
                shots.push(("diff", draw(&mut app, w, h)));
            }

            for (name, shot) in shots {
                println!("===== {} ({}x{}) =====\n{}\n", name, w, h, shot);
            }
        }
    }

    /// Writes `docs/screenshot.svg` from a real render of this repository.
    /// Run with `cargo test write_screenshot -- --ignored`.
    #[test]
    #[ignore]
    fn write_screenshot() {
        let root = std::env::current_dir().unwrap();
        let mut app = App::with_root(root.clone());
        settle(&mut app);
        app.tree.reveal(&root.join("src").join("viewer.rs"));
        let path = root.join("src").join("viewer.rs");
        app.viewer.open(&path);
        app.focus = crate::app::Focus::Viewer;
        // Show a find in progress for some highlighted matches.
        press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
        for c in "scroll".chars() {
            press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        let (w, h) = (128u16, 36u16);
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| render(&mut app, f)).unwrap();
        app.viewer.find_next(true);
        terminal.draw(|f| render(&mut app, f)).unwrap();
        let svg = buffer_to_svg(terminal.backend().buffer());
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs").join("screenshot.svg"), svg).unwrap();
    }

    fn rgb(color: Color, fallback: Color) -> String {
        match color {
            Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            Color::Reset => rgb(fallback, Color::Black),
            other => format!("{}", other).to_lowercase(),
        }
    }

    fn escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    /// Draws terminal cells as an SVG: a rectangle per background run and a text element per
    /// run of equally styled characters, stretched to the exact cell grid.
    fn buffer_to_svg(buf: &ratatui::buffer::Buffer) -> String {
        use ratatui::style::Modifier;
        let (cw, ch) = (9.0, 19.0);
        let area = buf.area;
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n\
             <rect width=\"100%\" height=\"100%\" rx=\"8\" fill=\"{bg}\"/>\n\
             <g font-family=\"Cascadia Mono, Consolas, 'DejaVu Sans Mono', Menlo, monospace\" font-size=\"15\">\n",
            w = area.width as f64 * cw,
            h = area.height as f64 * ch,
            bg = rgb(t().editor_bg, t().editor_bg),
        );
        for y in 0..area.height {
            let mut x = 0;
            while x < area.width {
                let cell = &buf[(x, y)];
                let key = (cell.fg, cell.bg, cell.modifier);
                let start = x;
                let mut text = String::new();
                while x < area.width {
                    let c = &buf[(x, y)];
                    if (c.fg, c.bg, c.modifier) != key {
                        break;
                    }
                    text.push_str(c.symbol());
                    x += 1;
                }
                let (px, py, run) = (start as f64 * cw, y as f64 * ch, (x - start) as f64 * cw);
                out.push_str(&format!(
                    "<rect x=\"{px}\" y=\"{py}\" width=\"{run}\" height=\"{ch}\" fill=\"{}\"/>\n",
                    rgb(key.1, t().editor_bg)
                ));
                if !text.trim().is_empty() {
                    let mut attrs = String::new();
                    if key.2.contains(Modifier::BOLD) {
                        attrs.push_str(" font-weight=\"bold\"");
                    }
                    if key.2.contains(Modifier::ITALIC) {
                        attrs.push_str(" font-style=\"italic\"");
                    }
                    out.push_str(&format!(
                        "<text x=\"{px}\" y=\"{}\" textLength=\"{run}\" lengthAdjust=\"spacingAndGlyphs\" xml:space=\"preserve\" fill=\"{}\"{attrs}>{}</text>\n",
                        py + ch * 0.75,
                        rgb(key.0, t().text),
                        escape(&text)
                    ));
                }
            }
        }
        out.push_str("</g>\n</svg>\n");
        out
    }
}
