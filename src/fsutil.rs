//! Filesystem helpers: listing folders, hidden-file detection, drives, paths.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// List the sub-folders of `dir`, sorted case-insensitively.
/// Files are ignored; symlinks that point at folders are included.
pub fn list_subdirs(dir: &Path, show_hidden: bool) -> io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for entry in read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        // `fs::metadata` follows symlinks, so linked folders count as folders.
        let Ok(meta) = fs::metadata(&path) else { continue };
        if !meta.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if !show_hidden && is_hidden(&name, &entry, &meta) {
            continue;
        }
        out.push((name.to_string_lossy().into_owned(), path));
    }
    out.sort_by_cached_key(|(name, _)| name.to_lowercase());
    Ok(out)
}

/// `fs::read_dir`, with a second chance on macOS.
///
/// macOS blocks terminals from Desktop, Documents, Downloads and external
/// volumes until the user allows it, and after a "Don't Allow" it never asks
/// again by itself. If that is why we were refused, we clear the stored answer
/// for this terminal and folder and try once more, which makes macOS show its
/// prompt again. If the user declines again (or on Linux and Windows, which have
/// no such prompt) the original error is returned unchanged.
pub fn read_dir(path: &Path) -> io::Result<fs::ReadDir> {
    match fs::read_dir(path) {
        Err(e) if ASK_AGAIN.load(Ordering::Relaxed) && ask_again(path, &e) => fs::read_dir(path),
        other => other,
    }
}

/// Whether to re-ask the system after a macOS privacy refusal (the `macos_ask_again` setting).
static ASK_AGAIN: AtomicBool = AtomicBool::new(true);

/// The `macos_ask_again` setting; on by default, ignored outside macOS.
pub fn set_ask_again(on: bool) {
    ASK_AGAIN.store(on, Ordering::Relaxed);
}

/// macOS: if `err` is a privacy refusal for a folder we can re-prompt for, clear the stored
/// answer for this terminal and return true so the caller retries. Otherwise false.
#[cfg(target_os = "macos")]
fn ask_again(path: &Path, err: &io::Error) -> bool {
    use std::process::{Command, Stdio};
    // "Operation not permitted" (EPERM) is how a privacy denial looks;
    // ordinary file permissions give "Permission denied" (EACCES) instead.
    if err.raw_os_error() != Some(1) {
        return false;
    }
    let Some(service) = tcc_service(path, home_dir().as_deref()) else {
        return false;
    };
    // macOS sets this to the app the terminal belongs to (Terminal, iTerm2, ...).
    let Some(app) = std::env::var("__CFBundleIdentifier").ok().filter(|s| !s.is_empty()) else {
        return false;
    };
    Command::new("tccutil")
        .args(["reset", service, &app])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Outside macOS there is no such prompt: never retry.
#[cfg(not(target_os = "macos"))]
fn ask_again(_path: &Path, _err: &io::Error) -> bool {
    false
}

/// The macOS privacy category that guards `path`, if it is one we can re-prompt for.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn tcc_service(path: &Path, home: Option<&Path>) -> Option<&'static str> {
    if let Some(h) = home {
        for (dir, service) in [
            ("Desktop", "SystemPolicyDesktopFolder"),
            ("Documents", "SystemPolicyDocumentsFolder"),
            ("Downloads", "SystemPolicyDownloadsFolder"),
        ] {
            if path.starts_with(h.join(dir)) {
                return Some(service);
            }
        }
    }
    if path.starts_with("/Volumes") {
        return Some("SystemPolicyRemovableVolumes");
    }
    None
}

/// Unix: hidden means the name starts with a dot.
#[cfg(unix)]
fn is_hidden(name: &std::ffi::OsStr, _entry: &fs::DirEntry, _meta: &fs::Metadata) -> bool {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().first() == Some(&b'.')
}

/// Windows: hidden means the Hidden file attribute is set.
#[cfg(windows)]
fn is_hidden(_name: &std::ffi::OsStr, entry: &fs::DirEntry, meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    /// The Windows attribute bit for hidden files and folders.
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    let attrs = entry
        .metadata()
        .map(|m| m.file_attributes())
        .unwrap_or_else(|_| meta.file_attributes());
    attrs & FILE_ATTRIBUTE_HIDDEN != 0
}

/// Other systems: fall back to the dot-prefix rule.
#[cfg(not(any(unix, windows)))]
fn is_hidden(name: &std::ffi::OsStr, _entry: &fs::DirEntry, _meta: &fs::Metadata) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// Available drive roots (`C:\`, `D:\`, ...). Empty on non-Windows systems.
#[cfg(windows)]
pub fn list_drives() -> Vec<PathBuf> {
    extern "system" {
        /// Windows API: a bit mask of the drive letters in use (bit 0 = A:).
        fn GetLogicalDrives() -> u32;
    }
    // SAFETY: GetLogicalDrives takes no arguments and has no preconditions.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1u32 << i) != 0)
        .map(|i| PathBuf::from(format!("{}:\\", (b'A' + i) as char)))
        .collect()
}

/// There are no drive letters outside Windows.
#[cfg(not(windows))]
pub fn list_drives() -> Vec<PathBuf> {
    Vec::new()
}

/// Make a path absolute and free of `.`/`..`, without Windows' `\\?\` prefix.
pub fn normalize(path: &Path) -> io::Result<PathBuf> {
    let canon = fs::canonicalize(path)?;
    Ok(strip_verbatim(canon))
}

/// Windows: turn `\\?\C:\x` back into `C:\x` (and `\\?\UNC\s\x` into `\\s\x`).
#[cfg(windows)]
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest.to_string())
    } else {
        p
    }
}

/// Elsewhere paths need no cleaning.
#[cfg(not(windows))]
fn strip_verbatim(p: PathBuf) -> PathBuf {
    p
}

/// The home folder (`HOME`, or `USERPROFILE` on Windows) if it exists as a folder.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cdx-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    #[cfg(unix)]
    fn hides_dotfolders_by_default() {
        let root = scratch("hidden");
        fs::create_dir(root.join("Beta")).unwrap();
        fs::create_dir(root.join("alpha")).unwrap();
        fs::create_dir(root.join(".secret")).unwrap();
        fs::write(root.join("file.txt"), "x").unwrap();

        let names = |show| -> Vec<String> {
            list_subdirs(&root, show)
                .unwrap()
                .into_iter()
                .map(|(n, _)| n)
                .collect()
        };
        assert_eq!(names(false), vec!["alpha", "Beta"]);
        assert_eq!(names(true), vec![".secret", "alpha", "Beta"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn privacy_prompts_apply_to_protected_folders_only() {
        let home = Path::new("/Users/me");
        let svc = |p: &str| tcc_service(Path::new(p), Some(home));
        assert_eq!(svc("/Users/me/Documents"), Some("SystemPolicyDocumentsFolder"));
        assert_eq!(svc("/Users/me/Desktop/x/y"), Some("SystemPolicyDesktopFolder"));
        assert_eq!(svc("/Users/me/Downloads"), Some("SystemPolicyDownloadsFolder"));
        assert_eq!(svc("/Volumes/USB"), Some("SystemPolicyRemovableVolumes"));
        assert_eq!(svc("/Users/me/projects"), None);
        assert_eq!(svc("/Users/me/DocumentsOld"), None);
        assert_eq!(tcc_service(Path::new("/tmp"), None), None);
    }

    #[test]
    fn normalize_resolves_dotdot() {
        let root = scratch("norm");
        fs::create_dir(root.join("a")).unwrap();
        let weird = root.join("a").join("..").join("a");
        assert_eq!(
            normalize(&weird).unwrap(),
            normalize(&root.join("a")).unwrap()
        );
        let _ = fs::remove_dir_all(&root);
    }
}
