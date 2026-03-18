# file3

A fast, terminal-based file explorer with syntax highlighting and git diff support.

## Install

```bash
# Clone or download the project, then:
cargo build --release

# Run directly
cargo run

# Or use the compiled binary
./target/release/file3
```

## Features

**File Navigation**

- Browse directories with arrow keys or `j`/`k`
- Enter directories with `Enter`
- Go up a level with `Backspace`
- Two-panel layout: file list on left, preview on right

**Syntax Highlighting**

- Automatic syntax highlighting for 100+ languages
- Supports Rust, JavaScript, TypeScript, Python, Go, and more

**Git Integration** (when in a git repository)

- Modified files marked with `~` in yellow
- Press `d` to toggle between file content and git diff view
- Diffs show additions in green, deletions in red, and hunk headers in cyan

**Scrolling**

- PageUp/PageDown to scroll through file content
- Works in both content and diff view

## Keybindings

| Key            | Action                                |
| -------------- | ------------------------------------- |
| `q`            | Quit                                  |
| `↑` / `k`      | Move selection up                     |
| `↓` / `j`      | Move selection down                   |
| `Enter`        | Open directory / enter folder         |
| `PageUp` / `u` | Scroll content up                     |
| `PageDown`     | Scroll content down                   |
| `d`            | Toggle git diff view (git repos only) |

## Requirements

- Rust 1.70+ (for `cargo`)
- A terminal with true color support (for syntax highlighting colors)
- Git (for git diff feature)

## Tips

- The app starts in your current working directory
- Files larger than 1MB are not loaded to prevent memory issues
- Binary files are detected and shown as `[binary file or too large]`
- Hidden files (starting with `.`) are hidden by default
