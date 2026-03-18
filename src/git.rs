use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn git_root(path: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .ok()?;

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
    let output = match Command::new("git")
        .args(["diff", "HEAD", "--name-only"])
        .current_dir(repo_root)
        .output()
    {
        Ok(o) => o,
        Err(_) => return HashSet::new(),
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
    let output = Command::new("git")
        .args(["diff", "HEAD", "--", relative_str])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}
