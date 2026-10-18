//! The interactive folder browser.
//!
//! The UI is drawn on **stderr** (in the alternate screen) so that stdout stays
//! clean for the single thing the shell wrapper needs: the chosen path.
//!
//! Layout: favourites on the left, a separator, folder navigation on the right.
//! Two modes decide where the cursor lives: `Nav` (the default) and `Favs`
//! (entered with Tab, for reordering, renaming and removing favourites).

use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::commands;
use crate::config::{Action, Config};
use crate::favorites::{self, Favorite};
use crate::fsutil;

/// How a browsing session ended; `main` turns it into output and an exit code.
pub enum Outcome {
    /// The user chose this folder: print it so the shell can `cd` there.
    Selected(PathBuf),
    /// The user left without choosing: print nothing and change nothing.
    Cancelled,
}

/// Where the browser currently is.
enum Location {
    /// An ordinary folder.
    Dir(PathBuf),
    /// Windows only: the list of drive letters above `C:\`.
    Drives,
}

/// What a row in the folder list stands for.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// The "." row: selecting it picks the folder you are standing in.
    Here,
    /// A sub-folder: Enter opens it, Space chooses it.
    Folder,
    /// A Windows drive root such as `C:\`, shown in the drive list.
    Drive,
}

/// One row of the folder list (the "." row, a folder, or a drive).
#[derive(Clone)]
struct Entry {
    /// Text shown on screen: the folder name, `.`, or the drive root.
    label: String,
    /// Absolute path this row stands for; what Space chooses.
    path: PathBuf,
    /// Which sort of row this is.
    kind: Kind,
}

/// Which panel owns the cursor.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// The cursor is in the folder list (the default).
    Nav,
    /// The cursor is in the favourites panel (reorder, rename, remove).
    Favs,
}

/// A one-line text prompt (used to rename a favourite).
struct Input {
    /// The text typed so far.
    buf: String,
    /// Index in `State::favs` of the favourite being renamed.
    target: usize,
}

/// Everything the browser knows while it runs: where it is, what is listed, the
/// favourites, which panel has the cursor, and any message for the user.
struct State {
    // navigation
    /// The folder (or drive list) being browsed.
    loc: Location,
    /// Whether hidden folders are listed; starts from the config, toggled at runtime.
    show_hidden: bool,
    /// The loaded configuration: key bindings, limits and switches.
    cfg: Config,
    /// Everything in the folder; `entries` is the part that matches `filter`.
    all: Vec<Entry>,
    /// The rows on screen: `all` narrowed by `filter`.
    entries: Vec<Entry>,
    /// Index in `entries` of the highlighted row.
    cursor: usize,
    /// Index in `entries` of the first visible row.
    scroll: usize,
    /// Rows moved by a page-up or page-down; set from the window height on each draw.
    page: usize,
    /// Text typed after `/`. Stays applied after filter mode is closed.
    filter: String,
    /// The filter box has the keyboard.
    filtering: bool,
    // favourites
    /// The favourites, in slot order (slot 1 first).
    favs: Vec<Favorite>,
    /// Where favourites are saved; None if there is no usable config folder.
    store: Option<PathBuf>,
    /// Which panel the keys act on.
    mode: Mode,
    /// Index in `favs` of the highlighted favourite.
    fav_cursor: usize,
    /// Index in `favs` of the first visible favourite.
    fav_scroll: usize,
    /// While an item is "picked up": the order before the pick-up, and its slot.
    grabbed: Option<(Vec<Favorite>, usize)>,
    /// The order was changed and has not been written to disk yet.
    dirty: bool,
    /// The rename prompt, while it is open.
    input: Option<Input>,
    // misc
    /// A one-line message for the footer; cleared on the next key press.
    notice: Option<String>,
    /// Terminal width in columns at the last draw (decides whether the favourites panel fits).
    term_w: usize,
}

/// Longest filter we accept; more than a name could ever need.
const FILTER_MAX_CHARS: usize = 64;

/// Restores the terminal no matter how we leave (return, error, or panic).
struct TermGuard;

impl TermGuard {
    /// Switch to raw mode and the alternate screen on stderr. Dropping the guard undoes both.
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = TermGuard; // from here on, Drop undoes raw mode
        execute!(io::stderr(), EnterAlternateScreen, cursor::Hide)?;
        Ok(guard)
    }
}

impl Drop for TermGuard {
    /// Leave the alternate screen and raw mode, ignoring errors (we may be panicking).
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Run the browser. `startup_notice` is shown on the first screen (config warnings).
pub fn run(start: PathBuf, cfg: Config, startup_notice: Option<String>) -> io::Result<Outcome> {
    let store = cfg.favorites_path();
    let (favs, warning) = match store.as_deref().map(favorites::load_from) {
        Some(Ok(f)) => (f, None),
        Some(Err(e)) => (Vec::new(), Some(format!("could not read favourites: {e}"))),
        None => (Vec::new(), None),
    };
    let clashes = favorites::command_clashes(&favs);
    let clash_notice = (!clashes.is_empty()).then(|| {
        let list: Vec<String> = clashes
            .iter()
            .map(|(slot, name)| format!("{slot} ('{name}')"))
            .collect();
        format!(
            "favourite {} is named like a cdx command and cannot be reached by name; rename it",
            list.join(", ")
        )
    });
    let mut state = State::new(start, favs, store, cfg);
    state.notice = warning.or(clash_notice).or(startup_notice);

    let guard = TermGuard::enter()?;
    let mut out = BufWriter::with_capacity(16 * 1024, io::stderr());

    let outcome = loop {
        state.draw(&mut out)?;
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = state.on_key(key) {
                    break outcome;
                }
            }
            _ => {} // resize and friends simply trigger a redraw
        }
    };

    // Whatever order the favourites are in when we leave is the order we keep.
    state.flush();
    let save_problem = if state.dirty { state.notice.take() } else { None };
    drop(guard);
    if let Some(msg) = save_problem {
        eprintln!("cdx: {msg}");
    }
    Ok(outcome)
}

impl State {
    /// A browser at `start` with the given favourites, save location and settings.
    fn new(start: PathBuf, favs: Vec<Favorite>, store: Option<PathBuf>, cfg: Config) -> Self {
        let mut state = State {
            loc: Location::Dir(start),
            show_hidden: cfg.ui.show_hidden,
            cfg,
            all: Vec::new(),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            page: 10,
            filter: String::new(),
            filtering: false,
            favs,
            store,
            mode: Mode::Nav,
            fav_cursor: 0,
            fav_scroll: 0,
            grabbed: None,
            dirty: false,
            input: None,
            notice: None,
            term_w: 80,
        };
        state.reload(None);
        state
    }

    // ------------------------------------------------------------- navigation

    /// Re-read the current location. `focus` puts the cursor on that path if present.
    fn reload(&mut self, focus: Option<&Path>) {
        self.notice = None;
        let mut entries = Vec::new();

        match &self.loc {
            Location::Drives => {
                for p in fsutil::list_drives() {
                    entries.push(Entry {
                        label: p.display().to_string(),
                        path: p,
                        kind: Kind::Drive,
                    });
                }
            }
            Location::Dir(dir) => {
                entries.push(Entry {
                    label: ".".into(),
                    path: dir.clone(),
                    kind: Kind::Here,
                });
                match fsutil::list_subdirs(dir, self.show_hidden) {
                    Ok(subdirs) => {
                        for (name, path) in subdirs {
                            entries.push(Entry {
                                label: name,
                                path,
                                kind: Kind::Folder,
                            });
                        }
                    }
                    Err(e) => self.notice = Some(format!("cannot read folder: {e}")),
                }
            }
        }

        self.all = entries;
        self.apply_filter(focus);
    }

    /// Rebuild the visible list from `all` and the filter text (case-insensitive,
    /// matches anywhere in the name). The "." row is always kept.
    fn apply_filter(&mut self, focus: Option<&Path>) {
        let exact = self.cfg.ui.filter_case_sensitive;
        let needle = if exact { self.filter.clone() } else { self.filter.to_lowercase() };
        let entries: Vec<Entry> = self
            .all
            .iter()
            .filter(|e| {
                e.kind == Kind::Here
                    || needle.is_empty()
                    || if exact {
                        e.label.contains(&needle)
                    } else {
                        e.label.to_lowercase().contains(&needle)
                    }
            })
            .cloned()
            .collect();
        // Land on the first real folder by default, so Space never picks "." by accident.
        let default = if entries.len() > 1 { 1 } else { 0 };
        self.cursor = focus
            .and_then(|f| entries.iter().position(|e| e.path == f))
            .unwrap_or(default);
        self.entries = entries;
        self.scroll = 0;
    }

    /// Move the cursor by `delta` rows, stopping at the first and last row.
    fn move_by(&mut self, delta: isize) {
        let last = self.entries.len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    /// The outcome of choosing the highlighted row (None if the list is empty).
    fn select(&self) -> Option<Outcome> {
        self.entries
            .get(self.cursor)
            .map(|e| Outcome::Selected(e.path.clone()))
    }

    /// Open the highlighted folder; "." does nothing, and a refusal becomes a notice.
    fn descend(&mut self) {
        let Some(entry) = self.entries.get(self.cursor) else {
            return;
        };
        if entry.kind == Kind::Here {
            return;
        }
        let path = entry.path.clone();
        match fsutil::read_dir(&path) {
            Ok(_) => {
                self.filter.clear();
                self.loc = Location::Dir(path);
                self.reload(None);
            }
            Err(e) => self.notice = Some(format!("cannot open {}: {e}", path.display())),
        }
    }

    /// Go to the parent folder with the cursor on the one we came from; on Windows a
    /// drive root leads to the drive list.
    fn ascend(&mut self) {
        let from = match &self.loc {
            Location::Dir(p) => p.clone(),
            Location::Drives => return,
        };
        self.loc = match from.parent() {
            Some(p) if !p.as_os_str().is_empty() => Location::Dir(p.to_path_buf()),
            // A drive root on Windows: step up to the list of drives.
            _ if cfg!(windows) => Location::Drives,
            // The filesystem root elsewhere: nowhere further to go.
            _ => return,
        };
        self.filter.clear();
        self.reload(Some(&from));
    }

    /// Show or hide hidden folders, keeping the cursor on the same row if it survives.
    fn toggle_hidden(&mut self) {
        let focus = self.entries.get(self.cursor).map(|e| e.path.clone());
        self.show_hidden = !self.show_hidden;
        self.reload(focus.as_deref());
    }

    /// Jump to the home folder.
    fn go_home(&mut self) {
        match fsutil::home_dir() {
            Some(home) => {
                self.filter.clear();
                self.loc = Location::Dir(home);
                self.reload(None);
            }
            None => self.notice = Some("home folder not found".into()),
        }
    }

    // -------------------------------------------------------------- favourites

    /// The folder being browsed, or None while the drive list is shown.
    fn current_dir(&self) -> Option<&Path> {
        match &self.loc {
            Location::Dir(p) => Some(p),
            Location::Drives => None,
        }
    }

    /// Slot of the favourite that is the folder we are browsing right now.
    fn current_fav_index(&self) -> Option<usize> {
        let here = self.current_dir()?;
        self.favs.iter().position(|f| f.path == here)
    }

    /// Write the favourites file. Returns a message for the user on failure.
    fn save_now(&mut self) -> Result<(), String> {
        let Some(path) = &self.store else {
            return Err("no config folder available, favourites cannot be saved".into());
        };
        favorites::save_to(path, &self.favs)
            .map_err(|e| format!("could not save favourites: {e}"))?;
        self.dirty = false;
        Ok(())
    }

    /// Save now; on failure, tell the user (and keep `dirty` set).
    fn persist(&mut self) {
        if let Err(msg) = self.save_now() {
            self.notice = Some(msg);
        }
    }

    /// Save only if there is something unsaved.
    fn flush(&mut self) {
        if self.dirty {
            self.persist();
        }
    }

    /// How a key is shown in messages and the help line, e.g. `tab` or `↑`.
    fn nav_key(&self, action: Action) -> String {
        self.cfg.keys.label(action).unwrap_or_else(|| "(unbound)".into())
    }

    /// Like `nav_key`, for the favourites-mode bindings.
    fn fav_key(&self, action: Action) -> String {
        self.cfg.fav_keys.label(action).unwrap_or_else(|| "(unbound)".into())
    }

    /// `b`: bookmark the highlighted item. On the "." row that is the folder we
    /// are standing in.
    fn bookmark_highlighted(&mut self) {
        let Some(path) = self.entries.get(self.cursor).map(|e| e.path.clone()) else {
            self.notice = Some("nothing to bookmark here".into());
            return;
        };
        if let Some(i) = self.favs.iter().position(|f| f.path == path) {
            self.notice = Some(format!("already a favourite, on key {}", i + 1));
            return;
        }
        if self.favs.len() >= favorites::MAX {
            self.notice = Some(format!(
                "favourites are full ({}). Press {}, then {} to remove one",
                favorites::MAX,
                self.nav_key(Action::ToggleFavorites),
                self.fav_key(Action::Remove)
            ));
            return;
        }
        if !favorites::is_storable(&path) {
            self.notice = Some("this path cannot be stored as a favourite".into());
            return;
        }
        let fav = Favorite::new(path);
        if fav.clashes_with_command() {
            self.notice = Some(commands::clash_message(&fav.label()));
            return;
        }
        self.favs.push(fav);
        self.dirty = true;
        self.notice = Some(format!("added as favourite {}", self.favs.len()));
        self.persist(); // overrides the notice only if saving failed
    }

    /// 1-9. In navigation mode: go to that favourite. In favourites mode: move the
    /// cursor there, or, while an item is picked up, move the item to that slot.
    fn press_number(&mut self, digit: char) {
        let idx = (digit as u8 - b'1') as usize;
        if idx >= self.favs.len() {
            self.notice = Some(format!(
                "slot {} is empty. Press {} to bookmark the highlighted folder",
                idx + 1,
                self.nav_key(Action::Bookmark)
            ));
            return;
        }
        match self.mode {
            Mode::Nav => self.jump_to_fav(idx),
            Mode::Favs => {
                if self.grabbed.is_some() {
                    let item = self.favs.remove(self.fav_cursor);
                    self.favs.insert(idx, item);
                    self.dirty = true;
                }
                self.fav_cursor = idx;
            }
        }
    }

    /// Go to the folder of favourite `idx`. If it was deleted, mark it missing and say how to remove it.
    fn jump_to_fav(&mut self, idx: usize) {
        let path = self.favs[idx].path.clone();
        let label = self.favs[idx].label();
        match fsutil::read_dir(&path) {
            Ok(_) => {
                self.filter.clear();
                self.loc = Location::Dir(path);
                self.reload(None);
            }
            Err(e) => {
                self.notice = Some(if e.kind() == io::ErrorKind::NotFound {
                    self.favs[idx].missing = true;
                    format!(
                        "'{label}' no longer exists (press {}, then {} to remove it)",
                        self.nav_key(Action::ToggleFavorites),
                        self.fav_key(Action::Remove)
                    )
                } else {
                    format!("cannot open '{label}': {e}")
                });
            }
        }
    }

    /// Switch between navigating and editing favourites (refused when the window is too
    /// narrow or there are no favourites).
    fn toggle_mode(&mut self) {
        match self.mode {
            Mode::Nav => {
                let min_w = self.cfg.ui.min_width_for_favorites;
                if self.term_w < min_w {
                    self.notice = Some(format!(
                        "window too narrow for favourites (needs {min_w} columns)"
                    ));
                } else if self.favs.is_empty() {
                    self.notice = Some(format!(
                        "no favourites yet. Press {} to bookmark the highlighted folder",
                        self.nav_key(Action::Bookmark)
                    ));
                } else {
                    self.mode = Mode::Favs;
                    let start = self.current_fav_index().unwrap_or(self.fav_cursor);
                    self.fav_cursor = start.min(self.favs.len() - 1);
                }
            }
            Mode::Favs => self.leave_favs(),
        }
    }

    /// Back to navigation. A pick-up in progress is dropped where it is, and the
    /// favourites are saved exactly as arranged.
    fn leave_favs(&mut self) {
        self.grabbed = None;
        self.mode = Mode::Nav;
        self.flush();
    }

    /// Move the favourites cursor; while an item is picked up it swaps places with its neighbour.
    fn fav_step(&mut self, delta: isize) {
        let last = self.favs.len().saturating_sub(1) as isize;
        let to = (self.fav_cursor as isize + delta).clamp(0, last) as usize;
        if self.grabbed.is_some() && to != self.fav_cursor {
            self.favs.swap(self.fav_cursor, to);
            self.dirty = true;
        }
        self.fav_cursor = to;
    }

    /// Remove the highlighted favourite and save.
    fn remove_fav(&mut self) {
        if self.favs.is_empty() {
            return;
        }
        let removed = self.favs.remove(self.fav_cursor);
        self.dirty = true;
        self.notice = Some(format!("removed '{}'", removed.label()));
        self.persist();
        if self.favs.is_empty() {
            self.mode = Mode::Nav;
        } else {
            self.fav_cursor = self.fav_cursor.min(self.favs.len() - 1);
        }
    }

    /// Open the alias prompt for the highlighted favourite, prefilled with its current alias.
    fn start_rename(&mut self) {
        if let Some(f) = self.favs.get(self.fav_cursor) {
            self.input = Some(Input {
                buf: f.alias.clone().unwrap_or_default(),
                target: self.fav_cursor,
            });
        }
    }

    // ------------------------------------------------------------------- keys

    /// Handle one key press: the rename prompt first, then Ctrl-C, the filter box, the fixed
    /// number keys, and finally the configured bindings for the current mode.
    /// Returns an outcome when the session should end.
    fn on_key(&mut self, key: KeyEvent) -> Option<Outcome> {
        self.notice = None;

        if self.input.is_some() {
            self.on_input_key(key);
            return None;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // Ctrl-C always quits, whatever is configured.
        if ctrl && key.code == KeyCode::Char('c') {
            return Some(Outcome::Cancelled);
        }

        // While the filter box is open, typed characters (digits and the letters
        // that are shortcuts elsewhere included) go into it.
        if self.filtering {
            self.on_filter_key(key);
            return None;
        }

        // The number keys are fixed: they are the favourites.
        if let KeyCode::Char(d @ '1'..='9') = key.code {
            if !ctrl && !alt {
                self.press_number(d);
                return None;
            }
        }

        let keymap = match self.mode {
            Mode::Nav => &self.cfg.keys,
            Mode::Favs => &self.cfg.fav_keys,
        };
        let action = keymap.lookup(&key)?;
        match self.mode {
            Mode::Nav => self.do_nav(action),
            Mode::Favs => self.do_favs(action),
        }
    }

    /// Keys while the filter box is open. Esc and Enter close it but keep the
    /// results on screen.
    fn on_filter_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.filtering = false,
            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.entries.len().saturating_sub(1),
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.apply_filter(None);
                }
            }
            KeyCode::Char('u') if ctrl => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.apply_filter(None);
                }
            }
            KeyCode::Char(c)
                if !ctrl && !c.is_control() && self.filter.chars().count() < FILTER_MAX_CHARS =>
            {
                self.filter.push(c);
                self.apply_filter(None);
            }
            _ => {}
        }
    }

    /// Run an action while navigating folders.
    fn do_nav(&mut self, action: Action) -> Option<Outcome> {
        match action {
            // First clears a filter left on the list; with none, it cancels.
            Action::Cancel => {
                if self.filter.is_empty() {
                    return Some(Outcome::Cancelled);
                }
                let focus = self.entries.get(self.cursor).map(|e| e.path.clone());
                self.filter.clear();
                self.apply_filter(focus.as_deref());
            }
            Action::Filter => self.filtering = true,
            Action::MoveUp => self.move_by(-1),
            Action::MoveDown => self.move_by(1),
            Action::PageUp => self.move_by(-(self.page as isize)),
            Action::PageDown => self.move_by(self.page as isize),
            Action::Top => self.cursor = 0,
            Action::Bottom => self.cursor = self.entries.len().saturating_sub(1),
            Action::Open => self.descend(),
            Action::Parent => self.ascend(),
            Action::Select => return self.select(),
            Action::Bookmark => self.bookmark_highlighted(),
            Action::ToggleHidden => self.toggle_hidden(),
            Action::Home => self.go_home(),
            Action::ToggleFavorites => self.toggle_mode(),
            Action::PickUp | Action::Rename | Action::Remove | Action::Back => {}
        }
        None
    }

    /// Run an action while editing favourites; actions that do not apply are ignored.
    fn do_favs(&mut self, action: Action) -> Option<Outcome> {
        let holding = self.grabbed.is_some();
        match action {
            Action::Cancel => return Some(Outcome::Cancelled),

            // Undo a pick-up in progress, otherwise step back to navigation.
            Action::Back => match self.grabbed.take() {
                Some((before, slot)) => {
                    self.favs = before;
                    self.fav_cursor = slot;
                }
                None => self.leave_favs(),
            },

            Action::MoveUp => self.fav_step(-1),
            Action::MoveDown => self.fav_step(1),

            // Pick up the item under the cursor, or put it down.
            Action::PickUp => {
                if self.grabbed.take().is_none() {
                    self.grabbed = Some((self.favs.clone(), self.fav_cursor));
                }
            }

            Action::Open if !holding => {
                let idx = self.fav_cursor;
                self.jump_to_fav(idx);
                self.leave_favs();
            }
            Action::Remove if !holding => self.remove_fav(),
            Action::Rename if !holding => self.start_rename(),
            Action::ToggleFavorites => self.toggle_mode(),
            _ => {}
        }
        None
    }

    /// Keys inside the rename prompt: text, Backspace, Enter to confirm, Esc to cancel.
    /// A name that is a cdx command word is refused.
    fn on_input_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Char('c') if ctrl => self.input = None,
            KeyCode::Enter => {
                if let Some(input) = self.input.take() {
                    let alias = input.buf.trim().to_string();
                    let alias = if alias.is_empty() { None } else { Some(alias) };
                    if let Some(f) = self.favs.get_mut(input.target) {
                        let renamed = Favorite { alias, ..f.clone() };
                        if renamed.clashes_with_command() {
                            self.notice = Some(commands::clash_message(&renamed.label()));
                        } else {
                            f.alias = renamed.alias;
                            self.dirty = true;
                            self.persist();
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(i) = self.input.as_mut() {
                    i.buf.pop();
                }
            }
            KeyCode::Char(c) if !ctrl && !c.is_control() => {
                let max = self.cfg.ui.max_alias_len;
                if let Some(i) = self.input.as_mut() {
                    if i.buf.chars().count() < max {
                        i.buf.push(c);
                    }
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- drawing

    /// Width of the favourites panel: fits the longest name, within sane limits and a third of the window.
    fn fav_panel_width(&self, term_w: usize) -> usize {
        let longest = self
            .favs
            .iter()
            .map(|f| f.label().chars().count())
            .max()
            .unwrap_or(0);
        (longest + 6).clamp(18, 30).min(term_w / 3).max(12)
    }

    /// Redraw the whole screen: header, both panels, footer message and help line.
    fn draw(&mut self, out: &mut impl Write) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        // Some terminals (and CI pseudo-terminals) report 0x0, meaning "unknown".
        let (w, h) = (if w == 0 { 80 } else { w as usize }, if h == 0 { 24 } else { h as usize });
        self.term_w = w;
        let list_h = h.saturating_sub(4).max(1);
        self.page = list_h.saturating_sub(1).max(1);

        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + list_h {
            self.scroll = self.cursor + 1 - list_h;
        }
        if self.fav_cursor < self.fav_scroll {
            self.fav_scroll = self.fav_cursor;
        } else if self.fav_cursor >= self.fav_scroll + list_h {
            self.fav_scroll = self.fav_cursor + 1 - list_h;
        }

        let show_favs = w >= self.cfg.ui.min_width_for_favorites;
        let lw = if show_favs { self.fav_panel_width(w) } else { 0 };
        let rx = if show_favs { lw + 3 } else { 0 };
        let rw = w.saturating_sub(rx).max(1);
        let in_favs = self.mode == Mode::Favs;

        for row in 0..h {
            queue!(
                out,
                cursor::MoveTo(0, row as u16),
                terminal::Clear(ClearType::CurrentLine)
            )?;
        }

        // ---- left: favourites
        if show_favs {
            let title_look = if in_favs { Look::Title } else { Look::Dim };
            put(out, 0, 0, &fit("Favourites", lw), title_look)?;
            put(
                out,
                0,
                1,
                &fit(&format!("{}/{}", self.favs.len(), favorites::MAX), lw),
                Look::Dim,
            )?;

            if self.favs.is_empty() {
                put(out, 0, 2, &fit("(none yet)", lw), Look::Dim)?;
                put(out, 0, 3, &fit("press b to add", lw), Look::Dim)?;
            }

            let here = self.current_fav_index();
            for i in 0..list_h {
                let idx = self.fav_scroll + i;
                let Some(fav) = self.favs.get(idx) else {
                    break;
                };
                let is_cursor = in_favs && idx == self.fav_cursor;
                let is_here = here == Some(idx);
                let holding = is_cursor && self.grabbed.is_some();

                let marker = if holding {
                    '↕'
                } else if is_cursor {
                    '▸'
                } else if is_here {
                    '●'
                } else {
                    ' '
                };
                let gone = if fav.missing { " ✗" } else { "" };
                let text = fit(&format!("{marker} {} {}{gone}", idx + 1, fav.label()), lw);
                let look = if holding {
                    Look::Grab
                } else if is_cursor {
                    Look::Cursor
                } else if fav.missing {
                    Look::Warn
                } else if is_here {
                    Look::Accent
                } else {
                    Look::Plain
                };
                put(out, 0, i + 2, &text, look)?;
            }

            for row in 0..(list_h + 2) {
                put(out, lw + 1, row, "│", Look::Dim)?;
            }
        }

        // ---- right: navigation
        let (title, count, total) = match &self.loc {
            Location::Dir(p) => (
                p.display().to_string(),
                self.entries.len().saturating_sub(1),
                self.all.len().saturating_sub(1),
            ),
            Location::Drives => ("Drives".to_string(), self.entries.len(), self.all.len()),
        };
        let hidden = if self.show_hidden { "shown" } else { "off" };
        let sub = if self.filter.is_empty() {
            format!(
                "{count} item{}  ·  hidden: {hidden}",
                if count == 1 { "" } else { "s" }
            )
        } else {
            format!("{count} of {total}  ·  hidden: {hidden}")
        };
        put(
            out,
            rx,
            0,
            &fit_left(&title, rw),
            if in_favs { Look::Dim } else { Look::Title },
        )?;
        put(out, rx, 1, &fit(&sub, rw), Look::Dim)?;

        for row in 0..list_h {
            let idx = self.scroll + row;
            let Some(entry) = self.entries.get(idx) else {
                break;
            };
            let label = match entry.kind {
                Kind::Here => ".  (this folder)".to_string(),
                Kind::Folder => format!("{}/", entry.label),
                Kind::Drive => entry.label.clone(),
            };
            let selected = idx == self.cursor;
            let line = fit(&format!("{} {}", if selected { "▸" } else { " " }, label), rw);
            let look = if selected && !in_favs {
                Look::Cursor
            } else if selected || entry.kind == Kind::Here || in_favs {
                Look::Dim
            } else {
                Look::Plain
            };
            put(out, rx, row + 2, &line, look)?;
        }
        if !self.filter.is_empty() && count == 0 {
            put(out, rx, 3, &fit("  (no matches)", rw), Look::Dim)?;
        }

        // ---- footer
        if let Some(input) = &self.input {
            let prompt = format!(
                "alias for {} (empty = folder name): {}_",
                input.target + 1,
                input.buf
            );
            put(out, 0, h.saturating_sub(2), &fit_left(&prompt, w), Look::Accent)?;
        } else if self.filtering {
            let prompt = format!("/{}_", self.filter);
            put(out, 0, h.saturating_sub(2), &fit_left(&prompt, w), Look::Accent)?;
        } else if let Some(n) = &self.notice {
            put(out, 0, h.saturating_sub(2), &fit(n, w), Look::Notice)?;
        } else if !self.filter.is_empty() {
            let line = format!("filter: {}   (/ edit   esc clear)", self.filter);
            put(out, 0, h.saturating_sub(2), &fit(&line, w), Look::Accent)?;
        }

        let help: String = match (self.input.is_some(), self.mode, self.grabbed.is_some()) {
            (true, _, _) => "enter save   esc cancel   (empty = use the folder's name)".into(),
            (_, Mode::Nav, _) if self.filtering => {
                "type to filter   ↑↓ move   enter/esc done (results stay)   ctrl-u clear".into()
            }
            (_, Mode::Nav, _) => format!(
                "{}{} move   {} select   {} filter   {} bookmark   1-9 jump   {} favs   {} hidden   {} cancel",
                self.nav_key(Action::MoveUp),
                self.nav_key(Action::MoveDown),
                self.nav_key(Action::Select),
                self.nav_key(Action::Filter),
                self.nav_key(Action::Bookmark),
                self.nav_key(Action::ToggleFavorites),
                self.nav_key(Action::ToggleHidden),
                self.nav_key(Action::Cancel),
            ),
            (_, Mode::Favs, false) => format!(
                "{}{} move   {} pick up   {} rename   {} remove   {} open   {} done   {} back",
                self.fav_key(Action::MoveUp),
                self.fav_key(Action::MoveDown),
                self.fav_key(Action::PickUp),
                self.fav_key(Action::Rename),
                self.fav_key(Action::Remove),
                self.fav_key(Action::Open),
                self.fav_key(Action::ToggleFavorites),
                self.fav_key(Action::Back),
            ),
            (_, Mode::Favs, true) => format!(
                "{}{} move item   1-9 send to slot   {} put down   {} undo   {} save & done",
                self.fav_key(Action::MoveUp),
                self.fav_key(Action::MoveDown),
                self.fav_key(Action::PickUp),
                self.fav_key(Action::Back),
                self.fav_key(Action::ToggleFavorites),
            ),
        };
        put(out, 0, h.saturating_sub(1), &fit(&help, w), Look::Dim)?;

        out.flush()
    }
}

/// How a piece of text is styled when printed with `put`.
#[derive(Clone, Copy)]
enum Look {
    /// Terminal defaults.
    Plain,
    /// Faded: help line, hints.
    Dim,
    /// Bold cyan: the current folder.
    Title,
    /// Cyan: the filter and rename prompts, the favourites column.
    Accent,
    /// Bold green: the highlighted row.
    Cursor,
    /// Bold yellow: a favourite that is picked up.
    Grab,
    /// Red: something that no longer exists.
    Warn,
    /// Yellow: one-line messages.
    Notice,
}

/// Print `text` at (col, row) in the given look.
fn put(out: &mut impl Write, col: usize, row: usize, text: &str, look: Look) -> io::Result<()> {
    queue!(out, cursor::MoveTo(col as u16, row as u16))?;
    match look {
        Look::Plain => {
            queue!(out, Print(text))?;
        }
        Look::Dim => {
            queue!(
                out,
                SetAttribute(Attribute::Dim),
                Print(text),
                SetAttribute(Attribute::Reset)
            )?;
        }
        Look::Title => {
            queue!(
                out,
                SetForegroundColor(Color::Cyan),
                SetAttribute(Attribute::Bold),
                Print(text),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        }
        Look::Accent => {
            queue!(out, SetForegroundColor(Color::Cyan), Print(text), ResetColor)?;
        }
        Look::Cursor => {
            queue!(
                out,
                SetForegroundColor(Color::Green),
                SetAttribute(Attribute::Bold),
                Print(text),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        }
        Look::Grab => {
            queue!(
                out,
                SetForegroundColor(Color::Yellow),
                SetAttribute(Attribute::Bold),
                Print(text),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        }
        Look::Warn => {
            queue!(out, SetForegroundColor(Color::Red), Print(text), ResetColor)?;
        }
        Look::Notice => {
            queue!(out, SetForegroundColor(Color::Yellow), Print(text), ResetColor)?;
        }
    }
    Ok(())
}

/// Truncate to `width` characters, ending with an ellipsis.
fn fit(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n <= width {
        s.to_string()
    } else if width <= 1 {
        s.chars().take(width).collect()
    } else {
        let mut t: String = s.chars().take(width - 1).collect();
        t.push('…');
        t
    }
}

/// Truncate from the left, keeping the end of a long path visible.
fn fit_left(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n <= width {
        s.to_string()
    } else if width <= 1 {
        s.chars().skip(n - width).collect()
    } else {
        let tail: String = s.chars().skip(n - (width - 1)).collect();
        format!("…{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cdx-ui-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A browser rooted at `root`, with favourites on `names` (folders created
    /// under root) and a store file inside root.
    fn setup(tag: &str, names: &[&str]) -> (State, PathBuf, PathBuf) {
        setup_with(tag, names, Config::default())
    }

    fn setup_with(tag: &str, names: &[&str], cfg: Config) -> (State, PathBuf, PathBuf) {
        let root = scratch(tag);
        let mut favs = Vec::new();
        for n in names {
            let p = root.join(n);
            fs::create_dir_all(&p).unwrap();
            favs.push(Favorite::new(p));
        }
        let store = root.join("cfg").join("favorites.tsv");
        let state = State::new(root.clone(), favs, Some(store.clone()), cfg);
        (state, root, store)
    }

    fn press(s: &mut State, codes: &[KeyCode]) {
        for c in codes {
            s.on_key(KeyEvent::new(*c, KeyModifiers::NONE));
        }
    }

    fn here(s: &State) -> PathBuf {
        s.current_dir().expect("in a folder").to_path_buf()
    }

    fn order(s: &State) -> Vec<String> {
        s.favs.iter().map(|f| f.label()).collect()
    }

    fn on_disk(store: &Path) -> Vec<String> {
        favorites::load_from(store)
            .unwrap()
            .iter()
            .map(|f| f.label())
            .collect()
    }

    #[test]
    fn b_on_the_dot_row_bookmarks_the_current_folder_and_saves_it() {
        let (mut s, root, store) = setup("bm", &[]);
        press(&mut s, &[KeyCode::Char('b')]); // empty folder: the cursor is on "."
        assert_eq!(s.favs.len(), 1);
        assert_eq!(s.favs[0].path, root);
        assert_eq!(favorites::load_from(&store).unwrap().len(), 1);
    }

    fn labels(s: &State) -> Vec<String> {
        s.entries.iter().map(|e| e.label.clone()).collect()
    }

    fn type_text(s: &mut State, text: &str) {
        for c in text.chars() {
            press(s, &[KeyCode::Char(c)]);
        }
    }

    fn filter_setup(tag: &str) -> (State, PathBuf) {
        let (mut s, root, _store) = setup(tag, &[]);
        for n in ["alpha", "alpine", "beta", "Gamma"] {
            fs::create_dir(root.join(n)).unwrap();
        }
        s.reload(None);
        (s, root)
    }

    #[test]
    fn slash_filters_as_you_type() {
        let (mut s, _root) = filter_setup("flt1");
        assert_eq!(labels(&s), [".", "alpha", "alpine", "beta", "Gamma"]);
        press(&mut s, &[KeyCode::Char('/')]);
        assert!(s.filtering);
        type_text(&mut s, "al");
        assert_eq!(labels(&s), [".", "alpha", "alpine"]);
        assert_eq!(s.cursor, 1, "cursor lands on the first match");
        type_text(&mut s, "p");
        type_text(&mut s, "i");
        assert_eq!(labels(&s), [".", "alpine"]);
        press(&mut s, &[KeyCode::Backspace]);
        assert_eq!(labels(&s), [".", "alpha", "alpine"], "backspace widens it again");
    }

    #[test]
    fn the_filter_ignores_case() {
        let (mut s, _root) = filter_setup("flt2");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "GAM");
        assert_eq!(labels(&s), [".", "Gamma"]);
    }

    #[test]
    fn esc_closes_filter_mode_but_leaves_the_results_filtered() {
        let (mut s, _root) = filter_setup("flt3");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "al");
        let out = s.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(out.is_none(), "Esc in filter mode never cancels cdx");
        assert!(!s.filtering);
        assert_eq!(labels(&s), [".", "alpha", "alpine"]);
        // Normal keys work on the filtered list again.
        press(&mut s, &[KeyCode::Char('j')]);
        assert_eq!(s.entries[s.cursor].label, "alpine");
    }

    #[test]
    fn enter_also_closes_filter_mode_keeping_results() {
        let (mut s, _root) = filter_setup("flt4");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "be");
        press(&mut s, &[KeyCode::Enter]);
        assert!(!s.filtering);
        assert_eq!(labels(&s), [".", "beta"]);
    }

    #[test]
    fn esc_in_navigation_clears_a_left_over_filter_before_cancelling() {
        let (mut s, _root) = filter_setup("flt5");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "al");
        press(&mut s, &[KeyCode::Esc]); // close filter mode
        let out = s.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(out.is_none(), "this Esc only clears the filter");
        assert!(s.filter.is_empty());
        assert_eq!(labels(&s).len(), 5);
        assert_eq!(s.entries[s.cursor].label, "alpha", "cursor stays on the same folder");
        let out = s.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(out, Some(Outcome::Cancelled)));
    }

    #[test]
    fn shortcut_keys_are_just_text_while_filtering() {
        let (mut s, _root) = filter_setup("flt6");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "b1q.~j");
        assert!(s.filtering);
        assert!(s.favs.is_empty(), "b did not bookmark");
        assert_eq!(s.mode, Mode::Nav);
        assert_eq!(s.filter, "b1q.~j");
        assert!(!s.show_hidden);
        assert_eq!(labels(&s), ["."], "nothing matches, only the . row remains");
    }

    #[test]
    fn arrows_move_through_the_matches_while_typing() {
        let (mut s, _root) = filter_setup("flt7");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "al");
        press(&mut s, &[KeyCode::Down]);
        assert_eq!(s.entries[s.cursor].label, "alpine");
        assert!(s.filtering);
    }

    #[test]
    fn moving_to_another_folder_drops_the_filter() {
        let (mut s, root) = filter_setup("flt8");
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "al");
        press(&mut s, &[KeyCode::Enter, KeyCode::Enter]); // close box, open "alpha"
        assert_eq!(here(&s), root.join("alpha"));
        assert!(s.filter.is_empty());
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "zz");
        press(&mut s, &[KeyCode::Enter, KeyCode::Left]);
        assert_eq!(here(&s), root);
        assert_eq!(labels(&s).len(), 5, "going up shows the whole folder");
    }

    #[test]
    fn toggling_hidden_keeps_the_filter() {
        let (mut s, root) = filter_setup("flt9");
        fs::create_dir(root.join(".alhidden")).unwrap();
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "al");
        press(&mut s, &[KeyCode::Enter, KeyCode::Char('.')]);
        assert_eq!(labels(&s), [".", ".alhidden", "alpha", "alpine"]);
    }

    fn cfg_from(text: &str) -> Config {
        let (cfg, warns) = crate::config::parse(text);
        assert!(warns.is_empty(), "{warns:?}");
        cfg
    }

    #[test]
    fn a_folder_named_like_a_command_cannot_be_bookmarked() {
        let (mut s, root, _store) = setup("clash1", &[]);
        for name in ["config", "Init", "fine"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        s.reload(None); // sorted: ".", config, fine, Init
        press(&mut s, &[KeyCode::Char('b')]); // on "config"
        assert!(s.favs.is_empty());
        assert!(s.notice.as_deref().unwrap().contains("'config' is a cdx command"));
        press(&mut s, &[KeyCode::Char('j'), KeyCode::Char('b')]); // "fine"
        assert_eq!(s.favs.len(), 1);
        press(&mut s, &[KeyCode::Char('j'), KeyCode::Char('b')]); // "Init": case does not matter
        assert_eq!(s.favs.len(), 1);
        assert!(s.notice.as_deref().unwrap().contains("cdx command"));
    }

    #[test]
    fn a_favourite_cannot_be_renamed_to_a_command_word() {
        let (mut s, _root, _store) = setup("clash2", &["proj"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('r')]);
        type_text(&mut s, "Config");
        press(&mut s, &[KeyCode::Enter]);
        assert!(s.input.is_none(), "the rename box closes");
        assert_eq!(s.favs[0].alias, None, "alias was not applied");
        assert!(s.notice.as_deref().unwrap().contains("cdx command"));
        // An ordinary alias still works.
        press(&mut s, &[KeyCode::Char('r')]);
        type_text(&mut s, "work");
        press(&mut s, &[KeyCode::Enter]);
        assert_eq!(s.favs[0].alias.as_deref(), Some("work"));
    }

    #[test]
    fn clearing_an_alias_cannot_expose_a_command_named_folder() {
        let (mut s, root, _store) = setup("clash3", &[]);
        // Hand-made favourite: a folder called "init", aliased to something harmless.
        let dir = root.join("init");
        fs::create_dir(&dir).unwrap();
        s.favs.push(Favorite { alias: Some("ok".into()), ..Favorite::new(dir) });
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('r'), KeyCode::Backspace, KeyCode::Backspace, KeyCode::Enter]); // empty alias
        assert_eq!(s.favs[0].alias.as_deref(), Some("ok"), "refused: the folder name is reserved");
        assert!(s.notice.is_some());
    }

    #[test]
    fn rebinding_a_key_changes_what_it_does() {
        let cfg = cfg_from("[keys]\nbookmark = B\nfilter = ctrl-f\nmove_down = s\nmove_up = w\n");
        let (mut s, root, _store) = setup_with("rebind", &[], cfg);
        fs::create_dir(root.join("one")).unwrap();
        fs::create_dir(root.join("two")).unwrap();
        s.reload(None);
        press(&mut s, &[KeyCode::Char('s')]);
        assert_eq!(s.entries[s.cursor].label, "two");
        press(&mut s, &[KeyCode::Char('w')]);
        assert_eq!(s.entries[s.cursor].label, "one");
        // The old bookmark key is now just an unbound key.
        press(&mut s, &[KeyCode::Char('b')]);
        assert!(s.favs.is_empty());
        s.on_key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT));
        assert_eq!(s.favs[0].path, root.join("one"));
        // And the filter now opens on Ctrl-F, not on /.
        press(&mut s, &[KeyCode::Char('/')]);
        assert!(!s.filtering);
        s.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(s.filtering);
    }

    #[test]
    fn rebound_favourites_keys_work_in_favourites_mode() {
        let cfg = cfg_from("[keys.favorites]\nremove = d\nrename = n\n");
        let (mut s, _root, _store) = setup_with("favrebind", &["a", "b"], cfg);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('x')]);
        assert_eq!(s.favs.len(), 2, "x no longer removes");
        press(&mut s, &[KeyCode::Char('d')]);
        assert_eq!(s.favs.len(), 1);
        press(&mut s, &[KeyCode::Char('n')]);
        assert!(s.input.is_some(), "n starts a rename");
    }

    #[test]
    fn the_number_keys_and_ctrl_c_stay_fixed() {
        let (mut s, _root, _store) = setup("fixed", &["a"]);
        let out = s.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(out, Some(Outcome::Cancelled)));
        press(&mut s, &[KeyCode::Char('1')]);
        assert_eq!(here(&s).file_name().unwrap(), "a");
    }

    #[test]
    fn unbinding_every_cancel_key_still_leaves_ctrl_c() {
        let cfg = cfg_from("[keys]\ncancel =\n");
        let (mut s, _root, _store) = setup_with("nocancel", &[], cfg);
        assert!(s.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).is_none());
        assert!(s.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)).is_none());
        let out = s.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(out, Some(Outcome::Cancelled)));
    }

    #[test]
    fn messages_name_the_keys_you_configured() {
        let cfg = cfg_from("[keys]\nbookmark = B\n");
        let (mut s, _root, _store) = setup_with("msgkeys", &[], cfg);
        press(&mut s, &[KeyCode::Char('3')]);
        let msg = s.notice.clone().unwrap();
        assert!(msg.contains("Press B to bookmark"), "{msg}");
    }

    #[test]
    fn the_filter_can_be_made_case_sensitive() {
        let cfg = cfg_from("[ui]\nfilter_case_sensitive = true\n");
        let (mut s, root, _store) = setup_with("fcase", &[], cfg);
        fs::create_dir(root.join("Gamma")).unwrap();
        fs::create_dir(root.join("gamut")).unwrap();
        s.reload(None);
        press(&mut s, &[KeyCode::Char('/')]);
        type_text(&mut s, "Gam");
        assert_eq!(labels(&s), [".", "Gamma"]);
    }

    #[test]
    fn show_hidden_in_the_config_applies_from_the_start() {
        let cfg = cfg_from("[ui]\nshow_hidden = true\n");
        let (mut s, root, _store) = setup_with("hid", &[], cfg);
        fs::create_dir(root.join(".secret")).unwrap();
        s.reload(None);
        assert!(labels(&s).contains(&".secret".to_string()));
    }

    #[test]
    fn the_alias_length_limit_is_configurable() {
        let cfg = cfg_from("[ui]\nmax_alias_len = 8\n");
        let (mut s, _root, _store) = setup_with("alen", &["proj"], cfg);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('r')]);
        type_text(&mut s, "abcdefghijkl");
        assert_eq!(s.input.as_ref().unwrap().buf, "abcdefgh");
    }

    #[test]
    fn the_favourites_panel_width_threshold_is_configurable() {
        let cfg = cfg_from("[ui]\nmin_width_for_favorites = 200\n");
        let (mut s, _root, _store) = setup_with("minw", &["a"], cfg);
        s.term_w = 120;
        press(&mut s, &[KeyCode::Tab]);
        assert_eq!(s.mode, Mode::Nav);
        assert!(s.notice.as_deref().unwrap().contains("needs 200 columns"));
    }

    #[test]
    fn b_bookmarks_the_highlighted_folder_not_the_one_we_are_in() {
        let (mut s, root, store) = setup("bmhl", &[]);
        fs::create_dir(root.join("alpha")).unwrap();
        fs::create_dir(root.join("beta")).unwrap();
        s.reload(None); // lands on the first real folder, "alpha"
        press(&mut s, &[KeyCode::Down, KeyCode::Char('b')]);
        assert_eq!(s.favs.len(), 1);
        assert_eq!(s.favs[0].path, root.join("beta"));
        assert_eq!(here(&s), root, "we did not navigate");
        assert_eq!(on_disk(&store), ["beta"]);
    }

    #[test]
    fn bookmarking_twice_is_refused() {
        let (mut s, _root, _store) = setup("dup", &[]);
        press(&mut s, &[KeyCode::Char('b'), KeyCode::Char('b')]);
        assert_eq!(s.favs.len(), 1);
        assert!(s.notice.as_deref().unwrap().contains("already"));
    }

    #[test]
    fn nine_is_the_limit() {
        let names: Vec<String> = (0..9).map(|i| format!("f{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let (mut s, _root, _store) = setup("full", &refs);
        press(&mut s, &[KeyCode::Char('g'), KeyCode::Char('b')]); // "." is not a favourite yet
        assert_eq!(s.favs.len(), 9);
        assert!(s.notice.as_deref().unwrap().contains("full"));
    }

    #[test]
    fn number_keys_navigate_to_the_favourite() {
        let (mut s, root, _store) = setup("jump", &["alpha", "beta", "gamma"]);
        press(&mut s, &[KeyCode::Char('2')]);
        assert_eq!(here(&s), root.join("beta"));
        press(&mut s, &[KeyCode::Char('3')]);
        assert_eq!(here(&s), root.join("gamma"));
        press(&mut s, &[KeyCode::Char('1')]);
        assert_eq!(here(&s), root.join("alpha"));
    }

    #[test]
    fn empty_slot_explains_itself_and_stays_put() {
        let (mut s, root, _store) = setup("empty", &["alpha"]);
        press(&mut s, &[KeyCode::Char('5')]);
        assert_eq!(here(&s), root);
        assert!(s.notice.as_deref().unwrap().contains("slot 5"));
    }

    #[test]
    fn tab_pick_up_move_put_down_tab_reorders_and_saves() {
        let (mut s, _root, store) = setup("reorder", &["a", "b", "c"]);
        press(&mut s, &[KeyCode::Tab]);
        assert_eq!(s.mode, Mode::Favs);
        press(&mut s, &[KeyCode::Char(' '), KeyCode::Down]); // pick up "a", move it down
        assert_eq!(order(&s), ["b", "a", "c"]);
        assert!(!store.exists(), "nothing is written until we leave favourites mode");
        press(&mut s, &[KeyCode::Char(' '), KeyCode::Tab]); // put down, finish
        assert_eq!(s.mode, Mode::Nav);
        assert_eq!(on_disk(&store), ["b", "a", "c"]);
    }

    #[test]
    fn tab_while_holding_keeps_the_arrangement() {
        let (mut s, _root, store) = setup("tabhold", &["a", "b", "c"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Down, KeyCode::Char(' '), KeyCode::Down]);
        assert_eq!(order(&s), ["a", "c", "b"]);
        press(&mut s, &[KeyCode::Tab]);
        assert_eq!(on_disk(&store), ["a", "c", "b"]);
    }

    #[test]
    fn esc_undoes_a_pick_up_then_leaves_the_mode() {
        let (mut s, _root, _store) = setup("undo", &["a", "b", "c"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char(' '), KeyCode::Down, KeyCode::Down]);
        assert_eq!(order(&s), ["b", "c", "a"]);
        press(&mut s, &[KeyCode::Esc]);
        assert_eq!(order(&s), ["a", "b", "c"]);
        assert_eq!(s.fav_cursor, 0);
        assert_eq!(s.mode, Mode::Favs, "first Esc only undoes");
        press(&mut s, &[KeyCode::Esc]);
        assert_eq!(s.mode, Mode::Nav);
    }

    #[test]
    fn number_while_holding_sends_the_item_to_that_slot() {
        let (mut s, _root, store) = setup("slot", &["a", "b", "c"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char(' '), KeyCode::Char('3'), KeyCode::Tab]);
        assert_eq!(on_disk(&store), ["b", "c", "a"]);
    }

    #[test]
    fn number_in_favourites_mode_moves_the_cursor_only() {
        let (mut s, root, _store) = setup("cursoronly", &["a", "b", "c"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('3')]);
        assert_eq!(s.fav_cursor, 2);
        assert_eq!(here(&s), root, "no navigation happened");
    }

    #[test]
    fn rename_sets_an_alias_and_empty_clears_it() {
        let (mut s, _root, store) = setup("alias", &["projects"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('r')]);
        type_text(&mut s, "Work");
        press(&mut s, &[KeyCode::Enter]);
        assert_eq!(order(&s), ["Work"]);
        assert_eq!(on_disk(&store), ["Work"]);

        press(&mut s, &[KeyCode::Char('r')]);
        for _ in 0..4 {
            press(&mut s, &[KeyCode::Backspace]);
        }
        press(&mut s, &[KeyCode::Enter]);
        assert_eq!(order(&s), ["projects"], "default name is back");
        assert_eq!(on_disk(&store), ["projects"]);
    }

    #[test]
    fn typing_in_the_prompt_does_not_trigger_shortcuts() {
        let (mut s, _root, _store) = setup("typing", &["a", "b"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('r')]);
        type_text(&mut s, "q1x b");
        assert!(s.input.is_some());
        assert_eq!(s.favs.len(), 2);
        press(&mut s, &[KeyCode::Esc]);
        assert!(s.input.is_none());
        assert_eq!(order(&s), ["a", "b"], "Esc cancels the rename");
    }

    #[test]
    fn x_removes_and_an_empty_list_returns_to_navigation() {
        let (mut s, _root, store) = setup("remove", &["a", "b"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Char('x')]);
        assert_eq!(order(&s), ["b"]);
        assert_eq!(on_disk(&store), ["b"]);
        press(&mut s, &[KeyCode::Char('x')]);
        assert!(s.favs.is_empty());
        assert_eq!(s.mode, Mode::Nav);
    }

    #[test]
    fn enter_in_favourites_mode_opens_it_and_returns_to_navigation() {
        let (mut s, root, _store) = setup("open", &["a", "b"]);
        press(&mut s, &[KeyCode::Tab, KeyCode::Down, KeyCode::Enter]);
        assert_eq!(s.mode, Mode::Nav);
        assert_eq!(here(&s), root.join("b"));
    }

    #[test]
    fn tab_with_no_favourites_explains_instead_of_switching() {
        let (mut s, _root, _store) = setup("nofavs", &[]);
        press(&mut s, &[KeyCode::Tab]);
        assert_eq!(s.mode, Mode::Nav);
        assert!(s.notice.as_deref().unwrap().contains("no favourites"));
    }

    #[test]
    fn a_deleted_favourite_is_reported_not_followed() {
        let (mut s, root, _store) = setup("gone", &["a"]);
        fs::remove_dir_all(root.join("a")).unwrap();
        press(&mut s, &[KeyCode::Char('1')]);
        assert_eq!(here(&s), root);
        assert!(s.favs[0].missing);
        assert!(s.notice.as_deref().unwrap().contains("no longer exists"));
    }

    #[test]
    fn tab_starts_on_the_favourite_you_are_standing_in() {
        let (mut s, _root, _store) = setup("startat", &["a", "b", "c"]);
        press(&mut s, &[KeyCode::Char('3'), KeyCode::Tab]);
        assert_eq!(s.fav_cursor, 2);
    }

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("hello", 10), "hello");
        assert_eq!(fit("hello world", 6), "hello…");
    }

    #[test]
    fn fit_left_keeps_the_tail() {
        assert_eq!(fit_left("/a/very/long/path", 7), "…g/path");
        assert_eq!(fit_left("short", 10), "short");
    }
}
