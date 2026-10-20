//! `cdx -n`: open a folder in a new terminal window instead of changing the
//! current shell's directory.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Open `dir` in a new terminal window. `command` is the user's own template
/// (`new_window.command` in the config); without one, a built-in choice is made.
pub fn open(dir: &Path, command: Option<&str>) -> io::Result<()> {
    match command.map(split_command) {
        None => platform_open(dir),
        Some(Err(e)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("new_window.command: {e}"),
        )),
        Some(Ok(words)) => match words.split_first() {
            None => platform_open(dir), // blank template: same as none
            Some((program, rest)) => {
                let path = dir.to_string_lossy();
                let args: Vec<String> = rest.iter().map(|w| w.replace("{path}", &path)).collect();
                spawn(Command::new(program).args(&args).current_dir(dir)).map_err(|e| {
                    io::Error::new(e.kind(), format!("could not run '{program}': {e}"))
                })
            }
        },
    }
}

/// Split a command line into words. Single or double quotes group words; there are
/// no escape characters, so Windows paths can be written as they are.
fn split_command(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            (None, c) => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err("unterminated quote".into());
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// Start `cmd` without waiting for it, detached from our terminal so that
/// closing this window does not close the new one.
fn spawn(cmd: &mut Command) -> io::Result<()> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_| ())
}

/// macOS: open Terminal (or iTerm2 when that is what you are in) at `dir`.
#[cfg(target_os = "macos")]
fn platform_open(dir: &Path) -> io::Result<()> {
    // Use the same terminal app you are in now, when we know it.
    let app = match std::env::var("TERM_PROGRAM").as_deref() {
        Ok("iTerm.app") => "iTerm",
        _ => "Terminal",
    };
    let status = Command::new("open")
        .arg("-a")
        .arg(app)
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("`open -a {app}` failed")))
    }
}

/// Windows: Windows Terminal if installed, otherwise a plain `cmd` window.
#[cfg(windows)]
fn platform_open(dir: &Path) -> io::Result<()> {
    // Windows Terminal if it is installed, otherwise a plain console window.
    match spawn(Command::new("wt").arg("-d").arg(dir)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        other => return other,
    }
    spawn(
        Command::new("cmd")
            .args(["/C", "start", "cdx", "/D"])
            .arg(dir)
            .arg("cmd.exe"),
    )
}

/// Linux and other Unix: the first installed terminal from a list of common ones.
#[cfg(all(unix, not(target_os = "macos")))]
fn platform_open(dir: &Path) -> io::Result<()> {
    use std::ffi::OsString;

    let has = |v: &str| std::env::var_os(v).is_some_and(|s| !s.is_empty());
    if !has("DISPLAY") && !has("WAYLAND_DISPLAY") {
        return Err(io::Error::other(
            "no graphical session found (DISPLAY and WAYLAND_DISPLAY are not set)",
        ));
    }

    let d: OsString = dir.as_os_str().to_os_string();
    let joined = |flag: &str| {
        let mut s = OsString::from(flag);
        s.push(&d);
        s
    };
    let flag = |f: &str| OsString::from(f);

    // First one that is installed wins. The ones at the end have no folder
    // option, so they simply inherit it from `current_dir`.
    let tries: Vec<(&str, Vec<OsString>)> = vec![
        ("gnome-terminal", vec![joined("--working-directory=")]),
        ("konsole", vec![flag("--workdir"), d.clone()]),
        ("xfce4-terminal", vec![joined("--working-directory=")]),
        ("mate-terminal", vec![joined("--working-directory=")]),
        ("tilix", vec![joined("--working-directory=")]),
        ("terminator", vec![joined("--working-directory=")]),
        ("lxterminal", vec![joined("--working-directory=")]),
        ("kitty", vec![flag("--directory"), d.clone()]),
        ("alacritty", vec![flag("--working-directory"), d.clone()]),
        ("wezterm", vec![flag("start"), flag("--cwd"), d.clone()]),
        ("foot", vec![flag("--working-directory"), d.clone()]),
        ("x-terminal-emulator", vec![]),
        ("xterm", vec![]),
    ];
    for (program, args) in &tries {
        match spawn(Command::new(program).args(args).current_dir(dir)) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
    }
    let names: Vec<&str> = tries.iter().map(|(p, _)| *p).collect();
    Err(io::Error::other(format!(
        "no supported terminal found (tried {})",
        names.join(", ")
    )))
}

/// Other systems cannot open a window.
#[cfg(not(any(unix, windows)))]
fn platform_open(_dir: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening a new window is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_split_into_words_with_quotes_grouping() {
        assert_eq!(
            split_command("kitty --directory {path}").unwrap(),
            ["kitty", "--directory", "{path}"]
        );
        assert_eq!(
            split_command(r#"  wt -d "{path}"   --title 'my shell' "" "#).unwrap(),
            ["wt", "-d", "{path}", "--title", "my shell", ""]
        );
        assert_eq!(
            split_command(r"C:\Tools\term.exe {path}").unwrap(),
            [r"C:\Tools\term.exe", "{path}"],
            "backslashes are not escapes"
        );
        assert!(split_command("").unwrap().is_empty());
        assert!(split_command("kitty 'oops").is_err());
    }

    #[test]
    fn a_broken_template_is_reported_not_run() {
        let e = open(Path::new("."), Some("kitty \"oops")).unwrap_err();
        assert!(e.to_string().contains("unterminated quote"), "{e}");
    }

    #[test]
    fn a_missing_program_names_itself_in_the_error() {
        let e = open(Path::new("."), Some("cdx-no-such-terminal-xyz {path}")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
        assert!(e.to_string().contains("cdx-no-such-terminal-xyz"), "{e}");
    }
}
