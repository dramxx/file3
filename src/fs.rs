use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_SIZE: u64 = 1_048_576;

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub fn read_dir(path: &Path) -> Vec<DirEntry> {
    let entries: Vec<DirEntry> = match fs::read_dir(path) {
        Ok(reader) => reader
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    return None;
                }
                let path = e.path();
                let is_dir = path.is_dir();
                Some(DirEntry { name, path, is_dir })
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    entries_sorted(entries)
}

fn entries_sorted(mut entries: Vec<DirEntry>) -> Vec<DirEntry> {
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    entries
}

pub fn read_file(path: &Path) -> Option<String> {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_FILE_SIZE => return None,
        Ok(_) => {}
        Err(_) => return None,
    }

    fs::read_to_string(path).ok()
}
