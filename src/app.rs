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

        let file_content = entries.iter().find(|e| !e.is_dir).and_then(|e| {
            if git_root
                .as_ref()
                .map(|root| git::git_dirty_files(root).contains(&e.path))
                .unwrap_or(false)
            {
                None
            } else {
                fs::read_file(&e.path)
            }
        });

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
        }
    }

    pub fn move_up(&mut self) {
        if !self.entries.is_empty() && self.selected > 0 {
            self.selected -= 1;
            self.load_selection();
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.entries.len().saturating_sub(1) {
            self.selected += 1;
            self.load_selection();
        }
    }

    pub fn enter(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
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

    pub fn go_up(&mut self) {
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

    pub fn toggle_diff(&mut self) {
        if !self.is_git_repo {
            return;
        }

        let entry = self.selected_entry().cloned();
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

        if let Some(entry) = self.entries.get(self.selected) {
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
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected)
    }

    pub fn is_dirty(&self, path: &PathBuf) -> bool {
        self.dirty_files.contains(path)
    }
}
