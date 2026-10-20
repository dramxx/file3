//! Ctrl+P: fuzzy file finder over a background-built list of project files.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

const MAX_RESULTS: usize = 200;

/// Walks the project (respecting `.gitignore`) on a background thread.
/// The receiver gets one sorted list of relative paths using `/` separators.
pub fn spawn_index(root: PathBuf) -> Receiver<Vec<String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(list_files(&root));
    });
    rx
}

pub fn list_files(root: &Path) -> Vec<String> {
    let mut files: Vec<String> = WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    files.sort_by_key(|f| f.to_lowercase());
    files
}

pub struct FinderItem {
    /// Position in the list that was ranked.
    pub index: usize,
    pub path: String,
    /// Char indices into `path` that matched the query.
    pub indices: Vec<usize>,
    pub recent: bool,
}

pub struct Finder {
    pub input: String,
    pub results: Vec<FinderItem>,
    pub selected: usize,
    pub offset: usize,
    /// Recently opened entries, most recent first; listed first while the query is empty.
    pub recent: Vec<String>,
}

impl Finder {
    pub fn new(files: &[String]) -> Self {
        let mut finder = Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            offset: 0,
            recent: Vec::new(),
        };
        finder.update(files);
        finder
    }

    pub fn with_recent(files: &[String], recent: Vec<String>) -> Self {
        let mut finder = Self::new(&[]);
        finder.recent = recent;
        finder.update(files);
        finder
    }

    pub fn update(&mut self, files: &[String]) {
        self.results = if self.input.trim().is_empty() && !self.recent.is_empty() {
            recent_first(files, &self.recent, MAX_RESULTS)
        } else {
            rank(&self.input, files, MAX_RESULTS)
        };
        self.selected = 0;
        self.offset = 0;
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.results.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last.max(0)) as usize;
    }

    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.results.get(self.selected).map(|r| r.path.as_str())
    }
}

/// The empty-query list: recently opened files that still exist, then the rest in order.
fn recent_first(files: &[String], recent: &[String], limit: usize) -> Vec<FinderItem> {
    let position: std::collections::HashMap<&str, usize> =
        files.iter().enumerate().map(|(i, f)| (f.as_str(), i)).collect();
    let recent_indices: Vec<usize> = recent
        .iter()
        .filter_map(|r| position.get(r.as_str()).copied())
        .collect();
    let rest = (0..files.len()).filter(|i| !recent_indices.contains(i));

    recent_indices
        .iter()
        .map(|&i| (i, true))
        .chain(rest.map(|i| (i, false)))
        .take(limit)
        .map(|(index, recent)| FinderItem {
            index,
            path: files[index].clone(),
            indices: Vec::new(),
            recent,
        })
        .collect()
}

pub fn rank(query: &str, files: &[String], limit: usize) -> Vec<FinderItem> {
    if query.trim().is_empty() {
        return files
            .iter()
            .take(limit)
            .enumerate()
            .map(|(index, f)| FinderItem {
                index,
                path: f.clone(),
                indices: Vec::new(),
                recent: false,
            })
            .collect();
    }

    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();

    let mut scored: Vec<(u32, usize)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            pattern
                .score(Utf32Str::new(f, &mut buf), &mut matcher)
                .map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(files[a.1].len().cmp(&files[b.1].len()))
            .then(files[a.1].cmp(&files[b.1]))
    });
    scored.truncate(limit);

    scored
        .into_iter()
        .map(|(_, i)| {
            let mut indices = Vec::new();
            pattern.indices(Utf32Str::new(&files[i], &mut buf), &mut matcher, &mut indices);
            indices.sort_unstable();
            indices.dedup();
            FinderItem {
                index: i,
                path: files[i].clone(),
                indices: indices.into_iter().map(|i| i as usize).collect(),
                recent: false,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<String> {
        [
            "README.md",
            "src/app.rs",
            "src/ui.rs",
            "src/viewer.rs",
            "tests/app_test.rs",
            "docs/apple.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn test_empty_query_lists_files() {
        assert_eq!(rank("", &files(), 3).len(), 3);
    }

    #[test]
    fn test_fuzzy_match_ranks_best_first() {
        let results = rank("app", &files(), 10);
        assert_eq!(results[0].path, "src/app.rs");
        assert!(results.iter().all(|r| r.path.to_lowercase().contains('a')));
        assert!(!results.iter().any(|r| r.path == "src/ui.rs"));
    }

    #[test]
    fn test_indices_point_at_matched_chars() {
        let results = rank("vw", &files(), 10);
        let item = results.iter().find(|r| r.path == "src/viewer.rs").unwrap();
        let chars: Vec<char> = item.path.chars().collect();
        let matched: String = item.indices.iter().map(|&i| chars[i]).collect();
        assert_eq!(matched.to_lowercase(), "vw");
    }

    #[test]
    fn test_no_match() {
        assert!(rank("zzzz", &files(), 10).is_empty());
    }

    #[test]
    fn test_list_files_respects_gitignore_and_uses_slashes() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();
        std::fs::create_dir_all(temp.path().join("src").join("deep")).unwrap();
        std::fs::create_dir_all(temp.path().join("target")).unwrap();
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();
        std::fs::write(temp.path().join("src").join("deep").join("a.rs"), "").unwrap();
        std::fs::write(temp.path().join("target").join("out.bin"), "").unwrap();
        std::fs::write(temp.path().join(".git").join("HEAD"), "").unwrap();

        assert_eq!(list_files(temp.path()), vec![".gitignore", "src/deep/a.rs"]);
    }

    #[test]
    fn test_recent_files_come_first_until_typing() {
        let recent = vec!["src/ui.rs".to_string(), "gone.rs".to_string(), "README.md".to_string()];
        let mut finder = Finder::with_recent(&files(), recent);
        let paths: Vec<&str> = finder.results.iter().take(3).map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["src/ui.rs", "README.md", "src/app.rs"]);
        assert!(finder.results[1].recent && !finder.results[2].recent);
        assert_eq!(finder.results.len(), files().len());

        finder.input = "viewer".to_string();
        finder.update(&files());
        assert_eq!(finder.results[0].path, "src/viewer.rs");
    }

    #[test]
    fn test_finder_selection() {
        let mut finder = Finder::new(&files());
        finder.move_by(-1);
        assert_eq!(finder.selected, 0);
        finder.move_by(100);
        assert_eq!(finder.selected, files().len() - 1);
        finder.input = "ui".to_string();
        finder.update(&files());
        assert_eq!(finder.selected, 0);
        assert_eq!(finder.selected_path(), Some("src/ui.rs"));
    }
}
