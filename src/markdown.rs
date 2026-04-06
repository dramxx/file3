use pulldown_cmark::{html, Options, Parser};
use ratatui::{
    prelude::Stylize,
    style::Color,
    text::{Line, Span, Text},
};

pub fn render_markdown(markdown: &str) -> Text<'static> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    parse_html_to_ratatui(&html_output)
}

fn parse_html_to_ratatui(html: &str) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;

    for line in html.lines() {
        let line = line.trim();

        if line.starts_with("<pre>") || line.starts_with("<code") {
            in_code_block = true;
            continue;
        }

        if in_code_block {
            if line.starts_with("</code>") || line.starts_with("</pre>") {
                in_code_block = false;
                continue;
            }
            if !line.is_empty() {
                lines.push(Line::from(Span::raw(line.to_string())));
            }
            continue;
        }

        if line.starts_with("<h1>") {
            let text = extract_text(line);
            lines.push(Line::from(vec![Span::styled(text, ratatui::style::Style::default().bold().fg(Color::Rgb(97, 175, 239)))]));
        } else if line.starts_with("<h2>") {
            let text = extract_text(line);
            lines.push(Line::from(vec![Span::styled(text, ratatui::style::Style::default().bold().fg(Color::Rgb(97, 175, 239)))]));
        } else if line.starts_with("<h3>") {
            let text = extract_text(line);
            lines.push(Line::from(vec![Span::styled(text, ratatui::style::Style::default().bold().fg(Color::Rgb(97, 175, 239)))]));
        } else if line.starts_with("<h4>") || line.starts_with("<h5>") || line.starts_with("<h6>") {
            let text = extract_text(line);
            lines.push(Line::from(vec![Span::styled(text, ratatui::style::Style::default().bold().fg(Color::Rgb(97, 175, 239)))]));
        } else if line.starts_with("<ul>") || line.starts_with("<ol>") {
            continue;
        } else if line.starts_with("<li>") {
            let text = extract_text(line);
            lines.push(Line::from(vec![Span::raw("  • ".to_string()), Span::raw(text)]));
        } else if line.starts_with("<p>") {
            let text = extract_text(line);
            if !text.is_empty() {
                lines.push(Line::from(Span::raw(text)));
            }
        } else if line.starts_with("<blockquote>") {
            let text = extract_text(line);
            lines.push(Line::from(vec![Span::styled(text, ratatui::style::Style::default().fg(Color::Rgb(139, 233, 253)).italic())]));
        } else if line.starts_with("<hr>") {
            lines.push(Line::from(vec![Span::raw("─────────────────────────────────────────────────".to_string())]));
        } else if line.starts_with("<table>") {
            continue;
        } else if line.starts_with("<tr>") {
            let cells = extract_table_cells(line);
            let mut line_content = String::new();
            for (i, cell) in cells.iter().enumerate() {
                if i > 0 {
                    line_content.push_str(" | ");
                }
                line_content.push_str(cell);
            }
            lines.push(Line::from(line_content));
        } else if line.starts_with("</p>") || line.starts_with("</li>") || line.starts_with("</h") || line.starts_with("</table>") {
            continue;
        } else if !line.is_empty() {
            let text = strip_html_tags(line);
            if !text.is_empty() {
                lines.push(Line::from(text));
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from("".to_string()));
    }

    Text::from(lines)
}

fn extract_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    result.push(c);
                }
            }
        }
    }

    result.trim().to_string()
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    result.push(c);
                }
            }
        }
    }

    result.trim().to_string()
}

fn extract_table_cells(html: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut in_tag = false;
    let mut current_cell = String::new();
    let mut in_td = false;
    let mut in_th = false;

    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                if current_cell.starts_with("<th") {
                    in_th = true;
                }
                if current_cell.starts_with("<td") {
                    in_td = true;
                }
                if current_cell.starts_with("</th") || current_cell.starts_with("</td") {
                    if in_th || in_td {
                        cells.push(current_cell.trim().to_string());
                        current_cell.clear();
                        in_th = false;
                        in_td = false;
                    }
                }
            }
            '>' => {
                in_tag = false;
            }
            _ => {
                if !in_tag {
                    current_cell.push(c);
                }
            }
        }
    }

    cells
}

pub fn is_markdown_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdown")
}
