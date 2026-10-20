//! File icons. Nerd Font glyphs when enabled, otherwise nothing (folders still get a chevron).

use ratatui::style::Color;

const DEFAULT: (&str, Color) = ("\u{f15b}", Color::Rgb(157, 157, 157));

/// Icon and color for a file, by name. Only meaningful with a Nerd Font.
pub fn file_icon(name: &str) -> (&'static str, Color) {
    let lower = name.to_lowercase();
    match lower.as_str() {
        ".gitignore" | ".gitattributes" | ".gitmodules" => {
            return ("\u{e702}", Color::Rgb(241, 80, 47))
        }
        "dockerfile" => return ("\u{f308}", Color::Rgb(56, 151, 240)),
        "makefile" => return ("\u{e615}", Color::Rgb(157, 157, 157)),
        "cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" => {
            return ("\u{f023}", Color::Rgb(157, 157, 157))
        }
        _ => {}
    }

    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "rs" => ("\u{e7a8}", Color::Rgb(222, 165, 132)),
        "py" => ("\u{e73c}", Color::Rgb(255, 213, 59)),
        "js" | "mjs" | "cjs" => ("\u{e74e}", Color::Rgb(241, 224, 90)),
        "jsx" | "tsx" => ("\u{e7ba}", Color::Rgb(97, 218, 251)),
        "ts" | "mts" | "cts" => ("\u{e628}", Color::Rgb(49, 120, 198)),
        "json" | "jsonc" => ("\u{e60b}", Color::Rgb(203, 203, 65)),
        "md" | "markdown" | "mdown" => ("\u{e609}", Color::Rgb(66, 165, 245)),
        "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "env" => {
            ("\u{e615}", Color::Rgb(157, 157, 157))
        }
        "html" | "htm" => ("\u{e736}", Color::Rgb(228, 79, 57)),
        "css" | "scss" | "sass" | "less" => ("\u{e749}", Color::Rgb(66, 165, 245)),
        "go" => ("\u{e627}", Color::Rgb(0, 173, 216)),
        "c" | "h" => ("\u{e61e}", Color::Rgb(85, 154, 211)),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => ("\u{e61d}", Color::Rgb(85, 154, 211)),
        "cs" => ("\u{f031b}", Color::Rgb(104, 33, 122)),
        "java" => ("\u{e738}", Color::Rgb(204, 62, 68)),
        "kt" | "kts" => ("\u{e634}", Color::Rgb(127, 82, 255)),
        "swift" => ("\u{e755}", Color::Rgb(240, 81, 56)),
        "rb" => ("\u{e739}", Color::Rgb(204, 52, 45)),
        "php" => ("\u{e73d}", Color::Rgb(137, 147, 190)),
        "sh" | "bash" | "zsh" | "ps1" | "bat" | "cmd" => ("\u{f489}", Color::Rgb(137, 224, 81)),
        "sql" => ("\u{f1c0}", Color::Rgb(218, 216, 216)),
        "lock" => ("\u{f023}", Color::Rgb(157, 157, 157)),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "webp" | "bmp" => {
            ("\u{f1c5}", Color::Rgb(160, 116, 196))
        }
        "txt" | "log" => ("\u{f15c}", Color::Rgb(157, 157, 157)),
        "zip" | "tar" | "gz" | "7z" | "rar" => ("\u{f410}", Color::Rgb(175, 180, 43)),
        _ => DEFAULT,
    }
}

pub fn folder_icon(open: bool) -> &'static str {
    if open {
        "\u{f07c}"
    } else {
        "\u{f07b}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icons_by_extension_and_name() {
        assert_eq!(file_icon("main.RS").0, "\u{e7a8}");
        assert_eq!(file_icon(".gitignore").0, "\u{e702}");
        assert_eq!(file_icon("noext"), DEFAULT);
    }
}
