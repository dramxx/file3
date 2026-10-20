use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs::{self, DirEntry};

pub struct Row {
    pub entry: DirEntry,
    pub depth: usize,
}

/// The explorer tree: rows are the visible (expanded) part of the tree, in display order.
pub struct FileTree {
    pub root: PathBuf,
    pub show_all: bool,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub offset: usize,
    expanded: HashSet<PathBuf>,
}

impl FileTree {
    pub fn new(root: PathBuf) -> Self {
        let mut tree = Self {
            root,
            show_all: false,
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            expanded: HashSet::new(),
        };
        tree.rebuild();
        tree
    }

    /// Re-reads every expanded folder from disk, keeping the selection on the same path.
    pub fn rebuild(&mut self) {
        let selected_path = self.selected_path();
        self.rows.clear();
        let root = self.root.clone();
        self.push_children(&root, 0);

        if let Some(i) = selected_path.and_then(|p| self.index_of(&p)) {
            self.selected = i;
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn push_children(&mut self, dir: &Path, depth: usize) {
        for entry in fs::list_dir(dir, self.show_all) {
            let expand = entry.is_dir && self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            self.rows.push(Row { entry, depth });
            if expand {
                self.push_children(&path, depth + 1);
            }
        }
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        self.rows.get(self.selected).map(|r| &r.entry)
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_entry().map(|e| e.path.clone())
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.rows.iter().position(|r| r.entry.path == path)
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    pub fn select(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last.max(0)) as usize;
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        self.selected = self.rows.len().saturating_sub(1);
    }

    /// Expands a collapsed folder or collapses an expanded one.
    pub fn toggle(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else { return };
        if !row.entry.is_dir {
            return;
        }
        let path = row.entry.path.clone();
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.rebuild();
    }

    /// Right arrow: expand the selected folder, or step into it when already expanded.
    pub fn expand_selected(&mut self) {
        let Some(entry) = self.selected_entry() else { return };
        if !entry.is_dir {
            return;
        }
        if self.is_expanded(&entry.path) {
            let depth = self.rows[self.selected].depth;
            if self
                .rows
                .get(self.selected + 1)
                .is_some_and(|r| r.depth > depth)
            {
                self.selected += 1;
            }
        } else {
            self.toggle(self.selected);
        }
    }

    /// Left arrow: collapse the selected folder, or jump to the parent folder.
    pub fn collapse_selected(&mut self) {
        let Some(entry) = self.selected_entry() else { return };
        if entry.is_dir && self.is_expanded(&entry.path) {
            self.toggle(self.selected);
            return;
        }
        let depth = self.rows[self.selected].depth;
        if depth == 0 {
            return;
        }
        if let Some(parent) = self.rows[..self.selected]
            .iter()
            .rposition(|r| r.depth == depth - 1)
        {
            self.selected = parent;
        }
    }

    /// Expands every folder above `path` and selects it.
    pub fn reveal(&mut self, path: &Path) {
        for ancestor in path.ancestors().skip(1) {
            if ancestor == self.root || !ancestor.starts_with(&self.root) {
                break;
            }
            self.expanded.insert(ancestor.to_path_buf());
        }
        self.rebuild();
        if let Some(i) = self.index_of(path) {
            self.selected = i;
        }
    }

    pub fn expanded_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.expanded.iter().cloned().collect();
        paths.sort();
        paths
    }

    /// Expands the given folders (those that still exist).
    pub fn expand_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.expanded.extend(paths.into_iter().filter(|p| p.is_dir()));
        self.rebuild();
    }

    pub fn set_show_all(&mut self, show_all: bool) {
        self.show_all = show_all;
        self.rebuild();
    }

    /// Keeps the selected row inside a viewport of `height` rows.
    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
        self.clamp_offset(height);
    }

    pub fn scroll_by(&mut self, delta: isize, height: usize) {
        self.offset = (self.offset as isize + delta).max(0) as usize;
        self.clamp_offset(height);
    }

    fn clamp_offset(&mut self, height: usize) {
        self.offset = self.offset.min(self.rows.len().saturating_sub(height));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// root/
    ///   docs/guide.md
    ///   src/app/state.rs
    ///   src/main.rs
    ///   README.md
    fn sample() -> (TempDir, FileTree) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("src").join("app")).unwrap();
        std::fs::write(root.join("docs").join("guide.md"), "").unwrap();
        std::fs::write(root.join("src").join("app").join("state.rs"), "").unwrap();
        std::fs::write(root.join("src").join("main.rs"), "").unwrap();
        std::fs::write(root.join("README.md"), "").unwrap();
        let tree = FileTree::new(root.to_path_buf());
        (temp, tree)
    }

    fn names(tree: &FileTree) -> Vec<String> {
        tree.rows
            .iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.entry.name))
            .collect()
    }

    #[test]
    fn test_starts_collapsed() {
        let (_t, tree) = sample();
        assert_eq!(names(&tree), vec!["docs", "src", "README.md"]);
    }

    #[test]
    fn test_toggle_expands_and_collapses() {
        let (_t, mut tree) = sample();
        tree.toggle(1);
        assert_eq!(
            names(&tree),
            vec!["docs", "src", "  app", "  main.rs", "README.md"]
        );
        tree.toggle(1);
        assert_eq!(names(&tree), vec!["docs", "src", "README.md"]);
    }

    #[test]
    fn test_toggle_file_does_nothing() {
        let (_t, mut tree) = sample();
        tree.toggle(2);
        assert_eq!(tree.rows.len(), 3);
    }

    #[test]
    fn test_expand_selected_then_steps_into() {
        let (_t, mut tree) = sample();
        tree.select(1);
        tree.expand_selected();
        assert_eq!(tree.selected, 1);
        assert_eq!(tree.rows.len(), 5);
        tree.expand_selected();
        assert_eq!(tree.selected_entry().unwrap().name, "app");
    }

    #[test]
    fn test_collapse_selected_goes_to_parent_then_collapses() {
        let (_t, mut tree) = sample();
        tree.select(1);
        tree.expand_selected();
        tree.select(3); // src/main.rs
        tree.collapse_selected();
        assert_eq!(tree.selected_entry().unwrap().name, "src");
        tree.collapse_selected();
        assert_eq!(tree.rows.len(), 3);
        assert_eq!(tree.selected_entry().unwrap().name, "src");
    }

    #[test]
    fn test_reveal_expands_ancestors() {
        let (temp, mut tree) = sample();
        let target = temp.path().join("src").join("app").join("state.rs");
        tree.reveal(&target);
        assert_eq!(tree.selected_entry().unwrap().path, target);
        assert_eq!(
            names(&tree),
            vec!["docs", "src", "  app", "    state.rs", "  main.rs", "README.md"]
        );
    }

    #[test]
    fn test_rebuild_keeps_selection_and_sees_new_files() {
        let (temp, mut tree) = sample();
        tree.select(2); // README.md
        std::fs::write(temp.path().join("AAA.md"), "").unwrap();
        tree.rebuild();
        assert_eq!(tree.selected_entry().unwrap().name, "README.md");
        assert_eq!(tree.rows.len(), 4);
    }

    #[test]
    fn test_move_by_clamps() {
        let (_t, mut tree) = sample();
        tree.move_by(-5);
        assert_eq!(tree.selected, 0);
        tree.move_by(50);
        assert_eq!(tree.selected, 2);
    }

    #[test]
    fn test_ensure_visible_scrolls() {
        let (_t, mut tree) = sample();
        tree.select(2);
        tree.ensure_visible(2);
        assert_eq!(tree.offset, 1);
        tree.select(0);
        tree.ensure_visible(2);
        assert_eq!(tree.offset, 0);
    }
}
