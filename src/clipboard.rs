//! Copying text to the system clipboard without extra dependencies.

use std::io::Write;

/// Copies via the terminal (OSC 52, supported by Windows Terminal and most modern terminals,
/// also over SSH) and, on Windows, via `clip.exe` as well.
pub fn copy(text: &str) {
    let _ = write!(std::io::stdout(), "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = std::io::stdout().flush();

    if cfg!(windows) {
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("clip")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                // clip.exe detects UTF-16LE input; a byte order mark would end up in the clipboard.
                let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
                let _ = stdin.write_all(&bytes);
            }
            let _ = child.wait();
        }
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |acc, &b| (acc << 8) | b as u32) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("src/app.rs:120".as_bytes()), "c3JjL2FwcC5yczoxMjA=");
    }
}
