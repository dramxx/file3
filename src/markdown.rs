//! Renders markdown into styled terminal lines, roughly like VS Code's preview.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    prelude::Stylize,
    style::{Modifier, Style},
    text::{Line, Span, Text},
};

use crate::syntax::highlight_code_block;
use crate::text::str_width;
use crate::theme;

struct List {
    /// Next number for ordered lists.
    next: Option<u64>,
}

struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<Span<'static>>>>,
    row: Vec<Vec<Span<'static>>>,
    cell: Vec<Span<'static>>,
}

#[derive(Default)]
struct Renderer {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    styles: Vec<Style>,
    lists: Vec<List>,
    /// Bullet (or task checkbox) waiting for the first line of the current list item.
    bullet: Option<Span<'static>>,
    quote_depth: usize,
    code: Option<(String, String)>,
    table: Option<Table>,
    heading: Option<HeadingLevel>,
}

impl Renderer {
    fn style(&self) -> Style {
        self.styles
            .iter()
            .fold(Style::default().fg(theme::current().text), |acc, s| acc.patch(*s))
    }

    fn push_text(&mut self, text: &str) {
        let span = Span::styled(text.to_string(), self.style());
        match &mut self.table {
            Some(table) => table.cell.push(span),
            None => self.current.push(span),
        }
    }

    /// Indentation for list contents at the current depth.
    fn indent(&self) -> String {
        "   ".repeat(self.lists.len())
    }

    fn prefix(&mut self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for _ in 0..self.quote_depth {
            spans.push(Span::styled("▎ ", Style::default().fg(theme::current().text_faint)));
        }
        if let Some(bullet) = self.bullet.take() {
            let depth = self.lists.len().saturating_sub(1);
            spans.push(Span::raw("   ".repeat(depth)));
            spans.push(bullet);
        } else if !self.lists.is_empty() {
            spans.push(Span::raw(self.indent()));
        }
        spans
    }

    fn flush(&mut self) {
        if self.current.is_empty() && self.bullet.is_none() {
            return;
        }
        let mut spans = self.prefix();
        spans.append(&mut self.current);
        self.lines.push(Line::from(spans));
    }

    fn blank(&mut self) {
        if self.lines.last().is_some_and(|l| !l.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush();
                self.blank();
                self.heading = Some(level);
                let style = match level {
                    HeadingLevel::H1 | HeadingLevel::H2 => Style::default().fg(theme::current().accent).bold(),
                    HeadingLevel::H3 => Style::default().fg(theme::current().text_bright).bold(),
                    _ => Style::default().fg(theme::current().text).bold(),
                };
                self.styles.push(style);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
                self.styles.push(Style::default().fg(theme::current().text_dim).italic());
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::List(start) => {
                self.flush();
                if self.lists.is_empty() {
                    self.blank();
                }
                self.lists.push(List { next: start });
            }
            Tag::Item => {
                self.flush();
                let list = self.lists.last_mut();
                let bullet = match list.and_then(|l| l.next.as_mut()) {
                    Some(n) => {
                        let b = format!("{:>2}. ", n);
                        *n += 1;
                        b
                    }
                    None => " • ".to_string(),
                };
                self.bullet = Some(Span::styled(bullet, Style::default().fg(theme::current().text_dim)));
            }
            Tag::Emphasis => self.styles.push(Style::default().italic()),
            Tag::Strong => self.styles.push(Style::default().bold()),
            Tag::Strikethrough => self
                .styles
                .push(Style::default().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { .. } => self.styles.push(Style::default().fg(theme::current().link).underlined()),
            Tag::Image { .. } => {
                self.push_text("▣ ");
                self.styles.push(Style::default().fg(theme::current().text_dim));
            }
            Tag::Table(alignments) => {
                self.flush();
                self.blank();
                self.table = Some(Table {
                    alignments,
                    rows: Vec::new(),
                    row: Vec::new(),
                    cell: Vec::new(),
                });
            }
            Tag::TableHead => self.styles.push(Style::default().bold()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Heading(_) => {
                self.styles.pop();
                let level = self.heading.take();
                let width: usize = self.current.iter().map(|s| str_width(&s.content)).sum();
                self.flush();
                let rule = match level {
                    Some(HeadingLevel::H1) => Some("═"),
                    Some(HeadingLevel::H2) => Some("─"),
                    _ => None,
                };
                if let Some(rule) = rule {
                    self.lines.push(Line::styled(
                        rule.repeat(width.max(3)),
                        Style::default().fg(theme::current().guide),
                    ));
                }
                self.lines.push(Line::default());
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.styles.pop();
                self.quote_depth -= 1;
                if self.quote_depth == 0 {
                    self.blank();
                }
            }
            TagEnd::CodeBlock => {
                if let Some((lang, code)) = self.code.take() {
                    let highlighted = highlight_code_block(code.trim_end_matches('\n'), &lang);
                    let indent = self.indent();
                    let width = highlighted
                        .lines
                        .iter()
                        .map(|l| l.width())
                        .max()
                        .unwrap_or(0);
                    for line in highlighted.lines {
                        let pad = width - line.width();
                        let mut spans = vec![Span::raw(indent.clone()), Span::raw("  ").bg(theme::current().code_block_bg)];
                        spans.extend(line.spans.into_iter().map(|s| s.bg(theme::current().code_block_bg)));
                        spans.push(Span::raw(" ".repeat(pad + 2)).bg(theme::current().code_block_bg));
                        self.lines.push(Line::from(spans));
                    }
                    self.lines.push(Line::default());
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Item => self.flush(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image => {
                self.styles.pop();
            }
            TagEnd::TableCell => {
                if let Some(t) = &mut self.table {
                    let cell = std::mem::take(&mut t.cell);
                    t.row.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if tag == TagEnd::TableHead {
                    self.styles.pop();
                }
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.row);
                    t.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.render_table(table);
                }
            }
            _ => {}
        }
    }

    fn render_table(&mut self, table: Table) {
        let cols = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        let width = |cell: &Vec<Span>| cell.iter().map(|s| str_width(&s.content)).sum::<usize>();
        let widths: Vec<usize> = (0..cols)
            .map(|c| {
                table
                    .rows
                    .iter()
                    .filter_map(|r| r.get(c))
                    .map(width)
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let border = Style::default().fg(theme::current().guide);
        let indent = self.indent();

        for (r, row) in table.rows.into_iter().enumerate() {
            let mut spans = vec![Span::raw(indent.clone())];
            for (c, w) in widths.iter().enumerate() {
                if c > 0 {
                    spans.push(Span::styled(" │ ", border));
                }
                let cell = row.get(c).cloned().unwrap_or_default();
                let pad = w - width(&cell);
                let (before, after) = match table.alignments.get(c) {
                    Some(Alignment::Right) => (pad, 0),
                    Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                    _ => (0, pad),
                };
                spans.push(Span::raw(" ".repeat(before)));
                spans.extend(cell);
                spans.push(Span::raw(" ".repeat(after)));
            }
            self.lines.push(Line::from(spans));
            if r == 0 {
                let rule = widths
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("─┼─");
                self.lines.push(Line::from(vec![Span::raw(indent.clone()), Span::styled(rule, border)]));
            }
        }
        self.lines.push(Line::default());
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => match &mut self.code {
                Some((_, code)) => code.push_str(&text),
                None => self.push_text(&text),
            },
            Event::Code(code) => {
                let span = Span::styled(
                    code.to_string(),
                    self.style().fg(theme::current().inline_code).bg(theme::current().code_block_bg),
                );
                match &mut self.table {
                    Some(t) => t.cell.push(span),
                    None => self.current.push(span),
                }
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.blank();
                self.lines.push(Line::styled("─".repeat(40), Style::default().fg(theme::current().guide)));
                self.lines.push(Line::default());
            }
            Event::TaskListMarker(done) => {
                self.bullet = Some(if done {
                    Span::styled(" ☑ ", Style::default().fg(theme::current().added))
                } else {
                    Span::styled(" ☐ ", Style::default().fg(theme::current().text_dim))
                });
            }
            Event::InlineHtml(html) => self
                .current
                .push(Span::styled(html.to_string(), Style::default().fg(theme::current().text_faint))),
            Event::Html(html) => {
                for line in html.lines().filter(|l| !l.trim().is_empty()) {
                    self.current
                        .push(Span::styled(line.to_string(), Style::default().fg(theme::current().text_faint)));
                    self.flush();
                }
            }
            Event::FootnoteReference(name) => self.push_text(&format!("[{}]", name)),
            _ => {}
        }
    }
}

pub fn render_markdown(markdown: &str) -> Text<'static> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut renderer = Renderer::default();
    for event in Parser::new_ext(markdown, options) {
        renderer.event(event);
    }
    renderer.flush();
    while renderer.lines.last().is_some_and(|l| l.spans.is_empty()) {
        renderer.lines.pop();
    }
    if renderer.lines.is_empty() {
        renderer.lines.push(Line::default());
    }
    Text::from(renderer.lines)
}

pub fn is_markdown_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::line_text;

    fn render(md: &str) -> Vec<String> {
        render_markdown(md).lines.iter().map(line_text).collect()
    }

    #[test]
    fn test_headings_paragraphs_and_spacing() {
        assert_eq!(
            render("# Title\n\nSome *text* here.\n\n## Part\n\nMore."),
            vec!["Title", "═════", "", "Some text here.", "", "Part", "────", "", "More."]
        );
    }

    #[test]
    fn test_lists_nested_and_ordered() {
        assert_eq!(
            render("- one\n- two\n  - inner\n\n1. a\n2. b\n"),
            vec![" • one", " • two", "    • inner", "", " 1. a", " 2. b"]
        );
    }

    #[test]
    fn test_task_list() {
        assert_eq!(render("- [ ] todo\n- [x] done\n"), vec![" ☐ todo", " ☑ done"]);
    }

    #[test]
    fn test_code_block_is_kept_verbatim_and_padded() {
        let lines = render("```rust\nfn main() {}\n// x\n```\n");
        assert_eq!(lines, vec!["  fn main() {}  ", "  // x          "]);
    }

    #[test]
    fn test_inline_code_and_link_text() {
        assert_eq!(render("Use `cargo` and [docs](http://x)."), vec!["Use cargo and docs."]);
    }

    #[test]
    fn test_table_aligned() {
        assert_eq!(
            render("| Key | Action |\n| --- | --- |\n| q | Quit |\n| ↑ / k | Up |\n"),
            vec!["Key   │ Action", "──────┼───────", "q     │ Quit  ", "↑ / k │ Up    "]
        );
    }

    #[test]
    fn test_blockquote() {
        assert_eq!(render("> quoted\n\nafter"), vec!["▎ quoted", "", "after"]);
    }

    #[test]
    fn test_empty() {
        assert_eq!(render(""), vec![""]);
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file("README.MD"));
        assert!(!is_markdown_file("main.rs"));
    }
}
