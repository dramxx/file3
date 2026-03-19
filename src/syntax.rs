use ratatui::{
    style::Color,
    text::{Line, Span, Text},
};
use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

pub fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| {
        bat::assets::HighlightingAssets::from_binary()
            .get_syntax_set()
            .expect("Failed to load syntax set from bat")
            .clone()
    })
}

pub fn get_theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

fn find_syntax<'a>(syntax_set: &'a SyntaxSet, path: &str) -> &'a syntect::parsing::SyntaxReference {
    let path_obj = Path::new(path);

    if let Some(ext) = path_obj.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        if let Some(syntax) = syntax_set.find_syntax_by_extension(&ext_lower) {
            return syntax;
        }
    }

    if let Some(filename) = path_obj.file_name().and_then(|f| f.to_str()) {
        if let Some(syntax) = syntax_set.find_syntax_by_extension(filename) {
            return syntax;
        }
    }

    if let Ok(content) = std::fs::read_to_string(path) {
        if let Some(first_line) = content.lines().next() {
            if let Some(syntax) = syntax_set.find_syntax_by_first_line(first_line) {
                return syntax;
            }
        }
    }

    syntax_set.find_syntax_plain_text()
}

fn syntect_color_to_ratatui(s: &SyntectStyle) -> Color {
    Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b)
}

const MAX_LINE_WIDTH: usize = 400;

pub fn highlight_code(code: &str, file_path: &str) -> Text<'static> {
    let syntax_set = get_syntax_set();
    let theme_set = get_theme_set();

    let theme = theme_set
        .themes
        .get("base16-ocean.dark")
        .or_else(|| theme_set.themes.values().next());

    let Some(theme) = theme else {
        return Text::raw(code.to_string());
    };

    let syntax = find_syntax(syntax_set, file_path);

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines: Vec<Line<'static>> = Vec::new();

    for line in code.lines() {
        let ranges: Vec<(SyntectStyle, &str)> =
            highlighter.highlight_line(line, syntax_set).unwrap();

        let spans: Vec<Span> = ranges
            .into_iter()
            .map(|(style, text)| {
                Span::raw(text.to_string())
                    .style(ratatui::style::Style::default().fg(syntect_color_to_ratatui(&style)))
            })
            .collect();

        if spans.is_empty() {
            lines.push(Line::from(""));
        } else {
            let total_len: usize = spans.iter().map(|s| s.content.len()).sum();
            if total_len <= MAX_LINE_WIDTH {
                lines.push(Line::from(spans));
            } else {
                let mut current_line: Vec<Span> = Vec::new();
                let mut current_len = 0;
                for span in spans {
                    let span_len = span.content.len();
                    if current_len + span_len > MAX_LINE_WIDTH && !current_line.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_line)));
                        current_len = 0;
                    }
                    current_line.push(span);
                    current_len += span_len;
                }
                if !current_line.is_empty() {
                    lines.push(Line::from(current_line));
                }
            }
        }
    }

    Text {
        lines,
        alignment: None,
        style: ratatui::style::Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_code_rust() {
        let code = r#"fn main() {
    println!("Hello, world!");
}"#;
        let result = highlight_code(code, "test.rs");

        assert!(!result.lines.is_empty());
        assert!(result.lines.len() >= 2);
    }

    #[test]
    fn test_highlight_code_empty() {
        let code = "";
        let result = highlight_code(code, "test.txt");

        assert!(result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_multiline() {
        let code = "line1\nline2\nline3";
        let result = highlight_code(code, "test.txt");

        assert_eq!(result.lines.len(), 3);
    }

    #[test]
    fn test_highlight_code_with_tabs() {
        let code = "fn main() {\n\tprintln!(\"test\");\n}";
        let result = highlight_code(code, "test.rs");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_with_unicode() {
        let code = "// Comment with unicode: 世界\nlet x = 42;";
        let result = highlight_code(code, "test.rs");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_with_special_chars() {
        let code = "let s = \"hello\\nworld\";\nlet arr = [1, 2, 3];";
        let result = highlight_code(code, "test.rs");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_plain_text() {
        let code = "This is plain text content";
        let result = highlight_code(code, "test.txt");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_no_extension() {
        let code = "Some code without extension";
        let result = highlight_code(code, "Makefile");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_long_lines() {
        let code = format!("let x = \"{}\";", "x".repeat(1000));
        let result = highlight_code(&code, "test.rs");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_many_lines() {
        let code: String = (0..100)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = highlight_code(&code, "test.txt");

        assert_eq!(result.lines.len(), 100);
    }

    #[test]
    fn test_syntax_set_is_singleton() {
        let syntax1 = get_syntax_set();
        let syntax2 = get_syntax_set();

        assert!(std::ptr::eq(syntax1, syntax2));
    }

    #[test]
    fn test_theme_set_is_singleton() {
        let theme1 = get_theme_set();
        let theme2 = get_theme_set();

        assert!(std::ptr::eq(theme1, theme2));
    }

    #[test]
    fn test_theme_set_has_themes() {
        let theme_set = get_theme_set();
        assert!(!theme_set.themes.is_empty());
    }

    #[test]
    fn test_find_syntax_by_extension() {
        let syntax_set = get_syntax_set();

        let rust_syntax = find_syntax(syntax_set, "test.rs");
        assert!(rust_syntax.name.contains("Rust") || rust_syntax.name.contains("rust"));
    }

    #[test]
    fn test_find_syntax_plain_text_fallback() {
        let syntax_set = get_syntax_set();

        let syntax = find_syntax(syntax_set, "file.unknown");
        assert_eq!(syntax.name, "Plain Text");
    }

    #[test]
    fn test_syntax_with_path_separators() {
        let syntax_set = get_syntax_set();

        let syntax = find_syntax(syntax_set, "/path/to/file.rs");
        assert!(syntax.name.contains("Rust") || syntax.name.contains("rust"));
    }

    #[test]
    fn test_syntax_with_backslash_path() {
        let syntax_set = get_syntax_set();

        let syntax = find_syntax(syntax_set, "C:\\path\\to\\file.rs");
        assert!(syntax.name.contains("Rust") || syntax.name.contains("rust"));
    }

    #[test]
    fn test_highlight_code_with_crlf() {
        let code = "line1\r\nline2\r\nline3";
        let result = highlight_code(code, "test.txt");

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_highlight_code_very_long_line() {
        let long_line = format!(
            "Project(\"{}\") = \"{}  ",
            "{8BC9CEB8-8B4A-11D0-8D11-00A0C91BC942}",
            "TestProject".repeat(100)
        );
        assert!(long_line.len() > MAX_LINE_WIDTH);
        let result = highlight_code(&long_line, "test.sln");

        let total_chars: usize = result
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.len()).sum::<usize>())
            .sum();
        assert_eq!(total_chars, long_line.len());
        for line in &result.lines {
            let line_len: usize = line.spans.iter().map(|s| s.content.len()).sum();
            assert!(line_len <= MAX_LINE_WIDTH || result.lines.len() == 1);
        }
    }

    #[test]
    fn test_syntect_color_to_ratatui() {
        use syntect::highlighting::{Color, Style};

        let syntect_style = Style {
            foreground: Color {
                r: 255,
                g: 128,
                b: 64,
                a: 255,
            },
            background: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            font_style: syntect::highlighting::FontStyle::default(),
        };

        let color = syntect_color_to_ratatui(&syntect_style);

        match color {
            ratatui::style::Color::Rgb(r, g, b) => {
                assert_eq!(r, 255);
                assert_eq!(g, 128);
                assert_eq!(b, 64);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_text_struct_creation() {
        use ratatui::text::Line;

        let lines = vec![Line::from("Line 1"), Line::from("Line 2")];

        let text = Text {
            lines,
            alignment: None,
            style: ratatui::style::Style::default(),
        };

        assert_eq!(text.lines.len(), 2);
    }
}
