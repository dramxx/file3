use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use ratatui::style::Style;
use ratatui::text::Line;
use regex::{Regex, RegexBuilder};

use crate::fs::{self, FileLoad};
use crate::markdown::{is_markdown_file, render_markdown};
use crate::syntax::highlight_code;
use crate::text::{col_of_byte, expand_tabs, line_text, str_width};
use crate::theme;

/// Files longer than this are highlighted on a background thread (it takes ~0.1 ms per line);
/// until that finishes only the first lines are colored.
const SYNC_HIGHLIGHT_LINES: usize = 400;

/// Unchanged lines kept above a change when jumping to it.
const CHANGE_CONTEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Content,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Same,
    Added,
    Removed,
}

/// In-file search started with `/`.
pub struct FindState {
    pub query: String,
    /// Still typing the query.
    pub editing: bool,
    /// (line, start byte, end byte)
    pub matches: Vec<(usize, usize, usize)>,
    pub current: usize,
}

/// A line to draw attention to after jumping to it, with optional byte ranges to highlight.
pub struct Mark {
    pub line: usize,
    pub ranges: Vec<(usize, usize)>,
}

pub struct Viewer {
    pub path: Option<PathBuf>,
    /// Shown instead of content (binary file, folder, read error, ...).
    pub message: Option<String>,
    /// Plain text of each displayed line, tabs expanded.
    pub lines: Vec<String>,
    pub styled: Vec<Line<'static>>,
    /// Show line numbers (off for rendered markdown).
    pub numbered: bool,
    pub mode: ViewMode,
    /// In diff mode, each line's change and its line number in the new file.
    pub diff: Vec<(Change, Option<usize>)>,
    pub markdown_source: bool,
    pub scroll: usize,
    pub hscroll: usize,
    pub wrap: bool,
    pub mark: Option<Mark>,
    pub find: Option<FindState>,
    /// Text area size from the last render.
    pub height: usize,
    pub width: usize,
    longest: usize,
    /// Background highlighting of a long file: (file, mode, theme index, result).
    highlight_rx: Option<(PathBuf, ViewMode, usize, Receiver<Vec<Line<'static>>>)>,
}

impl Viewer {
    pub fn new() -> Self {
        Self {
            path: None,
            message: None,
            lines: Vec::new(),
            styled: Vec::new(),
            numbered: true,
            mode: ViewMode::Content,
            diff: Vec::new(),
            markdown_source: false,
            scroll: 0,
            hscroll: 0,
            wrap: false,
            mark: None,
            find: None,
            height: 20,
            width: 80,
            longest: 0,
            highlight_rx: None,
        }
    }

    pub fn open(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
        self.mode = ViewMode::Content;
        self.markdown_source = false;
        self.scroll = 0;
        self.hscroll = 0;
        self.mark = None;
        self.load();
    }

    /// Opens `path` (unless already shown) and jumps to a 0-based line.
    pub fn open_at(&mut self, path: &Path, line: usize, ranges: Vec<(usize, usize)>) {
        let same = self.path.as_deref() == Some(path) && self.mode == ViewMode::Content;
        if !same {
            self.open(path);
        }
        if self.is_markdown() && !self.markdown_source {
            // Line numbers refer to the source, not the rendered page.
            self.markdown_source = true;
            self.load();
        }
        self.goto_line(line);
        self.mark = Some(Mark { line, ranges });
    }

    pub fn show_message(&mut self, message: impl Into<String>) {
        self.path = None;
        self.mode = ViewMode::Content;
        self.set_lines(Vec::new(), Vec::new(), true);
        self.message = Some(message.into());
        self.scroll = 0;
        self.hscroll = 0;
        self.mark = None;
    }

    pub fn is_markdown(&self) -> bool {
        self.path
            .as_ref()
            .is_some_and(|p| is_markdown_file(&p.to_string_lossy()))
    }

    pub fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Re-reads the file from disk, keeping the scroll position.
    pub fn reload(&mut self) {
        if self.path.is_some() && self.mode == ViewMode::Content {
            self.load();
        }
    }

    /// Swaps in the background highlighting once it is done. Returns true when it did.
    pub fn poll_highlight(&mut self) -> bool {
        let Some((path, mode, theme_index, rx)) = &self.highlight_rx else { return false };
        let Ok(styled) = rx.try_recv() else { return false };
        let current = self.path.as_ref() == Some(path)
            && self.mode == *mode
            && *theme_index == theme::current_index();
        self.highlight_rx = None;
        if current && styled.len() == self.lines.len() {
            self.styled = styled;
            return true;
        }
        false
    }

    fn load(&mut self) {
        let Some(path) = self.path.clone() else { return };
        self.message = None;
        self.highlight_rx = None;
        match fs::load_file(&path) {
            FileLoad::Text(content) => {
                let name = path.to_string_lossy().into_owned();
                if self.is_markdown() && !self.markdown_source {
                    let styled = render_markdown(&content).lines;
                    let lines = styled.iter().map(line_text).collect();
                    self.set_lines(lines, styled, false);
                } else {
                    let lines: Vec<String> = content.lines().map(expand_tabs).collect();
                    let styled = if lines.len() <= SYNC_HIGHLIGHT_LINES {
                        highlight_code(&lines.join("\n"), &name).lines
                    } else {
                        let head = lines[..SYNC_HIGHLIGHT_LINES].join("\n");
                        let mut styled = highlight_code(&head, &name).lines;
                        styled.extend(lines[SYNC_HIGHLIGHT_LINES..].iter().map(|l| Line::from(l.clone())));

                        let (tx, rx) = mpsc::channel();
                        let all = lines.join("\n");
                        std::thread::spawn(move || {
                            let _ = tx.send(highlight_code(&all, &name).lines);
                        });
                        self.highlight_rx = Some((path.clone(), ViewMode::Content, theme::current_index(), rx));
                        styled
                    };
                    self.set_lines(lines, styled, true);
                }
            }
            FileLoad::Binary => self.set_message_for_file("Binary file — not shown"),
            FileLoad::TooLarge(size) => self.set_message_for_file(format!(
                "File is too large to preview ({:.1} MB, limit 1 MB)",
                size as f64 / 1_048_576.0
            )),
            FileLoad::Error(e) => self.set_message_for_file(format!("Can't read file: {}", e)),
        }
    }

    fn set_message_for_file(&mut self, message: impl Into<String>) {
        self.set_lines(Vec::new(), Vec::new(), true);
        self.message = Some(message.into());
    }

    fn set_lines(&mut self, lines: Vec<String>, mut styled: Vec<Line<'static>>, numbered: bool) {
        styled.resize(lines.len(), Line::default());
        self.longest = lines.iter().map(|l| str_width(l)).max().unwrap_or(0);
        self.lines = lines;
        self.styled = styled;
        self.numbered = numbered;
        self.scroll = self.scroll.min(self.max_scroll());
        self.refresh_find();
    }

    pub fn toggle_markdown_source(&mut self) {
        if self.is_markdown() && self.mode == ViewMode::Content {
            self.markdown_source = !self.markdown_source;
            self.scroll = 0;
            self.mark = None;
            self.load();
        }
    }

    /// Shows the whole file with the changes vs HEAD inline, scrolled to the first change.
    pub fn show_diff(&mut self, path: &Path, diff: Option<String>) {
        self.path = Some(path.to_path_buf());
        self.mode = ViewMode::Diff;
        self.markdown_source = false;
        self.scroll = 0;
        self.hscroll = 0;
        self.mark = None;
        self.message = None;
        self.highlight_rx = None;
        let (lines, diff) = parse_diff(diff.as_deref().unwrap_or(""));
        self.diff = diff;
        if lines.is_empty() {
            self.set_lines(Vec::new(), Vec::new(), true);
            self.message = Some("No changes vs HEAD".to_string());
            return;
        }
        let name = path.to_string_lossy().into_owned();
        let styled = if lines.len() <= SYNC_HIGHLIGHT_LINES {
            highlight_diff(&lines, &self.diff, &name)
        } else {
            let (tx, rx) = mpsc::channel();
            let (all, changes) = (lines.clone(), self.diff.clone());
            std::thread::spawn(move || {
                let _ = tx.send(highlight_diff(&all, &changes, &name));
            });
            self.highlight_rx = Some((path.to_path_buf(), ViewMode::Diff, theme::current_index(), rx));
            lines.iter().map(|l| Line::from(l.clone())).collect()
        };
        self.set_lines(lines, styled, true);
        if let Some(&first) = self.change_starts().first() {
            self.scroll = first.saturating_sub(CHANGE_CONTEXT).min(self.max_scroll());
        }
    }

    /// First line of each block of changed lines.
    fn change_starts(&self) -> Vec<usize> {
        (0..self.diff.len())
            .filter(|&i| self.diff[i].0 != Change::Same && (i == 0 || self.diff[i - 1].0 == Change::Same))
            .collect()
    }

    /// Scrolls to the next (or previous) block of changed lines.
    pub fn next_change(&mut self, forward: bool) {
        let starts = self.change_starts();
        // A block jumped to sits CHANGE_CONTEXT lines below the top.
        let anchor = self.scroll + CHANGE_CONTEXT;
        let target = if forward {
            starts.into_iter().find(|&i| i > anchor)
        } else {
            starts.into_iter().rev().find(|&i| i < anchor)
        };
        if let Some(i) = target {
            self.scroll = i.saturating_sub(CHANGE_CONTEXT).min(self.max_scroll());
        }
    }

    pub fn show_content(&mut self) {
        self.mode = ViewMode::Content;
        self.scroll = 0;
        self.load();
    }

    /// The 1-based line the user is looking at: the current find match, the line jumped to,
    /// or the top of the view.
    pub fn current_line(&self) -> usize {
        let line = self
            .find
            .as_ref()
            .and_then(|f| f.matches.get(f.current).map(|m| m.0))
            .or(self.mark.as_ref().map(|m| m.line))
            .unwrap_or(self.scroll);
        line + 1
    }

    pub fn diff_stats(&self) -> (usize, usize) {
        let count = |change| self.diff.iter().filter(|d| d.0 == change).count();
        (count(Change::Added), count(Change::Removed))
    }

    // ---- scrolling ----

    /// Whether long lines wrap: when toggled on, and always in the markdown preview.
    pub fn wraps(&self) -> bool {
        self.wrap || (self.mode == ViewMode::Content && self.is_markdown() && !self.markdown_source)
    }

    pub fn max_scroll(&self) -> usize {
        if self.wraps() {
            self.lines.len().saturating_sub(1)
        } else {
            self.lines.len().saturating_sub(self.height)
        }
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = (self.scroll as isize + delta).clamp(0, self.max_scroll() as isize) as usize;
    }

    pub fn page(&mut self, direction: isize) {
        let step = self.height.saturating_sub(2).max(1) as isize;
        self.scroll_by(direction * step);
    }

    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.max_scroll();
    }

    pub fn hscroll_by(&mut self, delta: isize) {
        if self.wraps() {
            return;
        }
        let max = self.longest.saturating_sub(self.width / 2);
        self.hscroll = (self.hscroll as isize + delta).clamp(0, max as isize) as usize;
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
        self.hscroll = 0;
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Scrolls so the 0-based `line` sits a third of the way down the view.
    pub fn goto_line(&mut self, line: usize) {
        let line = line.min(self.lines.len().saturating_sub(1));
        self.scroll = line.saturating_sub(self.height / 3).min(self.max_scroll());
    }

    /// Makes the given position visible, scrolling only when needed.
    fn reveal(&mut self, line: usize, byte: usize) {
        if line < self.scroll || line >= self.scroll + self.height.max(1) {
            self.goto_line(line);
        }
        if !self.wraps() {
            let col = self
                .lines
                .get(line)
                .map(|l| col_of_byte(l, byte))
                .unwrap_or(0);
            if col < self.hscroll || col >= self.hscroll + self.width.saturating_sub(4) {
                self.hscroll = col.saturating_sub(self.width / 3);
            }
        }
    }

    // ---- find in file ----

    pub fn start_find(&mut self) {
        let query = self.find.take().map(|f| f.query).unwrap_or_default();
        self.find = Some(FindState {
            query,
            editing: true,
            matches: Vec::new(),
            current: 0,
        });
        self.refresh_find();
    }

    pub fn find_input(&mut self, ch: char) {
        if let Some(find) = &mut self.find {
            find.query.push(ch);
        }
        self.refresh_find();
        self.jump_to_first_match_from_scroll();
    }

    pub fn find_backspace(&mut self) {
        if let Some(find) = &mut self.find {
            find.query.pop();
        }
        self.refresh_find();
        self.jump_to_first_match_from_scroll();
    }

    pub fn find_confirm(&mut self) {
        match &mut self.find {
            Some(find) if find.query.is_empty() => self.find = None,
            Some(find) => find.editing = false,
            None => {}
        }
    }

    pub fn find_cancel(&mut self) {
        self.find = None;
    }

    pub fn find_next(&mut self, forward: bool) {
        let Some(find) = &mut self.find else { return };
        if find.matches.is_empty() {
            return;
        }
        let n = find.matches.len();
        find.current = if forward {
            (find.current + 1) % n
        } else {
            (find.current + n - 1) % n
        };
        let (line, start, _) = find.matches[find.current];
        self.reveal(line, start);
    }

    fn refresh_find(&mut self) {
        let Some(find) = &mut self.find else { return };
        find.matches.clear();
        if let Some(re) = smart_case_regex(&find.query) {
            for (i, line) in self.lines.iter().enumerate() {
                for m in re.find_iter(line) {
                    if m.start() < m.end() {
                        find.matches.push((i, m.start(), m.end()));
                    }
                }
            }
        }
        find.current = find.current.min(find.matches.len().saturating_sub(1));
    }

    /// While typing, move to the first match at or below the top of the view.
    fn jump_to_first_match_from_scroll(&mut self) {
        let Some(find) = &mut self.find else { return };
        let Some(i) = find
            .matches
            .iter()
            .position(|&(line, _, _)| line >= self.scroll)
            .or(if find.matches.is_empty() { None } else { Some(0) })
        else {
            return;
        };
        find.current = i;
        let (line, start, _) = find.matches[i];
        self.reveal(line, start);
    }

    /// Byte ranges to highlight on a line: find matches and the jump mark.
    pub fn highlights(&self, line: usize) -> Vec<(usize, usize, Style)> {
        let mut out = Vec::new();
        if let Some(find) = &self.find {
            for (i, &(l, a, b)) in find.matches.iter().enumerate() {
                if l == line {
                    let bg = if i == find.current {
                        theme::current().match_current_bg
                    } else {
                        theme::current().match_bg
                    };
                    out.push((a, b, Style::default().bg(bg)));
                }
            }
        }
        if let Some(mark) = &self.mark {
            if mark.line == line {
                for &(a, b) in &mark.ranges {
                    out.push((a, b, Style::default().bg(theme::current().match_current_bg)));
                }
            }
        }
        out
    }
}

fn smart_case_regex(query: &str) -> Option<Regex> {
    if query.is_empty() {
        return None;
    }
    RegexBuilder::new(&regex::escape(query))
        .case_insensitive(!query.chars().any(char::is_uppercase))
        .build()
        .ok()
}

/// Splits a unified diff into plain lines and, for each, its change and new-file line number.
fn parse_diff(diff: &str) -> (Vec<String>, Vec<(Change, Option<usize>)>) {
    let mut lines = Vec::new();
    let mut changes = Vec::new();
    // Next line number in the new file, once inside a hunk.
    let mut next: Option<usize> = None;
    for line in diff.lines() {
        if line.starts_with("@@") {
            next = line
                .split(' ')
                .find_map(|w| w.strip_prefix('+'))
                .and_then(|w| w.split(',').next()?.parse().ok());
            continue;
        }
        let Some(n) = next.as_mut() else { continue };
        let change = match line.as_bytes().first() {
            Some(b'+') => Change::Added,
            Some(b'-') => Change::Removed,
            Some(b' ') | None => Change::Same,
            _ => continue, // "\ No newline at end of file"
        };
        let number = (change != Change::Removed).then(|| {
            *n += 1;
            *n - 1
        });
        lines.push(expand_tabs(line.get(1..).unwrap_or("")));
        changes.push((change, number));
    }
    (lines, changes)
}

/// Highlights the new and the old version of the file and takes each line from the one it belongs to.
fn highlight_diff(lines: &[String], changes: &[(Change, Option<usize>)], name: &str) -> Vec<Line<'static>> {
    let version = |skip: Change| {
        let text: Vec<&str> = lines
            .iter()
            .zip(changes)
            .filter(|(_, c)| c.0 != skip)
            .map(|(l, _)| l.as_str())
            .collect();
        highlight_code(&text.join("\n"), name).lines.into_iter()
    };
    let mut new = version(Change::Removed);
    let mut old = version(Change::Added);
    lines
        .iter()
        .zip(changes)
        .map(|(line, c)| {
            let styled = match c.0 {
                Change::Same => {
                    old.next();
                    new.next()
                }
                Change::Added => new.next(),
                Change::Removed => old.next(),
            };
            styled.unwrap_or_else(|| Line::from(line.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn viewer_with(content: &str, name: &str) -> (TempDir, Viewer) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(name);
        std::fs::write(&path, content).unwrap();
        let mut v = Viewer::new();
        v.height = 10;
        v.width = 40;
        v.open(&path);
        (temp, v)
    }

    fn numbered_lines(n: usize) -> String {
        (1..=n).map(|i| format!("line {}\n", i)).collect()
    }

    #[test]
    fn test_open_keeps_one_line_per_source_line() {
        let (_t, v) = viewer_with("fn a() {}\n\tlet x = 1;\n", "a.rs");
        assert_eq!(v.lines, vec!["fn a() {}", "    let x = 1;"]);
        assert_eq!(v.styled.len(), 2);
        assert!(v.numbered);
    }

    #[test]
    fn test_long_file_is_highlighted_in_background() {
        let code: String = (0..SYNC_HIGHLIGHT_LINES + 50)
            .map(|i| format!("let x{} = 1;\n", i))
            .collect();
        let (_t, mut v) = viewer_with(&code, "a.rs");
        let last = v.styled.len() - 1;
        assert_eq!(v.styled[last].spans.len(), 1, "tail starts out plain");
        assert!(v.styled[0].spans.len() > 1, "first screen is highlighted right away");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !v.poll_highlight() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(v.styled[last].spans.len() > 1);
    }

    #[test]
    fn test_binary_file_shows_message() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("x.bin");
        std::fs::write(&path, [1, 0, 2]).unwrap();
        let mut v = Viewer::new();
        v.open(&path);
        assert!(v.message.as_deref().unwrap().contains("Binary"));
        assert!(v.lines.is_empty());
    }

    #[test]
    fn test_scroll_is_clamped() {
        let (_t, mut v) = viewer_with(&numbered_lines(30), "a.txt");
        v.scroll_by(100);
        assert_eq!(v.scroll, 20);
        v.scroll_by(-100);
        assert_eq!(v.scroll, 0);
        v.scroll_bottom();
        assert_eq!(v.scroll, 20);
    }

    #[test]
    fn test_goto_line_puts_line_in_upper_third() {
        let (_t, mut v) = viewer_with(&numbered_lines(100), "a.txt");
        v.goto_line(50);
        assert_eq!(v.scroll, 47);
    }

    #[test]
    fn test_find_is_smart_case() {
        let (_t, mut v) = viewer_with("Foo\nfoo\nbar\n", "a.txt");
        v.start_find();
        v.find_input('f');
        v.find_input('o');
        assert_eq!(v.find.as_ref().unwrap().matches.len(), 2);
        v.find_backspace();
        v.find_backspace();
        v.find_input('F');
        assert_eq!(v.find.as_ref().unwrap().matches.len(), 1);
    }

    #[test]
    fn test_find_next_wraps_and_scrolls() {
        let mut content = numbered_lines(60);
        content.push_str("needle\n");
        content.insert_str(0, "needle\n");
        let (_t, mut v) = viewer_with(&content, "a.txt");
        v.start_find();
        for ch in "needle".chars() {
            v.find_input(ch);
        }
        v.find_confirm();
        assert_eq!(v.find.as_ref().unwrap().current, 0);
        v.find_next(true);
        assert_eq!(v.find.as_ref().unwrap().current, 1);
        assert!(v.scroll > 0);
        v.find_next(true);
        assert_eq!(v.find.as_ref().unwrap().current, 0);
    }

    #[test]
    fn test_empty_find_is_dropped_on_confirm() {
        let (_t, mut v) = viewer_with("x\n", "a.txt");
        v.start_find();
        v.find_confirm();
        assert!(v.find.is_none());
    }

    #[test]
    fn test_markdown_renders_and_toggles_source() {
        let (_t, mut v) = viewer_with("# Title\n\ntext\n", "README.md");
        assert!(!v.numbered);
        assert_eq!(v.lines[0], "Title");
        v.toggle_markdown_source();
        assert!(v.numbered);
        assert_eq!(v.lines[0], "# Title");
    }

    #[test]
    fn test_open_at_markdown_uses_source_lines() {
        let (t, mut v) = viewer_with("# Title\n\ntext\n", "README.md");
        v.open_at(&t.path().join("README.md"), 2, vec![(0, 4)]);
        assert_eq!(v.lines[2], "text");
        assert_eq!(v.mark.as_ref().unwrap().line, 2);
    }

    #[test]
    fn test_hscroll_clamped_and_disabled_when_wrapping() {
        let (_t, mut v) = viewer_with(&"x".repeat(100), "a.txt");
        v.hscroll_by(1000);
        assert_eq!(v.hscroll, 80);
        v.toggle_wrap();
        assert_eq!(v.hscroll, 0);
        v.hscroll_by(10);
        assert_eq!(v.hscroll, 0);
    }

    #[test]
    fn test_diff_view() {
        let mut v = Viewer::new();
        let path = Path::new("a.txt");
        v.show_diff(path, Some("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,3 @@\n keep\n-old\n+new\n+more\n\\ No newline at end of file\n".to_string()));
        assert_eq!(v.diff_stats(), (2, 1));
        assert_eq!(v.lines, vec!["keep", "old", "new", "more"]);
        assert_eq!(
            v.diff,
            vec![
                (Change::Same, Some(1)),
                (Change::Removed, None),
                (Change::Added, Some(2)),
                (Change::Added, Some(3)),
            ]
        );
        v.show_diff(path, None);
        assert_eq!(v.message.as_deref(), Some("No changes vs HEAD"));
    }

    #[test]
    fn test_next_change_jumps_between_blocks() {
        let mut v = Viewer::new();
        v.height = 5;
        let mut diff = "@@ -1,40 +1,40 @@\n".to_string();
        for i in 1..=40 {
            if i == 10 || i == 30 {
                diff.push_str(&format!("-line {i}\n+LINE {i}\n"));
            } else {
                diff.push_str(&format!(" line {i}\n"));
            }
        }
        v.show_diff(Path::new("a.txt"), Some(diff));
        // Opens on the first change, a few lines below the top.
        assert_eq!(v.scroll, 9 - CHANGE_CONTEXT);
        v.next_change(true);
        assert_eq!(v.scroll, 30 - CHANGE_CONTEXT);
        v.next_change(true);
        assert_eq!(v.scroll, 30 - CHANGE_CONTEXT);
        v.next_change(false);
        assert_eq!(v.scroll, 9 - CHANGE_CONTEXT);
    }
}
