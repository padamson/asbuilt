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

#![doc(
    html_logo_url = "https://padamson.github.io/asbuilt/mark.svg",
    html_favicon_url = "https://padamson.github.io/asbuilt/mark.svg"
)]

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

    fn noun(&self, kind: &str) -> Option<&'static str> {
        match kind {
            "container" => Some("crate"),
            "component" => Some("module"),
            _ => None,
        }
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

/// The deepest directory that holds every one of `files`; `None` for no
/// files, or files with no ancestor in common.
fn common_dir(files: &[PathBuf]) -> Option<PathBuf> {
    let (first, rest) = files.split_first()?;
    let mut dir = first.parent()?.to_path_buf();
    for file in rest {
        while !file.starts_with(&dir) {
            dir = dir.parent()?.to_path_buf();
        }
    }
    Some(dir)
}

/// A target's role: the lib (or a bin-only package's default bin) is
/// the crate's main namespace; a bin beside a lib, or a bin that is not
/// the package's default, is its own element (kind `bin`, modules under
/// it); tests, benches and examples collapse per crate.
fn role_of(
    krate: &CrateSpec,
    target: &discover::TargetSpec,
    config: &RustConfig,
) -> Option<TargetRole> {
    match target.kind {
        TargetKind::Lib | TargetKind::ProcMacro => Some(TargetRole::Main),
        TargetKind::Bin => {
            if krate.lib.is_none() && target.name.replace('-', "_") == krate.crate_name {
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
        let crate_dir = canonical(&krate.manifest_dir)?;
        container.path = Some(relative(&root_canonical, &crate_dir)?);
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
                        // A bin's root is its own kind and tagged too; its
                        // submodules are components like the lib's.
                        if path.is_empty() {
                            component.kind = ElementKind::Bin;
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
                    let (name, kind) = if matches!(role, TargetRole::Tests(_)) {
                        (TESTS_COMPONENT, ElementKind::Tests)
                    } else {
                        (EXAMPLES_COMPONENT, ElementKind::Examples)
                    };
                    let mut component = element(id, kind, name.to_string());
                    component.tags.push(name.to_string());
                    let kinds: &[TargetKind] = if matches!(role, TargetRole::Tests(_)) {
                        &[TargetKind::Test, TargetKind::Bench]
                    } else {
                        &[TargetKind::Example]
                    };
                    let folded = krate
                        .targets
                        .iter()
                        .filter(|t| kinds.contains(&t.kind))
                        .map(|t| canonical(&t.root))
                        .collect::<Result<Vec<_>, _>>()?;
                    // The deepest directory holding every target folded in,
                    // compared canonical because cargo reports a custom path
                    // as written (`member/../shared/it.rs`): `tests` for the
                    // usual layout, `benches` for benches alone, the crate
                    // itself when tests and benches are siblings. Nothing
                    // above the surveyed root can be named, so a target out
                    // there leaves the crate directory.
                    let dir = common_dir(&folded)
                        .filter(|dir| dir.starts_with(&root_canonical))
                        .unwrap_or_else(|| crate_dir.clone());
                    component.path = Some(relative(&root_canonical, &dir)?);
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
    fn rust_calls_a_container_a_crate() {
        assert_eq!(RustFrontend.noun("container"), Some("crate"));
    }

    #[test]
    fn rust_calls_a_component_a_module() {
        assert_eq!(RustFrontend.noun("component"), Some("module"));
    }

    #[test]
    fn rust_keeps_the_name_of_any_other_kind() {
        assert_eq!(RustFrontend.noun("bin"), None);
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
        package_with_targets(files, "")
    }

    /// A package `server` with `files`, and `targets` (`[[test]]`,
    /// `[[example]]` tables) appended to its manifest.
    fn package_with_targets(files: &[(&str, &str)], targets: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            format!(
                "[package]\nname = \"server\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{targets}"
            ),
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

    fn component_path(dir: &tempfile::TempDir, id: &str) -> Option<String> {
        let model = analyze(dir.path(), &RustConfig::default()).unwrap();
        model
            .elements
            .into_iter()
            .find(|e| e.id.join(".") == id)
            .and_then(|e| e.path)
    }

    // The tests and examples components point at the deepest directory
    // holding every target folded into them.

    #[test]
    fn benches_alone_put_the_tests_component_at_benches() {
        let dir = package_with(&[("src/lib.rs", ""), ("benches/b.rs", "fn main() {}")]);
        assert_eq!(
            component_path(&dir, "server.tests").as_deref(),
            Some("benches")
        );
    }

    #[test]
    fn tests_beside_benches_put_the_tests_component_at_the_crate() {
        let dir = package_with(&[
            ("src/lib.rs", ""),
            ("tests/it.rs", ""),
            ("benches/b.rs", "fn main() {}"),
        ]);
        assert_eq!(component_path(&dir, "server.tests").as_deref(), Some("."));
    }

    #[test]
    fn a_custom_test_path_beside_the_tests_directory_puts_the_component_at_the_crate() {
        // `aa` sorts before `zz`: the path must not depend on which target
        // comes first.
        let dir = package_with_targets(
            &[("src/lib.rs", ""), ("tests/zz.rs", ""), ("it/main.rs", "")],
            "[[test]]\nname = \"aa\"\npath = \"it/main.rs\"\n",
        );
        assert_eq!(component_path(&dir, "server.tests").as_deref(), Some("."));
    }

    #[test]
    fn a_custom_test_path_alone_names_its_directory() {
        let dir = package_with_targets(
            &[("src/lib.rs", ""), ("it/main.rs", "")],
            "[[test]]\nname = \"it\"\npath = \"it/main.rs\"\n",
        );
        assert_eq!(component_path(&dir, "server.tests").as_deref(), Some("it"));
    }

    #[test]
    fn a_test_file_at_the_crate_root_puts_the_component_at_the_crate_not_the_file() {
        let dir = package_with_targets(
            &[("src/lib.rs", ""), ("it.rs", "")],
            "[[test]]\nname = \"it\"\npath = \"it.rs\"\n",
        );
        assert_eq!(component_path(&dir, "server.tests").as_deref(), Some("."));
    }

    #[test]
    fn an_example_under_src_names_src() {
        let dir = package_with_targets(
            &[("src/lib.rs", ""), ("src/demo.rs", "fn main() {}")],
            "[[example]]\nname = \"demo\"\npath = \"src/demo.rs\"\n",
        );
        assert_eq!(
            component_path(&dir, "server.examples").as_deref(),
            Some("src")
        );
    }

    #[test]
    fn a_custom_example_directory_is_named() {
        let dir = package_with_targets(
            &[("src/lib.rs", ""), ("demos/show.rs", "fn main() {}")],
            "[[example]]\nname = \"show\"\npath = \"demos/show.rs\"\n",
        );
        assert_eq!(
            component_path(&dir, "server.examples").as_deref(),
            Some("demos")
        );
    }

    /// A workspace at the root with one member whose test lives at
    /// `test_path`, relative to the member.
    fn member_with_test_at(test_path: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in [
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n".to_string(),
            ),
            (
                "member/Cargo.toml",
                format!(
                    "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[test]]\nname = \"it\"\npath = \"{test_path}\"\n"
                ),
            ),
            ("member/src/lib.rs", String::new()),
        ] {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn a_test_outside_its_crate_but_inside_the_root_names_its_own_directory() {
        let dir = member_with_test_at("../shared/it.rs");
        fs::create_dir_all(dir.path().join("shared")).unwrap();
        fs::write(dir.path().join("shared/it.rs"), "").unwrap();
        assert_eq!(
            component_path(&dir, "member.tests").as_deref(),
            Some("shared")
        );
    }

    #[test]
    fn a_test_outside_the_surveyed_root_leaves_the_component_at_its_crate() {
        // A sibling temp dir: the common directory is above the root.
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(elsewhere.path().join("it.rs"), "").unwrap();
        let dir = member_with_test_at(&format!(
            "../../{}/it.rs",
            elsewhere.path().file_name().unwrap().to_str().unwrap()
        ));
        assert_eq!(
            component_path(&dir, "member.tests").as_deref(),
            Some("member")
        );
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
