//! Settings in `%APPDATA%\file3\config.toml` (`~/.config/file3/config.toml` elsewhere), and
//! per-project state next to it.
//!
//! Only flat `key = value` lines are used, so a small parser is enough.

use std::path::{Path, PathBuf};

pub const DEFAULT_EDITOR: &str = "code --goto {file}:{line}";

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub theme: String,
    pub nerd_font: bool,
    pub wrap: bool,
    /// Command for "open in editor"; `{file}` and `{line}` are filled in.
    pub editor: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: crate::theme::THEMES[0].name.to_string(),
            nerd_font: false,
            wrap: false,
            editor: DEFAULT_EDITOR.to_string(),
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FILE3_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("file3"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|d| d.join("file3"))
    }
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

impl Config {
    pub fn load() -> Self {
        config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for (key, value) in pairs(text) {
            match key.as_str() {
                "theme" => config.theme = value,
                "nerd_font" => config.nerd_font = value == "true",
                "wrap" => config.wrap = value == "true",
                "editor" => config.editor = value,
                _ => {}
            }
        }
        config
    }

    pub fn to_text(&self) -> String {
        format!(
            "# file3 settings (F1 → commands change these too)\n\
             theme = {}\n\
             nerd_font = {}\n\
             wrap = {}\n\
             # {{file}} and {{line}} are replaced; e.g. \"notepad++ -n{{line}} {{file}}\"\n\
             editor = {}\n",
            quote(&self.theme),
            self.nerd_font,
            self.wrap,
            quote(&self.editor),
        )
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = config_path() else {
            return Ok(());
        };
        write_file(&path, &self.to_text())
    }
}

/// Remembered state of one project folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectState {
    /// Expanded folders, relative to the project root with `/` separators.
    pub expanded: Vec<String>,
    pub file: Option<String>,
    pub scroll: usize,
    /// Most recent first.
    pub recent: Vec<String>,
}

impl ProjectState {
    pub fn path_for(root: &Path) -> Option<PathBuf> {
        let key = root.to_string_lossy().to_lowercase();
        // FNV-1a: stable across runs and Rust versions, unlike `DefaultHasher`.
        let hash = key
            .bytes()
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
        config_dir().map(|d| d.join("projects").join(format!("{:016x}.txt", hash)))
    }

    pub fn load(root: &Path) -> Self {
        Self::path_for(root)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut state = Self::default();
        for (key, value) in pairs(text) {
            match key.as_str() {
                "expanded" => state.expanded.push(value),
                "file" => state.file = Some(value),
                "scroll" => state.scroll = value.parse().unwrap_or(0),
                "recent" => state.recent.push(value),
                _ => {}
            }
        }
        state
    }

    pub fn to_text(&self, root: &Path) -> String {
        let mut out = format!("root = {}\n", quote(&root.to_string_lossy()));
        if let Some(file) = &self.file {
            out.push_str(&format!("file = {}\nscroll = {}\n", quote(file), self.scroll));
        }
        for e in &self.expanded {
            out.push_str(&format!("expanded = {}\n", quote(e)));
        }
        for r in &self.recent {
            out.push_str(&format!("recent = {}\n", quote(r)));
        }
        out
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let Some(path) = Self::path_for(root) else {
            return Ok(());
        };
        write_file(&path, &self.to_text(root))
    }
}

fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

/// `key = value` lines; `#` starts a comment line, string values may be quoted.
fn pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.starts_with('#') {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        Some((key.trim().to_string(), unquote(value.trim())))
    })
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn unquote(s: &str) -> String {
    match s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(inner) => {
            let mut out = String::new();
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    out.extend(chars.next());
                } else {
                    out.push(c);
                }
            }
            out
        }
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_round_trip() {
        let config = Config {
            theme: "Material Palenight".to_string(),
            nerd_font: true,
            wrap: true,
            editor: r#"C:\Program Files\x.exe "{file}""#.to_string(),
        };
        assert_eq!(Config::parse(&config.to_text()), config);
    }

    #[test]
    fn test_config_defaults_for_missing_and_unknown_keys() {
        let config = Config::parse("# comment\nwrap = true\nunknown = 1\n");
        assert!(config.wrap);
        assert_eq!(config.editor, DEFAULT_EDITOR);
        assert_eq!(config.theme, "Dark Modern");
    }

    #[test]
    fn test_project_state_round_trip() {
        let state = ProjectState {
            expanded: vec!["src".into(), "src/app".into()],
            file: Some("src/main.rs".into()),
            scroll: 42,
            recent: vec!["a.rs".into(), "b \"q\".rs".into()],
        };
        let root = Path::new("C:\\proj");
        assert_eq!(ProjectState::parse(&state.to_text(root)), state);
    }

    #[test]
    fn test_project_state_file_name_is_stable() {
        let a = ProjectState::path_for(Path::new("C:\\DEV\\x"));
        let b = ProjectState::path_for(Path::new("c:\\dev\\X"));
        assert_eq!(a, b);
    }
}
