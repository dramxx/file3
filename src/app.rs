use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::config::{Config, ProjectState};
use crate::finder::{self, Finder};
use crate::font;
use crate::palette::{Command, Palette, PaletteMode};
use crate::theme;
use crate::fs::DirEntry;
use crate::git;
use crate::search::{Field, ProjectSearch};
use crate::symbols::{find_symbols, SymbolPicker};
use crate::tree::FileTree;
use crate::viewer::{ViewMode, Viewer};
use crate::watcher::{Changes as DiskChanges, FsWatcher};

const MIN_SIDE_WIDTH: u16 = 20;
const MIN_VIEWER_WIDTH: u16 = 30;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const WHEEL_STEP: isize = 3;
const TOAST_TIME: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Side,
    Viewer,
}

/// What the left panel shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SideMode {
    Tree,
    Changes,
    Search,
}

pub enum Overlay {
    None,
    Finder(Finder),
    Palette(Palette),
    Symbols(SymbolPicker),
    GotoLine(String),
    Help,
    FontSetup(FontSetup),
}

/// Steps of the guided font setup, shown when icons are turned on without a Nerd Font.
#[derive(Debug, Clone, PartialEq)]
pub enum FontSetup {
    /// Offer to install the bundled font.
    Install,
    /// Offer to make it the font of this Windows Terminal (its settings file).
    Terminal(PathBuf),
    /// The outcome, closed with any key.
    Done(String),
}

/// Screen areas from the last render, used to map mouse clicks.
#[derive(Debug, Default, Clone, Copy)]
pub struct Areas {
    pub main: Rect,
    pub side: Rect,
    pub side_list: Rect,
    /// Case / whole word / regex / filters toggles.
    pub search_toggles: [Rect; 4],
    /// Query, include and exclude inputs.
    pub search_fields: [Rect; 3],
    pub viewer: Rect,
    pub viewer_text: Rect,
    pub popup: Rect,
    pub popup_list: Rect,
}

/// A place in a file, for back / forward.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub scroll: usize,
}

const MAX_HISTORY: usize = 100;
const MAX_RECENT: usize = 50;

/// The flat list of changed files shown with `f`.
#[derive(Default)]
pub struct Changes {
    pub entries: Vec<DirEntry>,
    pub selected: usize,
    pub offset: usize,
}

pub struct App {
    pub root: PathBuf,
    pub tree: FileTree,
    pub viewer: Viewer,
    pub search: ProjectSearch,
    pub changes: Changes,
    pub focus: Focus,
    pub side_mode: SideMode,
    pub overlay: Overlay,
    pub areas: Areas,
    /// Explorer width once the user dragged the border; automatic until then.
    pub side_width: Option<u16>,
    pub nerd_font: bool,
    pub running: bool,

    pub git_root: Option<PathBuf>,
    dirty_files: HashSet<PathBuf>,
    dirty_dirs: HashSet<PathBuf>,

    /// Project files for the finder, relative with `/` separators.
    pub files: Vec<String>,
    pub files_loading: bool,
    files_rx: Option<Receiver<Vec<String>>>,

    dragging_splitter: bool,
    last_click: Option<(Instant, Position)>,
    previewed_hit: Option<(PathBuf, usize)>,

    watcher: Option<FsWatcher>,

    /// Where the user was reading (file and scroll), kept up to date while the file has focus.
    reading: Option<Location>,
    /// Set by actions that jump within the same file (go to line, search result, ...).
    jumped: bool,
    back: Vec<Location>,
    forward: Vec<Location>,
    /// Recently read files, most recent first.
    pub recent: Vec<PathBuf>,

    pub config: Config,
    /// Save settings to disk (off in tests).
    persist: bool,
    /// Short message in the status bar, e.g. "Copied …".
    pub toast: Option<(String, Instant)>,
    /// Text waiting to be put on the clipboard (done by the main loop).
    pub clipboard: Option<String>,
}

impl App {
    /// The real app: browses the current folder and loads / saves settings.
    pub fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self::with_root(root);
        app.persist = true;
        app.config = Config::load();
        theme::set(theme::index_by_name(&app.config.theme).unwrap_or(0));
        app.nerd_font = app.config.nerd_font || std::env::var("FILE3_NERD_FONT").is_ok_and(|v| v != "0");
        app.viewer.wrap = app.config.wrap;
        app.viewer.reload();
        app.restore_state(&ProjectState::load(&app.root));
        app
    }

    fn rel(&self, path: &Path) -> Option<String> {
        path.strip_prefix(&self.root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    fn restore_state(&mut self, state: &ProjectState) {
        let abs = |rel: &String| self.root.join(rel);
        self.tree.expand_paths(state.expanded.iter().map(abs));
        self.recent = state.recent.iter().map(abs).filter(|p| p.is_file()).collect();
        if let Some(path) = state.file.as_ref().map(abs).filter(|p| p.is_file()) {
            self.tree.reveal(&path);
            self.viewer.open(&path);
            self.viewer.scroll = state.scroll;
            self.reading = Some(Location { path, scroll: state.scroll });
        }
    }

    pub fn project_state(&self) -> ProjectState {
        let location = self.current_location().or_else(|| self.reading.clone());
        ProjectState {
            expanded: self
                .tree
                .expanded_paths()
                .iter()
                .filter_map(|p| self.rel(p))
                .collect(),
            file: location.as_ref().and_then(|l| self.rel(&l.path)),
            scroll: location.map(|l| l.scroll).unwrap_or(0),
            recent: self.recent.iter().filter_map(|p| self.rel(p)).collect(),
        }
    }

    /// Remembers the open file, folders and recent files for the next start.
    pub fn save_state(&self) {
        if self.persist {
            let _ = self.project_state().save(&self.root);
        }
    }

    /// An app on `root` with default settings that are never saved (used by tests).
    pub fn with_root(root: PathBuf) -> Self {
        let mut app = Self {
            tree: FileTree::new(root.clone()),
            viewer: Viewer::new(),
            search: ProjectSearch::new(),
            changes: Changes::default(),
            focus: Focus::Side,
            side_mode: SideMode::Tree,
            overlay: Overlay::None,
            areas: Areas::default(),
            side_width: None,
            nerd_font: false,
            running: true,
            watcher: FsWatcher::new(&root),
            reading: None,
            jumped: false,
            back: Vec::new(),
            forward: Vec::new(),
            recent: Vec::new(),
            config: Config::default(),
            persist: false,
            toast: None,
            clipboard: None,
            git_root: None,
            dirty_files: HashSet::new(),
            dirty_dirs: HashSet::new(),
            files: Vec::new(),
            files_loading: true,
            files_rx: Some(finder::spawn_index(root.clone())),
            dragging_splitter: false,
            last_click: None,
            previewed_hit: None,
            root,
        };
        app.refresh_git();
        app.preview_tree_selection();
        app
    }

    // ---- git ----

    pub fn is_git_repo(&self) -> bool {
        self.git_root.is_some()
    }

    fn refresh_git(&mut self) {
        self.git_root = git::is_git_repo(&self.root)
            .then(|| git::git_root(&self.root))
            .flatten();
        self.dirty_files = self
            .git_root
            .as_ref()
            .map(|root| git::git_dirty_files(root))
            .unwrap_or_default();
        self.dirty_dirs = self
            .dirty_files
            .iter()
            .flat_map(|p| p.ancestors().skip(1).map(Path::to_path_buf))
            .collect();
        self.rebuild_changes();
    }

    pub fn is_dirty(&self, path: &Path) -> bool {
        self.dirty_files.contains(path)
    }

    /// A folder containing changed files.
    pub fn has_dirty_children(&self, path: &Path) -> bool {
        self.dirty_dirs.contains(path)
    }

    pub fn dirty_count(&self) -> usize {
        self.changes.entries.len()
    }

    fn rebuild_changes(&mut self) {
        let mut entries: Vec<DirEntry> = self
            .dirty_files
            .iter()
            .filter_map(|p| {
                let rel = p.strip_prefix(&self.root).ok()?;
                Some(DirEntry {
                    name: rel.to_string_lossy().replace('\\', "/"),
                    path: self.root.join(rel),
                    is_dir: false,
                    ignored: false,
                })
            })
            .collect();
        entries.sort_by_key(|e| e.name.to_lowercase());
        self.changes.entries = entries;
        self.changes.selected = self
            .changes
            .selected
            .min(self.changes.entries.len().saturating_sub(1));
    }

    // ---- background work ----

    /// Collects background results. Returns true when the screen needs a redraw.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.files_rx {
            if let Ok(files) = rx.try_recv() {
                self.files = files;
                self.files_loading = false;
                self.files_rx = None;
                if let Overlay::Finder(f) = &mut self.overlay {
                    f.update(&self.files);
                }
                changed = true;
            }
        }
        changed |= self.viewer.poll_highlight();
        if let Some(changes) = self.watcher.as_mut().and_then(|w| w.poll()) {
            self.apply_disk_changes(&changes);
            changed = true;
        }
        if self.toast.as_ref().is_some_and(|(_, t)| t.elapsed() > TOAST_TIME) {
            self.toast = None;
            changed = true;
        }
        if self.search.tick(&self.root) {
            if self.side_mode == SideMode::Search && self.focus == Focus::Side {
                self.preview_search_hit();
            }
            changed = true;
        }
        changed
    }

    /// Brings everything up to date after files changed on disk.
    fn apply_disk_changes(&mut self, changes: &DiskChanges) {
        if changes.files {
            self.tree.rebuild();
            let open_changed = self.viewer.path.as_ref().is_some_and(|p| {
                changes.paths.is_empty() || changes.paths.contains(p) || !p.exists()
            });
            self.refresh_git();
            if open_changed {
                if self.viewer.mode == ViewMode::Diff {
                    self.restyle_diff();
                } else {
                    self.viewer.reload();
                }
            }
            // The old list stays usable until the new one arrives.
            self.files_rx = Some(finder::spawn_index(self.root.clone()));
        } else if changes.git {
            self.refresh_git();
            if self.viewer.mode == ViewMode::Diff {
                self.restyle_diff();
            }
        }
    }

    /// Re-reads the tree, the open file, git status and the file index.
    pub fn reload(&mut self) {
        self.tree.rebuild();
        self.viewer.reload();
        self.refresh_git();
        self.files_loading = true;
        self.files_rx = Some(finder::spawn_index(self.root.clone()));
    }

    // ---- previews ----

    fn preview_tree_selection(&mut self) {
        let Some(entry) = self.tree.selected_entry().cloned() else {
            self.viewer.show_message("Empty folder");
            return;
        };
        self.preview_entry(&entry);
    }

    fn preview_entry(&mut self, entry: &DirEntry) {
        if entry.is_dir {
            self.viewer
                .show_message(format!("{}/ — press Enter or → to expand", entry.name));
        } else if self.viewer.path.as_deref() != Some(entry.path.as_path())
            || self.viewer.mode != ViewMode::Content
        {
            self.viewer.open(&entry.path);
        }
    }

    fn preview_change(&mut self) {
        if let Some(entry) = self.changes.entries.get(self.changes.selected).cloned() {
            if self.viewer.path.as_deref() != Some(entry.path.as_path()) || self.viewer.mode != ViewMode::Diff {
                self.show_diff(&entry.path);
            }
        } else {
            self.viewer.show_message("No uncommitted changes");
        }
    }

    fn preview_search_hit(&mut self) {
        let Some((file, hit)) = self.search.selected_hit() else { return };
        let key = (file.path.clone(), hit.line_number);
        if self.previewed_hit.as_ref() == Some(&key) {
            return;
        }
        let ranges = hit
            .ranges
            .iter()
            .map(|&(a, b)| {
                (
                    crate::text::expanded_offset(&hit.text, a),
                    crate::text::expanded_offset(&hit.text, b),
                )
            })
            .collect();
        self.viewer
            .open_at(&file.path, hit.line_number.saturating_sub(1), ranges);
        self.previewed_hit = Some(key);
    }

    fn side_height(&self) -> usize {
        self.areas.side_list.height as usize
    }

    // ---- actions ----

    fn set_side_mode(&mut self, mode: SideMode) {
        self.side_mode = mode;
        self.focus = Focus::Side;
        match mode {
            SideMode::Tree => self.preview_tree_selection(),
            SideMode::Changes => self.preview_change(),
            SideMode::Search => {
                self.previewed_hit = None;
                self.preview_search_hit();
            }
        }
    }

    fn open_palette(&mut self) {
        let available = Command::ALL
            .iter()
            .copied()
            .filter(|c| self.command_available(*c))
            .collect();
        self.overlay = Overlay::Palette(Palette::commands(available));
    }

    fn command_available(&self, command: Command) -> bool {
        match command {
            Command::ShowChanges | Command::ToggleDiff => self.is_git_repo(),
            Command::GoToLine | Command::GoToSymbol => {
                !self.viewer.lines.is_empty() && self.viewer.mode == ViewMode::Content
            }
            Command::ToggleMarkdownSource => self.viewer.is_markdown(),
            Command::CopyPath | Command::CopyPathWithLine | Command::OpenInEditor => {
                self.viewer.path.is_some() || self.tree.selected_entry().is_some_and(|e| !e.is_dir)
            }
            _ => true,
        }
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::GoToFile => self.open_finder(),
            Command::SearchInFiles => self.open_search(),
            Command::GoToLine => {
                if !self.viewer.lines.is_empty() {
                    self.overlay = Overlay::GotoLine(String::new());
                }
            }
            Command::GoToSymbol => self.open_symbols(),
            Command::GoBack => self.travel(true),
            Command::GoForward => self.travel(false),
            Command::ColorTheme => {
                self.overlay = Overlay::Palette(Palette::themes(theme::current_index()))
            }
            Command::ToggleIcons => self.toggle_icons(),
            Command::ToggleWrap => self.toggle_wrap(),
            Command::ToggleIgnored => self.toggle_show_all(),
            Command::ToggleMarkdownSource => self.viewer.toggle_markdown_source(),
            Command::ShowChanges => {
                if self.is_git_repo() {
                    self.set_side_mode(SideMode::Changes);
                }
            }
            Command::ToggleDiff => self.toggle_diff(),
            Command::Reload => self.reload(),
            Command::CopyPath => self.copy_path(false),
            Command::CopyPathWithLine => self.copy_path(true),
            Command::OpenInEditor => self.open_current_in_editor(),
            Command::OpenSettings => self.open_settings(),
            Command::Help => self.overlay = Overlay::Help,
            Command::Quit => self.running = false,
        }
    }

    fn toggle_icons(&mut self) {
        self.nerd_font = !self.nerd_font;
        self.config.nerd_font = self.nerd_font;
        self.save_config();
        if self.nerd_font && self.persist && !font::is_installed() {
            self.overlay = Overlay::FontSetup(FontSetup::Install);
        }
    }

    /// Installs the bundled font; the next step offers to set it in Windows Terminal.
    fn install_font() -> FontSetup {
        match font::install() {
            Err(e) => FontSetup::Done(format!("Couldn't install the font: {}", e)),
            Ok(()) => match font::windows_terminal_settings() {
                Some(path) => FontSetup::Terminal(path),
                None => FontSetup::Done(Self::font_hint()),
            },
        }
    }

    fn font_hint() -> String {
        format!(
            "Font installed. Set your terminal font to {} and restart the terminal to see the icons.",
            font::FACE
        )
    }

    fn handle_font_setup_key(&mut self, step: FontSetup, key: KeyEvent) {
        let yes = matches!(key.code, KeyCode::Char('y') | KeyCode::Enter);
        let no = matches!(key.code, KeyCode::Char('n') | KeyCode::Esc);
        self.overlay = match step {
            FontSetup::Install if yes => Overlay::FontSetup(Self::install_font()),
            FontSetup::Terminal(path) if yes => Overlay::FontSetup(FontSetup::Done(
                match font::set_windows_terminal_font(&path) {
                    Ok(()) => "Done. Restart Windows Terminal to see the icons.".to_string(),
                    Err(e) => format!("Couldn't change the settings ({}). {}", e, Self::font_hint()),
                },
            )),
            FontSetup::Terminal(_) if no => Overlay::FontSetup(FontSetup::Done(Self::font_hint())),
            FontSetup::Install if no => Overlay::None,
            FontSetup::Done(_) => Overlay::None,
            step => Overlay::FontSetup(step),
        };
    }

    fn toggle_wrap(&mut self) {
        self.viewer.toggle_wrap();
        self.config.wrap = self.viewer.wrap;
        self.save_config();
    }

    fn save_config(&mut self) {
        if self.persist {
            if let Err(e) = self.config.save() {
                self.show_toast(format!("Can't save settings: {}", e));
            }
        }
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }

    /// Switches the color theme and re-colors the open file.
    fn apply_theme(&mut self, index: usize) {
        if index == theme::current_index() {
            return;
        }
        theme::set(index);
        if self.viewer.mode == ViewMode::Diff {
            self.restyle_diff();
        } else {
            self.viewer.reload();
        }
    }

    /// Re-runs the diff of the open file, keeping the scroll position.
    fn restyle_diff(&mut self) {
        if let Some(path) = self.viewer.path.clone() {
            let scroll = self.viewer.scroll;
            self.show_diff(&path);
            self.viewer.scroll = scroll.min(self.viewer.max_scroll());
        }
    }

    fn show_diff(&mut self, path: &Path) {
        if let Some(git_root) = &self.git_root {
            self.viewer.show_diff(path, git::git_diff(git_root, path));
        }
    }

    fn open_settings(&mut self) {
        let Some(path) = crate::config::config_path() else {
            self.show_toast("No settings folder found");
            return;
        };
        if !path.exists() {
            if let Err(e) = self.config.save() {
                self.show_toast(format!("Can't create settings: {}", e));
                return;
            }
        }
        self.open_in_editor(&path, 1);
    }

    /// `y` / `Y`: copies the relative path of the open (or selected) file, optionally with the line.
    fn copy_path(&mut self, with_line: bool) {
        let Some(path) = self.viewer.path.clone().or_else(|| self.tree.selected_path()) else {
            return;
        };
        let mut text = self.rel(&path).unwrap_or_else(|| path.to_string_lossy().into_owned());
        if with_line && self.viewer.path.as_ref() == Some(&path) && !self.viewer.lines.is_empty() {
            text.push_str(&format!(":{}", self.viewer.current_line()));
        }
        self.show_toast(format!("Copied {}", text));
        self.clipboard = Some(text);
    }

    /// `e`: opens the file in the editor at the line being read.
    fn open_current_in_editor(&mut self) {
        if let Some(path) = self.viewer.path.clone() {
            let line = self.viewer.current_line();
            self.open_in_editor(&path, line);
        } else if let Some(entry) = self.tree.selected_entry().filter(|e| !e.is_dir) {
            let path = entry.path.clone();
            self.open_in_editor(&path, 1);
        }
    }

    /// Opens `path` at a 1-based line with the configured editor command.
    pub fn open_in_editor(&mut self, path: &Path, line: usize) {
        match spawn_editor(&self.config.editor, path, line) {
            Ok(program) => self.show_toast(format!("Opened in {}", program)),
            Err(e) => self.show_toast(format!("Can't run editor: {}", e)),
        }
    }

    fn open_finder(&mut self) {
        let recent = self
            .recent
            .iter()
            .filter_map(|p| p.strip_prefix(&self.root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        self.overlay = Overlay::Finder(Finder::with_recent(&self.files, recent));
    }

    // ---- history ----

    fn current_location(&self) -> Option<Location> {
        let path = self.viewer.path.clone()?;
        (self.viewer.mode == ViewMode::Content).then_some(Location {
            path,
            scroll: self.viewer.scroll,
        })
    }

    /// Called after every input: while the file has focus, keeps track of where the user is
    /// reading, and records the previous place in the history when they moved somewhere else.
    fn sync_history(&mut self) {
        if self.focus != Focus::Viewer || !matches!(self.overlay, Overlay::None) {
            return;
        }
        let Some(current) = self.current_location() else { return };
        let moved = self
            .reading
            .as_ref()
            .is_some_and(|r| r.path != current.path || self.jumped);
        if moved {
            let previous = self.reading.take().unwrap();
            if self.back.last() != Some(&previous) {
                self.back.push(previous);
                if self.back.len() > MAX_HISTORY {
                    self.back.remove(0);
                }
            }
            self.forward.clear();
        }
        if self.reading.as_ref().is_none_or(|r| r.path != current.path) {
            self.note_recent(current.path.clone());
        }
        self.reading = Some(current);
        self.jumped = false;
    }

    fn note_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }

    /// Alt+← / Alt+→.
    fn travel(&mut self, backwards: bool) {
        let current = self.current_location().or_else(|| self.reading.clone());
        let target = if backwards { self.back.pop() } else { self.forward.pop() };
        let Some(target) = target else {
            self.show_toast(if backwards { "No previous location" } else { "No next location" });
            return;
        };
        if let Some(current) = current {
            if backwards {
                self.forward.push(current);
            } else {
                self.back.push(current);
            }
        }
        self.show_location(&target);
        self.reading = Some(target);
        self.jumped = false;
        self.focus = Focus::Viewer;
    }

    fn show_location(&mut self, location: &Location) {
        if !location.path.exists() {
            self.show_toast("That file no longer exists");
            return;
        }
        if self.viewer.path.as_ref() != Some(&location.path) || self.viewer.mode != ViewMode::Content {
            self.viewer.open(&location.path);
        }
        self.viewer.scroll = location.scroll.min(self.viewer.max_scroll());
        if self.side_mode == SideMode::Tree {
            self.tree.reveal(&location.path);
            self.tree.ensure_visible(self.side_height());
        }
    }

    fn open_search(&mut self) {
        self.set_side_mode(SideMode::Search);
    }

    /// Opens a file chosen in the finder: reveal it in the tree and focus it.
    fn open_from_finder(&mut self, rel: &str) {
        let path = self.root.join(rel);
        self.tree.reveal(&path);
        self.tree.ensure_visible(self.side_height());
        self.side_mode = SideMode::Tree;
        self.viewer.open(&path);
        self.focus = Focus::Viewer;
        self.jumped = true;
    }

    fn toggle_diff(&mut self) {
        let Some(path) = self.viewer.path.clone() else { return };
        if self.viewer.mode == ViewMode::Diff {
            self.viewer.show_content();
        } else {
            self.show_diff(&path);
        }
    }

    fn toggle_show_all(&mut self) {
        self.tree.set_show_all(!self.tree.show_all);
        self.tree.ensure_visible(self.side_height());
    }

    // ---- keyboard ----

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.dispatch_key(key);
        self.sync_history();
    }

    fn dispatch_key(&mut self, key: KeyEvent) {
        if self.handle_overlay_key(key) {
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) && matches!(key.code, KeyCode::Left | KeyCode::Right) {
            self.travel(key.code == KeyCode::Left);
            return;
        }
        if key.code == KeyCode::F(1) {
            self.open_palette();
            return;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl {
            match key.code {
                KeyCode::Char('p') | KeyCode::Char('P')
                    if key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    self.open_palette()
                }
                KeyCode::Char('p') | KeyCode::Char('P') => self.open_finder(),
                KeyCode::Char('o') | KeyCode::Char('O')
                    if key.modifiers.contains(KeyModifiers::SHIFT) =>
                {
                    self.open_symbols()
                }
                KeyCode::Char('r') | KeyCode::Char('R') => self.open_symbols(),
                KeyCode::Char('f') | KeyCode::Char('F') => self.open_search(),
                KeyCode::Char('g') | KeyCode::Char('G') => {
                    if !self.viewer.lines.is_empty() {
                        self.overlay = Overlay::GotoLine(String::new());
                    }
                }
                KeyCode::Char('c') | KeyCode::Char('q') => self.running = false,
                _ => {}
            }
            return;
        }

        if self.viewer.find.as_ref().is_some_and(|f| f.editing) {
            self.handle_find_key(key);
            return;
        }

        if self.focus == Focus::Side && self.side_mode == SideMode::Search {
            if self.handle_search_key(key) {
                return;
            }
        } else if self.handle_global_key(key) {
            return;
        }

        match self.focus {
            Focus::Side => match self.side_mode {
                SideMode::Tree => self.handle_tree_key(key),
                SideMode::Changes => self.handle_changes_key(key),
                SideMode::Search => {}
            },
            Focus::Viewer => self.handle_viewer_key(key),
        }
    }

    /// Keys that work everywhere except while typing.
    fn handle_global_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Tab | KeyCode::BackTab => self.toggle_focus(),
            KeyCode::Char('h') => self.toggle_show_all(),
            KeyCode::Char('i') => self.toggle_icons(),
            KeyCode::Char('r') => self.reload(),
            KeyCode::Char('y') => self.copy_path(false),
            KeyCode::Char('Y') => self.copy_path(true),
            KeyCode::Char('e') => self.open_current_in_editor(),
            KeyCode::Char('f') if self.is_git_repo() => {
                let mode = if self.side_mode == SideMode::Changes {
                    SideMode::Tree
                } else {
                    SideMode::Changes
                };
                self.set_side_mode(mode);
            }
            KeyCode::Char('d') if self.is_git_repo() => self.toggle_diff(),
            KeyCode::Char('w') => self.toggle_wrap(),
            KeyCode::Char('m') => self.viewer.toggle_markdown_source(),
            KeyCode::PageUp => self.viewer.page(-1),
            KeyCode::PageDown => self.viewer.page(1),
            _ => return false,
        }
        true
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Side => Focus::Viewer,
            Focus::Viewer => Focus::Side,
        };
    }

    fn handle_tree_key(&mut self, key: KeyEvent) {
        let height = self.side_height();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.tree.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.tree.move_by(1),
            KeyCode::Home | KeyCode::Char('g') => self.tree.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.tree.select_last(),
            KeyCode::Right | KeyCode::Char('l') => self.tree.expand_selected(),
            KeyCode::Left => self.tree.collapse_selected(),
            KeyCode::Enter | KeyCode::Char(' ') => match self.tree.selected_entry() {
                Some(e) if e.is_dir => self.tree.toggle(self.tree.selected),
                Some(_) => self.focus = Focus::Viewer,
                None => {}
            },
            KeyCode::Char('/') => {
                self.focus = Focus::Viewer;
                self.viewer.start_find();
            }
            _ => return,
        }
        self.tree.ensure_visible(height);
        self.preview_tree_selection();
    }

    fn handle_changes_key(&mut self, key: KeyEvent) {
        let last = self.changes.entries.len().saturating_sub(1);
        let height = self.side_height().max(1);
        let c = &mut self.changes;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => c.selected = c.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => c.selected = (c.selected + 1).min(last),
            KeyCode::Home | KeyCode::Char('g') => c.selected = 0,
            KeyCode::End | KeyCode::Char('G') => c.selected = last,
            KeyCode::Enter => {
                self.focus = Focus::Viewer;
                return;
            }
            KeyCode::Esc => {
                self.set_side_mode(SideMode::Tree);
                return;
            }
            _ => return,
        }
        if c.selected < c.offset {
            c.offset = c.selected;
        } else if c.selected >= c.offset + height {
            c.offset = c.selected + 1 - height;
        }
        self.preview_change();
    }

    /// Search panel with focus: printable keys type into the query. Returns false for keys
    /// it doesn't handle.
    fn handle_search_key(&mut self, key: KeyEvent) -> bool {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let height = self.side_height();
        match key.code {
            KeyCode::Char(c) if alt => match c.to_ascii_lowercase() {
                'c' => self.toggle_search_option(0),
                'w' => self.toggle_search_option(1),
                'r' => self.toggle_search_option(2),
                'f' => self.toggle_search_option(3),
                _ => return false,
            },
            KeyCode::Char(c) => {
                self.search.field_text().push(c);
                self.search.changed();
            }
            KeyCode::Backspace => {
                self.search.field_text().pop();
                self.search.changed();
            }
            KeyCode::Up => {
                self.search.move_by(-1);
                self.search.ensure_visible(height);
                self.preview_search_hit();
            }
            KeyCode::Down => {
                self.search.move_by(1);
                self.search.ensure_visible(height);
                self.preview_search_hit();
            }
            KeyCode::Enter => {
                if self.search.is_pending() {
                    self.search.start(self.root.clone());
                } else if self.search.selected_hit().is_some() {
                    self.preview_search_hit();
                    self.focus = Focus::Viewer;
                    self.jumped = true;
                }
            }
            KeyCode::Esc => self.set_side_mode(SideMode::Tree),
            KeyCode::Tab | KeyCode::BackTab => {
                if !self.search.next_field(key.code == KeyCode::Tab) {
                    self.toggle_focus();
                }
            }
            KeyCode::PageUp => self.viewer.page(-1),
            KeyCode::PageDown => self.viewer.page(1),
            _ => return false,
        }
        true
    }

    fn toggle_search_option(&mut self, which: usize) {
        let o = &mut self.search.options;
        match which {
            0 => o.case_sensitive = !o.case_sensitive,
            1 => o.whole_word = !o.whole_word,
            2 => o.regex = !o.regex,
            _ => {
                self.search.toggle_filters();
                if self.search.include.is_empty() && self.search.exclude.is_empty() {
                    return;
                }
            }
        }
        self.search.changed();
    }

    fn handle_viewer_key(&mut self, key: KeyEvent) {
        let v = &mut self.viewer;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => v.scroll_by(-1),
            KeyCode::Down | KeyCode::Char('j') => v.scroll_by(1),
            KeyCode::Char(' ') => v.page(1),
            KeyCode::Home | KeyCode::Char('g') => v.scroll_top(),
            KeyCode::End | KeyCode::Char('G') => v.scroll_bottom(),
            KeyCode::Left => v.hscroll_by(-4),
            KeyCode::Right | KeyCode::Char('l') => v.hscroll_by(4),
            KeyCode::Char('/') => v.start_find(),
            KeyCode::Char('n') if v.find.is_none() && v.mode == ViewMode::Diff => v.next_change(true),
            KeyCode::Char('N') if v.find.is_none() && v.mode == ViewMode::Diff => v.next_change(false),
            KeyCode::Char('n') => v.find_next(true),
            KeyCode::Char('N') => v.find_next(false),
            KeyCode::Esc => {
                if v.find.is_some() || v.mark.is_some() {
                    v.find_cancel();
                    v.mark = None;
                } else {
                    self.focus = Focus::Side;
                }
            }
            _ => {}
        }
    }

    fn handle_find_key(&mut self, key: KeyEvent) {
        let v = &mut self.viewer;
        match key.code {
            KeyCode::Char(c) => v.find_input(c),
            KeyCode::Backspace => v.find_backspace(),
            KeyCode::Enter => v.find_confirm(),
            KeyCode::Esc => v.find_cancel(),
            KeyCode::Down => v.find_next(true),
            KeyCode::Up => v.find_next(false),
            _ => {}
        }
    }

    /// Returns true when an overlay consumed the key.
    fn handle_overlay_key(&mut self, key: KeyEvent) -> bool {
        match &mut self.overlay {
            Overlay::None => false,
            Overlay::Palette(_) => {
                self.handle_palette_key(key);
                true
            }
            Overlay::Symbols(_) => {
                self.handle_symbols_key(key);
                true
            }
            Overlay::Help => {
                self.overlay = Overlay::None;
                true
            }
            Overlay::FontSetup(step) => {
                let step = step.clone();
                self.handle_font_setup_key(step, key);
                true
            }
            Overlay::GotoLine(input) => {
                match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() => input.push(c),
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Enter => {
                        if let Ok(n) = input.parse::<usize>() {
                            let line = n.saturating_sub(1).min(self.viewer.lines.len().saturating_sub(1));
                            self.viewer.goto_line(line);
                            self.viewer.mark = Some(crate::viewer::Mark { line, ranges: Vec::new() });
                            self.jumped = true;
                            self.focus = Focus::Viewer;
                        }
                        self.overlay = Overlay::None;
                    }
                    KeyCode::Esc => self.overlay = Overlay::None,
                    _ => {}
                }
                true
            }
            Overlay::Finder(finder) => {
                let height = self.areas.popup_list.height as usize;
                match key.code {
                    KeyCode::Esc => self.overlay = Overlay::None,
                    KeyCode::Enter => {
                        if let Some(rel) = finder.selected_path().map(str::to_string) {
                            self.overlay = Overlay::None;
                            self.open_from_finder(&rel);
                        }
                    }
                    KeyCode::Up => finder.move_by(-1),
                    KeyCode::Down => finder.move_by(1),
                    KeyCode::PageUp => finder.move_by(-(height.max(1) as isize)),
                    KeyCode::PageDown => finder.move_by(height.max(1) as isize),
                    KeyCode::Backspace => {
                        finder.input.pop();
                        finder.update(&self.files);
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.overlay = Overlay::None
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        finder.input.push(c);
                        finder.update(&self.files);
                    }
                    _ => {}
                }
                if let Overlay::Finder(f) = &mut self.overlay {
                    f.ensure_visible(height);
                }
                true
            }
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) {
        let Overlay::Palette(mut palette) = std::mem::replace(&mut self.overlay, Overlay::None) else {
            return;
        };
        let height = (self.areas.popup_list.height as isize).max(1);
        match key.code {
            KeyCode::Esc => {
                if let PaletteMode::Themes { original } = palette.mode {
                    self.apply_theme(original);
                }
                return;
            }
            KeyCode::Enter => {
                self.accept_palette(palette);
                return;
            }
            KeyCode::Up => palette.picker.move_by(-1),
            KeyCode::Down => palette.picker.move_by(1),
            KeyCode::PageUp => palette.picker.move_by(-height),
            KeyCode::PageDown => palette.picker.move_by(height),
            KeyCode::Backspace => {
                palette.picker.input.pop();
                palette.update();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                palette.picker.input.push(c);
                palette.update();
            }
            _ => {}
        }
        palette.picker.ensure_visible(height as usize);
        self.preview_palette_theme(&palette);
        self.overlay = Overlay::Palette(palette);
    }

    fn open_symbols(&mut self) {
        let Some(path) = self.viewer.path.clone() else { return };
        if self.viewer.lines.is_empty() || self.viewer.mode != ViewMode::Content {
            return;
        }
        if self.viewer.is_markdown() && !self.viewer.markdown_source {
            // Symbol lines refer to the source.
            self.viewer.toggle_markdown_source();
        }
        let symbols = find_symbols(&self.viewer.lines, &path.to_string_lossy());
        if symbols.is_empty() {
            self.show_toast("No symbols found in this file");
            return;
        }
        let current = self.viewer.current_line() - 1;
        self.overlay = Overlay::Symbols(SymbolPicker::new(symbols, current, self.viewer.scroll));
        // Show where the preselected symbol is.
        self.handle_symbols_key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
    }

    fn handle_symbols_key(&mut self, key: KeyEvent) {
        let Overlay::Symbols(mut symbols) = std::mem::replace(&mut self.overlay, Overlay::None) else {
            return;
        };
        let height = (self.areas.popup_list.height as isize).max(1);
        match key.code {
            KeyCode::Esc => {
                self.viewer.scroll = symbols.origin_scroll;
                self.viewer.mark = None;
                return;
            }
            KeyCode::Enter => {
                if symbols.selected().is_some() {
                    self.focus = Focus::Viewer;
                    self.jumped = true;
                } else {
                    self.viewer.scroll = symbols.origin_scroll;
                }
                return;
            }
            KeyCode::Up => symbols.picker.move_by(-1),
            KeyCode::Down => symbols.picker.move_by(1),
            KeyCode::PageUp => symbols.picker.move_by(-height),
            KeyCode::PageDown => symbols.picker.move_by(height),
            KeyCode::Backspace => {
                symbols.picker.input.pop();
                symbols.update();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                symbols.picker.input.push(c);
                symbols.update();
            }
            _ => {}
        }
        symbols.picker.ensure_visible(height as usize);
        // The file follows the highlighted symbol.
        if let Some(symbol) = symbols.selected() {
            let ranges = self.viewer.lines[symbol.line]
                .find(&symbol.name)
                .map(|start| vec![(start, start + symbol.name.len())])
                .unwrap_or_default();
            self.viewer.goto_line(symbol.line);
            self.viewer.mark = Some(crate::viewer::Mark {
                line: symbol.line,
                ranges,
            });
        }
        self.overlay = Overlay::Symbols(symbols);
    }

    /// In the theme picker the highlighted theme is applied right away.
    fn preview_palette_theme(&mut self, palette: &Palette) {
        if let (PaletteMode::Themes { .. }, Some(index)) = (&palette.mode, palette.selected_index()) {
            self.apply_theme(index);
        }
    }

    fn accept_palette(&mut self, palette: Palette) {
        match palette.mode {
            PaletteMode::Commands(_) => {
                if let Some(command) = palette.selected_command() {
                    self.execute(command);
                }
            }
            PaletteMode::Themes { original } => {
                let index = palette.selected_index().unwrap_or(original);
                self.apply_theme(index);
                self.config.theme = theme::THEMES[index].name.to_string();
                self.save_config();
            }
        }
    }

    /// Pasted text goes into whichever input is active (newlines become spaces).
    pub fn handle_paste(&mut self, text: &str) {
        let text = text.trim_end_matches(['\r', '\n']).replace(['\r', '\n'], " ");
        if let Overlay::Finder(finder) = &mut self.overlay {
            finder.input.push_str(&text);
            finder.update(&self.files);
        } else if self.viewer.find.as_ref().is_some_and(|f| f.editing) {
            text.chars().for_each(|c| self.viewer.find_input(c));
        } else if self.side_mode == SideMode::Search && self.focus == Focus::Side {
            self.search.input.push_str(&text);
            self.search.changed();
        } else if matches!(self.overlay, Overlay::None) {
            // Pasting anywhere else starts a project search for it.
            self.open_search();
            self.search.input = text;
            self.search.changed();
        }
    }

    // ---- mouse ----

    /// Returns true when the screen needs a redraw.
    pub fn handle_mouse(&mut self, m: MouseEvent) -> bool {
        let redraw = self.dispatch_mouse(m);
        self.sync_history();
        redraw
    }

    fn dispatch_mouse(&mut self, m: MouseEvent) -> bool {
        let pos = Position::new(m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let double = self.last_click.is_some_and(|(t, p)| p == pos && t.elapsed() < DOUBLE_CLICK);
                self.last_click = Some((Instant::now(), pos));
                self.click(pos, double);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging_splitter => {
                let main = self.areas.main;
                let max = main.width.saturating_sub(MIN_VIEWER_WIDTH).max(MIN_SIDE_WIDTH);
                self.side_width = Some((m.column.saturating_sub(main.x) + 1).clamp(MIN_SIDE_WIDTH, max));
            }
            MouseEventKind::Up(MouseButton::Left) => self.dragging_splitter = false,
            MouseEventKind::ScrollDown => self.wheel(pos, WHEEL_STEP),
            MouseEventKind::ScrollUp => self.wheel(pos, -WHEEL_STEP),
            MouseEventKind::ScrollRight => self.viewer.hscroll_by(4),
            MouseEventKind::ScrollLeft => self.viewer.hscroll_by(-4),
            _ => return false,
        }
        true
    }

    fn click(&mut self, pos: Position, double: bool) {
        match &mut self.overlay {
            Overlay::None => {}
            Overlay::Finder(finder) => {
                let list = self.areas.popup_list;
                if list.contains(pos) {
                    let index = finder.offset + (pos.y - list.y) as usize;
                    if let Some(rel) = finder.results.get(index).map(|r| r.path.clone()) {
                        self.overlay = Overlay::None;
                        self.open_from_finder(&rel);
                    }
                } else if !self.areas.popup.contains(pos) {
                    self.overlay = Overlay::None;
                }
                return;
            }
            Overlay::Palette(palette) => {
                let list = self.areas.popup_list;
                if list.contains(pos) {
                    let index = palette.picker.offset + (pos.y - list.y) as usize;
                    if index < palette.picker.results.len() {
                        palette.picker.selected = index;
                        self.handle_palette_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    }
                } else if !self.areas.popup.contains(pos) {
                    self.handle_palette_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
                }
                return;
            }
            Overlay::Symbols(symbols) => {
                let list = self.areas.popup_list;
                if list.contains(pos) {
                    let index = symbols.picker.offset + (pos.y - list.y) as usize;
                    if index < symbols.picker.results.len() {
                        symbols.picker.selected = index;
                        self.handle_symbols_key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                        self.handle_symbols_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    }
                } else if !self.areas.popup.contains(pos) {
                    self.handle_symbols_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
                }
                return;
            }
            Overlay::GotoLine(_) | Overlay::Help | Overlay::FontSetup(_) => {
                self.overlay = Overlay::None;
                return;
            }
        }

        let side = self.areas.side;
        if pos.x == side.right().saturating_sub(1) && pos.y >= side.y && pos.y < side.bottom() {
            self.dragging_splitter = true;
            return;
        }

        if side.contains(pos) {
            self.focus = Focus::Side;
            if self.side_mode == SideMode::Search {
                for (i, r) in self.areas.search_toggles.iter().enumerate() {
                    if r.contains(pos) {
                        self.toggle_search_option(i);
                        return;
                    }
                }
                let fields = [Field::Query, Field::Include, Field::Exclude];
                for (r, field) in self.areas.search_fields.iter().zip(fields) {
                    if r.contains(pos) {
                        self.search.field = field;
                        return;
                    }
                }
            }
            let list = self.areas.side_list;
            if list.contains(pos) {
                self.click_side_row((pos.y - list.y) as usize, double);
            }
        } else if self.areas.viewer.contains(pos) {
            self.focus = Focus::Viewer;
        }
    }

    fn click_side_row(&mut self, row: usize, double: bool) {
        match self.side_mode {
            SideMode::Tree => {
                let index = self.tree.offset + row;
                let Some(entry) = self.tree.rows.get(index).map(|r| r.entry.clone()) else {
                    return;
                };
                self.tree.select(index);
                if entry.is_dir {
                    self.tree.toggle(index);
                } else if double {
                    self.focus = Focus::Viewer;
                }
                self.preview_tree_selection();
            }
            SideMode::Changes => {
                let index = self.changes.offset + row;
                if index < self.changes.entries.len() {
                    self.changes.selected = index;
                    self.preview_change();
                    if double {
                        self.focus = Focus::Viewer;
                    }
                }
            }
            SideMode::Search => {
                self.search.select_row(self.search.offset + row);
                self.preview_search_hit();
                if double {
                    self.focus = Focus::Viewer;
                    self.jumped = true;
                }
            }
        }
    }

    fn wheel(&mut self, pos: Position, delta: isize) {
        if let Overlay::Finder(finder) = &mut self.overlay {
            finder.move_by(delta.signum());
            finder.ensure_visible(self.areas.popup_list.height as usize);
            return;
        }
        if matches!(self.overlay, Overlay::Palette(_)) {
            let code = if delta > 0 { KeyCode::Down } else { KeyCode::Up };
            self.handle_palette_key(KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
        if matches!(self.overlay, Overlay::Symbols(_)) {
            let code = if delta > 0 { KeyCode::Down } else { KeyCode::Up };
            self.handle_symbols_key(KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
        if self.areas.side.contains(pos) {
            let height = self.side_height();
            match self.side_mode {
                SideMode::Tree => self.tree.scroll_by(delta, height),
                SideMode::Search => self.search.scroll_by(delta, height),
                SideMode::Changes => {
                    let max = self.changes.entries.len().saturating_sub(height);
                    self.changes.offset =
                        ((self.changes.offset as isize + delta).max(0) as usize).min(max);
                }
            }
        } else if self.areas.viewer.contains(pos) {
            self.viewer.scroll_by(delta);
        }
    }
}

/// Splits a command line into words; double quotes group words.
fn split_command(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut has_word = false;
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has_word {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            c => {
                current.push(c);
                has_word = true;
            }
        }
    }
    if has_word {
        words.push(current);
    }
    words
}

/// The editor command with `{file}` and `{line}` filled in (the file is appended when the
/// command has no `{file}`).
fn editor_args(command: &str, path: &Path, line: usize) -> Vec<String> {
    let file = path.to_string_lossy();
    let mut words = split_command(command);
    if !command.contains("{file}") {
        words.push("{file}".to_string());
    }
    words
        .into_iter()
        .map(|w| w.replace("{file}", &file).replace("{line}", &line.to_string()))
        .collect()
}

/// Starts the editor detached from the terminal. Returns the program name.
fn spawn_editor(command: &str, path: &Path, line: usize) -> Result<String, String> {
    use std::process::{Command as Process, Stdio};

    let args = editor_args(command, path, line);
    let Some(program) = args.first().cloned() else {
        return Err("no editor command set".to_string());
    };
    // On Windows editors are often `.cmd` scripts (like `code`), which only `cmd` can run.
    let mut process = if cfg!(windows) {
        let mut p = Process::new("cmd");
        p.arg("/C").args(&args);
        p
    } else {
        let mut p = Process::new(&program);
        p.args(&args[1..]);
        p
    };
    process
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| program.clone())
        .map_err(|e| format!("{} ({})", program, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};
    use tempfile::TempDir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char(c))
        }
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// root/
    ///   src/main.rs   "fn main() {}\n// TODO later\n"
    ///   notes.txt     "hello\n"
    fn project() -> (TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("main.rs"), "fn main() {}\n// TODO later\n").unwrap();
        std::fs::write(root.join("notes.txt"), "hello\n").unwrap();
        let mut app = App::with_root(root.to_path_buf());
        app.areas.side = Rect::new(0, 1, 30, 20);
        app.areas.side_list = Rect::new(1, 2, 28, 18);
        app.areas.viewer = Rect::new(30, 1, 50, 20);
        app.areas.main = Rect::new(0, 1, 80, 20);
        (temp, app)
    }

    fn wait_for_files(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.files_loading && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn test_starts_with_tree_and_folder_preview() {
        let (_t, app) = project();
        assert_eq!(app.side_mode, SideMode::Tree);
        assert_eq!(app.tree.rows.len(), 2);
        assert!(app.viewer.message.as_deref().unwrap().starts_with("src/"));
    }

    #[test]
    fn test_font_setup_steps() {
        let (_t, mut app) = project();
        app.overlay = Overlay::FontSetup(FontSetup::Install);
        app.handle_key(key(KeyCode::Char('x')));
        assert!(matches!(app.overlay, Overlay::FontSetup(FontSetup::Install)));
        app.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(app.overlay, Overlay::None));

        app.overlay = Overlay::FontSetup(FontSetup::Terminal(PathBuf::from("settings.json")));
        app.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(&app.overlay, Overlay::FontSetup(FontSetup::Done(m)) if m.contains(font::FACE)));
        app.handle_key(key(KeyCode::Char('q')));
        assert!(matches!(app.overlay, Overlay::None));
        assert!(app.running);
    }

    #[test]
    fn test_moving_selection_previews_file() {
        let (t, mut app) = project();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.viewer.path, Some(t.path().join("notes.txt")));
        assert_eq!(app.viewer.lines, vec!["hello"]);
    }

    #[test]
    fn test_enter_expands_folder_and_focuses_file() {
        let (t, mut app) = project();
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.tree.rows.len(), 3);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.viewer.path, Some(t.path().join("src").join("main.rs")));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Viewer);
    }

    #[test]
    fn test_click_folder_toggles_and_click_file_previews() {
        let (t, mut app) = project();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 2));
        assert_eq!(app.tree.rows.len(), 3);
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 3));
        assert_eq!(app.viewer.path, Some(t.path().join("src").join("main.rs")));
        assert_eq!(app.focus, Focus::Side);
        // Double click focuses the viewer.
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 3));
        assert_eq!(app.focus, Focus::Viewer);
    }

    #[test]
    fn test_drag_splitter_resizes_side_panel() {
        let (_t, mut app) = project();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 29, 5));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 40, 5));
        assert_eq!(app.side_width, Some(41));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 79, 5));
        assert_eq!(app.side_width, Some(50));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 79, 5));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 30, 5));
        assert_eq!(app.side_width, Some(50));
    }

    #[test]
    fn test_finder_opens_and_reveals_file() {
        let (t, mut app) = project();
        wait_for_files(&mut app);
        app.handle_key(ctrl('p'));
        for c in "main".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::None));
        let path = t.path().join("src").join("main.rs");
        assert_eq!(app.viewer.path, Some(path.clone()));
        assert_eq!(app.tree.selected_path(), Some(path));
        assert_eq!(app.focus, Focus::Viewer);
    }

    #[test]
    fn test_project_search_types_and_opens_hit_at_line() {
        let (t, mut app) = project();
        app.handle_key(ctrl('f'));
        assert_eq!(app.side_mode, SideMode::Search);
        for c in "todo".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        // Letters type into the query instead of triggering shortcuts.
        assert_eq!(app.search.input, "todo");
        assert!(app.viewer.find.is_none());
        app.search.start(app.root.clone());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.search.running && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        app.tick();
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Viewer);
        assert_eq!(app.viewer.path, Some(t.path().join("src").join("main.rs")));
        let mark = app.viewer.mark.as_ref().unwrap();
        assert_eq!(mark.line, 1);
        assert_eq!(mark.ranges, vec![(3, 7)]);
        // Esc in the search panel goes back to the tree.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.side_mode, SideMode::Tree);
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn test_changes_on_disk_refresh_tree_and_open_file() {
        let (t, mut app) = project();
        app.handle_key(key(KeyCode::Down)); // notes.txt
        app.handle_key(key(KeyCode::Down));
        app.viewer.scroll = 0;
        std::fs::write(t.path().join("notes.txt"), "rewritten\nby agent\n").unwrap();
        std::fs::write(t.path().join("new.rs"), "").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while app.viewer.lines != ["rewritten", "by agent"] && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(app.viewer.lines, vec!["rewritten", "by agent"]);
        assert!(app.tree.rows.iter().any(|r| r.entry.name == "new.rs"));
        assert_eq!(app.tree.selected_entry().unwrap().name, "notes.txt");
    }

    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent {
            modifiers: KeyModifiers::ALT,
            ..key(code)
        }
    }

    fn open_with_finder(app: &mut App, name: &str) {
        app.handle_key(ctrl('p'));
        type_text(app, name);
        app.handle_key(key(KeyCode::Enter));
    }

    #[test]
    fn test_back_and_forward_between_files_and_lines() {
        let (t, mut app) = project();
        wait_for_files(&mut app);
        let notes = t.path().join("notes.txt");
        let main = t.path().join("src").join("main.rs");

        open_with_finder(&mut app, "notes");
        open_with_finder(&mut app, "main");
        assert_eq!(app.viewer.path, Some(main.clone()));

        // Go to line in the same file is a jump too.
        app.handle_key(ctrl('g'));
        app.handle_key(key(KeyCode::Char('2')));
        app.handle_key(key(KeyCode::Enter));

        app.handle_key(alt(KeyCode::Left));
        assert_eq!(app.viewer.path, Some(main.clone()));
        app.handle_key(alt(KeyCode::Left));
        assert_eq!(app.viewer.path, Some(notes.clone()));
        assert_eq!(app.tree.selected_path(), Some(notes.clone()));
        app.handle_key(alt(KeyCode::Left));
        assert!(app.toast.as_ref().unwrap().0.contains("No previous"));

        app.handle_key(alt(KeyCode::Right));
        assert_eq!(app.viewer.path, Some(main.clone()));

        // Most recently read first in the finder.
        assert_eq!(app.recent, vec![main, notes]);
        app.handle_key(ctrl('p'));
        match &app.overlay {
            Overlay::Finder(f) => {
                assert_eq!(f.results[0].path, "src/main.rs");
                assert_eq!(f.results[1].path, "notes.txt");
            }
            _ => panic!("finder not open"),
        }
    }

    #[test]
    fn test_project_state_restores_folders_file_and_recent() {
        let (t, mut app) = project();
        wait_for_files(&mut app);
        open_with_finder(&mut app, "notes");
        open_with_finder(&mut app, "main");
        app.viewer.scroll = 1;
        let state = app.project_state();
        assert_eq!(state.expanded, vec!["src"]);
        assert_eq!(state.file.as_deref(), Some("src/main.rs"));
        assert_eq!(state.recent, vec!["src/main.rs", "notes.txt"]);

        let mut fresh = App::with_root(t.path().to_path_buf());
        fresh.restore_state(&state);
        let main = t.path().join("src").join("main.rs");
        assert_eq!(fresh.viewer.path, Some(main.clone()));
        assert_eq!(fresh.viewer.scroll, 1);
        assert_eq!(fresh.tree.selected_path(), Some(main));
        assert_eq!(fresh.tree.rows.len(), 3);
        assert_eq!(fresh.recent.len(), 2);
    }

    #[test]
    fn test_arrowing_through_tree_does_not_fill_history() {
        let (_t, mut app) = project();
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Down));
        assert!(app.back.is_empty());
        assert!(app.recent.is_empty());
    }

    #[test]
    fn test_go_to_symbol_previews_jumps_and_restores() {
        let (t, mut app) = project();
        let code: String = (0..100)
            .map(|i| match i {
                10 => "fn early() {}\n".to_string(),
                80 => "fn late_one() {}\n".to_string(),
                _ => format!("// line {}\n", i),
            })
            .collect();
        let path = t.path().join("big.rs");
        std::fs::write(&path, code).unwrap();
        app.viewer.open(&path);
        app.focus = Focus::Viewer;

        app.handle_key(ctrl('r'));
        type_text(&mut app, "late");
        assert!(app.viewer.scroll > 50, "file follows the selection");
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.viewer.scroll, 0, "Esc goes back");
        assert!(app.viewer.mark.is_none());

        app.handle_key(ctrl('r'));
        type_text(&mut app, "late");
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::None));
        let mark = app.viewer.mark.as_ref().unwrap();
        assert_eq!((mark.line, mark.ranges.clone()), (80, vec![(3, 11)]));

        app.handle_key(alt(KeyCode::Left));
        assert_eq!(app.viewer.scroll, 0, "back returns to where reading was");
    }

    #[test]
    fn test_copy_path_with_line() {
        let (_t, mut app) = project();
        app.handle_key(key(KeyCode::Enter)); // expand src
        app.handle_key(key(KeyCode::Down)); // src/main.rs
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.clipboard.take().as_deref(), Some("src/main.rs"));

        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('/')));
        type_text(&mut app, "todo");
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('Y')));
        assert_eq!(app.clipboard.take().as_deref(), Some("src/main.rs:2"));
        assert!(app.toast.as_ref().unwrap().0.contains("src/main.rs:2"));
    }

    #[test]
    fn test_palette_runs_commands() {
        let (_t, mut app) = project();
        app.handle_key(key(KeyCode::F(1)));
        type_text(&mut app, "wrap");
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::None));
        assert!(app.viewer.wrap);
        assert!(app.config.wrap);
    }

    #[test]
    fn test_theme_picker_previews_and_reverts() {
        let (_t, mut app) = project();
        theme::set(0);
        app.handle_key(key(KeyCode::F(1)));
        type_text(&mut app, "color theme");
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(&app.overlay, Overlay::Palette(p) if matches!(p.mode, PaletteMode::Themes { .. })));

        app.handle_key(key(KeyCode::Down));
        assert_eq!(theme::current_index(), 1, "moving previews the theme");
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(theme::current_index(), 0, "Esc restores the original");

        app.execute(Command::ColorTheme);
        type_text(&mut app, "one dark");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(theme::current().name, "One Dark");
        assert_eq!(app.config.theme, "One Dark");
        theme::set(0);
    }

    #[test]
    fn test_ctrl_shift_p_opens_palette() {
        let (_t, mut app) = project();
        app.handle_key(KeyEvent {
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ..key(KeyCode::Char('P'))
        });
        assert!(matches!(app.overlay, Overlay::Palette(_)));
    }

    #[test]
    fn test_editor_args() {
        let path = Path::new("C:\\my dir\\a.rs");
        assert_eq!(
            editor_args("code --goto {file}:{line}", path, 12),
            vec!["code", "--goto", "C:\\my dir\\a.rs:12"]
        );
        assert_eq!(
            editor_args("\"C:\\Program Files\\Notepad++\\notepad++.exe\" -n{line}", path, 3),
            vec!["C:\\Program Files\\Notepad++\\notepad++.exe", "-n3", "C:\\my dir\\a.rs"]
        );
    }

    #[test]
    fn test_paste_goes_to_active_input() {
        let (_t, mut app) = project();
        app.handle_paste("fn main\n");
        assert_eq!(app.side_mode, SideMode::Search);
        assert_eq!(app.search.input, "fn main");

        app.handle_key(ctrl('p'));
        app.handle_paste("notes");
        match &app.overlay {
            Overlay::Finder(f) => assert_eq!(f.input, "notes"),
            _ => panic!("finder closed"),
        }
    }

    #[test]
    fn test_goto_line_overlay() {
        let (_t, mut app) = project();
        app.handle_key(key(KeyCode::Down));
        app.handle_key(ctrl('g'));
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.viewer.mark.as_ref().unwrap().line, 0);
    }

    #[test]
    fn test_find_in_file_captures_typing() {
        let (_t, mut app) = project();
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.running);
        assert_eq!(app.viewer.find.as_ref().unwrap().query, "q");
        app.handle_key(key(KeyCode::Esc));
        assert!(app.viewer.find.is_none());
    }

    #[test]
    fn test_q_quits() {
        let (_t, mut app) = project();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.running);
    }

    fn git(dir: &Path, args: &[&str]) {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_dirty_markers_and_changes_list() {
        let (t, _) = project();
        let root = t.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        git(root, &["add", "."]);
        git(root, &["commit", "-m", "init"]);
        std::fs::write(root.join("src").join("main.rs"), "changed\n").unwrap();

        let mut app = App::with_root(root.to_path_buf());
        assert!(app.is_git_repo());
        assert!(app.is_dirty(&root.join("src").join("main.rs")));
        assert!(app.has_dirty_children(&root.join("src")));
        assert!(!app.is_dirty(&root.join("notes.txt")));

        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.side_mode, SideMode::Changes);
        assert_eq!(app.changes.entries[0].name, "src/main.rs");
        // The changes list shows the file with its changes inline.
        assert_eq!(app.viewer.mode, ViewMode::Diff);
        assert_eq!(app.viewer.lines, vec!["fn main() {}", "// TODO later", "changed"]);
        assert_eq!(app.viewer.diff_stats(), (1, 2));

        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.viewer.mode, ViewMode::Content);
        assert_eq!(app.viewer.lines, vec!["changed"]);
    }
}
