use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

const MAX_FILE_SIZE: u64 = 1_048_576;

#[derive(Debug, Clone, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Excluded by `.gitignore` (or the `.git` folder); only listed when showing everything.
    pub ignored: bool,
}

#[derive(Debug, PartialEq)]
pub enum FileLoad {
    Text(String),
    Binary,
    TooLarge(u64),
    Error(String),
}

/// Lists one directory level. Without `show_all`, gitignored entries and `.git` are left out.
pub fn list_dir(path: &Path, show_all: bool) -> Vec<DirEntry> {
    let visible: HashSet<PathBuf> = walk_level(path, true).into_iter().collect();
    let paths = if show_all {
        walk_level(path, false)
    } else {
        visible.iter().cloned().collect()
    };

    let entries = paths
        .into_iter()
        .map(|path| DirEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            is_dir: path.is_dir(),
            ignored: !visible.contains(&path),
            path,
        })
        .collect();

    entries_sorted(entries)
}

fn walk_level(path: &Path, filtered: bool) -> Vec<PathBuf> {
    WalkBuilder::new(path)
        .max_depth(Some(1))
        .hidden(false)
        .require_git(false)
        .git_ignore(filtered)
        .git_exclude(filtered)
        .git_global(filtered)
        .ignore(filtered)
        .parents(filtered)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.depth() == 1)
        .filter(|e| !filtered || e.file_name() != ".git")
        .map(|e| e.into_path())
        .collect()
}

fn entries_sorted(mut entries: Vec<DirEntry>) -> Vec<DirEntry> {
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    entries
}

pub fn load_file(path: &Path) -> FileLoad {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return FileLoad::Error(e.to_string()),
    };

    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => return FileLoad::Error(e.to_string()),
    };

    if len > MAX_FILE_SIZE {
        return FileLoad::TooLarge(len);
    }

    let mut bytes = Vec::with_capacity(len as usize);
    if let Err(e) = file.read_to_end(&mut bytes) {
        return FileLoad::Error(e.to_string());
    }

    if bytes[..bytes.len().min(8192)].contains(&0) {
        return FileLoad::Binary;
    }

    FileLoad::Text(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn names(entries: &[DirEntry]) -> Vec<String> {
        entries.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn test_list_dir_empty() {
        let temp = create_temp_dir();
        assert!(list_dir(temp.path(), false).is_empty());
    }

    #[test]
    fn test_list_dir_shows_dotfiles_but_not_git_folder() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join(".git")).unwrap();
        std::fs::create_dir(temp.path().join(".github")).unwrap();
        std::fs::write(temp.path().join(".env"), "").unwrap();

        let names = names(&list_dir(temp.path(), false));
        assert!(!names.contains(&".git".to_string()));
        assert!(names.contains(&".github".to_string()));
        assert!(names.contains(&".env".to_string()));
    }

    #[test]
    fn test_list_dir_respects_gitignore() {
        let temp = create_temp_dir();
        std::fs::write(temp.path().join(".gitignore"), "target/\n*.log\n").unwrap();
        std::fs::create_dir(temp.path().join("target")).unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("debug.log"), "").unwrap();

        let names = names(&list_dir(temp.path(), false));
        assert!(names.contains(&"src".to_string()));
        assert!(!names.contains(&"target".to_string()));
        assert!(!names.contains(&"debug.log".to_string()));
    }

    #[test]
    fn test_list_dir_respects_parent_gitignore() {
        let temp = create_temp_dir();
        std::fs::write(temp.path().join(".gitignore"), "*.log\n").unwrap();
        std::fs::create_dir(temp.path().join("sub")).unwrap();
        std::fs::write(temp.path().join("sub").join("a.log"), "").unwrap();
        std::fs::write(temp.path().join("sub").join("a.txt"), "").unwrap();

        let names = names(&list_dir(&temp.path().join("sub"), false));
        assert_eq!(names, vec!["a.txt".to_string()]);
    }

    #[test]
    fn test_list_dir_show_all_marks_ignored() {
        let temp = create_temp_dir();
        std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();
        std::fs::create_dir(temp.path().join("target")).unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();

        let entries = list_dir(temp.path(), true);
        let find = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
        assert!(find("target").ignored);
        assert!(find(".git").ignored);
        assert!(!find("src").ignored);
    }

    #[test]
    fn test_list_dir_sorts_dirs_first() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join("zzz_dir")).unwrap();
        std::fs::write(temp.path().join("aaa_file"), "").unwrap();

        let entries = list_dir(temp.path(), false);
        assert!(entries[0].is_dir);
        assert!(!entries[1].is_dir);
    }

    #[test]
    fn test_list_dir_sorts_case_insensitive() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join("Apple")).unwrap();
        std::fs::create_dir(temp.path().join("banana")).unwrap();
        std::fs::create_dir(temp.path().join("Cherry")).unwrap();

        assert_eq!(
            names(&list_dir(temp.path(), false)),
            vec!["Apple", "banana", "Cherry"]
        );
    }

    #[test]
    fn test_list_dir_nonexistent_path() {
        assert!(list_dir(Path::new("/nonexistent/path/12345"), false).is_empty());
    }

    #[test]
    fn test_load_file_basic() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        assert_eq!(load_file(&file_path), FileLoad::Text("Hello, World!".to_string()));
    }

    #[test]
    fn test_load_file_nonexistent() {
        assert!(matches!(
            load_file(Path::new("/nonexistent/file.txt")),
            FileLoad::Error(_)
        ));
    }

    #[test]
    fn test_load_file_too_large() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("large.txt");
        std::fs::write(&file_path, "x".repeat(MAX_FILE_SIZE as usize + 1)).unwrap();

        assert_eq!(load_file(&file_path), FileLoad::TooLarge(MAX_FILE_SIZE + 1));
    }

    #[test]
    fn test_load_file_exactly_max_size() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("exact.txt");
        std::fs::write(&file_path, "x".repeat(MAX_FILE_SIZE as usize)).unwrap();

        match load_file(&file_path) {
            FileLoad::Text(s) => assert_eq!(s.len(), MAX_FILE_SIZE as usize),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn test_load_file_binary() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("binary.bin");
        std::fs::write(&file_path, [0x48, 0x00, 0x6c]).unwrap();

        assert_eq!(load_file(&file_path), FileLoad::Binary);
    }

    #[test]
    fn test_load_file_invalid_utf8_is_lossy() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("latin1.txt");
        std::fs::write(&file_path, [b'a', 0xE9, b'b']).unwrap();

        assert_eq!(load_file(&file_path), FileLoad::Text("a\u{FFFD}b".to_string()));
    }

    #[test]
    fn test_load_file_unicode() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("unicode.txt");
        std::fs::write(&file_path, "Hello 世界 🌍 Привет").unwrap();

        match load_file(&file_path) {
            FileLoad::Text(s) => assert!(s.contains("世界")),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn test_load_file_empty() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("empty.txt");
        std::fs::write(&file_path, "").unwrap();

        assert_eq!(load_file(&file_path), FileLoad::Text(String::new()));
    }
}
