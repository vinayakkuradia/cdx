//! cdx's own sub-commands, in one table.
//!
//! A command's name is a word `cdx` answers itself instead of treating it as a
//! favourite or handing it to `cd`. So every command name is also **reserved**:
//! no favourite may be called that (as alias or as folder name), and the shell
//! integration passes it straight through. Add a command here (name, usage,
//! summary, function) and all of that follows automatically.

use std::process::ExitCode;

use crate::config::Config;
use crate::init;

/// One cdx command: its name (also a reserved word), how it is used, and what runs it.
pub struct Command {
    /// The word typed after `cdx`, lower case. Reserved: no favourite may use it.
    pub name: &'static str,
    /// The command as shown in `--help`, starting with the name, e.g. `config <WHAT>`.
    pub usage: &'static str,
    /// One line for `--help` saying what the command does.
    pub summary: &'static str,
    /// Runs the command with the arguments that follow its name and returns the exit code.
    pub run: fn(&[String]) -> ExitCode,
}

/// The table of commands. Add a row here to add a command: it is reserved, listed in
/// `--help` and passed through by the shell wrappers automatically.
pub const COMMANDS: &[Command] = &[
    Command {
        name: "init",
        usage: "init <SHELL>",
        summary: "print the shell integration (bash, zsh, fish, powershell, cmd)",
        run: run_init,
    },
    Command {
        name: "config",
        usage: "config <WHAT>",
        summary: "init (write a commented config.ini), path, show, check",
        run: run_config,
    },
];

/// The command called exactly `name`.
pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// Is `name` a command name? Case-insensitive, so `Config` is refused as well.
pub fn is_reserved(name: &str) -> bool {
    let n = name.trim();
    COMMANDS.iter().any(|c| c.name.eq_ignore_ascii_case(n))
}

/// Why a favourite cannot have this name.
pub fn clash_message(name: &str) -> String {
    format!("'{name}' is a cdx command, so it cannot be a favourite's name (`cdx {name}` would run the command)")
}

/// The names of all commands, in table order.
pub fn names() -> impl Iterator<Item = &'static str> {
    COMMANDS.iter().map(|c| c.name)
}

/// The command lines for `--help`.
pub fn help_lines() -> String {
    COMMANDS
        .iter()
        .map(|c| format!("    cdx {:<20} {}\n", c.usage, c.summary))
        .collect()
}

/// `cdx init <shell>`: print the shell wrapper.
fn run_init(args: &[String]) -> ExitCode {
    match args.first() {
        Some(shell) => init::run(shell),
        None => {
            eprintln!("cdx: usage: cdx init <{}>", init::SHELLS.replace(", ", "|"));
            ExitCode::from(2)
        }
    }
}

/// `cdx-bin config <init|path|show|check>`.
fn run_config(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("init") => {
            let force = args.get(1).map(String::as_str) == Some("--force");
            match Config::write_default_file(force) {
                Ok((path, true)) => {
                    println!("wrote {}", path.display());
                    ExitCode::SUCCESS
                }
                Ok((path, false)) => {
                    eprintln!(
                        "cdx: {} already exists; left untouched (use `config init --force` to replace it)",
                        path.display()
                    );
                    ExitCode::from(1)
                }
                Err(e) => {
                    eprintln!("cdx: could not write the config file: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Some("path") => match Config::load().0.config_path() {
            Some(p) => {
                println!("{}", p.display());
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("cdx: cannot find your home folder");
                ExitCode::from(2)
            }
        },
        Some("show") => {
            let (cfg, warns) = Config::load();
            for w in &warns {
                eprintln!("cdx: {w}");
            }
            print!("{}", cfg.to_ini());
            ExitCode::SUCCESS
        }
        Some("check") => {
            let (cfg, warns) = Config::load();
            let place = cfg
                .config_path()
                .filter(|p| p.is_file())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "no config.ini, using the defaults".into());
            if warns.is_empty() {
                println!("config OK ({place})");
                ExitCode::SUCCESS
            } else {
                for w in &warns {
                    eprintln!("cdx: {w}");
                }
                ExitCode::from(1)
            }
        }
        _ => {
            eprintln!("cdx: usage: cdx config <init [--force]|path|show|check>");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_well_formed() {
        let mut seen = Vec::new();
        for c in COMMANDS {
            assert!(!c.name.is_empty() && !c.name.starts_with('-'), "{}", c.name);
            assert_eq!(c.name, c.name.to_lowercase(), "names are lower case");
            assert!(!c.name.contains(char::is_whitespace));
            assert!(c.usage.starts_with(c.name), "{}", c.usage);
            assert!(!seen.contains(&c.name), "duplicate command {}", c.name);
            seen.push(c.name);
        }
    }

    #[test]
    fn every_command_name_is_reserved_ignoring_case() {
        for name in names() {
            assert!(is_reserved(name));
            assert!(is_reserved(&name.to_uppercase()));
            assert!(is_reserved(&format!("  {name} ")));
            assert!(find(name).is_some());
        }
        assert!(is_reserved("config") && is_reserved("Config") && is_reserved("INIT"));
    }

    #[test]
    fn ordinary_names_are_not_reserved() {
        for name in ["work", "configs", "my-config", "con", "", "1", "initial"] {
            assert!(!is_reserved(name), "{name}");
        }
        assert!(find("Config").is_none(), "commands themselves match exactly");
    }

    #[test]
    fn the_message_names_the_word() {
        assert!(clash_message("config").contains("'config'"));
    }

    #[test]
    fn help_lists_every_command() {
        let help = help_lines();
        for name in names() {
            assert!(help.contains(&format!("cdx {name}")), "{name}");
        }
    }
}
