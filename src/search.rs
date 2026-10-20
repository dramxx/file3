//! Project-wide text search (Ctrl+Shift+F / Ctrl+F), powered by ripgrep's libraries.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::overrides::{Override, OverrideBuilder};
use ignore::{WalkBuilder, WalkState};

const MAX_HITS: usize = 20_000;
const MAX_HITS_PER_FILE: usize = 1_000;
/// Wait this long after the last keystroke before searching.
const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

#[derive(Debug, Clone)]
pub struct Hit {
    /// 1-based, as reported by the searcher.
    pub line_number: usize,
    pub text: String,
    /// Byte ranges of the matches within `text`.
    pub ranges: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct FileHits {
    pub path: PathBuf,
    /// Relative to the project root, `/` separators.
    pub rel: String,
    pub hits: Vec<Hit>,
}

enum Msg {
    File(u64, FileHits),
    Done(u64),
    Error(u64, String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    Query,
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row {
    File(usize),
    Hit(usize, usize),
}

pub struct ProjectSearch {
    pub input: String,
    pub options: SearchOptions,
    /// Comma-separated globs, like VS Code's "files to include" / "files to exclude".
    pub include: String,
    pub exclude: String,
    pub show_filters: bool,
    /// Which input typing goes to.
    pub field: Field,
    pub files: Vec<FileHits>,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub offset: usize,
    pub running: bool,
    pub error: Option<String>,
    /// More hits exist than were collected.
    pub truncated: bool,
    /// The user moved the cursor since the search started; until then it stays on the first hit.
    moved: bool,
    generation: u64,
    cancel: Arc<AtomicBool>,
    pending: Option<Instant>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl ProjectSearch {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            input: String::new(),
            options: SearchOptions::default(),
            include: String::new(),
            exclude: String::new(),
            show_filters: false,
            field: Field::Query,
            files: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            running: false,
            error: None,
            truncated: false,
            moved: false,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: None,
            tx,
            rx,
        }
    }

    /// The text of the input that has the cursor.
    pub fn field_text(&mut self) -> &mut String {
        match self.field {
            Field::Query => &mut self.input,
            Field::Include => &mut self.include,
            Field::Exclude => &mut self.exclude,
        }
    }

    /// Tab order through the visible inputs; returns false when leaving the last one.
    pub fn next_field(&mut self, forward: bool) -> bool {
        let order: &[Field] = if self.show_filters {
            &[Field::Query, Field::Include, Field::Exclude]
        } else {
            &[Field::Query]
        };
        let i = order.iter().position(|f| *f == self.field).unwrap_or(0) as isize;
        let next = i + if forward { 1 } else { -1 };
        if next < 0 || next >= order.len() as isize {
            return false;
        }
        self.field = order[next as usize];
        true
    }

    pub fn toggle_filters(&mut self) {
        self.show_filters = !self.show_filters;
        if !self.show_filters {
            self.field = Field::Query;
        }
    }

    pub fn hit_count(&self) -> usize {
        self.files.iter().map(|f| f.hits.len()).sum()
    }

    /// The query or options changed: search again shortly.
    pub fn changed(&mut self) {
        self.pending = Some(Instant::now());
    }

    /// Starts due searches and collects results. Returns true when something changed.
    pub fn tick(&mut self, root: &Path) -> bool {
        let mut changed = false;
        if self.pending.is_some_and(|t| t.elapsed() >= DEBOUNCE) {
            self.pending = None;
            self.start(root.to_path_buf());
            changed = true;
        }
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::File(g, file) if g == self.generation => {
                    let keep = self.selected_key();
                    let pos = self
                        .files
                        .binary_search_by(|f| f.rel.cmp(&file.rel))
                        .unwrap_or_else(|p| p);
                    self.files.insert(pos, file);
                    self.rebuild_rows(keep);
                    changed = true;
                }
                Msg::Done(g) if g == self.generation => {
                    self.running = false;
                    self.truncated = self.hit_count() >= MAX_HITS;
                    changed = true;
                }
                Msg::Error(g, e) if g == self.generation => {
                    self.running = false;
                    self.error = Some(e);
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }

    /// Whether a debounced search is waiting to start.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn start(&mut self, root: PathBuf) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.files.clear();
        self.rows.clear();
        self.selected = 0;
        self.offset = 0;
        self.error = None;
        self.truncated = false;
        self.moved = false;
        self.pending = None;

        if self.input.is_empty() {
            self.running = false;
            return;
        }
        self.running = true;
        spawn(
            root,
            self.input.clone(),
            self.options,
            (self.include.clone(), self.exclude.clone()),
            self.generation,
            self.cancel.clone(),
            self.tx.clone(),
        );
    }

    /// Identifies the selected hit independently of row positions.
    fn selected_key(&self) -> Option<(String, usize)> {
        if !self.moved {
            return None;
        }
        match self.rows.get(self.selected)? {
            Row::Hit(f, h) => Some((self.files[*f].rel.clone(), *h)),
            Row::File(_) => None,
        }
    }

    /// Results arrive in random order and get sorted in; keeps the cursor on the same hit,
    /// otherwise on the first one.
    fn rebuild_rows(&mut self, keep: Option<(String, usize)>) {
        self.rows.clear();
        for (f, file) in self.files.iter().enumerate() {
            self.rows.push(Row::File(f));
            for h in 0..file.hits.len() {
                self.rows.push(Row::Hit(f, h));
            }
        }
        self.selected = keep
            .and_then(|(rel, h)| {
                self.rows.iter().position(
                    |r| matches!(r, Row::Hit(f, h2) if *h2 == h && self.files[*f].rel == rel),
                )
            })
            .unwrap_or(1)
            .min(self.rows.len().saturating_sub(1));
    }

    pub fn selected_hit(&self) -> Option<(&FileHits, &Hit)> {
        match self.rows.get(self.selected)? {
            Row::Hit(f, h) => {
                let file = &self.files[*f];
                Some((file, &file.hits[*h]))
            }
            Row::File(_) => None,
        }
    }

    /// Moves between hits, skipping file header rows.
    pub fn move_by(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let step = delta.signum();
        let mut remaining = delta.abs();
        let mut i = self.selected as isize;
        let last = self.rows.len() as isize - 1;
        while remaining > 0 {
            let mut j = i + step;
            while (0..=last).contains(&j) && matches!(self.rows[j as usize], Row::File(_)) {
                j += step;
            }
            if !(0..=last).contains(&j) {
                break;
            }
            i = j;
            remaining -= 1;
        }
        self.selected = i as usize;
        self.moved = true;
    }

    /// Selects a row by index (mouse click); a file header selects its first hit.
    pub fn select_row(&mut self, index: usize) {
        match self.rows.get(index) {
            Some(Row::Hit(..)) => self.selected = index,
            Some(Row::File(_)) => self.selected = (index + 1).min(self.rows.len() - 1),
            None => return,
        }
        self.moved = true;
    }

    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        // Keep the file header above the first hit visible too.
        let top = if self.selected > 0 && matches!(self.rows[self.selected - 1], Row::File(_)) {
            self.selected - 1
        } else {
            self.selected
        };
        if top < self.offset {
            self.offset = top;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    pub fn scroll_by(&mut self, delta: isize, height: usize) {
        let max = self.rows.len().saturating_sub(height);
        self.offset = ((self.offset as isize + delta).max(0) as usize).min(max);
    }
}

/// Turns the include / exclude fields into ripgrep-style overrides. A plain name such as
/// `src` also covers everything inside that folder.
pub fn build_overrides(root: &Path, include: &str, exclude: &str) -> Result<Override, String> {
    let mut builder = OverrideBuilder::new(root);
    builder.case_insensitive(true).map_err(|e| e.to_string())?;
    for (list, prefix) in [(include, ""), (exclude, "!")] {
        for glob in list.split(',').map(str::trim).filter(|g| !g.is_empty()) {
            let glob = glob.trim_start_matches("./").trim_end_matches('/');
            builder
                .add(&format!("{}{}", prefix, glob))
                .map_err(|e| e.to_string())?;
            if !glob.contains(['*', '?', '[']) {
                builder
                    .add(&format!("{}{}/**", prefix, glob))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    builder.build().map_err(|e| e.to_string())
}

pub fn build_matcher(query: &str, options: SearchOptions) -> Result<RegexMatcher, String> {
    let pattern = if options.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    RegexMatcherBuilder::new()
        .case_insensitive(!options.case_sensitive)
        .word(options.whole_word)
        .line_terminator(Some(b'\n'))
        .build(&pattern)
        .map_err(|e| e.to_string())
}

fn spawn(
    root: PathBuf,
    query: String,
    options: SearchOptions,
    (include, exclude): (String, String),
    generation: u64,
    cancel: Arc<AtomicBool>,
    tx: Sender<Msg>,
) {
    std::thread::spawn(move || {
        let matcher = match build_matcher(&query, options) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(Msg::Error(generation, first_line(&e)));
                return;
            }
        };
        let overrides = match build_overrides(&root, &include, &exclude) {
            Ok(o) => o,
            Err(e) => {
                let _ = tx.send(Msg::Error(generation, first_line(&e)));
                return;
            }
        };
        let total = Arc::new(AtomicUsize::new(0));

        WalkBuilder::new(&root)
            .hidden(false)
            .require_git(false)
            .overrides(overrides)
            .filter_entry(|e| e.file_name() != ".git")
            .build_parallel()
            .run(|| {
                let tx = tx.clone();
                let matcher = matcher.clone();
                let cancel = cancel.clone();
                let total = total.clone();
                let root = root.clone();
                let mut searcher = SearcherBuilder::new()
                    .binary_detection(BinaryDetection::quit(0))
                    .line_number(true)
                    .build();

                Box::new(move |entry| {
                    if cancel.load(Ordering::Relaxed) || total.load(Ordering::Relaxed) >= MAX_HITS {
                        return WalkState::Quit;
                    }
                    let Ok(entry) = entry else {
                        return WalkState::Continue;
                    };
                    if !entry.file_type().is_some_and(|t| t.is_file()) {
                        return WalkState::Continue;
                    }

                    let mut hits = Vec::new();
                    let _ = searcher.search_path(
                        &matcher,
                        entry.path(),
                        Lossy(|line_number, line| {
                            let text = line.trim_end_matches(['\r', '\n']);
                            let mut ranges = Vec::new();
                            let _ = matcher.find_iter(text.as_bytes(), |m| {
                                if m.start() < m.end() {
                                    ranges.push((m.start(), m.end()));
                                }
                                true
                            });
                            hits.push(Hit {
                                line_number: line_number as usize,
                                text: text.to_string(),
                                ranges,
                            });
                            Ok(hits.len() < MAX_HITS_PER_FILE)
                        }),
                    );

                    if !hits.is_empty() {
                        total.fetch_add(hits.len(), Ordering::Relaxed);
                        let rel = entry
                            .path()
                            .strip_prefix(&root)
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default();
                        let file = FileHits {
                            path: entry.path().to_path_buf(),
                            rel,
                            hits,
                        };
                        if tx.send(Msg::File(generation, file)).is_err() {
                            return WalkState::Quit;
                        }
                    }
                    WalkState::Continue
                })
            });

        let _ = tx.send(Msg::Done(generation));
    });
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("invalid pattern")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn project() -> TempDir {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(
            root.join("src").join("a.rs"),
            "fn main() {\n    let Value = 1;\n    let value_x = 2;\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("src").join("b.rs"), "// value\n").unwrap();
        std::fs::write(root.join("target").join("c.rs"), "value\n").unwrap();
        std::fs::write(root.join("bin.dat"), b"value\0\0").unwrap();
        temp
    }

    fn run(root: &Path, query: &str, options: SearchOptions) -> ProjectSearch {
        let mut s = ProjectSearch::new();
        s.input = query.to_string();
        s.options = options;
        s.start(root.to_path_buf());
        let deadline = Instant::now() + Duration::from_secs(10);
        while s.running && Instant::now() < deadline {
            s.tick(root);
            std::thread::sleep(Duration::from_millis(5));
        }
        s
    }

    #[test]
    fn test_finds_case_insensitive_sorted_and_skips_ignored() {
        let temp = project();
        let s = run(temp.path(), "value", SearchOptions::default());
        let rels: Vec<&str> = s.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(s.hit_count(), 3);
        let first = &s.files[0].hits[0];
        assert_eq!(first.line_number, 2);
        assert_eq!(&first.text[first.ranges[0].0..first.ranges[0].1], "Value");
    }

    #[test]
    fn test_case_sensitive_and_whole_word() {
        let temp = project();
        let opts = SearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        assert_eq!(run(temp.path(), "Value", opts).hit_count(), 1);

        let opts = SearchOptions {
            whole_word: true,
            ..Default::default()
        };
        // `value_x` is not a whole-word match.
        assert_eq!(run(temp.path(), "value", opts).hit_count(), 2);
    }

    #[test]
    fn test_regex_option() {
        let temp = project();
        assert_eq!(
            run(temp.path(), "let \\w+ =", SearchOptions::default()).hit_count(),
            0
        );
        let opts = SearchOptions {
            regex: true,
            ..Default::default()
        };
        assert_eq!(run(temp.path(), "let \\w+ =", opts).hit_count(), 2);
    }

    #[test]
    fn test_invalid_regex_reports_error() {
        let temp = project();
        let opts = SearchOptions {
            regex: true,
            ..Default::default()
        };
        let s = run(temp.path(), "(", opts);
        assert!(s.error.is_some());
    }

    #[test]
    fn test_navigation_skips_file_rows() {
        let temp = project();
        let mut s = run(temp.path(), "value", SearchOptions::default());
        // rows: File(a), Hit, Hit, File(b), Hit
        assert_eq!(s.rows.len(), 5);
        assert_eq!(s.selected, 1);
        s.move_by(1);
        assert_eq!(s.selected, 2);
        s.move_by(1);
        assert_eq!(s.selected, 4);
        s.move_by(1);
        assert_eq!(s.selected, 4);
        s.move_by(-3);
        assert_eq!(s.selected, 1);
        s.select_row(3);
        assert_eq!(s.selected, 4);
        assert_eq!(s.selected_hit().unwrap().0.rel, "src/b.rs");
    }

    fn run_filtered(root: &Path, include: &str, exclude: &str) -> Vec<String> {
        let mut s = ProjectSearch::new();
        s.input = "value".to_string();
        s.include = include.to_string();
        s.exclude = exclude.to_string();
        s.start(root.to_path_buf());
        let deadline = Instant::now() + Duration::from_secs(10);
        while s.running && Instant::now() < deadline {
            s.tick(root);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(s.error.is_none(), "{:?}", s.error);
        s.files.iter().map(|f| f.rel.clone()).collect()
    }

    #[test]
    fn test_include_and_exclude_filters() {
        let temp = project();
        std::fs::create_dir_all(temp.path().join("docs")).unwrap();
        std::fs::write(temp.path().join("docs").join("notes.md"), "value\n").unwrap();
        let root = temp.path();

        assert_eq!(run_filtered(root, "", ""), vec!["docs/notes.md", "src/a.rs", "src/b.rs"]);
        assert_eq!(run_filtered(root, "src", ""), vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(run_filtered(root, "*.md", ""), vec!["docs/notes.md"]);
        assert_eq!(run_filtered(root, "", "b.rs, docs"), vec!["src/a.rs"]);
        assert_eq!(run_filtered(root, "src/**/*.RS", "a.rs"), vec!["src/b.rs"]);
    }

    #[test]
    fn test_tab_moves_through_visible_fields() {
        let mut s = ProjectSearch::new();
        assert!(!s.next_field(true), "only the query field");
        s.toggle_filters();
        assert!(s.next_field(true));
        assert_eq!(s.field, Field::Include);
        s.field_text().push_str("src");
        assert_eq!(s.include, "src");
        assert!(s.next_field(true));
        assert!(!s.next_field(true));
        assert_eq!(s.field, Field::Exclude);
        s.toggle_filters();
        assert_eq!(s.field, Field::Query);
    }

    #[test]
    fn test_empty_query_clears() {
        let temp = project();
        let s = run(temp.path(), "", SearchOptions::default());
        assert!(s.files.is_empty());
        assert!(!s.running);
    }
}
