//! Remembers the last folder each file picker was used in - File A (base)
//! and File B (donor) separately, since the two are typically kept in
//! different but consistent places - so each dialog reopens where that side
//! was last picked from. Persisted as a tiny `key=path` text file under the
//! XDG config directory.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecentDirs {
    pub base: Option<PathBuf>,
    pub donor: Option<PathBuf>,
}

impl RecentDirs {
    /// Loads the saved folders, or empty defaults if there is no file yet
    /// (or it can't be read - this is a convenience, never an error).
    pub fn load() -> Self {
        config_file().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| Self::parse(&s)).unwrap_or_default()
    }

    /// Best-effort write; a failure only means the next launch starts in
    /// the dialog's default folder.
    pub fn save(&self) {
        let Some(path) = config_file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.serialize());
    }

    /// Records the folder containing `file` for the base (File A) or donor
    /// (File B) side. Returns true if the stored folder changed.
    pub fn remember(&mut self, file: &Path, is_base: bool) -> bool {
        let Some(dir) = file.parent().filter(|d| !d.as_os_str().is_empty()) else { return false };
        let slot = if is_base { &mut self.base } else { &mut self.donor };
        if slot.as_deref() == Some(dir) {
            return false;
        }
        *slot = Some(dir.to_path_buf());
        true
    }

    /// The folder to open the picker in, if it still exists.
    pub fn start_dir(&self, is_base: bool) -> Option<PathBuf> {
        let dir = if is_base { &self.base } else { &self.donor };
        dir.clone().filter(|d| d.is_dir())
    }

    fn parse(contents: &str) -> Self {
        let mut dirs = Self::default();
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            if value.is_empty() {
                continue;
            }
            match key.trim() {
                "base" => dirs.base = Some(PathBuf::from(value)),
                "donor" => dirs.donor = Some(PathBuf::from(value)),
                _ => {}
            }
        }
        dirs
    }

    fn serialize(&self) -> String {
        let mut out = String::new();
        for (key, dir) in [("base", &self.base), ("donor", &self.donor)] {
            // Paths that can't round-trip through a line-based UTF-8 file
            // are simply not remembered.
            if let Some(s) = dir.as_ref().and_then(|d| d.to_str()).filter(|s| !s.contains('\n')) {
                out.push_str(&format!("{key}={s}\n"));
            }
        }
        out
    }
}

fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("mediamerger").join("recent_dirs"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_parse_round_trip_keeps_sides_separate() {
        let dirs = RecentDirs { base: Some("/media/movies".into()), donor: Some("/media/rips/audio".into()) };
        assert_eq!(RecentDirs::parse(&dirs.serialize()), dirs);
    }

    #[test]
    fn parse_ignores_unknown_and_malformed_lines() {
        let dirs = RecentDirs::parse("garbage\nfoo=/x\nbase=/a=b\ndonor=\n");
        assert_eq!(dirs.base, Some(PathBuf::from("/a=b")));
        assert_eq!(dirs.donor, None);
    }

    #[test]
    fn remember_updates_only_the_given_side() {
        let mut dirs = RecentDirs::default();
        assert!(dirs.remember(Path::new("/media/movies/Film.mkv"), true));
        assert!(dirs.remember(Path::new("/media/rips/Film.mkv"), false));
        assert!(!dirs.remember(Path::new("/media/movies/Other.mkv"), true));
        assert_eq!(dirs.base, Some(PathBuf::from("/media/movies")));
        assert_eq!(dirs.donor, Some(PathBuf::from("/media/rips")));
    }

    #[test]
    fn start_dir_skips_folders_that_no_longer_exist() {
        let dirs = RecentDirs { base: Some("/definitely/not/a/real/dir".into()), donor: Some(std::env::temp_dir()) };
        assert_eq!(dirs.start_dir(true), None);
        assert_eq!(dirs.start_dir(false), Some(std::env::temp_dir()));
    }
}
