use ratatui::{
    style::Color,
    text::{Line, Span, Text},
};
use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::theme::{self, THEMES};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
/// Syntax themes, one per entry of `theme::THEMES`.
static SYNTAX_THEMES: OnceLock<Vec<Theme>> = OnceLock::new();

const PALENIGHT: &str = include_str!("../assets/palenight.tmTheme");

pub fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| {
        bat::assets::HighlightingAssets::from_binary()
            .get_syntax_set()
            .expect("Failed to load syntax set from bat")
            .clone()
    })
}

/// The syntax theme matching the active color theme.
pub fn get_theme() -> &'static Theme {
    theme_for(theme::current_index())
}

pub fn theme_for(index: usize) -> &'static Theme {
    let themes = SYNTAX_THEMES.get_or_init(|| {
        let assets = bat::assets::HighlightingAssets::from_binary();
        THEMES
            .iter()
            .map(|t| match t.syntax {
                "palenight" => ThemeSet::load_from_reader(&mut std::io::Cursor::new(PALENIGHT))
                    .expect("bundled palenight theme is valid"),
                name => assets.get_theme(name).clone(),
            })
            .collect()
    });
    &themes[index.min(themes.len() - 1)]
}

pub fn find_syntax<'a>(syntax_set: &'a SyntaxSet, path: &str) -> &'a syntect::parsing::SyntaxReference {
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

    syntax_set.find_syntax_plain_text()
}

pub fn syntax_name(file_path: &str) -> &'static str {
    find_syntax(get_syntax_set(), file_path).name.as_str()
}

fn syntect_color_to_ratatui(s: &SyntectStyle) -> Color {
    Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b)
}

/// Lines longer than this (minified code, data) are shown without highlighting; syntect is slow on them.
const MAX_LINE_WIDTH: usize = 2000;

/// Highlights `code`, producing exactly one output line per input line.
pub fn highlight_code(code: &str, file_path: &str) -> Text<'static> {
    let syntax_set = get_syntax_set();
    highlight_with(code, find_syntax(syntax_set, file_path))
}

/// Highlights a fenced code block by its language tag (`rust`, `py`, `json`, ...).
pub fn highlight_code_block(code: &str, lang: &str) -> Text<'static> {
    let syntax_set = get_syntax_set();
    let syntax = syntax_set
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    highlight_with(code, syntax)
}

fn highlight_with(code: &str, syntax: &syntect::parsing::SyntaxReference) -> Text<'static> {
    let syntax_set = get_syntax_set();
    let mut highlighter = HighlightLines::new(syntax, get_theme());
    let mut lines: Vec<Line<'static>> = Vec::new();

    for line in code.lines() {
        let ranges = if line.len() <= MAX_LINE_WIDTH {
            highlighter.highlight_line(line, syntax_set).ok()
        } else {
            None
        };

        let spans: Vec<Span> = match ranges {
            Some(ranges) => ranges
                .into_iter()
                .map(|(style, text)| {
                    Span::raw(text.to_string())
                        .style(ratatui::style::Style::default().fg(syntect_color_to_ratatui(&style)))
                })
                .collect(),
            None => vec![Span::raw(line.to_string())],
        };

        lines.push(Line::from(spans));
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
    fn test_theme_is_singleton() {
        let theme1 = get_theme();
        let theme2 = get_theme();

        assert!(std::ptr::eq(theme1, theme2));
    }

    #[test]
    fn test_every_color_theme_has_its_syntax_theme() {
        let names: Vec<Option<&str>> = (0..THEMES.len())
            .map(|i| theme_for(i).name.as_deref())
            .collect();
        assert_eq!(
            names,
            vec![
                Some("Visual Studio Dark+"),
                Some("Palenight"),
                Some("TwoDark"),
                Some("GitHub")
            ]
        );
        assert!(theme_for(1).scopes.len() > 10);
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
            "TestProject".repeat(200)
        );
        assert!(long_line.len() > MAX_LINE_WIDTH);
        let code = format!("short\n{}\nshort", long_line);
        let result = highlight_code(&code, "test.sln");

        // One output line per input line; the long one is kept whole and unstyled.
        assert_eq!(result.lines.len(), 3);
        assert_eq!(result.lines[1].spans.len(), 1);
        assert_eq!(result.lines[1].spans[0].content, long_line);
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
