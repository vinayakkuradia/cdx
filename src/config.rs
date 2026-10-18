//! User configuration: one place for every setting a person might want to change.
//!
//! * where cdx keeps its files (`~/.cdx`, or `$CDX_CONFIG_DIR`),
//! * every key binding,
//! * a few switches and limits (hidden folders, filter case, panel width, ...),
//! * the command used by `cdx -n`.
//!
//! The defaults live in this file. Users override them in `~/.cdx/config.ini`;
//! `cdx-bin config init` writes a fully commented starting point. A problem in
//! the file never stops cdx: it falls back to the default and reports a warning.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Name of the folder, inside your home folder, where cdx keeps its files.
pub const DIR_NAME: &str = ".cdx";
/// Name of the settings file inside that folder.
pub const CONFIG_FILE: &str = "config.ini";
/// Default name of the favourites file inside that folder.
pub const DEFAULT_FAVORITES_FILE: &str = "favorites.tsv";
/// Set this environment variable to keep cdx's files somewhere else.
pub const ENV_DIR: &str = "CDX_CONFIG_DIR";

// ======================================================================= paths

/// The value of environment variable `name`, treating an empty value as unset.
fn non_empty_env(name: &str) -> Option<std::ffi::OsString> {
    env::var_os(name).filter(|v| !v.is_empty())
}

/// Your home folder (`HOME`, or `USERPROFILE` on Windows).
fn home() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    non_empty_env(var).map(PathBuf::from)
}

/// `~/.cdx` on macOS/Linux, `%USERPROFILE%\.cdx` on Windows (or `$CDX_CONFIG_DIR`).
pub fn data_dir_from_env() -> Option<PathBuf> {
    if let Some(dir) = non_empty_env(ENV_DIR) {
        return Some(PathBuf::from(dir));
    }
    home().map(|h| h.join(DIR_NAME))
}

/// Where earlier versions of cdx kept the favourites file.
fn legacy_favorites_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        non_empty_env("APPDATA").map(|a| PathBuf::from(a).join("cdx").join(DEFAULT_FAVORITES_FILE))
    }
    #[cfg(not(windows))]
    {
        let base = non_empty_env("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".config")))?;
        Some(base.join("cdx").join(DEFAULT_FAVORITES_FILE))
    }
}

/// Copy `old` to `new`, once. Never overwrites, never deletes. `Ok(true)` if it copied.
pub fn migrate_favorites(old: &Path, new: &Path) -> io::Result<bool> {
    if new.exists() || !old.is_file() {
        return Ok(false);
    }
    if let Some(dir) = new.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(old, new)?;
    Ok(true)
}

// ===================================================================== actions

/// Everything a key can be bound to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    // browsing
    /// Move the cursor up one row.
    MoveUp,
    /// Move the cursor down one row.
    MoveDown,
    /// Move up one screenful.
    PageUp,
    /// Move down one screenful.
    PageDown,
    /// Jump to the first row.
    Top,
    /// Jump to the last row.
    Bottom,
    /// Open the highlighted folder (or, in favourites mode, go to the favourite).
    Open,
    /// Go up to the parent folder.
    Parent,
    /// Choose the highlighted folder and leave.
    Select,
    /// Leave without choosing; with a filter showing, clears the filter first.
    Cancel,
    /// Show or hide hidden folders.
    ToggleHidden,
    /// Jump to the home folder.
    Home,
    /// Bookmark the highlighted folder as a favourite.
    Bookmark,
    /// Open the filter box.
    Filter,
    /// Switch between browsing and editing favourites.
    ToggleFavorites,
    // favourites mode
    /// Pick up (or put down) a favourite, to reorder it.
    PickUp,
    /// Open the alias prompt for a favourite.
    Rename,
    /// Remove a favourite.
    Remove,
    /// Undo a pick-up, or leave favourites mode.
    Back,
}

impl Action {
    /// Every action, in the order they are listed in the documentation.
    pub const ALL: &'static [Action] = &[
        Action::MoveUp,
        Action::MoveDown,
        Action::PageUp,
        Action::PageDown,
        Action::Top,
        Action::Bottom,
        Action::Open,
        Action::Parent,
        Action::Select,
        Action::Cancel,
        Action::ToggleHidden,
        Action::Home,
        Action::Bookmark,
        Action::Filter,
        Action::ToggleFavorites,
        Action::PickUp,
        Action::Rename,
        Action::Remove,
        Action::Back,
    ];

    /// The name used in the config file.
    pub fn name(self) -> &'static str {
        match self {
            Action::MoveUp => "move_up",
            Action::MoveDown => "move_down",
            Action::PageUp => "page_up",
            Action::PageDown => "page_down",
            Action::Top => "top",
            Action::Bottom => "bottom",
            Action::Open => "open",
            Action::Parent => "parent",
            Action::Select => "select",
            Action::Cancel => "cancel",
            Action::ToggleHidden => "toggle_hidden",
            Action::Home => "home",
            Action::Bookmark => "bookmark",
            Action::Filter => "filter",
            Action::ToggleFavorites => "toggle_favorites",
            Action::PickUp => "pick_up",
            Action::Rename => "rename",
            Action::Remove => "remove",
            Action::Back => "back",
        }
    }

    /// The action called `s` in the config file (case-insensitive), if there is one.
    pub fn from_name(s: &str) -> Option<Action> {
        Action::ALL
            .iter()
            .copied()
            .find(|a| a.name().eq_ignore_ascii_case(s))
    }

    /// One line saying what the action does; written into the generated config file.
    pub fn description(self) -> &'static str {
        match self {
            Action::MoveUp => "move the cursor up",
            Action::MoveDown => "move the cursor down",
            Action::PageUp => "move up one page",
            Action::PageDown => "move down one page",
            Action::Top => "jump to the first item",
            Action::Bottom => "jump to the last item",
            Action::Open => "open the highlighted folder",
            Action::Parent => "go up one folder",
            Action::Select => "choose the highlighted folder (cd there)",
            Action::Cancel => "leave without changing folder (clears a filter first)",
            Action::ToggleHidden => "show or hide hidden folders",
            Action::Home => "jump to your home folder",
            Action::Bookmark => "bookmark the highlighted folder",
            Action::Filter => "start filtering by name",
            Action::ToggleFavorites => "switch between browsing and editing favourites",
            Action::PickUp => "pick up / put down a favourite to reorder it",
            Action::Rename => "give a favourite an alias",
            Action::Remove => "remove a favourite",
            Action::Back => "undo a pick-up, or leave favourites mode",
        }
    }
}

/// Actions that make sense while browsing folders.
pub const BROWSE_ACTIONS: &[Action] = &[
    Action::MoveUp,
    Action::MoveDown,
    Action::PageUp,
    Action::PageDown,
    Action::Top,
    Action::Bottom,
    Action::Open,
    Action::Parent,
    Action::Select,
    Action::Cancel,
    Action::ToggleHidden,
    Action::Home,
    Action::Bookmark,
    Action::Filter,
    Action::ToggleFavorites,
];

/// Actions that make sense while editing favourites.
pub const FAVORITES_ACTIONS: &[Action] = &[
    Action::MoveUp,
    Action::MoveDown,
    Action::PickUp,
    Action::Open,
    Action::Rename,
    Action::Remove,
    Action::Back,
    Action::Cancel,
    Action::ToggleFavorites,
];

// ====================================================================== keys

/// One key as written in the config file: a key plus the modifiers that must be held.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct KeySpec {
    /// Which key: a character or a named key such as Enter or Up.
    pub code: KeyCode,
    /// Ctrl must be held.
    pub ctrl: bool,
    /// Alt must be held.
    pub alt: bool,
    /// Shift must be held (ignored for characters, which already carry it).
    pub shift: bool,
}

impl KeySpec {
    /// The text form used in the config file: `ctrl-x`, `space`, `up`, `G`.
    pub fn name(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("ctrl-");
        }
        if self.alt {
            s.push_str("alt-");
        }
        if self.shift {
            s.push_str("shift-");
        }
        s.push_str(&match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Up => "up".into(),
            KeyCode::Down => "down".into(),
            KeyCode::Left => "left".into(),
            KeyCode::Right => "right".into(),
            KeyCode::Enter => "enter".into(),
            KeyCode::Esc => "esc".into(),
            KeyCode::Tab => "tab".into(),
            KeyCode::BackTab => "backtab".into(),
            KeyCode::Backspace => "backspace".into(),
            KeyCode::Delete => "delete".into(),
            KeyCode::Insert => "insert".into(),
            KeyCode::Home => "home".into(),
            KeyCode::End => "end".into(),
            KeyCode::PageUp => "pageup".into(),
            KeyCode::PageDown => "pagedown".into(),
            KeyCode::F(n) => format!("f{n}"),
            other => format!("{other:?}").to_lowercase(),
        });
        s
    }

    /// A friendlier form for the on-screen help line (arrows as symbols).
    pub fn pretty(&self) -> String {
        if !self.ctrl && !self.alt && !self.shift {
            match self.code {
                KeyCode::Up => return "↑".into(),
                KeyCode::Down => return "↓".into(),
                KeyCode::Left => return "←".into(),
                KeyCode::Right => return "→".into(),
                _ => {}
            }
        }
        self.name()
    }

    /// Keys cdx keeps for itself: the number keys (favourites) and Ctrl-C (quit).
    fn is_reserved(&self) -> bool {
        match self.code {
            KeyCode::Char('1'..='9') => !self.ctrl && !self.alt,
            KeyCode::Char('c') => self.ctrl && !self.alt,
            _ => false,
        }
    }
}

/// Parse `k`, `G`, `/`, `space`, `ctrl-x`, `alt-left`, `shift-up`, `f5`, ...
pub fn parse_key(text: &str) -> Result<KeySpec, String> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return Err("empty key".into());
    }
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    loop {
        let lower = rest.to_ascii_lowercase();
        if rest.len() > 5 && lower.starts_with("ctrl-") {
            ctrl = true;
            rest = &rest[5..];
        } else if rest.len() > 4 && lower.starts_with("alt-") {
            alt = true;
            rest = &rest[4..];
        } else if rest.len() > 6 && lower.starts_with("shift-") {
            shift = true;
            rest = &rest[6..];
        } else {
            break;
        }
    }
    let lower = rest.to_ascii_lowercase();
    let code = match lower.as_str() {
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "space" => KeyCode::Char(' '),
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            let n: u8 = f[1..].parse().unwrap_or(0);
            if (1..=12).contains(&n) {
                KeyCode::F(n)
            } else {
                return Err(format!("unknown key '{rest}'"));
            }
        }
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return Err(format!("unknown key '{rest}'")),
            }
        }
    };
    Ok(KeySpec {
        code,
        ctrl,
        alt,
        shift,
    })
}

/// Which keys trigger which actions, for one mode.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyMap {
    /// Each action with the keys bound to it; the order decides which action wins a conflict.
    entries: Vec<(Action, Vec<KeySpec>)>,
}

impl KeyMap {
    /// Build a map from `(action, "key key ...")` rows; used for the built-in defaults.
    fn from_table(table: &[(Action, &str)]) -> KeyMap {
        let entries = table
            .iter()
            .map(|(action, keys)| {
                let specs = keys
                    .split_whitespace()
                    .map(|k| parse_key(k).expect("built-in key names are valid"))
                    .collect();
                (*action, specs)
            })
            .collect();
        KeyMap { entries }
    }

    /// The keys bound to `action` (empty if none).
    pub fn keys(&self, action: Action) -> &[KeySpec] {
        self.entries
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, k)| k.as_slice())
            .unwrap_or(&[])
    }

    /// The actions this map covers, in order.
    pub fn actions(&self) -> impl Iterator<Item = Action> + '_ {
        self.entries.iter().map(|(a, _)| *a)
    }

    /// Replace the keys bound to `action`; ignored if the map does not cover that action.
    pub fn set(&mut self, action: Action, keys: Vec<KeySpec>) {
        if let Some(slot) = self.entries.iter_mut().find(|(a, _)| *a == action) {
            slot.1 = keys;
        }
    }

    /// The first key bound to `action`, as shown in the on-screen help.
    pub fn label(&self, action: Action) -> Option<String> {
        self.keys(action).first().map(KeySpec::pretty)
    }

    /// Which action does this key press trigger?
    ///
    /// For characters Shift is ignored (the character already carries it). For other
    /// keys an exact Shift match wins, but a binding without Shift also answers
    /// Shift+key, so Shift+Down still moves down.
    pub fn lookup(&self, ev: &KeyEvent) -> Option<Action> {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        let alt = ev.modifiers.contains(KeyModifiers::ALT);
        let shift = ev.modifiers.contains(KeyModifiers::SHIFT);

        for pass in 0..2 {
            for (action, specs) in &self.entries {
                for spec in specs {
                    if spec.ctrl != ctrl || spec.alt != alt || spec.code != ev.code {
                        continue;
                    }
                    let ok = match spec.code {
                        KeyCode::Char(_) => true,
                        _ if pass == 0 => spec.shift == shift,
                        _ => !spec.shift,
                    };
                    if ok {
                        return Some(*action);
                    }
                }
            }
        }
        None
    }

    /// Keys bound to more than one action (the earlier action wins).
    pub fn conflicts(&self) -> Vec<(KeySpec, Action, Action)> {
        let mut out = Vec::new();
        for (i, (a, specs)) in self.entries.iter().enumerate() {
            for spec in specs {
                for (b, others) in &self.entries[i + 1..] {
                    if others.contains(spec) {
                        out.push((*spec, *a, *b));
                    }
                }
            }
        }
        out
    }

    /// The built-in key bindings for browsing folders.
    pub fn browse_defaults() -> KeyMap {
        KeyMap::from_table(&[
            (Action::MoveUp, "up k"),
            (Action::MoveDown, "down j"),
            (Action::PageUp, "pageup"),
            (Action::PageDown, "pagedown"),
            (Action::Top, "home g"),
            (Action::Bottom, "end G"),
            (Action::Open, "right l enter"),
            (Action::Parent, "left h backspace"),
            (Action::Select, "space"),
            (Action::Cancel, "esc q"),
            (Action::ToggleHidden, "."),
            (Action::Home, "~"),
            (Action::Bookmark, "b"),
            (Action::Filter, "/"),
            (Action::ToggleFavorites, "tab backtab"),
        ])
    }

    /// The built-in key bindings for editing favourites.
    pub fn favorites_defaults() -> KeyMap {
        KeyMap::from_table(&[
            (Action::MoveUp, "up k"),
            (Action::MoveDown, "down j"),
            (Action::PickUp, "space"),
            (Action::Open, "enter right l"),
            (Action::Rename, "r"),
            (Action::Remove, "x delete"),
            (Action::Back, "esc"),
            (Action::Cancel, "q"),
            (Action::ToggleFavorites, "tab backtab"),
        ])
    }
}

// =================================================================== settings

/// Interface settings (`[ui]` in the config file).
#[derive(Clone, Debug, PartialEq)]
pub struct Ui {
    /// Show hidden folders from the start (the -h flag does the same).
    pub show_hidden: bool,
    /// Does the filter care about upper/lower case?
    pub filter_case_sensitive: bool,
    /// Narrower windows hide the favourites panel.
    pub min_width_for_favorites: usize,
    /// Longest alias you can type when renaming a favourite.
    pub max_alias_len: usize,
}

impl Default for Ui {
    /// The built-in interface settings.
    fn default() -> Self {
        Ui {
            show_hidden: false,
            filter_case_sensitive: false,
            min_width_for_favorites: 48,
            max_alias_len: 40,
        }
    }
}

/// Everything configurable: where files live, interface settings, the `-n` command and key
/// bindings. `Config::default()` is what you get without a config file.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Where cdx keeps its files. Not settable from the file itself.
    pub data_dir: Option<PathBuf>,
    /// Relative to `data_dir`, or absolute.
    pub favorites_file: PathBuf,
    /// Interface settings.
    pub ui: Ui,
    /// macOS: when a protected folder was refused earlier, ask the system again.
    pub macos_ask_again: bool,
    /// Template for `cdx -n`, e.g. `kitty --directory {path}`. None = built-in choice.
    pub new_window_command: Option<String>,
    /// Key bindings while browsing folders (`[keys]`).
    pub keys: KeyMap,
    /// Key bindings while editing favourites (`[keys.favorites]`).
    pub fav_keys: KeyMap,
}

impl Default for Config {
    /// The built-in configuration: every setting at its default.
    fn default() -> Self {
        Config {
            data_dir: None,
            favorites_file: PathBuf::from(DEFAULT_FAVORITES_FILE),
            ui: Ui::default(),
            macos_ask_again: true,
            new_window_command: None,
            keys: KeyMap::browse_defaults(),
            fav_keys: KeyMap::favorites_defaults(),
        }
    }
}

impl Config {
    /// Defaults, overlaid with `config.ini` if there is one. Problems in the file
    /// never stop cdx: they come back as warnings.
    pub fn load() -> (Config, Vec<String>) {
        let cfg = Config {
            data_dir: data_dir_from_env(),
            ..Config::default()
        };
        let Some(file) = cfg.config_path() else {
            return (cfg, Vec::new());
        };
        match fs::read_to_string(&file) {
            Ok(text) => parse_into(cfg, &text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (cfg, Vec::new()),
            Err(e) => {
                let msg = format!("could not read {}: {e}", file.display());
                (cfg, vec![msg])
            }
        }
    }

    /// Where the settings file lives (it may not exist); None if there is no data folder.
    pub fn config_path(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(CONFIG_FILE))
    }

    /// Where the favourites file lives; None if there is no data folder.
    pub fn favorites_path(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(&self.favorites_file))
    }

    /// First run after an upgrade from a version that kept favourites elsewhere: copy them over.
    /// Only when everything is at its default place.
    pub fn migrate_legacy_favorites(&self) -> io::Result<bool> {
        if non_empty_env(ENV_DIR).is_some() || self.favorites_file != Path::new(DEFAULT_FAVORITES_FILE) {
            return Ok(false);
        }
        match (legacy_favorites_path(), self.favorites_path()) {
            (Some(old), Some(new)) => migrate_favorites(&old, &new),
            _ => Ok(false),
        }
    }

    /// Write a commented default `config.ini`. An existing file is only replaced with
    /// `force`. Returns the path and whether it was written.
    pub fn write_default_file(force: bool) -> io::Result<(PathBuf, bool)> {
        let defaults = Config {
            data_dir: data_dir_from_env(),
            ..Config::default()
        };
        let Some(path) = defaults.config_path() else {
            return Err(io::Error::other("cannot find your home folder"));
        };
        if path.exists() && !force {
            return Ok((path, false));
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, defaults.to_ini())?;
        Ok((path, true))
    }

    /// The whole configuration as an ini file, with explanations.
    pub fn to_ini(&self) -> String {
        let b = |v: bool| if v { "true" } else { "false" };
        let mut s = String::new();
        s.push_str(
            "# cdx configuration.\n\
             # Lines starting with # or ; are comments. Delete a line to get its default back.\n\
             # Key names: single characters (k, G, /, .), or space enter esc tab backtab\n\
             # backspace delete insert home end pageup pagedown up down left right f1..f12,\n\
             # optionally with ctrl- alt- shift- prefixes (ctrl-x, alt-left).\n\
             # Several keys per action are separated by spaces. An empty value unbinds.\n\
             # Fixed, not configurable: the number keys 1-9 (favourites), Ctrl-C (quit),\n\
             # and the keys inside the filter box and the rename box.\n\n",
        );

        s.push_str("[paths]\n# Relative to the cdx folder (where this file lives), or absolute.\n");
        s.push_str(&format!("favorites_file = {}\n\n", self.favorites_file.display()));

        s.push_str("[ui]\n");
        s.push_str(&format!(
            "# Show hidden folders from the start (cdx -h does the same).\nshow_hidden = {}\n",
            b(self.ui.show_hidden)
        ));
        s.push_str(&format!(
            "# Should the filter tell upper and lower case apart?\nfilter_case_sensitive = {}\n",
            b(self.ui.filter_case_sensitive)
        ));
        s.push_str(&format!(
            "# Windows narrower than this many columns hide the favourites panel.\n\
             min_width_for_favorites = {}\n",
            self.ui.min_width_for_favorites
        ));
        s.push_str(&format!(
            "# Longest alias you can type when renaming a favourite.\nmax_alias_len = {}\n\n",
            self.ui.max_alias_len
        ));

        s.push_str("[permissions]\n");
        s.push_str(&format!(
            "# macOS only: when you earlier refused access to Desktop, Documents, Downloads or an\n\
             # external drive, ask the system again when you open it. Needs a normal terminal app.\n\
             macos_ask_again = {}\n\n",
            b(self.macos_ask_again)
        ));

        s.push_str(
            "[new_window]\n# Command for `cdx -n`. {path} is replaced by the folder, which is also the\n\
             # working directory. Quote arguments that contain spaces. Empty = built-in choice.\n",
        );
        match &self.new_window_command {
            Some(c) => s.push_str(&format!("command = {c}\n\n")),
            None => s.push_str("# command = kitty --directory {path}\n\n"),
        }

        s.push_str("[keys]\n# While browsing folders.\n");
        for action in self.keys.actions() {
            s.push_str(&format!("# {}\n", action.description()));
            s.push_str(&format!("{} = {}\n", action.name(), join_keys(self.keys.keys(action))));
        }
        s.push_str("\n[keys.favorites]\n# While editing favourites (after toggle_favorites).\n");
        for action in self.fav_keys.actions() {
            s.push_str(&format!("# {}\n", action.description()));
            s.push_str(&format!(
                "{} = {}\n",
                action.name(),
                join_keys(self.fav_keys.keys(action))
            ));
        }
        s
    }
}

/// Key names separated by spaces, as written after `=` in the config file.
fn join_keys(keys: &[KeySpec]) -> String {
    keys.iter().map(KeySpec::name).collect::<Vec<_>>().join(" ")
}

// ===================================================================== parsing

/// The section names a config file may contain.
const SECTIONS: &[&str] = &["paths", "ui", "permissions", "new_window", "keys", "keys.favorites"];

/// Parse a config file on top of the defaults.
#[cfg(test)]
pub fn parse(text: &str) -> (Config, Vec<String>) {
    parse_into(Config::default(), text)
}

/// Read `text` on top of `cfg`. Mistakes are skipped and reported in the returned warnings.
fn parse_into(mut cfg: Config, text: &str) -> (Config, Vec<String>) {
    let mut warns = Vec::new();
    let mut section = String::new();

    for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_ascii_lowercase();
            if !SECTIONS.contains(&section.as_str()) {
                warns.push(format!("config line {n}: unknown section [{section}]"));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warns.push(format!("config line {n}: expected 'name = value'"));
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        let known = match section.as_str() {
            "paths" => match key.as_str() {
                "favorites_file" if !value.is_empty() => {
                    cfg.favorites_file = PathBuf::from(value);
                    true
                }
                _ => false,
            },
            "ui" => {
                let u = &mut cfg.ui;
                match key.as_str() {
                    "show_hidden" => set_bool(&mut u.show_hidden, value, n, &mut warns),
                    "filter_case_sensitive" => {
                        set_bool(&mut u.filter_case_sensitive, value, n, &mut warns)
                    }
                    "min_width_for_favorites" => {
                        set_num(&mut u.min_width_for_favorites, value, 30, 200, n, &mut warns)
                    }
                    "max_alias_len" => set_num(&mut u.max_alias_len, value, 8, 100, n, &mut warns),
                    _ => false,
                }
            }
            "permissions" => match key.as_str() {
                "macos_ask_again" => set_bool(&mut cfg.macos_ask_again, value, n, &mut warns),
                _ => false,
            },
            "new_window" => match key.as_str() {
                "command" => {
                    cfg.new_window_command = (!value.is_empty()).then(|| value.to_string());
                    true
                }
                _ => false,
            },
            "keys" => set_keys(&mut cfg.keys, BROWSE_ACTIONS, &key, value, n, &mut warns),
            "keys.favorites" => set_keys(&mut cfg.fav_keys, FAVORITES_ACTIONS, &key, value, n, &mut warns),
            _ => true, // unknown section: already reported
        };
        if !known {
            warns.push(format!("config line {n}: unknown setting '{key}' in [{section}]"));
        }
    }

    for (section, map) in [("keys", &cfg.keys), ("keys.favorites", &cfg.fav_keys)] {
        for (spec, a, b) in map.conflicts() {
            warns.push(format!(
                "config: key '{}' is bound to both {} and {} in [{section}]; {} wins",
                spec.name(),
                a.name(),
                b.name(),
                a.name()
            ));
        }
    }
    (cfg, warns)
}

/// Parse a true/false value into `dst`; a bad value leaves `dst` alone and adds a warning.
/// Always returns true (the setting name was known).
fn set_bool(dst: &mut bool, value: &str, n: usize, warns: &mut Vec<String>) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => *dst = true,
        "false" | "no" | "off" | "0" => *dst = false,
        _ => warns.push(format!("config line {n}: '{value}' is not true/false")),
    }
    true
}

/// Parse a number into `dst`, clamped to `min..=max`; a bad value leaves `dst` alone and warns.
/// Always returns true (the setting name was known).
fn set_num(dst: &mut usize, value: &str, min: usize, max: usize, n: usize, warns: &mut Vec<String>) -> bool {
    match value.parse::<usize>() {
        Ok(v) => *dst = v.clamp(min, max),
        Err(_) => warns.push(format!("config line {n}: '{value}' is not a number")),
    }
    true
}

/// Apply a `[keys]` line: look up the action, parse its keys, drop reserved or invalid ones.
/// Returns false if `key` is not an action available in this section.
fn set_keys(
    map: &mut KeyMap,
    allowed: &[Action],
    key: &str,
    value: &str,
    n: usize,
    warns: &mut Vec<String>,
) -> bool {
    let Some(action) = Action::from_name(key).filter(|a| allowed.contains(a)) else {
        return false;
    };
    let mut specs = Vec::new();
    let mut had_error = false;
    for token in value.split_whitespace() {
        match parse_key(token) {
            Ok(spec) if spec.is_reserved() => {
                warns.push(format!(
                    "config line {n}: '{token}' is reserved (number keys and Ctrl-C are fixed)"
                ));
                had_error = true;
            }
            Ok(spec) => specs.push(spec),
            Err(e) => {
                warns.push(format!("config line {n}: {e}"));
                had_error = true;
            }
        }
    }
    if had_error && specs.is_empty() {
        return true; // nothing usable: keep the default binding
    }
    map.set(action, specs);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn defaults_survive_a_round_trip_through_the_ini_text() {
        let original = Config::default();
        let (back, warns) = parse(&original.to_ini());
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(back, original);
    }

    #[test]
    fn the_built_in_bindings_have_no_conflicts() {
        assert!(KeyMap::browse_defaults().conflicts().is_empty());
        assert!(KeyMap::favorites_defaults().conflicts().is_empty());
    }

    #[test]
    fn the_built_in_bindings_avoid_the_reserved_keys() {
        for map in [KeyMap::browse_defaults(), KeyMap::favorites_defaults()] {
            for a in map.actions() {
                assert!(map.keys(a).iter().all(|k| !k.is_reserved()), "{a:?}");
            }
        }
    }

    #[test]
    fn every_action_is_listed_in_its_context_and_names_round_trip() {
        let cfg = Config::default();
        for a in BROWSE_ACTIONS {
            assert!(cfg.keys.actions().any(|x| x == *a), "{a:?}");
        }
        for a in FAVORITES_ACTIONS {
            assert!(cfg.fav_keys.actions().any(|x| x == *a), "{a:?}");
        }
        for a in Action::ALL {
            assert_eq!(Action::from_name(a.name()), Some(*a));
        }
    }

    #[test]
    fn overriding_a_binding_replaces_the_default() {
        let (cfg, warns) = parse("[keys]\nbookmark = B ctrl-b\n");
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(
            cfg.keys.lookup(&ev(KeyCode::Char('B'), KeyModifiers::SHIFT)),
            Some(Action::Bookmark)
        );
        assert_eq!(
            cfg.keys.lookup(&ev(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Some(Action::Bookmark)
        );
        assert_eq!(cfg.keys.lookup(&ev(KeyCode::Char('b'), KeyModifiers::NONE)), None);
    }

    #[test]
    fn an_empty_value_unbinds_an_action() {
        let (cfg, warns) = parse("[keys]\nhome =\n");
        assert!(warns.is_empty(), "{warns:?}");
        assert!(cfg.keys.keys(Action::Home).is_empty());
    }

    #[test]
    fn bad_input_warns_but_never_breaks_anything() {
        let text = "[keys]\nbookmark = banana\nfly = x\n[nonsense]\nui = 1\n[ui]\nshow_hidden = maybe\nmax_alias_len = lots\nno equals sign\n";
        let (cfg, warns) = parse(text);
        assert_eq!(cfg.keys.keys(Action::Bookmark), Config::default().keys.keys(Action::Bookmark));
        assert!(!cfg.ui.show_hidden);
        assert!(warns.len() >= 6, "{warns:?}");
        assert!(warns.iter().any(|w| w.contains("unknown key 'banana'")));
        assert!(warns.iter().any(|w| w.contains("unknown setting 'fly'")));
        assert!(warns.iter().any(|w| w.contains("unknown section [nonsense]")));
    }

    #[test]
    fn numbers_are_clamped_to_sane_ranges() {
        let (cfg, _) = parse("[ui]\nmin_width_for_favorites = 5\nmax_alias_len = 5000\n");
        assert_eq!(cfg.ui.min_width_for_favorites, 30);
        assert_eq!(cfg.ui.max_alias_len, 100);
    }

    #[test]
    fn switches_and_the_new_window_command_are_read() {
        let (cfg, warns) = parse(
            "[ui]\nshow_hidden = yes\nfilter_case_sensitive = on\n[permissions]\nmacos_ask_again = off\n\
             [new_window]\ncommand = kitty --directory {path}\n",
        );
        assert!(warns.is_empty(), "{warns:?}");
        assert!(cfg.ui.show_hidden && cfg.ui.filter_case_sensitive && !cfg.macos_ask_again);
        assert_eq!(cfg.new_window_command.as_deref(), Some("kitty --directory {path}"));
        assert_eq!(parse("[new_window]\ncommand =\n").0.new_window_command, None);
    }

    #[test]
    fn conflicting_bindings_are_reported() {
        let (_cfg, warns) = parse("[keys]\nbookmark = j\n");
        assert!(warns.iter().any(|w| w.contains("bound to both")), "{warns:?}");
    }

    #[test]
    fn the_number_keys_and_ctrl_c_cannot_be_rebound() {
        let (cfg, warns) = parse("[keys]\nbookmark = 5\nhome = ctrl-c ~\n");
        assert_eq!(warns.iter().filter(|w| w.contains("reserved")).count(), 2, "{warns:?}");
        assert_eq!(cfg.keys.keys(Action::Bookmark), Config::default().keys.keys(Action::Bookmark));
        assert_eq!(cfg.keys.keys(Action::Home).len(), 1, "the usable key stays");
    }

    #[test]
    fn action_names_exist_only_in_the_right_context() {
        let (_cfg, warns) = parse("[keys]\nrename = z\n[keys.favorites]\nbookmark = z\n");
        assert_eq!(warns.len(), 2, "{warns:?}");
    }

    #[test]
    fn key_names_parse_and_print_consistently() {
        for text in ["k", "G", "/", "-", "space", "ctrl-x", "alt-left", "shift-up", "f5", "pageup", "~"] {
            let spec = parse_key(text).unwrap();
            assert_eq!(parse_key(&spec.name()).unwrap(), spec, "{text}");
        }
        assert!(parse_key("ctrl-").is_err());
        assert!(parse_key("f13").is_err());
        assert!(parse_key("nonsense").is_err());
    }

    #[test]
    fn shift_plus_arrow_still_moves_but_an_exact_shift_binding_wins() {
        let mut cfg = Config::default();
        let shift_down = ev(KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(cfg.keys.lookup(&shift_down), Some(Action::MoveDown));
        cfg.keys.set(Action::PageDown, vec![parse_key("shift-down").unwrap()]);
        assert_eq!(cfg.keys.lookup(&shift_down), Some(Action::PageDown));
        assert_eq!(
            cfg.keys.lookup(&ev(KeyCode::Down, KeyModifiers::NONE)),
            Some(Action::MoveDown)
        );
    }

    #[test]
    fn back_tab_matches_even_though_terminals_add_shift() {
        let keys = KeyMap::browse_defaults();
        assert_eq!(
            keys.lookup(&ev(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Some(Action::ToggleFavorites)
        );
    }

    #[test]
    fn ctrl_and_alt_must_match_exactly() {
        let keys = KeyMap::browse_defaults();
        assert_eq!(keys.lookup(&ev(KeyCode::Char('j'), KeyModifiers::CONTROL)), None);
        assert_eq!(keys.lookup(&ev(KeyCode::Char('j'), KeyModifiers::ALT)), None);
        assert_eq!(
            keys.lookup(&ev(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(Action::Bottom)
        );
    }

    #[test]
    fn labels_use_symbols_for_arrows() {
        let keys = KeyMap::browse_defaults();
        assert_eq!(keys.label(Action::MoveUp).as_deref(), Some("↑"));
        assert_eq!(keys.label(Action::Select).as_deref(), Some("space"));
    }

    #[test]
    fn paths_follow_the_data_dir() {
        let cfg = Config {
            data_dir: Some(PathBuf::from("/data/.cdx")),
            ..Config::default()
        };
        assert_eq!(cfg.favorites_path(), Some(PathBuf::from("/data/.cdx/favorites.tsv")));
        assert_eq!(cfg.config_path(), Some(PathBuf::from("/data/.cdx/config.ini")));
        let elsewhere = env::temp_dir().join("favs.tsv");
        let (mut abs, _) = parse(&format!("[paths]\nfavorites_file = {}\n", elsewhere.display()));
        abs.data_dir = Some(PathBuf::from("/data/.cdx"));
        assert_eq!(abs.favorites_path(), Some(elsewhere));
    }

    #[test]
    fn legacy_favourites_are_copied_once_and_never_overwritten() {
        let dir = env::temp_dir().join(format!("cdx-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (old, new) = (dir.join("old.tsv"), dir.join("sub").join("new.tsv"));

        assert!(!migrate_favorites(&old, &new).unwrap(), "nothing to migrate");
        fs::write(&old, "A\t/a\n").unwrap();
        assert!(migrate_favorites(&old, &new).unwrap());
        assert_eq!(fs::read_to_string(&new).unwrap(), "A\t/a\n");
        assert!(old.exists(), "the old file is left alone");

        fs::write(&old, "B\t/b\n").unwrap();
        assert!(!migrate_favorites(&old, &new).unwrap(), "never overwrites");
        assert_eq!(fs::read_to_string(&new).unwrap(), "A\t/a\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
