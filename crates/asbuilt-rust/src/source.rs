//! Where source text comes from. The walk reads through this trait so
//! its tests run over an in-memory map and never touch a disk.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub trait FileSource {
    /// The file's text with line endings normalized to `\n`.
    fn read(&self, path: &Path) -> io::Result<String>;
    fn exists(&self, path: &Path) -> bool;
}

/// `\r\n` to `\n`, so doc text and spans are the same on every platform.
pub fn normalize_newlines(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n")
    } else {
        text
    }
}

/// The real filesystem.
pub struct FsSource;

impl FileSource for FsSource {
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path).map(normalize_newlines)
    }

    fn exists(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// An in-memory tree for tests: path to text.
#[derive(Debug, Default)]
pub struct MapSource(pub BTreeMap<PathBuf, String>);

impl MapSource {
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self(
            files
                .iter()
                .map(|(p, t)| (PathBuf::from(p), t.to_string()))
                .collect(),
        )
    }
}

impl FileSource for MapSource {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.0
            .get(path)
            .map(|t| normalize_newlines(t.clone()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.0.contains_key(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_becomes_lf_and_lone_lf_is_untouched() {
        assert_eq!(normalize_newlines("a\r\nb\n".into()), "a\nb\n");
        assert_eq!(normalize_newlines("a\nb".into()), "a\nb");
    }

    #[test]
    fn a_map_source_reads_what_it_holds_and_reports_the_rest_as_not_found() {
        let src = MapSource::new(&[("src/lib.rs", "mod a;\r\n")]);
        assert_eq!(src.read(Path::new("src/lib.rs")).unwrap(), "mod a;\n");
        assert!(src.exists(Path::new("src/lib.rs")));
        assert!(!src.exists(Path::new("src/a.rs")));
        let err = src.read(Path::new("src/a.rs")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn the_filesystem_source_reads_a_real_file_and_normalizes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rs");
        std::fs::write(&path, "//! Doc.\r\n").unwrap();
        assert_eq!(FsSource.read(&path).unwrap(), "//! Doc.\n");
        assert!(FsSource.exists(&path));
        assert!(!FsSource.exists(dir.path()));
    }
}
