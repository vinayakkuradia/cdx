//! `cdx -n`: open a folder in a new terminal window instead of changing the
//! current shell's directory.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn open(dir: &Path) -> io::Result<()> {
    platform_open(dir)
}

/// Start `cmd` without waiting for it, detached from our terminal so that
/// closing this window does not close the new one.
#[allow(dead_code)] // every platform uses it, but a cfg'd-out one might not
fn spawn(cmd: &mut Command) -> io::Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_| ())
}

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

#[cfg(not(any(unix, windows)))]
fn platform_open(_dir: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening a new window is not supported on this platform",
    ))
}
