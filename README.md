# file3

A fast, terminal-based file explorer with syntax highlighting and git diff support.

## Install

```bash
# Install once (makes file3 available globally)
cargo install --path .

# Run
file3

# Or run without installing
cargo run
```

## Features

**File Navigation**
- Browse directories with arrow keys or `j`/`k`
- Enter directories with `Enter`
- Navigate up with `..` row or `Backspace`
- Two-panel layout: file list on left, preview on right

**Syntax Highlighting**
- Automatic syntax highlighting for 100+ languages
- Supports Rust, JavaScript, TypeScript, Python, Go, and more

**Git Integration** (when in a git repository)
- Modified files marked with `●` in yellow
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
| `..` + Enter   | Go up one directory                  |
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
- Binary files (images, PDFs, EXEs, etc.) show empty preview
- Hidden files (starting with `.`) are hidden by default
