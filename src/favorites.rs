//! Favourite (bookmarked) folders and how they are stored.
//!
//! The file is plain text, one favourite per line, in slot order (line 1 is
//! key `1`, line 2 is key `2`, ...):
//!
//! ```text
//! <alias><TAB><path>
//! ```
//!
//! The alias may be empty, meaning "use the folder's own name".

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Number keys 1-9, so at most nine favourites.
pub const MAX: usize = 9;

#[derive(Clone, Debug, PartialEq)]
pub struct Favorite {
    pub path: PathBuf,
    pub alias: Option<String>,
    /// Set when the folder did not exist at load time. Never saved.
    pub missing: bool,
}

impl Favorite {
    pub fn new(path: PathBuf) -> Self {
        Favorite {
            path,
            alias: None,
            missing: false,
        }
    }

    /// What the user sees: the alias if there is one, else the folder's name.
    pub fn label(&self) -> String {
        match &self.alias {
            Some(a) if !a.is_empty() => a.clone(),
            _ => default_label(&self.path),
        }
    }
}

pub fn default_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// A path we can write to our one-line-per-favourite file without ambiguity.
pub fn is_storable(path: &Path) -> bool {
    match path.to_str() {
        Some(s) => !s.is_empty() && !s.contains(['\n', '\r']),
        None => false,
    }
}

// ------------------------------------------------------------ load and save

/// A missing file simply means "no favourites yet".
pub fn load_from(path: &Path) -> io::Result<Vec<Favorite>> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut favs = parse(&text);
    for f in &mut favs {
        f.missing = !f.path.is_dir();
    }
    Ok(favs)
}

/// Write via a temporary file and rename, so a crash cannot leave a half-written list.
pub fn save_to(path: &Path, favs: &[Favorite]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tsv.tmp");
    fs::write(&tmp, serialize(favs))?;
    fs::rename(&tmp, path)
}

pub fn parse(text: &str) -> Vec<Favorite> {
    let mut out: Vec<Favorite> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let Some((alias, path)) = line.split_once('\t') else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let path = PathBuf::from(path);
        if out.iter().any(|f| f.path == path) {
            continue;
        }
        let alias = alias.trim();
        out.push(Favorite {
            path,
            alias: (!alias.is_empty()).then(|| alias.to_string()),
            missing: false,
        });
        if out.len() == MAX {
            break;
        }
    }
    out
}

pub fn serialize(favs: &[Favorite]) -> String {
    let mut s = String::new();
    for f in favs {
        let alias: String = f
            .alias
            .as_deref()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        s.push_str(&alias);
        s.push('\t');
        s.push_str(&f.path.to_string_lossy());
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn fav(path: &str, alias: Option<&str>) -> Favorite {
        Favorite {
            path: PathBuf::from(path),
            alias: alias.map(String::from),
            missing: false,
        }
    }

    #[test]
    fn label_prefers_alias_then_folder_name() {
        assert_eq!(fav("/home/me/projects", None).label(), "projects");
        assert_eq!(fav("/home/me/projects", Some("Work")).label(), "Work");
        assert_eq!(fav("/home/me/projects", Some("")).label(), "projects");
        assert_eq!(fav("/", None).label(), "/");
    }

    #[test]
    fn roundtrip_keeps_order_aliases_and_spaces() {
        let favs = vec![
            fav("/a b/c", Some("My place")),
            fav("/plain", None),
            fav("/x/#hash", Some("#1")),
        ];
        assert_eq!(parse(&serialize(&favs)), favs);
    }

    #[test]
    fn parse_skips_garbage_and_duplicates_and_caps_at_nine() {
        let mut text = String::from("no tab on this line\n\t\n\t/dup\nalias\t/dup\n");
        for i in 0..20 {
            text.push_str(&format!("\t/p{i}\n"));
        }
        let favs = parse(&text);
        assert_eq!(favs.len(), MAX);
        assert_eq!(favs[0].path, PathBuf::from("/dup"));
        assert_eq!(favs[0].alias, None, "first occurrence wins");
    }

    #[test]
    fn parse_handles_windows_line_endings() {
        let favs = parse("Docs\tC:\\Users\\me\\Documents\r\n");
        assert_eq!(favs[0].path, PathBuf::from("C:\\Users\\me\\Documents"));
    }

    #[test]
    fn storable_rejects_newlines() {
        assert!(is_storable(Path::new("/ok/path")));
        assert!(!is_storable(Path::new("/bad\npath")));
    }

    #[test]
    fn save_and_load_roundtrip_on_disk() {
        let dir = env::temp_dir().join(format!("cdx-fav-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("nested").join("favorites.tsv");
        assert!(load_from(&file).unwrap().is_empty(), "missing file is fine");

        let favs = vec![fav("/one", Some("First")), fav("/two", None)];
        save_to(&file, &favs).unwrap();
        let back = load_from(&file).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].alias.as_deref(), Some("First"));
        assert!(back[0].missing, "/one does not exist, so it is flagged");
        let _ = fs::remove_dir_all(&dir);
    }
}
