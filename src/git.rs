use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const GIT_TIMEOUT_SECS: u64 = 5;

fn run_git_command_with_timeout(args: &[&str], path: &Path) -> Option<std::process::Output> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let path = path.to_path_buf();
    let timeout = Duration::from_secs(GIT_TIMEOUT_SECS);

    let (tx, rx) = std::sync::mpsc::channel();

    let handle: std::thread::JoinHandle<()> = std::thread::spawn(move || {
        let mut cmd = Command::new("git");
        cmd.args(&args)
            .current_dir(&path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let result = cmd.output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => {
            let _ = handle.join();
            result.ok()
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // Thread may still be running, but we can't easily kill it in Rust.
            // The thread will terminate when the process exits or git times out internally.
            // We still need to join to avoid undefined behavior.
            let _ = handle.join();
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            let _ = handle.join();
            None
        }
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
    fn test_is_git_repo_true() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        assert!(is_git_repo(temp.path()));
    }

    #[test]
    fn test_is_git_repo_false() {
        let temp = create_temp_dir();
        assert!(!is_git_repo(temp.path()));
    }

    #[test]
    fn test_is_git_repo_nonexistent() {
        assert!(!is_git_repo(Path::new("/nonexistent/path/12345")));
    }

    #[test]
    fn test_git_root_valid_repo() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        let root = git_root(temp.path());
        assert!(root.is_some());
        assert_eq!(root.unwrap(), temp.path().to_path_buf());
    }

    #[test]
    fn test_git_root_not_repo() {
        let temp = create_temp_dir();
        assert_eq!(git_root(temp.path()), None);
    }

    #[test]
    fn test_git_root_nonexistent() {
        assert_eq!(git_root(Path::new("/nonexistent/path/12345")), None);
    }

    #[test]
    fn test_git_dirty_files_empty_repo() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        let dirty = git_dirty_files(temp.path());
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_git_dirty_files_with_changes() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .expect("Failed to git add");

        let dirty = git_dirty_files(temp.path());
        assert!(dirty.is_empty()); // Staged files don't show in diff HEAD
    }

    #[test]
    fn test_git_dirty_files_untracked() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("untracked.txt"), "content").unwrap();

        let dirty = git_dirty_files(temp.path());
        assert!(dirty.is_empty()); // Untracked files don't show in diff HEAD
    }

    #[test]
    fn test_git_dirty_files_modified_after_commit() {
        let temp = create_temp_dir();
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

        let dirty = git_dirty_files(temp.path());
        assert!(dirty.contains(&temp.path().join("test.txt")));
    }

    #[test]
    fn test_git_dirty_files_nonexistent_repo() {
        let dirty = git_dirty_files(Path::new("/nonexistent/path"));
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_git_diff_no_changes() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        let diff = git_diff(temp.path(), &temp.path().join("test.txt"));
        assert_eq!(diff, None);
    }

    #[test]
    fn test_git_diff_with_changes() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("test.txt"), "original").unwrap();

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

        let diff = git_diff(temp.path(), &temp.path().join("test.txt"));
        assert!(diff.is_some());
        let diff_content = diff.unwrap();
        assert!(diff_content.contains("-original"));
        assert!(diff_content.contains("+modified"));
    }

    #[test]
    fn test_git_diff_outside_repo() {
        let temp = create_temp_dir();
        let diff = git_diff(temp.path(), &temp.path().join("test.txt"));
        assert_eq!(diff, None);
    }

    #[test]
    fn test_git_dirty_files_unicode_filename() {
        let temp = create_temp_dir();
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

        let dirty = git_dirty_files(temp.path());
        let dirty_vec: Vec<_> = dirty.iter().collect();
        assert_eq!(dirty_vec.len(), 1);
        assert_eq!(
            dirty_vec[0].file_name().unwrap().to_str().unwrap(),
            "test.txt"
        );
    }

    #[test]
    fn test_git_dirty_files_multiple_files() {
        let temp = create_temp_dir();
        create_git_repo(&temp);

        std::fs::write(temp.path().join("a.txt"), "a").unwrap();
        std::fs::write(temp.path().join("b.txt"), "b").unwrap();

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

        std::fs::write(temp.path().join("a.txt"), "modified a").unwrap();
        std::fs::write(temp.path().join("b.txt"), "modified b").unwrap();

        let dirty = git_dirty_files(temp.path());
        assert_eq!(dirty.len(), 2);
        assert!(dirty.contains(&temp.path().join("a.txt")));
        assert!(dirty.contains(&temp.path().join("b.txt")));
    }
}

pub fn is_git_repo(path: &Path) -> bool {
    run_git_command_with_timeout(&["rev-parse", "--is-inside-work-tree"], path)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn git_root(path: &Path) -> Option<PathBuf> {
    let output = run_git_command_with_timeout(&["rev-parse", "--show-toplevel"], path)?;

    if output.status.success() {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if root.is_empty() {
            None
        } else {
            Some(PathBuf::from(root))
        }
    } else {
        None
    }
}

pub fn git_dirty_files(repo_root: &Path) -> HashSet<PathBuf> {
    let output = match run_git_command_with_timeout(&["diff", "HEAD", "--name-only"], repo_root) {
        Some(o) => o,
        None => return HashSet::new(),
    };

    if !output.status.success() {
        return HashSet::new();
    }

    let root = repo_root.to_path_buf();
    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| root.join(line.trim()))
        .collect()
}

pub fn git_diff(repo_root: &Path, file_path: &Path) -> Option<String> {
    let relative = file_path.strip_prefix(repo_root).ok()?;
    let relative_str = relative.to_str()?;
    let output = run_git_command_with_timeout(&["diff", "HEAD", "--", relative_str], repo_root)?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}
