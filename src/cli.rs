//! Reading the command line.
//!
//! cdx is an extension of `cd`. The rules, in order:
//!
//! 1. No arguments opens the browser (cdx's own job; `cd` alone would go home).
//! 2. A cdx command (`init`, `config`, ...) or one of cdx's options runs cdx.
//! 3. A single bare word or digit is looked up among the favourites first. A
//!    favourite wins; if there is none, the shell's `cd` gets the arguments.
//! 4. Anything else (`..`, `-`, `~/src`, `-L link`, `/some/dir`) goes to `cd`
//!    exactly as typed.

use crate::commands;

/// Printed instead of a folder when the shell wrapper should run `cd` itself.
/// It can never be mistaken for a folder: those are printed as absolute paths.
pub const PASS_TO_CD: &str = "cdx:use-cd";

/// What the command line asks cdx to do (see `parse` for the rules).
#[derive(Debug, PartialEq)]
pub enum Request {
    /// Print the help text.
    Help,
    /// Print the version.
    Version,
    /// A cdx command by name, with the arguments after it.
    Command(String, Vec<String>),
    /// Open the browser, in the current folder or at `start`.
    Browse {
        /// Folder to start in; None means the current folder.
        start: Option<String>,
        /// Show hidden folders (`-h`).
        hidden: bool,
        /// Open the result in a new terminal window (`-n`).
        new_window: bool,
    },
    /// One word that may name a favourite. If it does not: `else_cd` hands the
    /// arguments to `cd`; otherwise it is a folder to browse or open.
    Target {
        /// The word as typed: a slot number, a favourite name, or (with cdx options) a folder.
        arg: String,
        /// Show hidden folders (`-h`).
        hidden: bool,
        /// Open the result in a new terminal window (`-n`).
        new_window: bool,
        /// True when no cdx option was used: a word that is not a favourite goes to `cd`.
        else_cd: bool,
    },
    /// Not cdx's business: the shell's `cd` handles the arguments.
    PassToCd,
    /// The arguments make no sense; print this message and fail.
    Error(String),
}

/// A single digit 1-9 names a favourite slot (returned as a 0-based index).
pub fn dial_slot(arg: &str) -> Option<usize> {
    match arg.as_bytes() {
        [d @ b'1'..=b'9'] => Some((*d - b'1') as usize),
        _ => None,
    }
}

/// A bare word (no slashes, drive colon, or leading `.`/`~`) may be a favourite's name.
pub fn is_plain_word(arg: &str) -> bool {
    !arg.is_empty()
        && !arg.starts_with('-')
        && !arg.contains(['/', '\\', ':'])
        && !arg.starts_with(['.', '~'])
}

/// Could `arg` be a speed dial rather than something for `cd`?
pub fn is_dial_candidate(arg: &str) -> bool {
    dial_slot(arg).is_some() || is_plain_word(arg)
}

/// Decide what `args` (everything after the program name) mean. Pure: touches no files.
pub fn parse(args: &[String]) -> Request {
    let Some(first) = args.first() else {
        return Request::Browse {
            start: None,
            hidden: false,
            new_window: false,
        };
    };
    if commands::find(first).is_some() {
        return Request::Command(first.clone(), args[1..].to_vec());
    }

    let (mut hidden, mut new_window, mut browse) = (false, false, false);
    let (mut help, mut version, mut after_dashes) = (false, false, false);
    let mut rest: Vec<String> = Vec::new();
    for a in args {
        if after_dashes {
            rest.push(a.clone());
            continue;
        }
        match a.as_str() {
            "-h" | "--hidden" => hidden = true,
            "-n" | "--new-window" => new_window = true,
            "-b" | "--browse" => browse = true,
            "--help" => help = true,
            "-V" | "--version" => version = true,
            "--" => {
                after_dashes = true;
                rest.push(a.clone());
            }
            _ => rest.push(a.clone()),
        }
    }
    if help {
        return Request::Help;
    }
    if version {
        return Request::Version;
    }

    let owned = hidden || new_window || browse;
    if !owned {
        return match rest.as_slice() {
            [word] if is_dial_candidate(word) => Request::Target {
                arg: word.clone(),
                hidden,
                new_window,
                else_cd: true,
            },
            _ => Request::PassToCd,
        };
    }

    // cdx's own options were used: nothing is passed to cd any more.
    if let Some(bad) = rest.iter().find(|a| a.starts_with('-') && a.len() > 1) {
        return Request::Error(format!("unknown option '{bad}' (try --help)"));
    }
    match rest.as_slice() {
        [] => Request::Browse {
            start: None,
            hidden,
            new_window,
        },
        [path] if browse => Request::Browse {
            start: Some(path.clone()),
            hidden,
            new_window,
        },
        [arg] => Request::Target {
            arg: arg.clone(),
            hidden,
            new_window,
            else_cd: false,
        },
        _ => Request::Error("too many arguments (try --help)".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Request {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn target(arg: &str, hidden: bool, new_window: bool, else_cd: bool) -> Request {
        Request::Target {
            arg: arg.into(),
            hidden,
            new_window,
            else_cd,
        }
    }

    #[test]
    fn no_arguments_opens_the_browser() {
        assert_eq!(
            p(&[]),
            Request::Browse {
                start: None,
                hidden: false,
                new_window: false
            }
        );
    }

    #[test]
    fn everyday_cd_usage_goes_to_cd() {
        for args in [
            &[".."][..],
            &["-"],
            &["../x"],
            &["./src"],
            &["/tmp"],
            &["~"],
            &["~/src"],
            &["C:\\Users"],
            &["-L", "link"],
            &["-P", "link"],
            &["--", "-weird"],
            &["a", "b"],
            &["-x"],
            &["."],
        ] {
            assert_eq!(p(args), Request::PassToCd, "{args:?}");
        }
    }

    #[test]
    fn one_bare_word_or_digit_might_be_a_favourite_and_otherwise_is_cd() {
        assert_eq!(p(&["work"]), target("work", false, false, true));
        assert_eq!(p(&["3"]), target("3", false, false, true));
        assert_eq!(p(&["10"]), target("10", false, false, true), "only 1-9 are slots, but it is still a plain word");
    }

    #[test]
    fn commands_win_over_everything() {
        assert_eq!(p(&["config"]), Request::Command("config".into(), vec![]));
        assert_eq!(
            p(&["config", "init", "--force"]),
            Request::Command("config".into(), vec!["init".into(), "--force".into()])
        );
        assert_eq!(p(&["init", "bash"]), Request::Command("init".into(), vec!["bash".into()]));
    }

    #[test]
    fn help_and_version() {
        assert_eq!(p(&["--help"]), Request::Help);
        assert_eq!(p(&["-V"]), Request::Version);
        assert_eq!(p(&["--version"]), Request::Version);
    }

    #[test]
    fn own_options_open_the_browser_or_act_on_a_favourite() {
        assert_eq!(
            p(&["-h"]),
            Request::Browse {
                start: None,
                hidden: true,
                new_window: false
            }
        );
        assert_eq!(
            p(&["-n"]),
            Request::Browse {
                start: None,
                hidden: false,
                new_window: true
            }
        );
        assert_eq!(
            p(&["-b", "/tmp"]),
            Request::Browse {
                start: Some("/tmp".into()),
                hidden: false,
                new_window: false
            }
        );
        assert_eq!(
            p(&["--browse", "-h", "../x"]),
            Request::Browse {
                start: Some("../x".into()),
                hidden: true,
                new_window: false
            }
        );
        assert_eq!(p(&["-n", "2"]), target("2", false, true, false));
        assert_eq!(p(&["-n", "/tmp"]), target("/tmp", false, true, false));
        assert_eq!(p(&["-h", "work"]), target("work", true, false, false));
    }

    #[test]
    fn mixing_own_options_with_cd_options_is_an_error() {
        assert!(matches!(p(&["-n", "-L"]), Request::Error(m) if m.contains("unknown option '-L'")));
        assert!(matches!(p(&["-h", "a", "b"]), Request::Error(m) if m.contains("too many")));
    }

    #[test]
    fn what_counts_as_a_plain_word() {
        for w in ["work", "7", "my-proj", "Config"] {
            assert!(is_dial_candidate(w), "{w}");
        }
        for w in ["", "-", "-x", ".", "..", "~", "a/b", "/a", "C:", "a\\b", ".hidden"] {
            assert!(!is_dial_candidate(w), "{w}");
        }
        assert_eq!(dial_slot("1"), Some(0));
        assert_eq!(dial_slot("9"), Some(8));
        assert_eq!(dial_slot("0"), None);
        assert_eq!(dial_slot("12"), None);
    }
}
