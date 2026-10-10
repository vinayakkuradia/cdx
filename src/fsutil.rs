//! Filesystem helpers: listing folders, hidden-file detection, drives, paths.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// List the sub-folders of `dir`, sorted case-insensitively.
/// Files are ignored; symlinks that point at folders are included.
pub fn list_subdirs(dir: &Path, show_hidden: bool) -> io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
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

#[cfg(unix)]
fn is_hidden(name: &std::ffi::OsStr, _entry: &fs::DirEntry, _meta: &fs::Metadata) -> bool {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().first() == Some(&b'.')
}

#[cfg(windows)]
fn is_hidden(_name: &std::ffi::OsStr, entry: &fs::DirEntry, meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    let attrs = entry
        .metadata()
        .map(|m| m.file_attributes())
        .unwrap_or_else(|_| meta.file_attributes());
    attrs & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(any(unix, windows)))]
fn is_hidden(name: &std::ffi::OsStr, _entry: &fs::DirEntry, _meta: &fs::Metadata) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// Available drive roots (`C:\`, `D:\`, ...). Empty on non-Windows systems.
#[cfg(windows)]
pub fn list_drives() -> Vec<PathBuf> {
    extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    // SAFETY: GetLogicalDrives takes no arguments and has no preconditions.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1u32 << i) != 0)
        .map(|i| PathBuf::from(format!("{}:\\", (b'A' + i) as char)))
        .collect()
}

#[cfg(not(windows))]
pub fn list_drives() -> Vec<PathBuf> {
    Vec::new()
}

/// Make a path absolute and free of `.`/`..`, without Windows' `\\?\` prefix.
pub fn normalize(path: &Path) -> io::Result<PathBuf> {
    let canon = fs::canonicalize(path)?;
    Ok(strip_verbatim(canon))
}

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

#[cfg(not(windows))]
fn strip_verbatim(p: PathBuf) -> PathBuf {
    p
}

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
