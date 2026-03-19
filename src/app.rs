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
}

impl App {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
                    self.entries = fs::read_dir(&self.current_dir);
                    self.selected = 0;
                    self.file_content = None;
                    self.scroll = 0;
                    self.view_mode = ViewMode::Content;
                    self.diff_content = None;
                    self.refresh_git_state();
                }
            }
        }
    }

    pub fn go_up(&mut self) {
        if !self.show_dirty_only {
            if let Some(parent) = self.current_dir.parent() {
                self.current_dir = parent.to_path_buf();
                self.entries = fs::read_dir(&self.current_dir);
                self.selected = 0;
                self.file_content = None;
                self.scroll = 0;
                self.view_mode = ViewMode::Content;
                self.diff_content = None;
                self.refresh_git_state();
            }
        }
    }

    pub fn toggle_diff(&mut self) {
        if !self.is_git_repo || self.show_dirty_only {
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
                if self.diff_content.is_none() {
                    self.diff_content = self
                        .git_root
                        .as_ref()
                        .and_then(|root| git::git_diff(root, &entry.path));
                }
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

    fn collect_dirty_files(&mut self) {
        self.dirty_entries.clear();

        if let Some(ref git_root) = self.git_root {
            let git_root = git_root.clone();
            let current_dir = self.current_dir.clone();
            self.traverse_for_dirty(&current_dir, &git_root);
            self.dirty_entries
                .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
    }

    fn traverse_for_dirty(&mut self, dir: &PathBuf, git_root: &PathBuf) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();

                if name.starts_with('.') {
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
        self.is_git_repo = git::is_git_repo(&self.current_dir);
        self.git_root = self
            .is_git_repo
            .then(|| git::git_root(&self.current_dir))
            .flatten();
        self.dirty_files = self
            .git_root
            .as_ref()
            .map(|root| git::git_dirty_files(root))
            .unwrap_or_default();

        if self.show_dirty_only {
            self.collect_dirty_files();
        }
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        if self.show_dirty_only {
            self.dirty_entries.get(self.selected)
        } else {
            if self.selected_is_parent() {
                None
            } else {
                let index = if self.is_at_root() {
                    self.selected
                } else {
                    self.selected.saturating_sub(1)
                };
                self.entries.get(index)
            }
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
