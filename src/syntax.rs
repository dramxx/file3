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

        lines.push(Line::from(spans));
    }

    Text {
        lines,
        alignment: None,
        style: ratatui::style::Style::default(),
    }
}
