//! The bundled Nerd Font (CaskaydiaCove NFM, SIL OFL, see `assets/font/LICENSE`) and installing
//! it for the current user, so file icons work without hunting for a font.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};

const FONT: &[u8] = include_bytes!("../assets/font/CaskaydiaCoveNerdFontMono-Regular.ttf");
const FILE_NAME: &str = "CaskaydiaCoveNerdFontMono-Regular.ttf";
/// The name to pick in the terminal's font settings.
pub const FACE: &str = "CaskaydiaCove NFM";

/// Where the font goes for the current user.
fn user_font_dir() -> Option<PathBuf> {
    let env = |name| std::env::var_os(name).map(PathBuf::from);
    if cfg!(windows) {
        env("LOCALAPPDATA").map(|d| d.join(r"Microsoft\Windows\Fonts"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Fonts"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("fonts"))
    }
}

pub fn is_installed() -> bool {
    let system = cfg!(windows)
        .then(|| std::env::var_os("WINDIR").map(|w| PathBuf::from(w).join("Fonts")))
        .flatten();
    [user_font_dir(), system]
        .into_iter()
        .flatten()
        .any(|dir| dir.join(FILE_NAME).is_file())
}

/// Writes the font into the user's font folder and registers it.
pub fn install() -> Result<()> {
    let dir = user_font_dir().ok_or_else(|| anyhow!("no user font folder"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(FILE_NAME);
    std::fs::write(&path, FONT).with_context(|| format!("can't write {}", path.display()))?;

    if cfg!(windows) {
        let status = quiet(Command::new("reg").args([
            "add",
            r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts",
            "/v",
            &format!("{} Regular (TrueType)", FACE),
            "/t",
            "REG_SZ",
            "/d",
            &path.to_string_lossy(),
            "/f",
        ]))
        .status()?;
        if !status.success() {
            return Err(anyhow!("can't register the font"));
        }
    } else if cfg!(not(target_os = "macos")) {
        let _ = quiet(Command::new("fc-cache").arg("-f")).status();
    }
    Ok(())
}

/// Keeps child processes from writing over the TUI.
fn quiet(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
}

/// The settings file of the Windows Terminal we run in, if any.
pub fn windows_terminal_settings() -> Option<PathBuf> {
    std::env::var_os("WT_SESSION")?;
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    [
        r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        r"Microsoft\Windows Terminal\settings.json",
    ]
    .iter()
    .map(|rel| local.join(rel))
    .find(|p| p.is_file())
}

/// Makes the font the default for all Windows Terminal profiles. Keeps a `.bak` copy.
pub fn set_windows_terminal_font(path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut settings: Value =
        serde_json::from_str(&strip_jsonc(&text)).context("can't read Windows Terminal settings")?;
    let defaults = settings
        .get_mut("profiles")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("unexpected Windows Terminal settings layout"))?
        .entry("defaults")
        .or_insert_with(|| Value::Object(Map::new()));
    let font = defaults
        .as_object_mut()
        .ok_or_else(|| anyhow!("unexpected Windows Terminal settings layout"))?
        .entry("font")
        .or_insert_with(|| Value::Object(Map::new()));
    font.as_object_mut()
        .ok_or_else(|| anyhow!("unexpected Windows Terminal settings layout"))?
        .insert("face".into(), FACE.into());

    std::fs::write(path.with_extension("json.bak"), &text)?;
    std::fs::write(path, serde_json::to_string_pretty(&settings)?)?;
    Ok(())
}

/// Removes `//` and `/* */` comments and trailing commas, which Windows Terminal allows.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(n) = next {
                    out.push(n);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' {
            let rest = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(rest, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_jsonc() {
        let text = "{\n // c\n \"a\": \"x // y\", /* b */ \"b\": [1, 2,],\n}";
        let value: Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(value["a"], "x // y");
        assert_eq!(value["b"], serde_json::json!([1, 2]));
    }

    #[test]
    fn test_set_windows_terminal_font() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let text = "{ \"profiles\": { \"defaults\": { \"font\": { \"size\": 14 } }, \"list\": [] } }";
        std::fs::write(&path, text).unwrap();
        set_windows_terminal_font(&path).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["profiles"]["defaults"]["font"]["face"], FACE);
        assert_eq!(value["profiles"]["defaults"]["font"]["size"], 14);
        assert_eq!(std::fs::read_to_string(path.with_extension("json.bak")).unwrap(), text);
    }
}
