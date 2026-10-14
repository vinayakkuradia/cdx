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

use crate::favorites::{self, Favorite};
use crate::fsutil;

/// Below this many columns the favourites panel is hidden (the 1-9 keys still work).
const MIN_WIDTH_FOR_FAVS: usize = 48;
const ALIAS_MAX_CHARS: usize = 40;

pub enum Outcome {
    Selected(PathBuf),
    Cancelled,
}

/// Where the browser currently is.
enum Location {
    Dir(PathBuf),
    /// Windows only: the list of drive letters above `C:\`.
    Drives,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// The "." row: selecting it picks the folder you are standing in.
    Here,
    Folder,
    Drive,
}

#[derive(Clone)]
struct Entry {
    label: String,
    path: PathBuf,
    kind: Kind,
}

/// Which panel owns the cursor.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Nav,
    Favs,
}

/// A one-line text prompt (used to rename a favourite).
struct Input {
    buf: String,
    target: usize,
}

struct State {
    // navigation
    loc: Location,
    show_hidden: bool,
    /// Everything in the folder; `entries` is the part that matches `filter`.
    all: Vec<Entry>,
    entries: Vec<Entry>,
    cursor: usize,
    scroll: usize,
    page: usize,
    /// Text typed after `/`. Stays applied after filter mode is closed.
    filter: String,
    /// The filter box has the keyboard.
    filtering: bool,
    // favourites
    favs: Vec<Favorite>,
    store: Option<PathBuf>,
    mode: Mode,
    fav_cursor: usize,
    fav_scroll: usize,
    /// While an item is "picked up": the order before the pick-up, and its slot.
    grabbed: Option<(Vec<Favorite>, usize)>,
    /// The order was changed and has not been written to disk yet.
    dirty: bool,
    input: Option<Input>,
    // misc
    notice: Option<String>,
    term_w: usize,
}

/// Longest filter we accept; more than a name could ever need.
const FILTER_MAX_CHARS: usize = 64;

/// Restores the terminal no matter how we leave (return, error, or panic).
struct TermGuard;

impl TermGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = TermGuard; // from here on, Drop undoes raw mode
        execute!(io::stderr(), EnterAlternateScreen, cursor::Hide)?;
        Ok(guard)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub fn run(start: PathBuf, show_hidden: bool) -> io::Result<Outcome> {
    let (favs, warning) = match favorites::load() {
        Ok(f) => (f, None),
        Err(e) => (Vec::new(), Some(format!("could not read favourites: {e}"))),
    };
    let mut state = State::new(start, show_hidden, favs, favorites::file_path());
    if warning.is_some() {
        state.notice = warning;
    }

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
    fn new(start: PathBuf, show_hidden: bool, favs: Vec<Favorite>, store: Option<PathBuf>) -> Self {
        let mut state = State {
            loc: Location::Dir(start),
            show_hidden,
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
        let needle = self.filter.to_lowercase();
        let entries: Vec<Entry> = self
            .all
            .iter()
            .filter(|e| {
                e.kind == Kind::Here || needle.is_empty() || e.label.to_lowercase().contains(&needle)
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

    fn move_by(&mut self, delta: isize) {
        let last = self.entries.len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    fn select(&self) -> Option<Outcome> {
        self.entries
            .get(self.cursor)
            .map(|e| Outcome::Selected(e.path.clone()))
    }

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

    fn toggle_hidden(&mut self) {
        let focus = self.entries.get(self.cursor).map(|e| e.path.clone());
        self.show_hidden = !self.show_hidden;
        self.reload(focus.as_deref());
    }

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
                "favourites are full ({}). Press Tab, then x to remove one",
                favorites::MAX
            ));
            return;
        }
        if !favorites::is_storable(&path) {
            self.notice = Some("this path cannot be stored as a favourite".into());
            return;
        }
        self.favs.push(Favorite::new(path));
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
                "slot {} is empty. Press b to bookmark the highlighted folder",
                idx + 1
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
                    format!("'{label}' no longer exists (press Tab, then x to remove it)")
                } else {
                    format!("cannot open '{label}': {e}")
                });
            }
        }
    }

    fn toggle_mode(&mut self) {
        match self.mode {
            Mode::Nav => {
                if self.term_w < MIN_WIDTH_FOR_FAVS {
                    self.notice = Some(format!(
                        "window too narrow for favourites (needs {MIN_WIDTH_FOR_FAVS} columns)"
                    ));
                } else if self.favs.is_empty() {
                    self.notice = Some("no favourites yet. Press b to bookmark the highlighted folder".into());
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

    fn fav_step(&mut self, delta: isize) {
        let last = self.favs.len().saturating_sub(1) as isize;
        let to = (self.fav_cursor as isize + delta).clamp(0, last) as usize;
        if self.grabbed.is_some() && to != self.fav_cursor {
            self.favs.swap(self.fav_cursor, to);
            self.dirty = true;
        }
        self.fav_cursor = to;
    }

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

    fn start_rename(&mut self) {
        if let Some(f) = self.favs.get(self.fav_cursor) {
            self.input = Some(Input {
                buf: f.alias.clone().unwrap_or_default(),
                target: self.fav_cursor,
            });
        }
    }

    // ------------------------------------------------------------------- keys

    fn on_key(&mut self, key: KeyEvent) -> Option<Outcome> {
        self.notice = None;

        if self.input.is_some() {
            self.on_input_key(key);
            return None;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // While the filter box is open, typed characters (digits and `b` included)
        // go into it. Ctrl-C still quits.
        if self.filtering {
            if ctrl && key.code == KeyCode::Char('c') {
                return Some(Outcome::Cancelled);
            }
            self.on_filter_key(key);
            return None;
        }

        match key.code {
            KeyCode::Char('c') if ctrl => return Some(Outcome::Cancelled),
            KeyCode::Tab | KeyCode::BackTab => {
                self.toggle_mode();
                return None;
            }
            KeyCode::Char(d @ '1'..='9') => {
                self.press_number(d);
                return None;
            }
            _ => {}
        }

        match self.mode {
            Mode::Nav => self.on_key_nav(key),
            Mode::Favs => self.on_key_favs(key),
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

    fn on_key_nav(&mut self, key: KeyEvent) -> Option<Outcome> {
        match key.code {
            KeyCode::Char('q') => return Some(Outcome::Cancelled),
            // Esc first clears a filter left on the list; with none, it cancels.
            KeyCode::Esc => {
                if self.filter.is_empty() {
                    return Some(Outcome::Cancelled);
                }
                let focus = self.entries.get(self.cursor).map(|e| e.path.clone());
                self.filter.clear();
                self.apply_filter(focus.as_deref());
            }
            KeyCode::Char('/') => self.filtering = true,

            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => self.cursor = self.entries.len().saturating_sub(1),

            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.descend(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => self.ascend(),

            KeyCode::Char(' ') => return self.select(),

            KeyCode::Char('b') => self.bookmark_highlighted(),
            KeyCode::Char('.') => self.toggle_hidden(),
            KeyCode::Char('~') => self.go_home(),
            _ => {}
        }
        None
    }

    fn on_key_favs(&mut self, key: KeyEvent) -> Option<Outcome> {
        let holding = self.grabbed.is_some();
        match key.code {
            KeyCode::Char('q') => return Some(Outcome::Cancelled),

            // Esc: undo a pick-up in progress, otherwise step back to navigation.
            KeyCode::Esc => match self.grabbed.take() {
                Some((before, slot)) => {
                    self.favs = before;
                    self.fav_cursor = slot;
                }
                None => self.leave_favs(),
            },

            KeyCode::Up | KeyCode::Char('k') => self.fav_step(-1),
            KeyCode::Down | KeyCode::Char('j') => self.fav_step(1),

            // Space: pick up the item under the cursor, or put it down.
            KeyCode::Char(' ') => {
                if self.grabbed.take().is_none() {
                    self.grabbed = Some((self.favs.clone(), self.fav_cursor));
                }
            }

            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if !holding => {
                let idx = self.fav_cursor;
                self.jump_to_fav(idx);
                self.leave_favs();
            }
            KeyCode::Char('x') | KeyCode::Delete if !holding => self.remove_fav(),
            KeyCode::Char('r') if !holding => self.start_rename(),
            _ => {}
        }
        None
    }

    fn on_input_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Char('c') if ctrl => self.input = None,
            KeyCode::Enter => {
                if let Some(input) = self.input.take() {
                    let alias = input.buf.trim().to_string();
                    if let Some(f) = self.favs.get_mut(input.target) {
                        f.alias = if alias.is_empty() { None } else { Some(alias) };
                        self.dirty = true;
                        self.persist();
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(i) = self.input.as_mut() {
                    i.buf.pop();
                }
            }
            KeyCode::Char(c) if !ctrl && !c.is_control() => {
                if let Some(i) = self.input.as_mut() {
                    if i.buf.chars().count() < ALIAS_MAX_CHARS {
                        i.buf.push(c);
                    }
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- drawing

    fn fav_panel_width(&self, term_w: usize) -> usize {
        let longest = self
            .favs
            .iter()
            .map(|f| f.label().chars().count())
            .max()
            .unwrap_or(0);
        (longest + 6).clamp(18, 30).min(term_w / 3).max(12)
    }

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

        let show_favs = w >= MIN_WIDTH_FOR_FAVS;
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

        let help = match (self.input.is_some(), self.mode, self.grabbed.is_some()) {
            (true, _, _) => "enter save   esc cancel   (empty = use the folder's name)",
            (_, Mode::Nav, _) if self.filtering => {
                "type to filter   ↑↓ move   enter/esc done (results stay)   ctrl-u clear"
            }
            (_, Mode::Nav, _) => {
                "↑↓←→ move   space select   / filter   b bookmark   1-9 jump   tab favs   . hidden   esc cancel"
            }
            (_, Mode::Favs, false) => {
                "↑↓ move   space pick up   r rename   x remove   enter open   tab done   esc back"
            }
            (_, Mode::Favs, true) => {
                "↑↓ move item   1-9 send to slot   space put down   esc undo   tab save & done"
            }
        };
        put(out, 0, h.saturating_sub(1), &fit(help, w), Look::Dim)?;

        out.flush()
    }
}

#[derive(Clone, Copy)]
enum Look {
    Plain,
    Dim,
    Title,
    Accent,
    Cursor,
    Grab,
    Warn,
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
        let root = scratch(tag);
        let mut favs = Vec::new();
        for n in names {
            let p = root.join(n);
            fs::create_dir_all(&p).unwrap();
            favs.push(Favorite::new(p));
        }
        let store = root.join("cfg").join("favorites.tsv");
        let state = State::new(root.clone(), false, favs, Some(store.clone()));
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
