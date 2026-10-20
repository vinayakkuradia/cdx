//! `cdx-bin init <shell>` prints the tiny wrapper that makes `cdx` able to
//! change your shell's directory (a child process cannot do that itself).
//!
//! The wrapper asks `cdx-bin` what to do:
//! * a folder path: `cd` there;
//! * the word `PASS_TO_CD`: this was ordinary `cd` usage, so run `cd` with the
//!   original arguments;
//! * nothing: stay where you are.
//!
//! Commands (`init`, `config`, ...) and `--help`/`--version` are passed straight
//! through; the list of names comes from the commands table, so a new command
//! needs no change here.

use std::process::ExitCode;

use crate::cli::PASS_TO_CD;
use crate::commands;

/// The wrapper for bash and zsh.
const POSIX: &str = r#"# cdx shell integration (bash / zsh)
cdx() {
  case "${1-}" in
    {{CASE}}) command cdx-bin "$@"; return $? ;;
  esac
  local dir
  dir="$(command cdx-bin "$@")" || return $?
  if [ "$dir" = "{{PASS}}" ]; then
    cd "$@"
  elif [ -n "$dir" ] && [ -d "$dir" ]; then
    cd -- "$dir" || return $?
  fi
}
"#;

/// The wrapper for fish.
const FISH: &str = r#"# cdx shell integration (fish)
function cdx
    switch "$argv[1]"
        case {{FISH_CASE}}
            command cdx-bin $argv
            return $status
    end
    set -l dir (command cdx-bin $argv)
    or return $status
    if test "$dir" = "{{PASS}}"
        cd $argv
    else if test -n "$dir"; and test -d "$dir"
        cd -- "$dir"
    end
end
"#;

/// The wrapper for PowerShell 5.1 and 7+.
const POWERSHELL: &str = r#"# cdx shell integration (PowerShell 5.1 / 7+)
function cdx {
    if ($args.Count -gt 0 -and $args[0] -in @({{PS_LIST}})) {
        & cdx-bin @args
        return
    }
    $prev = [Console]::OutputEncoding
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    try { $dir = & cdx-bin @args } finally { [Console]::OutputEncoding = $prev }
    if ($LASTEXITCODE -ne 0) { return }
    if ($dir -eq '{{PASS}}') {
        Set-Location @args
    } elseif ($dir -and (Test-Path -LiteralPath $dir -PathType Container)) {
        Set-Location -LiteralPath $dir
    }
}
"#;

// cmd.exe has no functions, so this one is saved as cdx.cmd somewhere on PATH.
/// The wrapper for cmd.exe, saved as `cdx.cmd` because cmd has no functions.
const CMD: &str = r#"@echo off
rem cdx shell integration (cmd.exe) - save as cdx.cmd somewhere on your PATH
{{CMD_CHECKS}}
set "CDX_DIR="
for /f "usebackq delims=" %%i in (`cdx-bin %*`) do set "CDX_DIR=%%i"
if not defined CDX_DIR goto :eof
if "%CDX_DIR%"=="{{PASS}}" goto usecd
if exist "%CDX_DIR%\" cd /d "%CDX_DIR%"
set "CDX_DIR="
goto :eof
:usecd
set "CDX_DIR="
cd /d %*
goto :eof
:passthrough
cdx-bin %*
"#;

/// The shells `init` knows, for messages.
pub const SHELLS: &str = "bash, zsh, fish, powershell, cmd";

/// Words the wrapper hands straight to `cdx-bin`: every command, plus the info flags.
fn passthrough_words() -> Vec<&'static str> {
    commands::names()
        .chain(["--help", "-V", "--version"])
        .collect()
}

/// The wrapper script for `shell`, or None if the shell is unknown.
fn script(shell: &str) -> Option<String> {
    let words = passthrough_words();
    let text = match shell.to_ascii_lowercase().as_str() {
        "bash" | "zsh" | "sh" => POSIX.replace("{{CASE}}", &words.join("|")),
        "fish" => FISH.replace("{{FISH_CASE}}", &words.join(" ")),
        "powershell" | "pwsh" => {
            let list: Vec<String> = words.iter().map(|w| format!("'{w}'")).collect();
            POWERSHELL.replace("{{PS_LIST}}", &list.join(", "))
        }
        "cmd" => {
            let checks: Vec<String> = words
                .iter()
                .map(|w| format!("if /i \"%~1\"==\"{w}\" goto passthrough"))
                .collect();
            CMD.replace("{{CMD_CHECKS}}", &checks.join("\n"))
                .replace('\n', "\r\n")
        }
        _ => return None,
    };
    Some(text.replace("{{PASS}}", PASS_TO_CD))
}

/// `cdx init <shell>`: print the wrapper for `shell`, or an error if it is unknown.
pub fn run(shell: &str) -> ExitCode {
    match script(shell) {
        Some(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("cdx: unknown shell '{shell}'. Supported: {SHELLS}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 6] = ["bash", "zsh", "fish", "powershell", "pwsh", "cmd"];

    #[test]
    fn every_wrapper_is_complete() {
        for shell in ALL {
            let s = script(shell).unwrap();
            assert!(!s.contains("{{"), "{shell}: unfilled placeholder");
            assert!(s.contains(PASS_TO_CD), "{shell}: no pass-to-cd handling");
            for w in passthrough_words() {
                assert!(s.contains(w), "{shell} does not pass '{w}' through");
            }
        }
    }

    #[test]
    fn a_new_command_would_be_passed_through_automatically() {
        // The lists are built from the commands table, not typed out.
        let names: Vec<_> = commands::names().collect();
        assert!(!names.is_empty());
        assert!(passthrough_words().starts_with(&names));
    }

    #[test]
    fn cmd_uses_windows_line_endings_only() {
        let s = script("cmd").unwrap();
        assert!(s.contains("\r\n") && !s.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn unknown_shells_are_refused() {
        assert!(script("tcsh").is_none());
    }

    #[test]
    fn the_marker_cannot_be_a_folder() {
        // Folders are always printed as absolute paths.
        assert!(!std::path::Path::new(PASS_TO_CD).is_absolute());
        assert!(!PASS_TO_CD.starts_with('/') && !PASS_TO_CD.contains('\\'));
    }
}
