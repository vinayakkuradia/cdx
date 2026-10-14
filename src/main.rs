mod browser;
mod favorites;
mod fsutil;
mod init;
mod newwin;

use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use browser::Outcome;
use favorites::Favorite;

fn help() -> String {
    format!(
        "cdx {version} - navigate to any folder, then jump there

USAGE:
    cdx [-h] [-n] [PATH]   browse (starting at PATH, or the current folder)
    cdx [-n] <N|NAME>      speed dial: go straight to favourite N (1-9), or to the
                           favourite whose name or alias is NAME (no browser)
    cdx init <SHELL>       print the shell integration ({shells})

OPTIONS:
    -h, --hidden           show hidden folders
    -n, --new-window       open the chosen folder in a new terminal window
                           instead of changing this shell's folder
        --help             show this help
    -V, --version          show version

KEYS:
    Up/Down or j/k         move            PageUp/PageDown   jump a page
    Right/l/Enter          open folder     Left/h/Backspace  go up
    Space                  select folder and cd there
    .                      toggle hidden   ~                 home folder
    /                      filter folders as you type (Enter/Esc keeps the
                           results; Esc again clears them)
    Esc/q/Ctrl-C           cancel (stay where you are)

FAVOURITES (shown on the left, up to nine):
    b                      bookmark the highlighted folder
    1-9                    jump to that favourite
    Tab                    edit favourites; press Tab again to finish (saved)
        Up/Down            move the cursor
        Space              pick up an item, move it with Up/Down, Space to put down
        1-9                while holding: send the item to that slot
        r                  rename (set an alias; empty = folder name)
        x                  remove          Enter  open it    Esc  undo / back
    stored in: {favfile}

SETUP:
    bash/zsh    eval \"$(cdx-bin init bash)\"        in ~/.bashrc or ~/.zshrc
    fish        cdx-bin init fish | source          in ~/.config/fish/config.fish
    PowerShell  Invoke-Expression (& cdx-bin init powershell | Out-String)   in $PROFILE
    cmd         cdx-bin init cmd > cdx.cmd          then put cdx.cmd on your PATH
",
        version = env!("CARGO_PKG_VERSION"),
        shells = init::SHELLS,
        favfile = favorites::file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(no config folder found)".into()),
    )
}

/// What the command line asked for.
#[derive(Debug)]
enum Start {
    /// Open the browser here.
    Browse(PathBuf),
    /// A favourite was named: go straight there, no browser.
    Jump(PathBuf),
}

/// A single digit 1-9 names a favourite slot (returned as a 0-based index).
fn dial_slot(arg: &str) -> Option<usize> {
    match arg.as_bytes() {
        [d @ b'1'..=b'9'] => Some((*d - b'1') as usize),
        _ => None,
    }
}

/// A bare word (no slashes, drive colon, or leading `.`/`~`) may be a favourite's name.
fn is_plain_word(arg: &str) -> bool {
    !arg.is_empty() && !arg.contains(['/', '\\', ':']) && !arg.starts_with(['.', '~'])
}

/// Could `arg` be a speed dial rather than a folder path?
fn is_dial_candidate(arg: &str) -> bool {
    dial_slot(arg).is_some() || is_plain_word(arg)
}

fn jump_target(slot: usize, fav: &Favorite) -> Result<Start, String> {
    if fav.path.is_dir() {
        Ok(Start::Jump(fav.path.clone()))
    } else {
        Err(format!(
            "favourite {} '{}' no longer exists: {}",
            slot + 1,
            fav.label(),
            fav.path.display()
        ))
    }
}

/// Turn the PATH argument into a [`Start`]. `1`-`9` and favourite names are speed
/// dials; anything else is a folder to browse from.
fn resolve(arg: &str, favs: &[Favorite]) -> Result<Start, String> {
    if let Some(slot) = dial_slot(arg) {
        return match favs.get(slot) {
            Some(fav) => jump_target(slot, fav),
            None if favs.is_empty() => Err(format!(
                "no favourite {} (you have none yet; press b inside cdx to add one)",
                slot + 1
            )),
            None => Err(format!(
                "no favourite {} (you have {}, so slots 1-{})",
                slot + 1,
                favs.len(),
                favs.len()
            )),
        };
    }
    if !is_plain_word(arg) {
        return Ok(Start::Browse(PathBuf::from(arg)));
    }
    let lower = arg.to_lowercase();
    let hit = favs
        .iter()
        .position(|f| f.label() == arg)
        .or_else(|| favs.iter().position(|f| f.label().to_lowercase() == lower));
    match hit {
        Some(i) => jump_target(i, &favs[i]),
        // Not a favourite: an ordinary folder in the current directory still works.
        None if Path::new(arg).is_dir() => Ok(Start::Browse(PathBuf::from(arg))),
        None => Err(format!(
            "no favourite named '{arg}', and no folder with that name"
        )),
    }
}

/// Hand the chosen folder over: print it for the shell wrapper, or open a window.
fn deliver(path: &Path, new_window: bool) -> ExitCode {
    if new_window {
        // Nothing goes to stdout, so the wrapper leaves this shell where it is.
        return match newwin::open(path) {
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

fn main() -> ExitCode {
    let mut show_hidden = false;
    let mut new_window = false;
    let mut target: Option<String> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--hidden" => show_hidden = true,
            "-n" | "--new-window" => new_window = true,
            "--help" => {
                print!("{}", help());
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("cdx {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "init" => {
                return match args.next() {
                    Some(shell) => init::run(&shell),
                    None => {
                        eprintln!("cdx: usage: cdx init <{}>", init::SHELLS.replace(", ", "|"));
                        ExitCode::from(2)
                    }
                };
            }
            s if s.starts_with('-') && s.len() > 1 => {
                eprintln!("cdx: unknown option '{s}' (try --help)");
                return ExitCode::from(2);
            }
            _ => target = Some(arg),
        }
    }

    let start = match target {
        None => match env::current_dir() {
            Ok(p) => Start::Browse(p),
            Err(e) => {
                eprintln!("cdx: cannot determine the current folder: {e}");
                return ExitCode::from(2);
            }
        },
        Some(arg) if is_dial_candidate(&arg) => {
            let favs = match favorites::load() {
                Ok(f) => f,
                // A broken favourites file only matters if a number was typed.
                Err(e) if dial_slot(&arg).is_some() => {
                    eprintln!("cdx: could not read favourites: {e}");
                    return ExitCode::from(2);
                }
                Err(_) => Vec::new(),
            };
            match resolve(&arg, &favs) {
                Ok(s) => s,
                Err(msg) => {
                    eprintln!("cdx: {msg}");
                    return ExitCode::from(2);
                }
            }
        }
        Some(arg) => Start::Browse(PathBuf::from(arg)),
    };

    let chosen = match start {
        Start::Jump(path) => path,
        Start::Browse(start) => {
            let start = match fsutil::normalize(&start) {
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
            match browser::run(start, show_hidden) {
                Ok(Outcome::Selected(path)) => path,
                // Cancelled is a normal, intended outcome: print nothing (so the
                // wrapper leaves you where you are) and exit successfully.
                Ok(Outcome::Cancelled) => return ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("cdx: terminal error: {e}");
                    return ExitCode::from(2);
                }
            }
        }
    };
    deliver(&chosen, new_window)
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
    fn a_digit_jumps_to_that_slot() {
        let root = scratch("digit");
        let (a, b) = (root.join("a"), root.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let favs = vec![fav(a, None), fav(b.clone(), None)];
        assert!(matches!(resolve("2", &favs), Ok(Start::Jump(p)) if p == b));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_slot_is_an_error_that_says_how_many_exist() {
        let root = scratch("slot");
        let favs = vec![fav(root.clone(), None)];
        let msg = resolve("5", &favs).unwrap_err();
        assert!(msg.contains("no favourite 5") && msg.contains("1-1"), "{msg}");
        let msg = resolve("1", &[]).unwrap_err();
        assert!(msg.contains("none yet"), "{msg}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_alias_or_folder_name_matches_ignoring_case() {
        let root = scratch("alias");
        let work = root.join("projects");
        fs::create_dir_all(&work).unwrap();
        let favs = vec![fav(work.clone(), Some("Work"))];
        assert!(matches!(resolve("Work", &favs), Ok(Start::Jump(p)) if p == work));
        assert!(matches!(resolve("work", &favs), Ok(Start::Jump(p)) if p == work));
        let unnamed = vec![fav(work.clone(), None)];
        assert!(matches!(resolve("projects", &unnamed), Ok(Start::Jump(p)) if p == work));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unknown_name_is_an_error_but_paths_still_browse() {
        let msg = resolve("definitely-not-here-xyz", &[]).unwrap_err();
        assert!(msg.contains("no favourite named"), "{msg}");
        assert!(matches!(resolve("./x", &[]), Ok(Start::Browse(_))));
        assert!(matches!(resolve("../x", &[]), Ok(Start::Browse(_))));
        assert!(matches!(resolve("/tmp", &[]), Ok(Start::Browse(_))));
        assert!(matches!(resolve("~", &[]), Ok(Start::Browse(_))));
    }

    #[test]
    fn a_deleted_favourite_is_reported() {
        let root = scratch("gone");
        let gone = root.join("gone");
        let favs = vec![fav(gone, Some("old"))];
        let msg = resolve("1", &favs).unwrap_err();
        assert!(msg.contains("no longer exists") && msg.contains("'old'"), "{msg}");
        let _ = fs::remove_dir_all(&root);
    }
}
