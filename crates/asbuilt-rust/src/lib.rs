//! Rust front-end for asbuilt.
//!
//! Reads a Cargo workspace and produces the language-agnostic model
//! `asbuilt-core` emits: each crate a container, each module a
//! component, each module-to-module reference a relation labeled with
//! the item names it references. The stages, one module each:
//! [`discover`] finds crates and targets with `cargo metadata`,
//! [`walk`] follows `mod` declarations from each target root into a
//! module tree, [`visit`] collects what each file references,
//! [`resolve`] turns those paths into modules and items, and
//! [`aggregate`] makes relations of them. Only the first two touch the
//! filesystem, through [`source::FileSource`]. [`analyze`] runs them
//! in order; [`RustFrontend`] is the same behind the core's trait.

pub mod aggregate;
pub mod config;
pub mod discover;
pub mod error;
pub mod resolve;
pub mod source;
pub mod visit;
pub mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use asbuilt_core::model::{Element, ElementKind, Id, Model};
use asbuilt_core::{Config, Frontend};

pub use config::RustConfig;
pub use error::RustFrontendError;

use aggregate::{EXAMPLES_COMPONENT, TESTS_COMPONENT, element_id};
use discover::{CrateSpec, TargetKind, discover};
use resolve::{Location, ResolveTree, TargetKey, TargetRole};
use source::FsSource;
use visit::ModuleFacts;

/// The tag on a bin target's component when the crate also has a lib.
pub const BIN_TAG: &str = "bin";

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

/// The core's view of this front-end.
pub struct RustFrontend;

impl Frontend for RustFrontend {
    fn name(&self) -> &str {
        "rust"
    }

    fn detect(&self, root: &Path) -> bool {
        detect(root)
    }

    fn analyze(&self, root: &Path, config: &Config) -> asbuilt_core::Result<Model> {
        let rust = RustConfig::from_config(config)?;
        analyze(root, &rust).map_err(|source| asbuilt_core::Error::Frontend {
            frontend: "rust".into(),
            source: Box::new(source),
        })
    }
}

/// `path` canonicalized, with the Windows verbatim prefix dropped so
/// `strip_prefix` against the root works.
fn canonical(path: &Path) -> Result<PathBuf, RustFrontendError> {
    let canonical = path
        .canonicalize()
        .map_err(|source| RustFrontendError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let text = canonical
        .to_str()
        .ok_or_else(|| RustFrontendError::NonUtf8Path {
            path: canonical.clone(),
        })?;
    Ok(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text)))
}

/// `path` relative to `root`, `/`-separated on every platform, `.` for
/// the root itself. `path` must exist; a path that is not under the
/// root is an error.
fn relative(root: &Path, path: &Path) -> Result<String, RustFrontendError> {
    let path = canonical(path)?;
    let rel = path
        .strip_prefix(root)
        .map_err(|_| RustFrontendError::PathOutsideRoot {
            path: path.clone(),
            root: root.to_path_buf(),
        })?;
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| RustFrontendError::NonUtf8Path { path: path.clone() })?
                    .to_string(),
            ),
            _ => {
                return Err(RustFrontendError::PathOutsideRoot {
                    path: path.clone(),
                    root: root.to_path_buf(),
                });
            }
        }
    }
    if parts.is_empty() {
        return Ok(".".to_string());
    }
    Ok(parts.join("/"))
}

/// A target's role: the lib (or a bin-only package's default bin) is
/// the crate's main namespace; a bin beside a lib, or a bin that is not
/// the package's default, is its own component; tests, benches and
/// examples collapse per crate.
fn role_of(
    krate: &CrateSpec,
    target: &discover::TargetSpec,
    config: &RustConfig,
) -> Option<TargetRole> {
    let has_lib = krate
        .targets
        .iter()
        .any(|t| matches!(t.kind, TargetKind::Lib | TargetKind::ProcMacro));
    match target.kind {
        TargetKind::Lib | TargetKind::ProcMacro => Some(TargetRole::Main),
        TargetKind::Bin => {
            if !has_lib && target.name.replace('-', "_") == krate.crate_name {
                Some(TargetRole::Main)
            } else {
                Some(TargetRole::Bin(target.name.clone()))
            }
        }
        TargetKind::Test | TargetKind::Bench => config
            .include_tests
            .then(|| TargetRole::Tests(target.name.clone())),
        TargetKind::Example => config
            .include_examples
            .then(|| TargetRole::Examples(target.name.clone())),
    }
}

fn element(id: Id, kind: ElementKind, title: String) -> Element {
    Element {
        id,
        kind,
        title,
        description: None,
        technology: None,
        path: None,
        tags: vec![],
    }
}

/// Survey the workspace at `root` into a model.
pub fn analyze(root: &Path, config: &RustConfig) -> Result<Model, RustFrontendError> {
    let root_canonical = canonical(root)?;
    let crates = discover(root, config)?;

    let mut tree = ResolveTree::default();
    let mut facts: BTreeMap<Location, ModuleFacts> = BTreeMap::new();
    let mut elements: Vec<Element> = Vec::new();
    let mut ids: BTreeSet<Id> = BTreeSet::new();

    /// Push an element unless its id is already taken: a bin named like
    /// a module, or a module named `tests` or `examples`.
    fn add(
        elements: &mut Vec<Element>,
        ids: &mut BTreeSet<Id>,
        element: Element,
        package: &str,
    ) -> Result<(), RustFrontendError> {
        if !ids.insert(element.id.clone()) {
            return Err(RustFrontendError::TargetNameCollision {
                package: package.to_string(),
                name: element.id.last().cloned().unwrap_or_default(),
            });
        }
        elements.push(element);
        Ok(())
    }

    for krate in &crates {
        let crate_id = vec![krate.crate_name.clone()];
        let mut container = element(
            crate_id.clone(),
            ElementKind::Container,
            krate.package.clone(),
        );
        container.technology = Some(krate.technology.to_string());
        container.path = Some(relative(&root_canonical, &krate.manifest_dir)?);
        let container_index = elements.len();
        add(&mut elements, &mut ids, container, &krate.package)?;
        let mut synthetic: BTreeSet<Id> = BTreeSet::new();

        for target in &krate.targets {
            let Some(role) = role_of(krate, target, config) else {
                continue;
            };
            let key = TargetKey {
                crate_name: krate.crate_name.clone(),
                role: role.clone(),
            };
            let module_tree = walk::load_tree(&target.root, &FsSource)?;
            tree.add_target(key.clone(), role == TargetRole::Main);

            for (path, node) in &module_tree.modules {
                tree.module_mut(&key, path)
                    .children
                    .extend(node.children.iter().cloned());
            }
            for file in &module_tree.files {
                for (module, collected) in visit::collect(&file.ast, &file.module) {
                    let node = tree.module_mut(&key, &module);
                    node.defines.extend(collected.defines.iter().cloned());
                    for u in &collected.uses {
                        if u.glob {
                            node.globs.push((u.path.clone(), u.reexport));
                        } else if u.alias != "_" {
                            node.imports.insert(u.alias.clone(), u.path.clone());
                            if u.reexport {
                                node.reexports.insert(u.alias.clone());
                            }
                        }
                    }
                    let entry = facts
                        .entry(Location {
                            target: key.clone(),
                            module: module.clone(),
                        })
                        .or_default();
                    entry.defines.extend(collected.defines);
                    entry.uses.extend(collected.uses);
                    entry.refs.extend(collected.refs);
                }
            }

            match &role {
                TargetRole::Main | TargetRole::Bin(_) => {
                    for (path, node) in &module_tree.modules {
                        let loc = Location {
                            target: key.clone(),
                            module: path.clone(),
                        };
                        let id = element_id(&loc);
                        if id == crate_id {
                            elements[container_index].description = node.doc.clone();
                            continue;
                        }
                        let title = id.last().cloned().unwrap_or_default();
                        let mut component = element(id, ElementKind::Component, title);
                        component.description = node.doc.clone();
                        component.path = Some(relative(&root_canonical, &node.file)?);
                        if path.is_empty() {
                            component.tags.push(BIN_TAG.to_string());
                        }
                        add(&mut elements, &mut ids, component, &krate.package)?;
                    }
                }
                TargetRole::Tests(_) | TargetRole::Examples(_) => {
                    let loc = Location {
                        target: key.clone(),
                        module: vec![],
                    };
                    let id = element_id(&loc);
                    if !synthetic.insert(id.clone()) {
                        continue;
                    }
                    let name = if matches!(role, TargetRole::Tests(_)) {
                        TESTS_COMPONENT
                    } else {
                        EXAMPLES_COMPONENT
                    };
                    let mut component = element(id, ElementKind::Component, name.to_string());
                    component.tags.push(name.to_string());
                    let dir = relative(&root_canonical, &krate.manifest_dir)?;
                    component.path = Some(if dir == "." {
                        name.to_string()
                    } else {
                        format!("{dir}/{name}")
                    });
                    add(&mut elements, &mut ids, component, &krate.package)?;
                }
            }
        }
    }

    let relations = aggregate::relations(&tree, &facts);
    let mut model = Model {
        elements,
        relations,
        ..Default::default()
    };
    model.normalize();
    Ok(model)
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
        assert!(RustFrontend.detect(dir.path()));
        assert_eq!(RustFrontend.name(), "rust");
    }

    #[test]
    fn a_directory_without_a_cargo_manifest_is_not_detected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!detect(dir.path()));
        assert!(!RustFrontend.detect(dir.path()));
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

    #[test]
    fn a_relative_path_is_slash_separated_and_the_root_itself_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("a/b/c.rs"), "").unwrap();
        let root = canonical(dir.path()).unwrap();
        assert_eq!(
            relative(&root, &dir.path().join("a/b/c.rs")).unwrap(),
            "a/b/c.rs"
        );
        assert_eq!(relative(&root, dir.path()).unwrap(), ".");
    }

    #[test]
    fn a_path_outside_the_root_is_an_error_naming_both() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("x.rs"), "").unwrap();
        let root_c = canonical(root.path()).unwrap();
        match relative(&root_c, &other.path().join("x.rs")) {
            Err(RustFrontendError::PathOutsideRoot { path, root: r }) => {
                assert!(path.ends_with("x.rs"));
                assert_eq!(r, root_c);
            }
            other => panic!("expected PathOutsideRoot, got {other:?}"),
        }
    }

    fn package_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"server\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        for (name, text) in files {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn a_bin_named_like_a_lib_module_is_a_collision_naming_both() {
        let dir = package_with(&[
            ("src/lib.rs", "pub mod cli;"),
            ("src/cli.rs", ""),
            ("src/bin/cli.rs", "fn main() {}"),
        ]);
        match analyze(dir.path(), &RustConfig::default()) {
            Err(RustFrontendError::TargetNameCollision { package, name }) => {
                assert_eq!((package.as_str(), name.as_str()), ("server", "cli"));
            }
            other => panic!("expected TargetNameCollision, got {other:?}"),
        }
    }

    #[test]
    fn a_lib_module_named_tests_collides_with_the_tests_component() {
        let dir = package_with(&[
            ("src/lib.rs", "pub mod tests;"),
            ("src/tests.rs", ""),
            ("tests/it.rs", ""),
        ]);
        match analyze(dir.path(), &RustConfig::default()) {
            Err(RustFrontendError::TargetNameCollision { name, .. }) => assert_eq!(name, "tests"),
            other => panic!("expected TargetNameCollision, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_root_is_an_io_error_naming_it() {
        match analyze(Path::new("/no/such/root"), &RustConfig::default()) {
            Err(RustFrontendError::Io { path, .. }) => {
                assert_eq!(path, PathBuf::from("/no/such/root"))
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
