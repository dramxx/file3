use std::path::PathBuf;

use crate::fs::{self, DirEntry};

pub struct App {
    pub current_dir: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected: usize,
    pub file_content: Option<String>,
    pub scroll: u16,
    pub running: bool,
}

impl App {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let entries = fs::read_dir(&current_dir);
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
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(3);
    }

    pub fn scroll_down(&mut self, height: u16) {
        let content_lines = self
            .file_content
            .as_ref()
            .map(|c| c.lines().count())
            .unwrap_or(0) as u16;
        let max_scroll = content_lines.saturating_sub(height.saturating_sub(3));
        self.scroll = self.scroll.saturating_add(3).min(max_scroll);
    }

    fn load_selection(&mut self) {
        self.scroll = 0;
        if let Some(entry) = self.entries.get(self.selected) {
            self.file_content = if entry.is_dir {
                None
            } else {
                fs::read_file(&entry.path)
            };
        }
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected)
    }
}
