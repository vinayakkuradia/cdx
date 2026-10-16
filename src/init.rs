//! `cdx-bin init <shell>` prints the tiny wrapper that makes `cdx` able to
//! change your shell's directory (a child process cannot do that itself).

use std::process::ExitCode;

const POSIX: &str = r#"# cdx shell integration (bash / zsh)
cdx() {
  case "${1-}" in
    init|config|--help|-V|--version) command cdx-bin "$@"; return $? ;;
  esac
  local dir
  dir="$(command cdx-bin "$@")" || return $?
  if [ -n "$dir" ] && [ -d "$dir" ]; then
    cd -- "$dir" || return $?
  fi
}
"#;

const FISH: &str = r#"# cdx shell integration (fish)
function cdx
    switch "$argv[1]"
        case init config --help -V --version
            command cdx-bin $argv
            return $status
    end
    set -l dir (command cdx-bin $argv)
    or return $status
    if test -n "$dir"; and test -d "$dir"
        cd -- "$dir"
    end
end
"#;

const POWERSHELL: &str = r#"# cdx shell integration (PowerShell 5.1 / 7+)
function cdx {
    if ($args.Count -gt 0 -and $args[0] -in @('init', 'config', '--help', '-V', '--version')) {
        & cdx-bin @args
        return
    }
    $prev = [Console]::OutputEncoding
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    try { $dir = & cdx-bin @args } finally { [Console]::OutputEncoding = $prev }
    if ($LASTEXITCODE -eq 0 -and $dir -and (Test-Path -LiteralPath $dir -PathType Container)) {
        Set-Location -LiteralPath $dir
    }
}
"#;

// cmd.exe has no functions, so this one is saved as cdx.cmd somewhere on PATH.
const CMD: &str = r#"@echo off
rem cdx shell integration (cmd.exe) - save as cdx.cmd somewhere on your PATH
if /i "%~1"=="init" goto passthrough
if /i "%~1"=="--help" goto passthrough
if /i "%~1"=="--version" goto passthrough
if /i "%~1"=="config" goto passthrough
if /i "%~1"=="-V" goto passthrough
set "CDX_DIR="
for /f "usebackq delims=" %%i in (`cdx-bin %*`) do set "CDX_DIR=%%i"
if not defined CDX_DIR goto :eof
if exist "%CDX_DIR%\" cd /d "%CDX_DIR%"
set "CDX_DIR="
goto :eof
:passthrough
cdx-bin %*
"#;

pub const SHELLS: &str = "bash, zsh, fish, powershell, cmd";

pub fn run(shell: &str) -> ExitCode {
    let script = match shell.to_ascii_lowercase().as_str() {
        "bash" | "zsh" | "sh" => POSIX.to_string(),
        "fish" => FISH.to_string(),
        "powershell" | "pwsh" => POWERSHELL.to_string(),
        "cmd" => CMD.replace('\n', "\r\n"),
        other => {
            eprintln!("cdx: unknown shell '{other}'. Supported: {SHELLS}");
            return ExitCode::from(2);
        }
    };
    print!("{script}");
    ExitCode::SUCCESS
}
