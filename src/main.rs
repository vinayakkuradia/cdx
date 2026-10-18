//! cdx: a folder browser for the terminal that doubles as an extension of `cd`.
//!
//! `cdx-bin` decides what a command line means (`cli`), runs the browser or a
//! command, and prints at most one thing on stdout for the shell wrapper (see
//! `init`): the folder to `cd` into, or the marker that hands the arguments to the
//! shell's own `cd`. All interface output goes to stderr.

/// The interactive folder browser.
mod browser;
/// Reading the command line: what is cdx's, what is `cd`'s.
mod cli;
/// cdx's own commands and the reserved-name list.
mod commands;
/// Settings, key bindings and where files live.
mod config;
/// Bookmarked folders and their file format.
mod favorites;
/// Folder listing, hidden detection, drives and paths.
mod fsutil;
/// The shell wrappers printed by `cdx init`.
mod init;
/// Opening a folder in a new terminal window (`-n`).
mod newwin;

use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use browser::Outcome;
use cli::Request;
use config::Config;
use favorites::Favorite;

/// The text of `--help`, including the command list and where cdx keeps its files.
fn help() -> String {
    format!(
        "cdx {version} - jump to any folder; works like `cd`, and more

USAGE:
    cdx                    open the folder browser (where plain `cd` would go home)
    cdx <N|NAME>           go to favourite N (1-9), or the favourite whose name or
                           alias is NAME (case-insensitive). Favourites come first.
    cdx <anything else>    handed to your shell's `cd`, exactly as typed:
                           cdx ..    cdx -    cdx ~/src    cdx -L link    cdx /some/dir
    cdx -b [PATH]          open the browser starting at PATH
{commands}
OPTIONS:
    -b, --browse           open the browser (at PATH if given)
    -h, --hidden           show hidden folders
    -n, --new-window       open the chosen folder in a new terminal window
                           instead of changing this shell's folder
        --help             this help
    -V, --version          print the version
    (These options are cdx's own; with them, nothing is passed to `cd`.)

KEYS (the defaults; every one can be changed in config.ini):
    up/k down/j            move            right/l/enter     open folder
    left/h/backspace       go up           space             choose this folder
    home/g end/G           first / last    pageup pagedown   scroll
    .                      toggle hidden   ~                 home folder
    /                      filter folders as you type (Enter/Esc keeps the
                           results; Esc again clears them)
    b                      bookmark the highlighted folder
    1-9                    jump to that favourite
    tab                    edit favourites (reorder, rename, remove)
    esc / q                cancel (changes nothing)

SETUP:
    A program cannot change its parent shell's folder, so cdx needs a tiny shell
    function. Add one line to your shell config (the installers do this for you):
        bash/zsh:    eval \"$(cdx-bin init bash)\"       (or zsh)
        fish:        cdx-bin init fish | source
        PowerShell:  Invoke-Expression (& cdx-bin init powershell | Out-String)
        cmd.exe:     cdx-bin init cmd > cdx.cmd         (put cdx.cmd on your PATH)

    files live in: {datadir}
    (config.ini = settings and key bindings, favorites.tsv = your favourites)
",
        version = env!("CARGO_PKG_VERSION"),
        commands = commands::help_lines(),
        datadir = config::data_dir_from_env()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(no home folder found)".into()),
    )
}

/// The favourite `arg` names, if any: a slot number (1-9) that exists, else a
/// favourite's alias or folder name (exact match first, then ignoring case).
/// `Some(Err(..))` means it exists but its folder is gone.
fn lookup(arg: &str, favs: &[Favorite]) -> Option<Result<PathBuf, String>> {
    let by_slot = cli::dial_slot(arg).filter(|&slot| slot < favs.len());
    let idx = by_slot.or_else(|| {
        if !cli::is_plain_word(arg) {
            return None;
        }
        let lower = arg.to_lowercase();
        favs.iter()
            .position(|f| f.label() == arg)
            .or_else(|| favs.iter().position(|f| f.label().to_lowercase() == lower))
    })?;
    let fav = &favs[idx];
    Some(if fav.path.is_dir() {
        Ok(fav.path.clone())
    } else {
        Err(format!(
            "favourite {} '{}' no longer exists: {}",
            idx + 1,
            fav.label(),
            fav.path.display()
        ))
    })
}

/// Tell the shell wrapper to run `cd` with the original arguments.
fn pass_to_cd() -> ExitCode {
    if io::stdout().is_terminal() {
        // Run directly (no wrapper): there is no shell function to hand this to.
        eprintln!(
            "cdx: that is for `cd`, which only your shell can run. Set up the shell integration\n     \
             (`cdx-bin --help`, see SETUP) and `cdx` will pass it on."
        );
    } else {
        println!("{}", cli::PASS_TO_CD);
    }
    ExitCode::SUCCESS
}

/// Hand the chosen folder over: print it for the shell wrapper, or (`new_window` is
/// `Some(custom command)`) open a new terminal window.
fn deliver(path: &Path, new_window: Option<Option<&str>>) -> ExitCode {
    if let Some(command) = new_window {
        // Nothing goes to stdout, so the wrapper leaves this shell where it is.
        return match newwin::open(path, command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cdx: could not open a new terminal window: {e}");
                ExitCode::from(2)
            }
        };
    }
    let mut stdout = io::stdout().lock();
    if writeln!(stdout, "{}", path.display()).is_err() {
        return ExitCode::from(2);
    }
    drop(stdout);
    if io::stdout().is_terminal() {
        eprintln!(
            "cdx: shell integration is not active, so your folder was not changed.\n     \
             Run `cdx-bin --help` and see SETUP, or `cdx-bin init <shell>`."
        );
    }
    ExitCode::SUCCESS
}

/// Load the configuration (and move old favourites over). Problems are returned,
/// not printed, so that a hand-off to `cd` stays silent.
fn setup(hidden: bool) -> (Config, Vec<String>) {
    let (mut cfg, mut warnings) = Config::load();
    if let Err(e) = cfg.migrate_legacy_favorites() {
        warnings.push(format!("could not copy your old favourites to the new place: {e}"));
    }
    fsutil::set_ask_again(cfg.macos_ask_again);
    // The -h flag adds to the config; it cannot switch hidden folders off.
    cfg.ui.show_hidden |= hidden;
    (cfg, warnings)
}

/// Print each configuration problem on stderr.
fn print_warnings(warnings: &[String]) {
    for w in warnings {
        eprintln!("cdx: {w}");
    }
}

/// Run the browser from `start`, then hand the result over.
fn browse(start: &Path, cfg: Config, warnings: &[String], new_window: bool) -> ExitCode {
    let start = match fsutil::normalize(start) {
        Ok(p) if p.is_dir() => p,
        Ok(p) => {
            eprintln!("cdx: not a folder: {}", p.display());
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("cdx: {}: {e}", start.display());
            return ExitCode::from(2);
        }
    };
    let notice = (!warnings.is_empty()).then(|| {
        format!(
            "config: {} problem{} (listed in your terminal after you leave; or run `cdx-bin config check`)",
            warnings.len(),
            if warnings.len() == 1 { "" } else { "s" }
        )
    });
    let command = cfg.new_window_command.clone();
    match browser::run(start, cfg, notice) {
        Ok(Outcome::Selected(path)) => deliver(&path, new_window.then_some(command.as_deref())),
        // Cancelled is a normal, intended outcome: print nothing (so the wrapper
        // leaves you where you are) and exit successfully.
        Ok(Outcome::Cancelled) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cdx: terminal error: {e}");
            ExitCode::from(2)
        }
    }
}

/// Open the browser (`cdx`, `cdx -b [PATH]`, `cdx -h`, `cdx -n`) and deliver the result.
fn run_browse(start: Option<String>, hidden: bool, new_window: bool) -> ExitCode {
    let (cfg, warnings) = setup(hidden);
    print_warnings(&warnings);
    let start = match start {
        Some(s) => PathBuf::from(s),
        None => match env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("cdx: cannot determine the current folder: {e}");
                return ExitCode::from(2);
            }
        },
    };
    browse(&start, cfg, &warnings, new_window)
}

/// Handle one word: a favourite if it names one; otherwise `cd` gets it, or (with cdx options)
/// it is taken as a folder to browse or open.
fn run_target(arg: String, hidden: bool, new_window: bool, else_cd: bool) -> ExitCode {
    let (cfg, warnings) = setup(hidden);

    let favs = if cli::is_dial_candidate(&arg) {
        let loaded = match cfg.favorites_path() {
            Some(p) => favorites::load_from(&p),
            None => Ok(Vec::new()),
        };
        match loaded {
            Ok(f) => f,
            // A broken favourites file only matters if a number was typed.
            Err(e) if cli::dial_slot(&arg).is_some() => {
                print_warnings(&warnings);
                eprintln!("cdx: could not read favourites: {e}");
                return ExitCode::from(2);
            }
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let window = new_window.then_some(cfg.new_window_command.as_deref());
    match lookup(&arg, &favs) {
        Some(Ok(path)) => {
            print_warnings(&warnings);
            deliver(&path, window)
        }
        Some(Err(msg)) => {
            print_warnings(&warnings);
            eprintln!("cdx: {msg}");
            ExitCode::from(2)
        }
        // Not a favourite, and no cdx option was used: plain `cd` usage.
        None if else_cd => pass_to_cd(),
        // Not a favourite, but cdx's own options were used: take it as a folder.
        None => {
            print_warnings(&warnings);
            let path = Path::new(&arg);
            if !path.is_dir() {
                eprintln!("cdx: no favourite or folder named '{arg}'");
                return ExitCode::from(2);
            }
            if new_window {
                match fsutil::normalize(path) {
                    Ok(p) => deliver(&p, window),
                    Err(e) => {
                        eprintln!("cdx: {arg}: {e}");
                        ExitCode::from(2)
                    }
                }
            } else {
                browse(path, cfg, &warnings, false)
            }
        }
    }
}

/// Read the command line, decide what it asks for, and do it.
fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match cli::parse(&args) {
        Request::Help => {
            print!("{}", help());
            ExitCode::SUCCESS
        }
        Request::Version => {
            println!("cdx {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Request::Command(name, rest) => match commands::find(&name) {
            Some(command) => (command.run)(&rest),
            None => ExitCode::from(2),
        },
        Request::PassToCd => pass_to_cd(),
        Request::Error(msg) => {
            eprintln!("cdx: {msg}");
            ExitCode::from(2)
        }
        Request::Browse {
            start,
            hidden,
            new_window,
        } => run_browse(start, hidden, new_window),
        Request::Target {
            arg,
            hidden,
            new_window,
            else_cd,
        } => run_target(arg, hidden, new_window, else_cd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("cdx-main-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fav(path: PathBuf, alias: Option<&str>) -> Favorite {
        Favorite {
            alias: alias.map(String::from),
            ..Favorite::new(path)
        }
    }

    #[test]
    fn a_digit_names_that_slot() {
        let root = scratch("digit");
        let (a, b) = (root.join("a"), root.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let favs = vec![fav(a, None), fav(b.clone(), None)];
        assert_eq!(lookup("2", &favs), Some(Ok(b)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_slot_or_name_is_not_a_favourite_so_cd_gets_it() {
        let root = scratch("miss");
        let favs = vec![fav(root.clone(), Some("work"))];
        assert_eq!(lookup("5", &favs), None);
        assert_eq!(lookup("1", &[]), None);
        assert_eq!(lookup("nope", &favs), None);
        assert_eq!(lookup("../work", &favs), None, "paths are never looked up");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_alias_or_folder_name_matches_exactly_first_then_ignoring_case() {
        let root = scratch("alias");
        let (work, other) = (root.join("projects"), root.join("other"));
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&other).unwrap();
        let favs = vec![fav(other.clone(), Some("work")), fav(work.clone(), Some("Work"))];
        assert_eq!(lookup("Work", &favs), Some(Ok(work.clone())), "exact beats case-insensitive");
        assert_eq!(lookup("work", &favs), Some(Ok(other)));
        assert!(lookup("WORK", &favs).is_some());
        let unnamed = vec![fav(work.clone(), None)];
        assert_eq!(lookup("projects", &unnamed), Some(Ok(work)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_slot_wins_over_a_favourite_whose_name_is_a_digit() {
        let root = scratch("slotwins");
        let (a, b) = (root.join("a"), root.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let favs = vec![fav(a.clone(), None), fav(b, Some("1"))];
        assert_eq!(lookup("1", &favs), Some(Ok(a)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deleted_favourite_is_reported_rather_than_passed_to_cd() {
        let root = scratch("gone");
        let favs = vec![fav(root.join("gone"), Some("old"))];
        let msg = lookup("1", &favs).unwrap().unwrap_err();
        assert!(msg.contains("no longer exists") && msg.contains("'old'"), "{msg}");
        assert!(lookup("old", &favs).unwrap().is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_help_mentions_the_cd_behaviour_and_every_command() {
        let h = help();
        assert!(h.contains("handed to your shell's `cd`"));
        for name in commands::names() {
            assert!(h.contains(&format!("cdx {name}")), "{name}");
        }
    }
}
