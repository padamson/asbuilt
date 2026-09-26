//! What the Rust front-end can get wrong, each naming the path or
//! module involved.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RustFrontendError {
    #[error("cargo metadata for {manifest}: {source}")]
    Metadata {
        manifest: PathBuf,
        #[source]
        source: cargo_metadata::Error,
    },

    #[error("[rust] extra_manifests: {manifest} does not exist")]
    ExtraManifestMissing { manifest: PathBuf },

    /// Two packages (a member and an extra manifest, say) whose crate
    /// names coincide, so a `use` of that name could mean either.
    #[error("two packages have the crate name `{crate_name}`: {} and {}", .manifests[0].display(), .manifests[1].display())]
    DuplicateCrateName {
        crate_name: String,
        manifests: [PathBuf; 2],
    },

    /// `mod x;` with neither `x.rs` nor `x/mod.rs` where rustc would look.
    #[error("{declared_in}: module `{module}` has no file; tried {}", .tried.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" and "))]
    ModuleFileNotFound {
        declared_in: PathBuf,
        module: String,
        tried: Vec<PathBuf>,
    },

    /// Both `x.rs` and `x/mod.rs` exist (rustc E0761).
    #[error("{declared_in}: module `{module}` has two files: {} and {}", .candidates[0].display(), .candidates[1].display())]
    AmbiguousModuleFile {
        declared_in: PathBuf,
        module: String,
        candidates: [PathBuf; 2],
    },

    #[error("{file}: {source}")]
    Parse {
        file: PathBuf,
        #[source]
        source: syn::Error,
    },

    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not UTF-8")]
    NonUtf8Path { path: PathBuf },

    #[error("{path} is outside the surveyed root {root}")]
    PathOutsideRoot { path: PathBuf, root: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_module_file_names_the_declaring_file_the_module_and_every_candidate() {
        let msg = RustFrontendError::ModuleFileNotFound {
            declared_in: "src/lib.rs".into(),
            module: "gone".into(),
            tried: vec!["src/gone.rs".into(), "src/gone/mod.rs".into()],
        }
        .to_string();
        for part in ["src/lib.rs", "`gone`", "src/gone.rs", "src/gone/mod.rs"] {
            assert!(msg.contains(part), "{msg}");
        }
    }

    #[test]
    fn a_duplicate_crate_name_names_both_manifests() {
        let msg = RustFrontendError::DuplicateCrateName {
            crate_name: "site".into(),
            manifests: ["a/Cargo.toml".into(), "b/Cargo.toml".into()],
        }
        .to_string();
        assert!(
            msg.contains("`site`") && msg.contains("a/Cargo.toml") && msg.contains("b/Cargo.toml"),
            "{msg}"
        );
    }
}
