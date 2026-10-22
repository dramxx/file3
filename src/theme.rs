//! Color themes. The active one is global so background highlighting threads can read it too.

#[cfg(not(test))]
use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::style::Color;

pub struct Theme {
    pub name: &'static str,
    /// Syntax theme: a bat theme name, or one of ours (see `syntax::get_theme`).
    pub syntax: &'static str,

    pub editor_bg: Color,
    pub sidebar_bg: Color,
    pub bar_bg: Color,
    pub input_bg: Color,
    pub popup_bg: Color,

    pub border: Color,
    pub border_focus: Color,
    /// Text on `border_focus` backgrounds (active toggles).
    pub on_accent: Color,

    pub selection: Color,
    pub selection_focus: Color,
    pub current_line_bg: Color,

    pub text: Color,
    pub text_bright: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    pub guide: Color,

    pub accent: Color,
    pub folder: Color,
    pub modified: Color,
    pub added: Color,
    pub removed: Color,
    pub hunk: Color,
    /// Row backgrounds of added / removed lines in the diff view.
    pub added_bg: Color,
    pub removed_bg: Color,
    pub error: Color,

    pub match_bg: Color,
    pub match_current_bg: Color,
    /// Fuzzy-matched characters in pickers.
    pub match_fg: Color,

    pub inline_code: Color,
    pub code_block_bg: Color,
    pub link: Color,
}

pub static THEMES: [Theme; 4] = [
    // VS Code "Dark Modern".
    Theme {
        name: "Dark Modern",
        syntax: "Visual Studio Dark+",
        editor_bg: Color::Rgb(31, 31, 31),
        sidebar_bg: Color::Rgb(24, 24, 24),
        bar_bg: Color::Rgb(24, 24, 24),
        input_bg: Color::Rgb(49, 49, 49),
        popup_bg: Color::Rgb(37, 37, 38),
        border: Color::Rgb(60, 60, 60),
        border_focus: Color::Rgb(0, 120, 212),
        on_accent: Color::Rgb(255, 255, 255),
        selection: Color::Rgb(55, 55, 61),
        selection_focus: Color::Rgb(4, 57, 94),
        current_line_bg: Color::Rgb(45, 45, 48),
        text: Color::Rgb(204, 204, 204),
        text_bright: Color::Rgb(240, 240, 240),
        text_dim: Color::Rgb(157, 157, 157),
        text_faint: Color::Rgb(110, 118, 129),
        guide: Color::Rgb(64, 64, 64),
        accent: Color::Rgb(55, 148, 255),
        folder: Color::Rgb(220, 182, 122),
        modified: Color::Rgb(226, 192, 141),
        added: Color::Rgb(129, 184, 139),
        removed: Color::Rgb(241, 76, 76),
        hunk: Color::Rgb(86, 156, 214),
        added_bg: Color::Rgb(37, 58, 40),
        removed_bg: Color::Rgb(72, 33, 35),
        error: Color::Rgb(241, 76, 76),
        match_bg: Color::Rgb(98, 51, 22),
        match_current_bg: Color::Rgb(158, 106, 3),
        match_fg: Color::Rgb(46, 170, 255),
        inline_code: Color::Rgb(206, 145, 120),
        code_block_bg: Color::Rgb(37, 37, 38),
        link: Color::Rgb(55, 148, 255),
    },
    // Material Theme "Palenight".
    Theme {
        name: "Material Palenight",
        syntax: "palenight",
        editor_bg: Color::Rgb(41, 45, 62),
        sidebar_bg: Color::Rgb(36, 39, 55),
        bar_bg: Color::Rgb(33, 36, 50),
        input_bg: Color::Rgb(51, 55, 74),
        popup_bg: Color::Rgb(45, 49, 68),
        border: Color::Rgb(58, 63, 88),
        border_focus: Color::Rgb(199, 146, 234),
        on_accent: Color::Rgb(41, 45, 62),
        selection: Color::Rgb(48, 52, 72),
        selection_focus: Color::Rgb(55, 60, 96),
        current_line_bg: Color::Rgb(50, 55, 76),
        text: Color::Rgb(166, 172, 205),
        text_bright: Color::Rgb(238, 255, 255),
        text_dim: Color::Rgb(128, 135, 168),
        text_faint: Color::Rgb(103, 110, 149),
        guide: Color::Rgb(58, 63, 88),
        accent: Color::Rgb(199, 146, 234),
        folder: Color::Rgb(130, 170, 255),
        modified: Color::Rgb(255, 203, 107),
        added: Color::Rgb(195, 232, 141),
        removed: Color::Rgb(240, 113, 120),
        hunk: Color::Rgb(137, 221, 255),
        added_bg: Color::Rgb(47, 68, 62),
        removed_bg: Color::Rgb(77, 46, 62),
        error: Color::Rgb(255, 83, 112),
        match_bg: Color::Rgb(84, 76, 70),
        match_current_bg: Color::Rgb(132, 106, 52),
        match_fg: Color::Rgb(130, 170, 255),
        inline_code: Color::Rgb(195, 232, 141),
        code_block_bg: Color::Rgb(36, 39, 55),
        link: Color::Rgb(130, 170, 255),
    },
    // Atom "One Dark".
    Theme {
        name: "One Dark",
        syntax: "TwoDark",
        editor_bg: Color::Rgb(40, 44, 52),
        sidebar_bg: Color::Rgb(33, 37, 43),
        bar_bg: Color::Rgb(33, 37, 43),
        input_bg: Color::Rgb(29, 31, 35),
        popup_bg: Color::Rgb(33, 37, 43),
        border: Color::Rgb(62, 68, 81),
        border_focus: Color::Rgb(97, 175, 239),
        on_accent: Color::Rgb(40, 44, 52),
        selection: Color::Rgb(44, 49, 58),
        selection_focus: Color::Rgb(51, 59, 76),
        current_line_bg: Color::Rgb(44, 49, 60),
        text: Color::Rgb(171, 178, 191),
        text_bright: Color::Rgb(215, 218, 224),
        text_dim: Color::Rgb(127, 132, 142),
        text_faint: Color::Rgb(99, 106, 120),
        guide: Color::Rgb(59, 64, 72),
        accent: Color::Rgb(97, 175, 239),
        folder: Color::Rgb(229, 192, 123),
        modified: Color::Rgb(229, 192, 123),
        added: Color::Rgb(152, 195, 121),
        removed: Color::Rgb(224, 108, 117),
        hunk: Color::Rgb(86, 182, 194),
        added_bg: Color::Rgb(45, 64, 52),
        removed_bg: Color::Rgb(72, 44, 52),
        error: Color::Rgb(224, 108, 117),
        match_bg: Color::Rgb(84, 74, 50),
        match_current_bg: Color::Rgb(138, 110, 48),
        match_fg: Color::Rgb(97, 175, 239),
        inline_code: Color::Rgb(152, 195, 121),
        code_block_bg: Color::Rgb(33, 37, 43),
        link: Color::Rgb(97, 175, 239),
    },
    // VS Code "Light Modern".
    Theme {
        name: "Light Modern",
        syntax: "GitHub",
        editor_bg: Color::Rgb(255, 255, 255),
        sidebar_bg: Color::Rgb(248, 248, 248),
        bar_bg: Color::Rgb(242, 242, 242),
        input_bg: Color::Rgb(232, 232, 232),
        popup_bg: Color::Rgb(248, 248, 248),
        border: Color::Rgb(206, 206, 206),
        border_focus: Color::Rgb(0, 95, 184),
        on_accent: Color::Rgb(255, 255, 255),
        selection: Color::Rgb(228, 230, 241),
        selection_focus: Color::Rgb(204, 228, 247),
        current_line_bg: Color::Rgb(240, 240, 240),
        text: Color::Rgb(59, 59, 59),
        text_bright: Color::Rgb(0, 0, 0),
        text_dim: Color::Rgb(97, 97, 97),
        text_faint: Color::Rgb(139, 148, 158),
        guide: Color::Rgb(220, 220, 220),
        accent: Color::Rgb(0, 95, 184),
        folder: Color::Rgb(176, 128, 32),
        modified: Color::Rgb(137, 93, 0),
        added: Color::Rgb(88, 125, 12),
        removed: Color::Rgb(173, 11, 11),
        hunk: Color::Rgb(0, 92, 197),
        added_bg: Color::Rgb(218, 245, 220),
        removed_bg: Color::Rgb(255, 224, 224),
        error: Color::Rgb(205, 49, 49),
        match_bg: Color::Rgb(250, 220, 170),
        match_current_bg: Color::Rgb(240, 180, 90),
        match_fg: Color::Rgb(0, 95, 184),
        inline_code: Color::Rgb(163, 21, 21),
        code_block_bg: Color::Rgb(243, 243, 243),
        link: Color::Rgb(0, 95, 184),
    },
];

#[cfg(not(test))]
static CURRENT: AtomicUsize = AtomicUsize::new(0);

// Tests run in parallel; a per-thread theme keeps one test's theme switch from leaking into
// another (background threads in tests always see the default theme).
#[cfg(test)]
thread_local! {
    static CURRENT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn current() -> &'static Theme {
    &THEMES[current_index()]
}

#[cfg(not(test))]
pub fn current_index() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

#[cfg(not(test))]
pub fn set(index: usize) {
    CURRENT.store(index.min(THEMES.len() - 1), Ordering::Relaxed);
}

#[cfg(test)]
pub fn current_index() -> usize {
    CURRENT.with(|c| c.get())
}

#[cfg(test)]
pub fn set(index: usize) {
    CURRENT.with(|c| c.set(index.min(THEMES.len() - 1)));
}

pub fn index_by_name(name: &str) -> Option<usize> {
    THEMES.iter().position(|t| t.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color) -> f64 {
        let Color::Rgb(r, g, b) = c else { panic!("themes use RGB colors") };
        let channel = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    /// WCAG contrast ratio.
    fn contrast(a: Color, b: Color) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn test_themes_are_readable() {
        let mut problems = Vec::new();
        for t in &THEMES {
            for (what, fg, bg, min) in [
                ("text on editor", t.text, t.editor_bg, 4.5),
                ("text on sidebar", t.text, t.sidebar_bg, 4.5),
                ("text on focused selection", t.text, t.selection_focus, 4.5),
                ("text on selection", t.text, t.selection, 4.5),
                ("dim text on sidebar", t.text_dim, t.sidebar_bg, 3.0),
                ("line numbers on editor", t.text_faint, t.editor_bg, 2.5),
                ("accent on bar", t.accent, t.bar_bg, 3.0),
                ("modified on sidebar", t.modified, t.sidebar_bg, 3.0),
                ("text on match", t.text_bright, t.match_current_bg, 3.0),
                ("toggle label", t.on_accent, t.border_focus, 3.0),
            ] {
                let c = contrast(fg, bg);
                if c < min {
                    problems.push(format!("{}: {} has contrast {:.2} < {}", t.name, what, c, min));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("
"));
    }

    #[test]
    fn test_set_and_find_by_name() {
        assert_eq!(index_by_name("material palenight"), Some(1));
        set(2);
        assert_eq!(current().name, "One Dark");
        set(99);
        assert_eq!(current_index(), THEMES.len() - 1);
        set(0);
    }
}
