//! Watches the project for changes on disk (e.g. an AI agent writing files).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Changes are reported once nothing happened for this long, so a burst of writes
/// (an agent editing many files, a `git checkout`) causes a single refresh.
const QUIET: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
pub struct Changes {
    /// Project files changed (ignored folders like `target/` don't count).
    pub files: bool,
    pub paths: HashSet<PathBuf>,
    /// Git state changed (commit, stage, checkout).
    pub git: bool,
}

pub struct FsWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    root: PathBuf,
    ignore: Gitignore,
    pending: Changes,
    last_event: Option<Instant>,
}

impl FsWatcher {
    pub fn new(root: &Path) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .ok()?;
        watcher.watch(root, RecursiveMode::Recursive).ok()?;

        let mut builder = GitignoreBuilder::new(root);
        builder.add(root.join(".gitignore"));
        let ignore = builder.build().unwrap_or_else(|_| Gitignore::empty());

        Some(Self {
            _watcher: watcher,
            rx,
            root: root.to_path_buf(),
            ignore,
            pending: Changes::default(),
            last_event: None,
        })
    }

    /// Returns what changed once things have been quiet for a moment.
    pub fn poll(&mut self) -> Option<Changes> {
        while let Ok(result) = self.rx.try_recv() {
            let Ok(event) = result else {
                // Watch errors (e.g. a lost event buffer): refresh everything to be safe.
                self.pending.files = true;
                self.last_event = Some(Instant::now());
                continue;
            };
            if matches!(event.kind, EventKind::Access(_)) {
                continue;
            }
            if event.need_rescan() {
                self.pending.files = true;
                self.last_event = Some(Instant::now());
            }
            for path in event.paths {
                self.classify(path);
            }
        }
        if self.last_event.is_some_and(|t| t.elapsed() >= QUIET) {
            self.last_event = None;
            return Some(std::mem::take(&mut self.pending));
        }
        None
    }

    fn classify(&mut self, path: PathBuf) {
        let Ok(rel) = path.strip_prefix(&self.root) else { return };
        if rel.starts_with(".git") {
            let git_state = rel == Path::new(".git").join("index")
                || rel == Path::new(".git").join("HEAD")
                || rel.starts_with(Path::new(".git").join("refs"));
            if git_state {
                self.pending.git = true;
                self.last_event = Some(Instant::now());
            }
            return;
        }
        if self
            .ignore
            .matched_path_or_any_parents(rel, path.is_dir())
            .is_ignore()
        {
            return;
        }
        self.pending.files = true;
        self.pending.paths.insert(path);
        self.last_event = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(watcher: &mut FsWatcher, timeout: Duration) -> Option<Changes> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(c) = watcher.poll() {
                return Some(c);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn test_reports_changed_files_but_not_ignored_ones() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        let mut watcher = FsWatcher::new(&root).unwrap();

        std::fs::write(root.join("target").join("out.o"), "x").unwrap();
        assert!(
            wait_for(&mut watcher, Duration::from_secs(1)).is_none(),
            "ignored folder is quiet"
        );

        std::fs::write(root.join("a.rs"), "x").unwrap();
        let changes = wait_for(&mut watcher, Duration::from_secs(5)).expect("change reported");
        assert!(changes.files);
        assert!(changes.paths.contains(&root.join("a.rs")));
    }
}
