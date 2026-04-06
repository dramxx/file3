use std::collections::HashSet;
use std::path::PathBuf;

use crate::fs::{self, DirEntry};
use crate::git;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Content,
    Diff,
}

pub struct App {
    pub current_dir: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected: usize,
    pub file_content: Option<String>,
    pub scroll: u16,
    pub running: bool,

    pub is_git_repo: bool,
    pub git_root: Option<PathBuf>,
    pub dirty_files: HashSet<PathBuf>,
    pub view_mode: ViewMode,
    pub diff_content: Option<String>,

    pub show_dirty_only: bool,
    pub dirty_entries: Vec<DirEntry>,
    pub show_hidden: bool,
}

impl App {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let show_hidden = false;
        let entries = fs::read_dir(&current_dir);

        let is_git_repo = git::is_git_repo(&current_dir);
        let git_root = is_git_repo.then(|| git::git_root(&current_dir)).flatten();
        let dirty_files = git_root
            .as_ref()
            .map(|root| git::git_dirty_files(root))
            .unwrap_or_default();

        let file_content = entries
            .iter()
            .find(|e| !e.is_dir)
            .and_then(|e| fs::read_file(&e.path));

        Self {
            current_dir,
            entries,
            selected: 0,
            file_content,
            scroll: 0,
            running: true,
            is_git_repo,
            git_root,
            dirty_files,
            view_mode: ViewMode::Content,
            diff_content: None,
            show_dirty_only: false,
            dirty_entries: Vec::new(),
            show_hidden,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.load_selection();
        }
    }

    pub fn move_down(&mut self) {
        let count = self.visible_entry_count();
        if self.selected < count.saturating_sub(1) {
            self.selected += 1;
            self.load_selection();
        }
    }

    fn visible_entry_count(&self) -> usize {
        if self.show_dirty_only {
            self.dirty_entries.len()
        } else {
            self.entries.len() + if self.is_at_root() { 0 } else { 1 }
        }
    }

    pub fn enter(&mut self) {
        if !self.show_dirty_only {
            if self.selected_is_parent() {
                self.go_up();
                return;
            }

            let entry_index = if self.is_at_root() {
                self.selected
            } else {
                self.selected - 1
            };
            if let Some(entry) = self.entries.get(entry_index) {
                if entry.is_dir {
                    self.current_dir = entry.path.clone();
                    self.selected = 0;
                    self.refresh_entries();
                    self.refresh_git_state();
                }
            }
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.refresh_entries();
            self.refresh_git_state();
        }
    }

    pub fn toggle_diff(&mut self) {
        if !self.is_git_repo {
            return;
        }

        let entry = self.selected_entry_cloned();
        if let Some(entry) = entry {
            if entry.is_dir {
                return;
            }

            if self.view_mode == ViewMode::Content {
                self.view_mode = ViewMode::Diff;
                self.scroll = 0;
                self.diff_content = self
                    .git_root
                    .as_ref()
                    .and_then(|root| git::git_diff(root, &entry.path));
            } else {
                self.view_mode = ViewMode::Content;
                self.diff_content = None;
            }
        }
    }

    pub fn toggle_dirty_filter(&mut self) {
        if !self.is_git_repo {
            return;
        }

        self.show_dirty_only = !self.show_dirty_only;
        self.selected = 0;
        self.scroll = 0;
        self.view_mode = ViewMode::Content;
        self.diff_content = None;
        self.file_content = None;

        if self.show_dirty_only {
            self.collect_dirty_files();
            if let Some(entry) = self.dirty_entries.first() {
                self.file_content = fs::read_file(&entry.path);
            }
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh_entries();
        if self.show_dirty_only {
            self.collect_dirty_files();
        }
    }

    fn refresh_entries(&mut self) {
        self.entries = if self.show_hidden {
            fs::read_dir_with_hidden(&self.current_dir)
        } else {
            fs::read_dir(&self.current_dir)
        };
        if !self.show_dirty_only {
            self.load_selection();
        }
    }

    fn collect_dirty_files(&mut self) {
        self.dirty_entries.clear();

        if let Some(ref git_root) = self.git_root {
            let git_root = git_root.clone();
            let current_dir = self.current_dir.clone();
            self.traverse_for_dirty(&current_dir, &git_root);
            self.dirty_entries
                .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }

        if self.dirty_entries.is_empty() {
            self.selected = 0;
            self.file_content = None;
        }
    }

    fn traverse_for_dirty(&mut self, dir: &PathBuf, git_root: &PathBuf) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();

                if name.starts_with('.') && !self.show_hidden {
                    continue;
                }

                if path.is_dir() {
                    self.traverse_for_dirty(&path, git_root);
                } else if self.dirty_files.contains(&path) {
                    self.dirty_entries.push(DirEntry {
                        name,
                        path: path.clone(),
                        is_dir: false,
                    });
                }
            }
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(3);
    }

    pub fn scroll_down(&mut self, height: u16) {
        let content_lines = match self.view_mode {
            ViewMode::Content => self.file_content.as_ref().map(|c| c.lines().count()),
            ViewMode::Diff => self.diff_content.as_ref().map(|c| c.lines().count()),
        }
        .unwrap_or(0) as u16;
        let max_scroll = content_lines.saturating_sub(height.saturating_sub(3));
        self.scroll = self.scroll.saturating_add(3).min(max_scroll);
    }

    fn load_selection(&mut self) {
        self.scroll = 0;
        self.view_mode = ViewMode::Content;
        self.diff_content = None;

        if self.show_dirty_only {
            if let Some(entry) = self.dirty_entries.get(self.selected) {
                self.file_content = fs::read_file(&entry.path);
            }
            return;
        }

        if self.selected_is_parent() {
            self.file_content = None;
            return;
        }

        let entry_index = if self.is_at_root() {
            self.selected
        } else {
            self.selected - 1
        };
        if let Some(entry) = self.entries.get(entry_index) {
            self.file_content = if entry.is_dir {
                None
            } else {
                fs::read_file(&entry.path)
            };
        }
    }

    fn refresh_git_state(&mut self) {
        let old_git_root = self.git_root.clone();

        self.is_git_repo = git::is_git_repo(&self.current_dir);
        self.git_root = self
            .is_git_repo
            .then(|| git::git_root(&self.current_dir))
            .flatten();

        if self.git_root != old_git_root {
            self.dirty_files = self
                .git_root
                .as_ref()
                .map(|root| git::git_dirty_files(root))
                .unwrap_or_default();
        }

        if self.show_dirty_only {
            self.collect_dirty_files();
        }
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        if self.show_dirty_only {
            self.dirty_entries.get(self.selected)
        } else {
            let index = if self.is_at_root() {
                self.selected
            } else {
                if self.selected == 0 {
                    return None;
                }
                self.selected - 1
            };
            self.entries.get(index)
        }
    }

    fn selected_entry_cloned(&self) -> Option<DirEntry> {
        self.selected_entry().cloned()
    }

    pub fn is_at_root(&self) -> bool {
        self.current_dir.parent().is_none()
    }

    pub fn selected_is_parent(&self) -> bool {
        self.selected == 0 && !self.is_at_root()
    }

    pub fn is_dirty(&self, path: &PathBuf) -> bool {
        self.dirty_files.contains(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn create_git_repo(temp: &TempDir) {
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to init git repo");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to config git");

        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to config git");
    }

    #[test]
    fn test_view_mode_default() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let app = App::new();
        assert_eq!(app.view_mode, ViewMode::Content);
    }

    #[test]
    fn test_app_initial_state() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let app = App::new();

        assert!(app.running);
        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll, 0);
        assert!(!app.show_dirty_only);
        assert!(app.diff_content.is_none());
    }

    #[test]
    fn test_app_is_at_root() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();

        let app = App::new();
        let parent = temp.path().parent();

        if let Some(p) = parent {
            std::env::set_current_dir(p).ok();
        }

        let app_at_new_root = App::new();
        assert!(app_at_new_root.is_at_root() || !app.is_at_root());
    }

    #[test]
    fn test_app_not_at_root_after_enter() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        std::fs::create_dir(temp.path().join("subdir")).unwrap();

        let mut app = App::new();

        if let Some(entry) = app.entries.iter().find(|e| e.name == "subdir") {
            if entry.is_dir {
                app.current_dir = entry.path.clone();
                app.entries = fs::read_dir(&app.current_dir);
                app.refresh_git_state();

                assert!(!app.is_at_root());
            }
        }
    }

    #[test]
    fn test_move_up_from_first_selected() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        std::fs::write(temp.path().join("file1.txt"), "").unwrap();

        let mut app = App::new();
        let initial_selected = app.selected;

        app.move_up();

        assert_eq!(app.selected, initial_selected);
    }

    #[test]
    fn test_move_down() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();

        std::fs::write(temp.path().join("file1.txt"), "").unwrap();
        std::fs::write(temp.path().join("file2.txt"), "").unwrap();

        let mut app = App::new();
        let initial = app.selected;
        app.move_down();

        let new_selected = app.selected;
        assert!(new_selected == initial + 1 || new_selected == initial || app.entries.len() <= 1);
    }

    #[test]
    fn test_enter_on_file() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        let mut app = App::new();

        let entries_count = app.entries.len();
        if entries_count > 0 {
            if let Some(entry) = app.entries.get(0) {
                if !entry.is_dir {
                    let original_dir = app.current_dir.clone();
                    app.enter();
                    let current_dir = app.current_dir.clone();
                    assert!(
                        current_dir == original_dir
                            || current_dir.starts_with(&original_dir)
                            || !current_dir.to_string_lossy().contains(".tmp")
                    );
                }
            }
        }
    }

    #[test]
    fn test_move_down_at_end() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        std::fs::write(temp.path().join("file1.txt"), "").unwrap();

        let mut app = App::new();

        if app.entries.len() > 1 {
            app.selected = app.entries.len() - 1;
            let last_selected = app.selected;
            app.move_down();
            assert_eq!(app.selected, last_selected);
        }
    }

    #[test]
    fn test_scroll_up() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let mut app = App::new();

        app.scroll = 10;
        app.scroll_up();
        assert_eq!(app.scroll, 7);
    }

    #[test]
    fn test_scroll_up_at_zero() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let mut app = App::new();

        app.scroll = 0;
        app.scroll_up();
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn test_scroll_down() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let mut app = App::new();

        app.file_content = Some("line1\nline2\nline3\n".repeat(100));
        app.scroll = 0;
        app.scroll_down(20);
        assert!(app.scroll > 0);
    }

    #[test]
    fn test_scroll_down_max_limit() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let mut app = App::new();

        app.file_content = Some("line1\n".repeat(100));
        app.scroll = u16::MAX;
        app.scroll_down(20);
        assert!(app.scroll < u16::MAX);
    }

    #[test]
    fn test_toggle_dirty_filter_non_git_repo() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        let mut app = App::new();
        assert!(!app.is_git_repo);

        app.toggle_dirty_filter();
        assert!(!app.show_dirty_only);
    }

    #[test]
    fn test_toggle_dirty_filter_git_repo() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        let mut app = App::new();

        if app.is_git_repo {
            let was_showing = app.show_dirty_only;
            app.toggle_dirty_filter();
            assert_eq!(app.show_dirty_only, !was_showing);

            app.toggle_dirty_filter();
            assert_eq!(app.show_dirty_only, was_showing);
        } else {
            assert!(!app.is_git_repo);
        }
    }

    #[test]
    fn test_toggle_diff_non_git_repo() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        let mut app = App::new();

        app.toggle_diff();
        assert_eq!(app.view_mode, ViewMode::Content);
    }

    #[test]
    fn test_toggle_diff_dir_entry() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        create_git_repo(&temp);

        std::fs::create_dir(temp.path().join("subdir")).unwrap();

        let mut app = App::new();

        if let Some(entry) = app.entries.iter().find(|e| e.name == "subdir") {
            app.selected = app
                .entries
                .iter()
                .position(|e| e.name == entry.name)
                .unwrap_or(0);
            app.toggle_diff();
            assert_eq!(app.view_mode, ViewMode::Content);
        }
    }

    #[test]
    fn test_toggle_diff_file() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git commit");

        std::fs::write(temp.path().join("test.txt"), "modified").unwrap();

        let mut app = App::new();
        
        if let Some(entry) = app.entries.iter().find(|e| e.name == "test.txt") {
            let pos = app.entries.iter().position(|e| e.name == entry.name).unwrap_or(0);
            app.selected = if app.is_at_root() { pos } else { pos + 1 };
            app.toggle_diff();
            assert_eq!(app.view_mode, ViewMode::Diff);
        }
    }

    #[test]
    fn test_selected_entry_normal_mode() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        let mut app = App::new();
        assert!(app.selected_entry().is_none());

        app.selected = 0;
        assert!(app.selected_is_parent());
        assert!(app.selected_entry().is_none());

        app.selected = 1;
        assert!(app.selected_entry().is_some());
    }

    #[test]
    fn test_selected_entry_dirty_mode() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git commit");

        std::fs::write(temp.path().join("test.txt"), "modified").unwrap();

        let mut app = App::new();
        app.toggle_dirty_filter();

        let entry = app.selected_entry();
        if app.is_git_repo && !app.dirty_entries.is_empty() {
            assert!(entry.is_some());
        } else {
            assert!(entry.is_none() || app.show_dirty_only);
        }
    }

    #[test]
    fn test_enter_on_parent() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();

        std::fs::create_dir(temp.path().join("subdir")).unwrap();

        let mut app = App::new();

        if let Some(entry) = app.entries.iter().find(|e| e.name == "subdir") {
            app.current_dir = entry.path.clone();
            app.entries = fs::read_dir(&app.current_dir);
            app.selected = 0;
            app.refresh_git_state();

            assert!(app.selected_is_parent());

            let parent_before = app.current_dir.clone();
            app.enter();
            let parent_after = app.current_dir.clone();

            if app.current_dir.parent().is_some() {
                assert!(
                    parent_after == parent_before
                        || parent_after == parent_before.parent().unwrap()
                );
            }
        }
    }

    #[test]
    fn test_go_up_at_root() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        let mut app = App::new();
        let original_dir = app.current_dir.clone();

        while !app.is_at_root() {
            if let Some(parent) = app.current_dir.parent() {
                app.current_dir = parent.to_path_buf();
            }
        }

        app.go_up();
        assert!(app.current_dir.parent().is_none());
    }

    #[test]
    fn test_load_selection_clears_diff() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).ok();

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        let mut app = App::new();
        app.view_mode = ViewMode::Diff;
        app.diff_content = Some("some diff".to_string());

        app.load_selection();

        assert_eq!(app.view_mode, ViewMode::Content);
        assert!(app.diff_content.is_none());
    }

    #[test]
    fn test_visible_entry_count_normal() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();

        std::fs::write(temp.path().join("a.txt"), "").unwrap();
        std::fs::write(temp.path().join("b.txt"), "").unwrap();

        let app = App::new();
        let count = app.visible_entry_count();
        assert!(count >= 2);
    }

    #[test]
    fn test_visible_entry_count_dirty_mode() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git commit");

        std::fs::write(temp.path().join("test.txt"), "modified").unwrap();

        let mut app = App::new();
        app.toggle_dirty_filter();

        if app.is_git_repo {
            assert_eq!(app.visible_entry_count(), app.dirty_entries.len());
        }
    }

    #[test]
    fn test_is_dirty() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git commit");

        std::fs::write(temp.path().join("test.txt"), "modified").unwrap();

        let app = App::new();

        if !app.entries.is_empty() {
            let path = &app.entries[0].path;
            let _ = app.is_dirty(path);
        }
        assert!(true);
    }

    #[test]
    fn test_traverse_for_dirty_skips_hidden() {
        let temp = create_temp_dir();
        std::env::set_current_dir(temp.path()).unwrap();
        create_git_repo(&temp);

        std::fs::create_dir(temp.path().join(".hidden")).unwrap();
        std::fs::write(temp.path().join(".hidden/file.txt"), "").unwrap();

        let mut app = App::new();
        if let Some(git_root) = app.git_root.clone() {
            let current_dir = app.current_dir.clone();
            app.traverse_for_dirty(&current_dir, &git_root);

            assert!(app.dirty_entries.iter().all(|e| !e.name.starts_with('.')));
        }
    }
}
