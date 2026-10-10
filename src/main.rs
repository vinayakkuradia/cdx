mod browser;
mod favorites;
mod fsutil;
mod init;

use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use browser::Outcome;

fn help() -> String {
    format!(
        "cdx {version} - navigate to any folder, then jump there

USAGE:
    cdx [-h] [PATH]        browse (starting at PATH, or the current folder)
    cdx init <SHELL>       print the shell integration ({shells})

OPTIONS:
    -h, --hidden           show hidden folders
        --help             show this help
    -V, --version          show version

KEYS:
    Up/Down or j/k         move            PageUp/PageDown   jump a page
    Right/l/Enter          open folder     Left/h/Backspace  go up
    Space                  select folder and cd there
    .                      toggle hidden   ~                 home folder
    Esc/q/Ctrl-C           cancel (stay where you are)

FAVOURITES (shown on the left, up to nine):
    b                      bookmark the folder you are in
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

fn main() -> ExitCode {
    let mut show_hidden = false;
    let mut start: Option<PathBuf> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--hidden" => show_hidden = true,
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
            _ => start = Some(PathBuf::from(&arg)),
        }
    }

    let start = match start {
        Some(p) => p,
        None => match env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("cdx: cannot determine the current folder: {e}");
                return ExitCode::from(2);
            }
        },
    };
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
        Ok(Outcome::Selected(path)) => {
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
        // Cancelled: print nothing, so the wrapper leaves you where you are.
        Ok(Outcome::Cancelled) => ExitCode::from(1),
        Err(e) => {
            eprintln!("cdx: terminal error: {e}");
            ExitCode::from(2)
        }
    }
}
