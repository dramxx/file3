//! F1: command palette, and the color theme picker it leads to.

use crate::finder::Finder;
use crate::theme::THEMES;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    GoToFile,
    SearchInFiles,
    GoToLine,
    GoToSymbol,
    GoBack,
    GoForward,
    CopyPath,
    CopyPathWithLine,
    OpenInEditor,
    ColorTheme,
    ToggleIcons,
    ToggleWrap,
    ToggleIgnored,
    ToggleMarkdownSource,
    ShowChanges,
    ToggleDiff,
    Reload,
    OpenSettings,
    Help,
    Quit,
}

impl Command {
    pub const ALL: &'static [Command] = &[
        Command::GoToFile,
        Command::SearchInFiles,
        Command::GoToLine,
        Command::GoToSymbol,
        Command::GoBack,
        Command::GoForward,
        Command::CopyPath,
        Command::CopyPathWithLine,
        Command::OpenInEditor,
        Command::ColorTheme,
        Command::ToggleIcons,
        Command::ToggleWrap,
        Command::ToggleIgnored,
        Command::ToggleMarkdownSource,
        Command::ShowChanges,
        Command::ToggleDiff,
        Command::Reload,
        Command::OpenSettings,
        Command::Help,
        Command::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Command::GoToFile => "Go to File…",
            Command::SearchInFiles => "Search: Find in Files",
            Command::GoToLine => "Go to Line…",
            Command::GoToSymbol => "Go to Symbol in File…",
            Command::GoBack => "Go Back",
            Command::GoForward => "Go Forward",
            Command::CopyPath => "File: Copy Relative Path",
            Command::CopyPathWithLine => "File: Copy Path with Line Number",
            Command::OpenInEditor => "File: Open in Editor",
            Command::ColorTheme => "Preferences: Color Theme",
            Command::ToggleIcons => "View: Toggle File Icons (Nerd Font)",
            Command::ToggleWrap => "View: Toggle Word Wrap",
            Command::ToggleIgnored => "View: Toggle Ignored Files",
            Command::ToggleMarkdownSource => "Markdown: Toggle Preview / Source",
            Command::ShowChanges => "Git: Show Changed Files",
            Command::ToggleDiff => "Git: Toggle Diff vs HEAD",
            Command::Reload => "File: Reload from Disk",
            Command::OpenSettings => "Preferences: Open Settings File",
            Command::Help => "Help: Keyboard Shortcuts",
            Command::Quit => "Quit",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Command::GoToFile => "Ctrl+P",
            Command::SearchInFiles => "Ctrl+F",
            Command::GoToLine => "Ctrl+G",
            Command::GoToSymbol => "Ctrl+R",
            Command::GoBack => "Alt+←",
            Command::GoForward => "Alt+→",
            Command::CopyPath => "y",
            Command::CopyPathWithLine => "Y",
            Command::OpenInEditor => "e",
            Command::ColorTheme => "",
            Command::ToggleIcons => "i",
            Command::ToggleWrap => "w",
            Command::ToggleIgnored => "h",
            Command::ToggleMarkdownSource => "m",
            Command::ShowChanges => "f",
            Command::ToggleDiff => "d",
            Command::Reload => "r",
            Command::OpenSettings => "",
            Command::Help => "?",
            Command::Quit => "q",
        }
    }
}

pub enum PaletteMode {
    Commands(Vec<Command>),
    /// Picking a theme; `original` is restored on Esc.
    Themes { original: usize },
}

pub struct Palette {
    pub mode: PaletteMode,
    pub labels: Vec<String>,
    pub picker: Finder,
}

impl Palette {
    pub fn commands(available: Vec<Command>) -> Self {
        let labels: Vec<String> = available.iter().map(|c| c.label().to_string()).collect();
        Self {
            picker: Finder::new(&labels),
            labels,
            mode: PaletteMode::Commands(available),
        }
    }

    pub fn themes(current: usize) -> Self {
        let labels: Vec<String> = THEMES.iter().map(|t| t.name.to_string()).collect();
        let mut picker = Finder::new(&labels);
        picker.selected = current;
        Self {
            picker,
            labels,
            mode: PaletteMode::Themes { original: current },
        }
    }

    pub fn title(&self) -> &'static str {
        match self.mode {
            PaletteMode::Commands(_) => "Commands",
            PaletteMode::Themes { .. } => "Color Theme",
        }
    }

    pub fn update(&mut self) {
        self.picker.update(&self.labels);
    }

    /// Index into the full list (commands or themes) of the selected row.
    pub fn selected_index(&self) -> Option<usize> {
        self.picker.results.get(self.picker.selected).map(|r| r.index)
    }

    pub fn selected_command(&self) -> Option<Command> {
        match &self.mode {
            PaletteMode::Commands(commands) => self.selected_index().map(|i| commands[i]),
            PaletteMode::Themes { .. } => None,
        }
    }

    /// Shortcut shown next to a row.
    pub fn key_for(&self, index: usize) -> &'static str {
        match &self.mode {
            PaletteMode::Commands(commands) => commands[index].key(),
            PaletteMode::Themes { .. } => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filtering_commands() {
        let mut p = Palette::commands(Command::ALL.to_vec());
        p.picker.input = "theme".to_string();
        p.update();
        assert_eq!(p.selected_command(), Some(Command::ColorTheme));
    }

    #[test]
    fn test_theme_picker_starts_on_current() {
        let p = Palette::themes(1);
        assert_eq!(p.selected_index(), Some(1));
        assert_eq!(p.selected_command(), None);
    }
}
