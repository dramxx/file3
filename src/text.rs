//! Helpers for styled lines measured in terminal columns.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

pub const TAB_WIDTH: usize = 4;

pub fn char_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Replaces tabs with spaces up to the next tab stop and drops carriage returns.
pub fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') && !line.contains('\r') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + 8);
    let mut col = 0;
    for ch in line.chars() {
        match ch {
            '\t' => {
                let n = TAB_WIDTH - col % TAB_WIDTH;
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            '\r' => {}
            _ => {
                out.push(ch);
                col += char_width(ch);
            }
        }
    }
    out
}

/// Maps a byte offset in a raw line to the same position after `expand_tabs`.
pub fn expanded_offset(raw: &str, byte: usize) -> usize {
    let byte = byte.min(raw.len());
    if raw.is_char_boundary(byte) {
        expand_tabs(&raw[..byte]).len()
    } else {
        0
    }
}

/// Display column at which byte offset `byte` of `s` starts.
pub fn col_of_byte(s: &str, byte: usize) -> usize {
    str_width(&s[..byte.min(s.len())])
}

/// The part of `line` between display columns `start` and `start + width`.
pub fn slice_line(line: &Line<'static>, start: usize, width: usize) -> Line<'static> {
    let end = start + width;
    let mut spans = Vec::new();
    let mut col = 0;
    'outer: for span in &line.spans {
        let mut buf = String::new();
        for ch in span.content.chars() {
            let w = char_width(ch);
            if col >= start && col + w <= end {
                buf.push(ch);
            }
            col += w;
            if col >= end {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf, span.style));
                }
                break 'outer;
            }
        }
        if !buf.is_empty() {
            spans.push(Span::styled(buf, span.style));
        }
    }
    Line::from(spans).style(line.style)
}

/// Splits `line` into pieces at most `width` columns wide, breaking after spaces when possible.
pub fn wrap_line(line: &Line<'static>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let text = line_text(line);
    let mut breaks = Vec::new();
    let mut col = 0;
    let mut row_start = 0;
    let mut last_space: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        let w = char_width(ch);
        if col + w > width && col > 0 {
            let at = match last_space {
                Some(b) if b > row_start => b,
                _ => i,
            };
            breaks.push(at);
            row_start = at;
            col = str_width(&text[at..i]);
            last_space = None;
        }
        col += w;
        if ch == ' ' {
            last_space = Some(i + ch.len_utf8());
        }
    }
    let mut rows = split_at(line, &breaks);
    // Trailing padding that spilled onto rows of its own is not worth a blank row.
    while rows.len() > 1 && rows.last().is_some_and(|r| line_text(r).trim().is_empty()) {
        rows.pop();
    }
    rows
}

/// Cuts a styled line into rows at the given byte offsets.
fn split_at(line: &Line<'static>, breaks: &[usize]) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut offset = 0;
    let mut next = 0;
    for span in &line.spans {
        let text = span.content.as_ref();
        let mut start = 0;
        while next < breaks.len() && breaks[next] <= offset + text.len() {
            let cut = breaks[next] - offset;
            if cut > start {
                current.push(Span::styled(text[start..cut].to_string(), span.style));
            }
            rows.push(Line::from(std::mem::take(&mut current)).style(line.style));
            start = cut;
            next += 1;
        }
        if start < text.len() {
            current.push(Span::styled(text[start..].to_string(), span.style));
        }
        offset += text.len();
    }
    rows.push(Line::from(current).style(line.style));
    rows
}

/// Patches `style` onto the given byte ranges of the line's text.
pub fn overlay(line: &Line<'static>, ranges: &[(usize, usize, Style)]) -> Line<'static> {
    if ranges.is_empty() {
        return line.clone();
    }
    let mut spans = Vec::new();
    let mut offset = 0;
    for span in &line.spans {
        let text = span.content.as_ref();
        let (start, end) = (offset, offset + text.len());
        let mut cuts = vec![start, end];
        for &(a, b, _) in ranges {
            for p in [a, b] {
                if p > start && p < end && text.is_char_boundary(p - start) {
                    cuts.push(p);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mut style = span.style;
            for &(ra, rb, s) in ranges {
                if ra <= a && b <= rb {
                    style = style.patch(s);
                }
            }
            spans.push(Span::styled(text[a - start..b - start].to_string(), style));
        }
        offset = end;
    }
    Line::from(spans).style(line.style)
}

pub fn line_text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// Shortens `s` from the left to fit `width` columns, marking the cut with `…`.
pub fn truncate_left(s: &str, width: usize) -> String {
    if str_width(s) <= width {
        return s.to_string();
    }
    let mut out: Vec<char> = Vec::new();
    let mut col = 1;
    for ch in s.chars().rev() {
        col += char_width(ch);
        if col > width {
            break;
        }
        out.push(ch);
    }
    std::iter::once('…').chain(out.into_iter().rev()).collect()
}

/// Shortens `s` from the right to fit `width` columns, marking the cut with `…`.
pub fn truncate_right(s: &str, width: usize) -> String {
    if str_width(s) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut col = 1;
    for ch in s.chars() {
        col += char_width(ch);
        if col > width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn styled() -> Line<'static> {
        Line::from(vec![
            Span::styled("let ", Style::default().fg(Color::Blue)),
            Span::styled("x = 1;", Style::default().fg(Color::White)),
        ])
    }

    #[test]
    fn test_expand_tabs_uses_tab_stops() {
        assert_eq!(expand_tabs("\tx"), "    x");
        assert_eq!(expand_tabs("ab\tx"), "ab  x");
        assert_eq!(expand_tabs("a\r"), "a");
    }

    #[test]
    fn test_slice_line_keeps_styles() {
        let sliced = slice_line(&styled(), 2, 5);
        assert_eq!(line_text(&sliced), "t x =");
        assert_eq!(sliced.spans.len(), 2);
        assert_eq!(sliced.spans[0].style.fg, Some(Color::Blue));
    }

    #[test]
    fn test_slice_line_past_end_is_empty() {
        assert_eq!(line_text(&slice_line(&styled(), 50, 5)), "");
    }

    #[test]
    fn test_slice_line_wide_chars() {
        let line = Line::from("a世界b");
        assert_eq!(line_text(&slice_line(&line, 1, 4)), "世界");
        // A wide char cut in half is dropped rather than overflowing.
        assert_eq!(line_text(&slice_line(&line, 1, 3)), "世");
    }

    #[test]
    fn test_wrap_line() {
        let rows = wrap_line(&styled(), 4);
        let texts: Vec<String> = rows.iter().map(line_text).collect();
        assert_eq!(texts, vec!["let ", "x = ", "1;"]);
    }

    #[test]
    fn test_wrap_breaks_at_words_and_keeps_styles() {
        let line = Line::from(vec![
            Span::styled("hello ", Style::default().fg(Color::Red)),
            Span::raw("world foo"),
        ]);
        let rows = wrap_line(&line, 8);
        let texts: Vec<String> = rows.iter().map(line_text).collect();
        assert_eq!(texts, vec!["hello ", "world ", "foo"]);
        assert_eq!(rows[0].spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn test_wrap_drops_rows_of_trailing_spaces() {
        let texts: Vec<String> = wrap_line(&Line::from("ab cd          "), 5)
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(texts, vec!["ab ", "cd   "]);
    }

    #[test]
    fn test_wrap_long_word_is_cut() {
        let texts: Vec<String> = wrap_line(&Line::from("abcdefghij"), 4)
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(texts, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn test_wrap_empty_line_gives_one_row() {
        assert_eq!(wrap_line(&Line::from(""), 10).len(), 1);
    }

    #[test]
    fn test_overlay_splits_spans() {
        let hl = Style::default().bg(Color::Red);
        let out = overlay(&styled(), &[(2, 5, hl)]);
        assert_eq!(line_text(&out), "let x = 1;");
        let marked: String = out
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(Color::Red))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(marked, "t x");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate_left("src/app/main.rs", 8), "…main.rs");
        assert_eq!(truncate_right("hello world", 6), "hello…");
        assert_eq!(truncate_right("hi", 6), "hi");
    }
}
