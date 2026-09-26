//! Rust front-end for asbuilt.
//!
//! Reads a Cargo workspace and produces the language-agnostic model
//! `asbuilt-core` emits: each crate a container, each module a
//! component, each module-to-module reference a relation labeled with
//! the item names it references. This crate is the detection half so
//! far; the survey lands in the next commits.

use std::path::Path;

/// Whether `root` is a code base this front-end can survey: it has a
/// `Cargo.toml` file at its top level.
///
/// ```
/// use std::path::Path;
///
/// assert!(!asbuilt_rust::detect(Path::new("/no/such/directory")));
/// ```
pub fn detect(root: &Path) -> bool {
    root.join("Cargo.toml").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_directory_with_a_cargo_manifest_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert!(detect(dir.path()));
    }

    #[test]
    fn a_directory_without_a_cargo_manifest_is_not_detected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!detect(dir.path()));
    }

    #[test]
    fn a_manifest_that_is_a_directory_does_not_count() {
        // `is_file`, not `exists`: a directory named Cargo.toml is not a
        // manifest cargo could read.
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("Cargo.toml")).unwrap();
        assert!(!detect(dir.path()));
    }

    #[test]
    fn a_manifest_in_a_subdirectory_does_not_count_for_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("member")).unwrap();
        fs::write(dir.path().join("member/Cargo.toml"), "[package]\n").unwrap();
        assert!(!detect(dir.path()));
    }
}
