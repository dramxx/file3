use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

const MAX_FILE_SIZE: u64 = 1_048_576;

#[derive(Debug, Clone, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_read_dir_empty() {
        let temp = create_temp_dir();
        let entries = read_dir(temp.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_dir_filters_hidden() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join(".hidden_dir")).unwrap();
        std::fs::write(temp.path().join(".hidden_file"), "").unwrap();
        std::fs::create_dir(temp.path().join("visible_dir")).unwrap();
        std::fs::write(temp.path().join("visible_file"), "").unwrap();

        let entries = read_dir(temp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();

        assert!(!names.contains(&".hidden_dir".to_string()));
        assert!(!names.contains(&".hidden_file".to_string()));
        assert!(names.contains(&"visible_dir".to_string()));
        assert!(names.contains(&"visible_file".to_string()));
    }

    #[test]
    fn test_read_dir_with_hidden_shows_hidden() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join(".hidden_dir")).unwrap();
        std::fs::write(temp.path().join(".hidden_file"), "").unwrap();
        std::fs::create_dir(temp.path().join("visible_dir")).unwrap();
        std::fs::write(temp.path().join("visible_file"), "").unwrap();

        let entries = read_dir_with_hidden(temp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();

        assert!(names.contains(&".hidden_dir".to_string()));
        assert!(names.contains(&".hidden_file".to_string()));
        assert!(names.contains(&"visible_dir".to_string()));
        assert!(names.contains(&"visible_file".to_string()));
    }

    #[test]
    fn test_read_dir_sorts_dirs_first() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join("aaa_file")).unwrap(); // named like file but is dir
        std::fs::write(temp.path().join("zzz_dir"), "").unwrap(); // named like dir but is file

        let entries = read_dir(temp.path());
        assert!(entries[0].is_dir);
        assert!(!entries[1].is_dir);
    }

    #[test]
    fn test_read_dir_sorts_case_insensitive() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join("Apple")).unwrap();
        std::fs::create_dir(temp.path().join("banana")).unwrap();
        std::fs::create_dir(temp.path().join("Cherry")).unwrap();

        let entries = read_dir(temp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();

        assert_eq!(names[0], "Apple");
        assert_eq!(names[1], "banana");
        assert_eq!(names[2], "Cherry");
    }

    #[test]
    fn test_read_dir_nonexistent_path() {
        let entries = read_dir(Path::new("/nonexistent/path/12345"));
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_file_basic() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let content = read_file(&file_path);
        assert_eq!(content, Some("Hello, World!".to_string()));
    }

    #[test]
    fn test_read_file_nonexistent() {
        let result = read_file(Path::new("/nonexistent/file.txt"));
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_file_too_large() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("large.txt");
        let large_content = "x".repeat(MAX_FILE_SIZE as usize + 1);
        std::fs::write(&file_path, large_content).unwrap();

        let result = read_file(&file_path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_file_exactly_max_size() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("exact.txt");
        let content = "x".repeat(MAX_FILE_SIZE as usize);
        std::fs::write(&file_path, &content).unwrap();

        let result = read_file(&file_path);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), MAX_FILE_SIZE as usize);
    }

    #[test]
    fn test_read_file_binary_content() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("binary.bin");
        let content = vec![0x48, 0x65, 0x6c, 0x6c, 0x6f]; // "Hello" in ASCII
        std::fs::write(&file_path, &content).unwrap();

        let result = read_file(&file_path);
        assert!(result.is_some());
        let content_str = result.unwrap();
        assert_eq!(content_str, "Hello");
    }

    #[test]
    fn test_read_file_unicode() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("unicode.txt");
        std::fs::write(&file_path, "Hello 世界 🌍 Привет").unwrap();

        let content = read_file(&file_path);
        assert!(content.is_some());
        assert!(content.unwrap().contains("世界"));
    }

    #[test]
    fn test_read_file_empty() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("empty.txt");
        std::fs::write(&file_path, "").unwrap();

        let content = read_file(&file_path);
        assert_eq!(content, Some("".to_string()));
    }

    #[test]
    fn test_read_file_trailing_newlines() {
        let temp = create_temp_dir();
        let file_path = temp.path().join("newlines.txt");
        std::fs::write(&file_path, "Line 1\nLine 2\nLine 3\n").unwrap();

        let content = read_file(&file_path).unwrap();
        assert_eq!(content.lines().count(), 3);
    }

    #[test]
    fn test_dir_entry_clone() {
        let entry = DirEntry {
            name: "test".to_string(),
            path: PathBuf::from("/test/path"),
            is_dir: true,
        };
        let cloned = entry.clone();
        assert_eq!(entry, cloned);
    }

    #[test]
    fn test_entries_sorted_with_files_and_dirs() {
        let temp = create_temp_dir();
        std::fs::create_dir(temp.path().join("docs")).unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("README.md"), "").unwrap();
        std::fs::write(temp.path().join("main.rs"), "").unwrap();

        let entries = read_dir(temp.path());

        let dirs: Vec<_> = entries.iter().filter(|e| e.is_dir).collect();
        let files: Vec<_> = entries.iter().filter(|e| !e.is_dir).collect();

        for dir in &dirs {
            for file in &files {
                assert!(
                    entries.iter().position(|e| e.name == dir.name)
                        < entries.iter().position(|e| e.name == file.name)
                );
            }
        }
    }
}

pub fn read_dir(path: &Path) -> Vec<DirEntry> {
    read_dir_impl(path, false)
}

pub fn read_dir_with_hidden(path: &Path) -> Vec<DirEntry> {
    read_dir_impl(path, true)
}

fn read_dir_impl(path: &Path, show_hidden: bool) -> Vec<DirEntry> {
    let entries: Vec<DirEntry> = match fs::read_dir(path) {
        Ok(reader) => reader
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if !show_hidden && name.starts_with('.') {
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
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return None,
    };

    if metadata.len() > MAX_FILE_SIZE {
        return None;
    }

    let mut reader = std::io::BufReader::new(file);
    let mut buffer = [0u8; 8192];
    let bytes_read = match reader.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return None,
    };

    if buffer[..bytes_read].iter().any(|&b| b == 0) {
        return None;
    }

    reader.seek(std::io::SeekFrom::Start(0)).ok()?;
    let mut content = String::new();
    match reader.read_to_string(&mut content) {
        Ok(_) => Some(content),
        Err(_) => None,
    }
}
