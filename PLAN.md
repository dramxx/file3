# file3 — TUI File Explorer Plan

---

## Versions

- **V1.0** — Core file explorer (this document, first section)
- **V1.1** — Git diff viewer (second section)

---

# V1.0 — Core File Explorer

A minimal IDE-like file explorer built in Rust with `ratatui`. Two-column layout:
left panel lists files/folders, right panel shows file contents.

---

## Project Setup

```bash
cargo new file3
cd file3
```

### `Cargo.toml` dependencies

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"      # terminal backend (keyboard events, raw mode)
anyhow = "1"            # error handling
```

---

## File Structure

```
src/
├── main.rs         # entry point — terminal setup, event loop
├── app.rs          # App state struct + state transitions
├── fs.rs           # filesystem helpers (read dir, read file)
└── ui.rs           # ratatui layout + widget rendering
```

---

## Core Data Model (`app.rs`)

```rust
pub struct App {
    pub current_dir: PathBuf,           // directory currently shown in left panel
    pub entries: Vec<DirEntry>,         // sorted entries in current_dir (dirs first)
    pub selected: usize,                // cursor index in left panel
    pub file_content: Option<String>,   // content of highlighted file (None for dirs)
    pub scroll: u16,                    // vertical scroll offset for right panel
    pub running: bool,                  // set to false to quit
}
```

`DirEntry` is a thin wrapper:

```rust
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}
```

Sorting rule: directories before files, both groups alphabetically.

---

## Filesystem Helpers (`fs.rs`)

Two functions:

1. **`read_dir(path) -> Vec<DirEntry>`**
   - Read the directory, filter out hidden files (names starting with `.`) — optional, configurable later.
   - Sort: dirs first, then files, each group sorted by name case-insensitively.

2. **`read_file(path) -> Option<String>`**
   - Attempt `std::fs::read_to_string`.
   - Return `None` for binary files (catch the UTF-8 error) — show a placeholder message in the right panel instead.
   - Limit to a reasonable size cap (e.g. 1 MB) to avoid loading huge files.

---

## UI Layout (`ui.rs`)

```
┌──────────────┬──────────────────────────────────────────┐
│  left panel  │  right panel                             │
│  (~20% wide) │  (~80% wide)                             │
│              │                                          │
│  📁 src      │  fn main() {                             │
│  📁 target   │      let mut app = App::new();           │
│ ▶ 📄 Cargo… │      …                                   │
│  📄 README   │  }                                       │
└──────────────┴──────────────────────────────────────────┘
 [q] Quit  [↑↓] Navigate  [Enter] Open  [Backspace] Up
```

### Layout split

```rust
let chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(20),
        Constraint::Percentage(80),
    ])
    .split(frame.area());
```

### Left panel — `List` widget

- Render entries as a `List` with a `ListState` for highlight tracking.
- Prefix each item: `📁 name/` for directories, `📄 name` for files.
- Highlight the selected row with a distinct style (e.g. reversed fg/bg).
- Show current directory path as the block title, truncated if too long.

### Right panel — `Paragraph` widget

- Wrap content in a `Paragraph` with `Block::bordered()`.
- Use `.scroll((app.scroll, 0))` for vertical scrolling.
- If `file_content` is `None` (directory selected or binary): render a dim
  placeholder like `"  Select a file to preview"` or `"  [binary file]"`.
- Show the file name in the block title.

---

## Event Loop (`main.rs`)

```
loop:
  1. draw UI  (ratatui frame)
  2. poll for crossterm event (timeout ~16 ms for ~60 fps feel)
  3. handle key:
       q / Ctrl-C  → quit
       ↑ / k       → move selection up
       ↓ / j       → move selection down
       Enter        → if dir: cd into it (update current_dir, reload entries)
       Backspace    → go up one directory (cd ..)
       PageUp/u     → scroll right panel up
       PageDown/d   → scroll right panel down
```

After every navigation event that changes the selected entry:

- Reset `scroll` to 0.
- Reload `file_content` if the new selection is a file.

---

## State Transitions

```
App::new()
  └─ current_dir = std::env::current_dir()
  └─ entries     = fs::read_dir(&current_dir)
  └─ selected    = 0
  └─ file_content = first entry preview (if file)

on ↑ / ↓:
  └─ clamp selected within 0..entries.len()
  └─ reload file_content for new selection

on Enter (directory):
  └─ current_dir = entries[selected].path
  └─ entries     = fs::read_dir(&current_dir)
  └─ selected    = 0
  └─ file_content = None (reset)

on Backspace:
  └─ current_dir = current_dir.parent() (if exists)
  └─ entries     = fs::read_dir(&current_dir)
  └─ selected    = 0
  └─ file_content = None

on Enter (file):
  └─ no-op (file is already previewed on the right)
```

---

## Terminal Setup / Teardown (`main.rs`)

Use the standard crossterm pattern — **always restore terminal on exit**,
including on panic:

```rust
crossterm::terminal::enable_raw_mode()?;
let mut stdout = std::io::stdout();
crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
let backend = CrosstermBackend::new(stdout);
let mut terminal = Terminal::new(backend)?;

// --- run app ---

// Teardown (put in a guard or explicit finally block):
crossterm::terminal::disable_raw_mode()?;
crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen)?;
```

Consider wrapping teardown in a `Drop` impl on a `TerminalGuard` struct so
the terminal is always restored even if the app panics.

---

## Build & Run

```bash
# Run from any directory — explores that directory
cargo run

# Release build
cargo build --release
./target/release/file3
```

---

## Incremental Build Order

1. **Skeleton** — `main.rs` sets up terminal + draws a blank frame. Confirms ratatui works.
2. **Filesystem** — implement `fs.rs`, unit-test sorting and read functions.
3. **Left panel** — render the `List` with real entries and keyboard navigation (↑↓).
4. **Directory traversal** — Enter to descend, Backspace to go up.
5. **Right panel** — show file content, scrolling.
6. **Polish** — status bar with keybinds, binary file handling, path truncation.

---

## Optional Enhancements (post-MVP)

- Show file size / last-modified in left panel (in a second column).
- Syntax-aware line numbers on the right panel.
- Mouse support (ratatui has mouse event support via crossterm).
- Search / filter entries in left panel (`/` to type).
- Config file for hidden file visibility, color theme.

---

# V1.1 — Git Diff Viewer

Builds on V1.0 without restructuring it. When the app is run inside a git
repository, files with uncommitted changes can be viewed in a diff mode,
toggled per-file with a single keypress.

No new dependencies required — everything shells out to the `git` binary via
`std::process::Command`.

---

## New Concepts

### Git context detection (once on startup)

```rust
fn is_git_repo(path: &Path) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

Store the result as `App.is_git_repo: bool`. If `false`, V1.1 features are
simply never activated — the app behaves exactly as V1.0.

### Dirty file set

On startup (and whenever the directory changes), fetch the list of files that
have a diff:

```rust
fn git_dirty_files(repo_root: &Path) -> HashSet<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD", "--name-only"])
        .current_dir(repo_root)
        .output();
    // parse stdout lines into absolute PathBufs
}
```

Store as `App.dirty_files: HashSet<PathBuf>`. Used by the left panel to mark
changed files visually.

### Fetching a diff for a single file

```rust
fn git_diff(repo_root: &Path, file_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD", "--", file_path.to_str()?])
        .current_dir(repo_root)
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()
}
```

Returns `None` if the file has no diff or git isn't available.

---

## Changes to `app.rs`

```rust
pub struct App {
    // --- V1.0 fields unchanged ---
    pub current_dir: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected: usize,
    pub file_content: Option<String>,
    pub scroll: u16,
    pub running: bool,

    // --- V1.1 additions ---
    pub is_git_repo: bool,
    pub git_root: Option<PathBuf>,       // root of the repo (for running git commands)
    pub dirty_files: HashSet<PathBuf>,   // files with a diff vs HEAD
    pub view_mode: ViewMode,             // Content or Diff
    pub diff_content: Option<String>,    // raw diff string for selected file
}

pub enum ViewMode {
    Content,
    Diff,
}
```

`view_mode` resets to `ViewMode::Content` on every navigation change (moving
selection, entering a directory). The user explicitly re-presses `d` to enter
diff mode for the new file.

---

## Changes to `fs.rs`

Add a `git.rs` module (or extend `fs.rs`) with the three functions above:
`is_git_repo`, `git_dirty_files`, `git_diff`. Keep them isolated so they're
easy to stub in tests.

---

## Changes to `ui.rs`

### Left panel — dirty file markers

If `app.is_git_repo`, check each entry against `app.dirty_files`. Modified
files get a subtle marker — a `~` suffix or an amber/yellow foreground color:

```rust
let style = if app.dirty_files.contains(&entry.path) {
    Style::default().fg(Color::Yellow)
} else {
    Style::default()
};
```

### Right panel — diff rendering

When `app.view_mode == ViewMode::Diff`, replace the plain `Paragraph` with a
styled one built from a `Text` of colored `Line`s:

```rust
fn render_diff(diff: &str) -> Text {
    diff.lines().map(|line| {
        let (content, style) = if line.starts_with('+') && !line.starts_with("+++") {
            (line, Style::default().fg(Color::Green))
        } else if line.starts_with('-') && !line.starts_with("---") {
            (line, Style::default().fg(Color::Red))
        } else if line.starts_with("@@") {
            (line, Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM))
        } else {
            (line, Style::default().fg(Color::DarkGray))
        };
        Line::from(Span::styled(content, style))
    }).collect()
}
```

The block title updates to show the mode: `" filename.rs [diff] "` vs
`" filename.rs "`.

If `view_mode == Diff` but `diff_content` is `None` (file is clean), show a
dim placeholder: `"  No changes vs HEAD"`.

### Status bar update

Add `[d] Toggle diff` to the hint line, shown only when `app.is_git_repo`.

---

## Event Loop additions (`main.rs`)

One new key binding:

```
d  →  if is_git_repo && selected entry is a file:
          toggle view_mode between Content and Diff
          if switching to Diff and diff_content is None: load it
          reset scroll to 0
```

---

## State Transitions (additions)

```
on d (file selected, is_git_repo):
  └─ view_mode = toggle(view_mode)
  └─ if view_mode == Diff && diff_content.is_none():
       diff_content = git::git_diff(&git_root, &selected_path)
  └─ scroll = 0

on ↑ / ↓ / Enter / Backspace:
  └─ (existing V1.0 transitions)
  └─ view_mode = ViewMode::Content   ← reset on every navigation
  └─ diff_content = None             ← clear, reload lazily on next 'd'
  └─ refresh dirty_files if entering a new directory
```

---

## Incremental Build Order (V1.1)

1. **`git.rs`** — implement and unit-test the three git helper functions.
2. **Startup detection** — populate `is_git_repo`, `git_root`, `dirty_files` in `App::new()`.
3. **Left panel markers** — yellow color for dirty files. Visually confirm it works.
4. **`d` keybind + `ViewMode`** — wire up toggle, load `diff_content` lazily.
5. **Diff renderer** — `render_diff()` with colored spans, plug into right panel.
6. **Polish** — status bar hint, `[diff]` title tag, `"No changes"` placeholder.

---

## Effort Estimate

| Piece                                  | Effort          |
| -------------------------------------- | --------------- |
| Git repo detection                     | ~15 min         |
| `git_dirty_files` + `git_diff` helpers | ~20 min         |
| Left panel dirty markers               | ~30 min         |
| `ViewMode` enum + `d` keybind          | ~30 min         |
| Colored diff renderer                  | ~1–2 hrs        |
| Status bar + polish                    | ~30 min         |
| **Total**                              | **~half a day** |
