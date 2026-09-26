//! Crates and their targets, from `cargo metadata --no-deps --offline`
//! on the root manifest and on each `[rust] extra_manifests` entry.
//! `--no-deps` reads manifests only: no resolution, no network, no
//! lockfile written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cargo_metadata::{MetadataCommand, Package, TargetKind as CargoTargetKind};

use crate::config::RustConfig;
use crate::error::RustFrontendError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateSpec {
    /// The package name as cargo spells it (`playwright-rs`).
    pub package: String,
    /// The name a `use` path starts with: the lib target's name when
    /// there is one (`playwright_rs`), else the package name with `-`
    /// as `_`.
    pub crate_name: String,
    pub technology: &'static str,
    pub manifest_path: PathBuf,
    pub manifest_dir: PathBuf,
    /// Sorted by kind then name.
    pub targets: Vec<TargetSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub kind: TargetKind,
    pub name: String,
    /// The file the module tree starts from (`src/lib.rs`, `tests/x.rs`).
    pub root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetKind {
    Lib,
    ProcMacro,
    Bin,
    Test,
    Bench,
    Example,
}

impl TargetKind {
    /// `None` for build scripts, which are not part of the model.
    fn from_cargo(kinds: &[CargoTargetKind]) -> Option<Self> {
        kinds.iter().find_map(|k| match k {
            CargoTargetKind::ProcMacro => Some(TargetKind::ProcMacro),
            CargoTargetKind::Lib
            | CargoTargetKind::RLib
            | CargoTargetKind::DyLib
            | CargoTargetKind::CDyLib
            | CargoTargetKind::StaticLib => Some(TargetKind::Lib),
            CargoTargetKind::Bin => Some(TargetKind::Bin),
            CargoTargetKind::Test => Some(TargetKind::Test),
            CargoTargetKind::Bench => Some(TargetKind::Bench),
            CargoTargetKind::Example => Some(TargetKind::Example),
            _ => None,
        })
    }
}

pub const TECHNOLOGY_LIBRARY: &str = "library crate";
pub const TECHNOLOGY_PROC_MACRO: &str = "proc-macro crate";
pub const TECHNOLOGY_BINARY: &str = "binary";
pub const TECHNOLOGY_TESTS: &str = "test crate";

impl CrateSpec {
    fn from_package(package: &Package) -> Self {
        let mut targets: Vec<TargetSpec> = package
            .targets
            .iter()
            .filter_map(|t| {
                TargetKind::from_cargo(&t.kind).map(|kind| TargetSpec {
                    kind,
                    name: t.name.clone(),
                    root: t.src_path.clone().into_std_path_buf(),
                })
            })
            .collect();
        targets.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));

        let lib = targets
            .iter()
            .find(|t| matches!(t.kind, TargetKind::Lib | TargetKind::ProcMacro));
        let crate_name = lib
            .map(|t| t.name.clone())
            .unwrap_or_else(|| package.name.replace('-', "_"));
        let technology = match lib.map(|t| t.kind) {
            Some(TargetKind::ProcMacro) => TECHNOLOGY_PROC_MACRO,
            Some(_) => TECHNOLOGY_LIBRARY,
            None if targets.iter().any(|t| t.kind == TargetKind::Bin) => TECHNOLOGY_BINARY,
            None => TECHNOLOGY_TESTS,
        };
        let manifest_path: PathBuf = package.manifest_path.clone().into_std_path_buf();
        let manifest_dir = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        Self {
            package: package.name.to_string(),
            crate_name,
            technology,
            manifest_path,
            manifest_dir,
            targets,
        }
    }
}

fn packages_of(manifest: &Path) -> Result<Vec<Package>, RustFrontendError> {
    MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .other_options(vec!["--offline".to_string()])
        .exec()
        .map(|m| m.packages)
        .map_err(|source| RustFrontendError::Metadata {
            manifest: manifest.to_path_buf(),
            source,
        })
}

/// Every crate the survey covers, sorted by crate name: the workspace at
/// `root`, then each extra manifest's packages. Only packages under the
/// root count: surveying one member of a larger workspace scopes the
/// model to that member (cargo reports every member regardless).
pub fn discover(root: &Path, config: &RustConfig) -> Result<Vec<CrateSpec>, RustFrontendError> {
    let root_canonical = root
        .canonicalize()
        .map_err(|source| RustFrontendError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    let mut manifests = vec![root.join("Cargo.toml")];
    for extra in &config.extra_manifests {
        let path = root.join(extra);
        if !path.is_file() {
            return Err(RustFrontendError::ExtraManifestMissing { manifest: path });
        }
        manifests.push(path);
    }

    let mut by_name: BTreeMap<String, CrateSpec> = BTreeMap::new();
    for manifest in &manifests {
        for package in packages_of(manifest)? {
            let spec = CrateSpec::from_package(&package);
            let under_root = spec
                .manifest_dir
                .canonicalize()
                .is_ok_and(|dir| dir.starts_with(&root_canonical));
            if !under_root {
                continue;
            }
            if let Some(first) = by_name.get(&spec.crate_name) {
                if first.manifest_path == spec.manifest_path {
                    continue;
                }
                return Err(RustFrontendError::DuplicateCrateName {
                    crate_name: spec.crate_name,
                    manifests: [first.manifest_path.clone(), spec.manifest_path],
                });
            }
            by_name.insert(spec.crate_name.clone(), spec);
        }
    }
    Ok(by_name.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_script_is_not_a_target() {
        assert_eq!(
            TargetKind::from_cargo(&[CargoTargetKind::CustomBuild]),
            None
        );
    }

    #[test]
    fn every_library_flavor_is_lib_and_proc_macro_is_its_own_kind() {
        for k in [
            CargoTargetKind::Lib,
            CargoTargetKind::RLib,
            CargoTargetKind::DyLib,
            CargoTargetKind::CDyLib,
            CargoTargetKind::StaticLib,
        ] {
            assert_eq!(TargetKind::from_cargo(&[k]), Some(TargetKind::Lib));
        }
        assert_eq!(
            TargetKind::from_cargo(&[CargoTargetKind::ProcMacro]),
            Some(TargetKind::ProcMacro)
        );
    }

    #[test]
    fn bin_test_bench_and_example_map_to_themselves() {
        assert_eq!(
            TargetKind::from_cargo(&[CargoTargetKind::Bin]),
            Some(TargetKind::Bin)
        );
        assert_eq!(
            TargetKind::from_cargo(&[CargoTargetKind::Test]),
            Some(TargetKind::Test)
        );
        assert_eq!(
            TargetKind::from_cargo(&[CargoTargetKind::Bench]),
            Some(TargetKind::Bench)
        );
        assert_eq!(
            TargetKind::from_cargo(&[CargoTargetKind::Example]),
            Some(TargetKind::Example)
        );
    }

    #[test]
    fn an_extra_manifest_that_does_not_exist_is_an_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let config = RustConfig {
            extra_manifests: vec![PathBuf::from("nowhere/Cargo.toml")],
            ..Default::default()
        };
        match discover(dir.path(), &config) {
            Err(RustFrontendError::ExtraManifestMissing { manifest }) => {
                assert_eq!(manifest, dir.path().join("nowhere/Cargo.toml"));
            }
            other => panic!("expected ExtraManifestMissing, got {other:?}"),
        }
    }

    #[test]
    fn a_root_without_a_manifest_is_a_metadata_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        match discover(dir.path(), &RustConfig::default()) {
            Err(RustFrontendError::Metadata { manifest, .. }) => {
                assert_eq!(manifest, dir.path().join("Cargo.toml"));
            }
            other => panic!("expected Metadata, got {other:?}"),
        }
    }
}
