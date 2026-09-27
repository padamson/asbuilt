//! The module tree of one target, from its root file down every `mod`
//! declaration. Follows `mod x;` to `x.rs` or `x/mod.rs`, honors
//! `#[path]`, enters inline `mod x { }`, and skips anything under
//! `#[cfg(test)]`. Every file comes from a declaration, never from a
//! directory listing, so discovery order is out of the picture.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::{Attribute, Expr, Item, Lit, Meta};

use crate::error::RustFrontendError;
use crate::source::FileSource;

/// A module path within one target; `[]` is the target root.
pub type ModulePath = Vec<String>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleNode {
    /// The file the module's items live in: its own file, or the
    /// enclosing file for an inline module.
    pub file: PathBuf,
    pub inline: bool,
    /// The first paragraph of the module's doc comment, markdown kept.
    pub doc: Option<String>,
    pub children: BTreeSet<String>,
}

/// One parsed file with the module path its top-level items belong to.
pub struct ParsedFile {
    pub path: PathBuf,
    pub module: ModulePath,
    pub ast: syn::File,
}

impl std::fmt::Debug for ParsedFile {
    // `syn::File` has no `Debug` without syn's `extra-traits`; the path
    // and module are what a failure message needs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedFile")
            .field("path", &self.path)
            .field("module", &self.module)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub struct ModuleTree {
    pub modules: BTreeMap<ModulePath, ModuleNode>,
    pub files: Vec<ParsedFile>,
}

impl ModuleTree {
    pub fn get(&self, path: &[String]) -> Option<&ModuleNode> {
        self.modules.get(path)
    }
}

/// `#[cfg(test)]` exactly; `cfg(any(test, ..))` and `cfg_attr` are not
/// special-cased.
pub fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && matches!(&a.meta, Meta::List(list) if list.tokens.to_string() == "test")
    })
}

/// The string of `#[name = "..."]`, if present.
fn string_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    attrs.iter().find_map(|a| {
        if !a.path().is_ident(name) {
            return None;
        }
        match &a.meta {
            Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(lit) => match &lit.lit {
                    Lit::Str(s) => Some(s.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    })
}

/// The first paragraph of the doc comments among `attrs`, in source
/// order, up to the first blank line. One leading space per line is
/// the one rustdoc strips; the rest of the markdown stays.
pub fn first_doc_paragraph(attrs: &[Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(lit) => match &lit.lit {
                    Lit::Str(s) => Some(s.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .flat_map(|text| {
            // A bare `//!` is `#[doc = ""]`, whose `lines()` is empty;
            // it is the blank line that ends the paragraph, so keep it.
            if text.is_empty() {
                return vec![String::new()];
            }
            text.lines()
                .map(|l| l.strip_prefix(' ').unwrap_or(l).to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    let joined = lines.join("\n");
    let first = joined.trim_start_matches('\n');
    let paragraph = first.split("\n\n").next().unwrap_or("").trim();
    (!paragraph.is_empty()).then(|| paragraph.to_string())
}

/// Load the tree rooted at `root_file`.
pub fn load_tree(
    root_file: &Path,
    source: &dyn FileSource,
) -> Result<ModuleTree, RustFrontendError> {
    let mut tree = ModuleTree::default();
    let dir = root_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    load_file(&mut tree, root_file, Vec::new(), &dir, source)?;
    Ok(tree)
}

fn load_file(
    tree: &mut ModuleTree,
    file: &Path,
    module: ModulePath,
    dir: &Path,
    source: &dyn FileSource,
) -> Result<(), RustFrontendError> {
    let text = source.read(file).map_err(|source| RustFrontendError::Io {
        path: file.to_path_buf(),
        source,
    })?;
    let ast = syn::parse_file(&text).map_err(|source| RustFrontendError::Parse {
        file: file.to_path_buf(),
        source,
    })?;
    let node = tree.modules.entry(module.clone()).or_default();
    node.file = file.to_path_buf();
    node.inline = false;
    node.doc = first_doc_paragraph(&ast.attrs);

    let mut pending = Vec::new();
    let reading = Reading { file, source };
    collect_mods(
        tree,
        &ast.items,
        &module,
        &reading,
        dir,
        false,
        &mut pending,
    )?;
    tree.files.push(ParsedFile {
        path: file.to_path_buf(),
        module,
        ast,
    });
    for (child_module, child_file, child_dir) in pending {
        load_file(tree, &child_file, child_module, &child_dir, source)?;
    }
    Ok(())
}

/// Record every `mod` among `items`, entering inline ones now and
/// queueing file ones for `load_file`.
/// The file being read and where its text comes from.
struct Reading<'a> {
    file: &'a Path,
    source: &'a dyn FileSource,
}

fn collect_mods(
    tree: &mut ModuleTree,
    items: &[Item],
    module: &ModulePath,
    reading: &Reading<'_>,
    dir: &Path,
    in_inline: bool,
    pending: &mut Vec<(ModulePath, PathBuf, PathBuf)>,
) -> Result<(), RustFrontendError> {
    let file = reading.file;
    let source = reading.source;
    for item in items {
        let Item::Mod(m) = item else { continue };
        if is_cfg_test(&m.attrs) {
            continue;
        }
        let name = m.ident.to_string();
        tree.modules
            .entry(module.clone())
            .or_default()
            .children
            .insert(name.clone());
        let mut child_module = module.clone();
        child_module.push(name.clone());

        match &m.content {
            Some((_, content)) => {
                let node = tree.modules.entry(child_module.clone()).or_default();
                node.file = file.to_path_buf();
                node.inline = true;
                if node.doc.is_none() {
                    node.doc = first_doc_paragraph(&m.attrs);
                }
                let child_dir = dir.join(&name);
                collect_mods(
                    tree,
                    content,
                    &child_module,
                    reading,
                    &child_dir,
                    true,
                    pending,
                )?;
            }
            None => {
                // `#[path]` wins outright: relative to the file's directory
                // at the top level of a file, and to the inline module's
                // directory inside an inline block (rustc's rule); the
                // target file's directory becomes the module's own.
                // Otherwise `dir/x.rs` or `dir/x/mod.rs`, and children of
                // either live under `dir/x/`.
                let (child_file, child_dir) = match string_attr(&m.attrs, "path") {
                    Some(explicit) => {
                        let base = if in_inline {
                            dir.to_path_buf()
                        } else {
                            file.parent().unwrap_or(Path::new("")).to_path_buf()
                        };
                        let child_file = base.join(explicit);
                        let child_dir = child_file
                            .parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_default();
                        (child_file, child_dir)
                    }
                    None => (
                        resolve_module_file(file, &name, dir, source)?,
                        dir.join(&name),
                    ),
                };
                pending.push((child_module, child_file, child_dir));
            }
        }
    }
    Ok(())
}

/// The file for a non-`#[path]` `mod x;`, or the error naming both
/// candidates.
pub fn resolve_module_file(
    declared_in: &Path,
    module: &str,
    dir: &Path,
    source: &dyn FileSource,
) -> Result<PathBuf, RustFrontendError> {
    let flat = dir.join(format!("{module}.rs"));
    let nested = dir.join(module).join("mod.rs");
    match (source.exists(&flat), source.exists(&nested)) {
        (true, false) => Ok(flat),
        (false, true) => Ok(nested),
        (true, true) => Err(RustFrontendError::AmbiguousModuleFile {
            declared_in: declared_in.to_path_buf(),
            module: module.to_string(),
            candidates: [flat, nested],
        }),
        (false, false) => Err(RustFrontendError::ModuleFileNotFound {
            declared_in: declared_in.to_path_buf(),
            module: module.to_string(),
            tried: vec![flat, nested],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MapSource;

    fn tree(files: &[(&str, &str)]) -> Result<ModuleTree, RustFrontendError> {
        load_tree(Path::new("src/lib.rs"), &MapSource::new(files))
    }

    fn path(s: &str) -> ModulePath {
        s.split('.').map(str::to_string).collect()
    }

    #[test]
    fn mod_x_finds_x_rs() {
        let t = tree(&[("src/lib.rs", "mod x;"), ("src/x.rs", "")]).unwrap();
        assert_eq!(t.get(&path("x")).unwrap().file, PathBuf::from("src/x.rs"));
    }

    #[test]
    fn mod_x_finds_x_mod_rs() {
        let t = tree(&[("src/lib.rs", "mod x;"), ("src/x/mod.rs", "")]).unwrap();
        assert_eq!(
            t.get(&path("x")).unwrap().file,
            PathBuf::from("src/x/mod.rs")
        );
    }

    #[test]
    fn both_candidates_present_is_ambiguous_naming_both() {
        let result = tree(&[
            ("src/lib.rs", "mod x;"),
            ("src/x.rs", ""),
            ("src/x/mod.rs", ""),
        ]);
        match result {
            Err(RustFrontendError::AmbiguousModuleFile {
                declared_in,
                module,
                candidates,
            }) => {
                assert_eq!(declared_in, PathBuf::from("src/lib.rs"));
                assert_eq!(module, "x");
                assert_eq!(
                    candidates,
                    [PathBuf::from("src/x.rs"), PathBuf::from("src/x/mod.rs")]
                );
            }
            other => panic!("expected AmbiguousModuleFile, got {other:?}"),
        }
    }

    #[test]
    fn neither_candidate_present_is_not_found_naming_the_declaring_file_and_both_tried() {
        match tree(&[("src/lib.rs", "mod gone;")]) {
            Err(RustFrontendError::ModuleFileNotFound {
                declared_in,
                module,
                tried,
            }) => {
                assert_eq!(declared_in, PathBuf::from("src/lib.rs"));
                assert_eq!(module, "gone");
                assert_eq!(
                    tried,
                    [
                        PathBuf::from("src/gone.rs"),
                        PathBuf::from("src/gone/mod.rs")
                    ]
                );
            }
            other => panic!("expected ModuleFileNotFound, got {other:?}"),
        }
    }

    #[test]
    fn a_child_of_a_plain_file_module_lives_in_the_directory_named_after_it() {
        let t = tree(&[
            ("src/lib.rs", "mod a;"),
            ("src/a.rs", "mod b;"),
            ("src/a/b.rs", ""),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("a.b")).unwrap().file,
            PathBuf::from("src/a/b.rs")
        );
    }

    #[test]
    fn a_child_of_a_mod_rs_module_lives_beside_it() {
        let t = tree(&[
            ("src/lib.rs", "mod a;"),
            ("src/a/mod.rs", "mod b;"),
            ("src/a/b.rs", ""),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("a.b")).unwrap().file,
            PathBuf::from("src/a/b.rs")
        );
    }

    #[test]
    fn a_path_attribute_is_honored_and_its_children_live_beside_the_target() {
        let t = tree(&[
            ("src/lib.rs", "#[path = \"other/place.rs\"]\nmod renamed;"),
            ("src/other/place.rs", "mod sub;"),
            ("src/other/sub.rs", ""),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("renamed")).unwrap().file,
            PathBuf::from("src/other/place.rs")
        );
        assert_eq!(
            t.get(&path("renamed.sub")).unwrap().file,
            PathBuf::from("src/other/sub.rs")
        );
    }

    #[test]
    fn a_path_attribute_never_probes_the_conventional_candidates() {
        // `renamed.rs` exists but must not be read: `#[path]` wins.
        let t = tree(&[
            ("src/lib.rs", "#[path = \"place.rs\"]\nmod renamed;"),
            ("src/place.rs", ""),
            ("src/renamed.rs", "mod would_fail;"),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("renamed")).unwrap().file,
            PathBuf::from("src/place.rs")
        );
    }

    #[test]
    fn a_path_attribute_inside_an_inline_module_is_relative_to_the_inline_directory() {
        let t = tree(&[
            ("src/lib.rs", "mod outer { #[path = \"x.rs\"] mod inner; }"),
            ("src/outer/x.rs", ""),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("outer.inner")).unwrap().file,
            PathBuf::from("src/outer/x.rs")
        );
    }

    #[test]
    fn a_path_attribute_at_the_top_of_a_plain_file_module_is_relative_to_that_file() {
        let t = tree(&[
            ("src/lib.rs", "mod a;"),
            ("src/a.rs", "#[path = \"x.rs\"] mod inner;"),
            ("src/x.rs", ""),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("a.inner")).unwrap().file,
            PathBuf::from("src/x.rs")
        );
    }

    #[test]
    fn an_inline_module_is_recorded_against_the_enclosing_file() {
        let t = tree(&[("src/lib.rs", "mod a { pub struct A; }")]).unwrap();
        let a = t.get(&path("a")).unwrap();
        assert!(a.inline);
        assert_eq!(a.file, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn a_file_child_of_an_inline_module_lives_under_the_inline_name() {
        let t = tree(&[("src/lib.rs", "mod a { mod b; }"), ("src/a/b.rs", "")]).unwrap();
        assert_eq!(
            t.get(&path("a.b")).unwrap().file,
            PathBuf::from("src/a/b.rs")
        );
        assert_eq!(
            t.get(&path("a")).unwrap().children,
            BTreeSet::from(["b".to_string()])
        );
    }

    #[test]
    fn a_cfg_test_module_is_neither_recorded_nor_loaded() {
        let t = tree(&[
            (
                "src/lib.rs",
                "#[cfg(test)]\nmod tests { use crate::x; }\n#[cfg(test)]\nmod more;\nmod kept;",
            ),
            ("src/kept.rs", ""),
        ])
        .unwrap();
        assert_eq!(t.get(&path("tests")), None);
        assert_eq!(t.get(&path("more")), None);
        assert_eq!(
            t.get(&[]).unwrap().children,
            BTreeSet::from(["kept".to_string()])
        );
    }

    #[test]
    fn cfg_test_is_matched_exactly() {
        let file: syn::File = syn::parse_str(
            "#[cfg(test)] mod a {}\n#[cfg(any(test, feature = \"x\"))] mod b {}\n#[cfg(unix)] mod c {}\nmod d {}",
        )
        .unwrap();
        let flags: Vec<bool> = file
            .items
            .iter()
            .map(|i| match i {
                Item::Mod(m) => is_cfg_test(&m.attrs),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(flags, [true, false, false, false]);
    }

    #[test]
    fn the_root_doc_first_paragraph_stops_at_the_blank_line_and_keeps_markdown() {
        let t = tree(&[(
            "src/lib.rs",
            "//! Frame is where `actions` are *sent*.\n//! Second line.\n//!\n//! Not this.\n",
        )])
        .unwrap();
        assert_eq!(
            t.get(&[]).unwrap().doc.as_deref(),
            Some("Frame is where `actions` are *sent*.\nSecond line.")
        );
    }

    #[test]
    fn a_file_without_docs_has_no_description() {
        let t = tree(&[("src/lib.rs", "pub struct A;")]).unwrap();
        assert_eq!(t.get(&[]).unwrap().doc, None);
    }

    #[test]
    fn a_leading_blank_doc_line_is_skipped() {
        let t = tree(&[("src/lib.rs", "//!\n//! Real.\n")]).unwrap();
        assert_eq!(t.get(&[]).unwrap().doc.as_deref(), Some("Real."));
    }

    #[test]
    fn an_inline_module_takes_its_inner_doc() {
        let t = tree(&[("src/lib.rs", "mod a {\n    //! Inner doc.\n}")]).unwrap();
        assert_eq!(
            t.get(&path("a")).unwrap().doc.as_deref(),
            Some("Inner doc.")
        );
    }

    #[test]
    fn a_file_module_takes_its_own_inner_doc_not_the_declaration_site() {
        let t = tree(&[
            ("src/lib.rs", "/// Outer.\nmod a;"),
            ("src/a.rs", "//! Own.\n"),
        ])
        .unwrap();
        assert_eq!(t.get(&path("a")).unwrap().doc.as_deref(), Some("Own."));
    }

    #[test]
    fn a_parse_error_names_the_file() {
        match tree(&[("src/lib.rs", "mod a;"), ("src/a.rs", "fn {")]) {
            Err(RustFrontendError::Parse { file, .. }) => {
                assert_eq!(file, PathBuf::from("src/a.rs"))
            }
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_root_file_is_an_io_error_naming_it() {
        match tree(&[]) {
            Err(RustFrontendError::Io { path, .. }) => {
                assert_eq!(path, PathBuf::from("src/lib.rs"))
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn two_inline_modules_with_one_name_under_different_cfgs_merge() {
        let t = tree(&[
            (
                "src/lib.rs",
                "#[cfg(unix)] mod imp { mod u; }\n#[cfg(windows)] mod imp { mod w; }",
            ),
            ("src/imp/u.rs", ""),
            ("src/imp/w.rs", ""),
        ])
        .unwrap();
        assert_eq!(
            t.get(&path("imp")).unwrap().children,
            BTreeSet::from(["u".to_string(), "w".to_string()])
        );
        assert_eq!(t.modules.len(), 4);
    }

    #[test]
    fn a_parsed_file_debugs_as_its_path_and_module() {
        let t = tree(&[("src/lib.rs", "mod a;"), ("src/a.rs", "")]).unwrap();
        let shown = format!("{:?}", t.files[1]);
        // The path joins with the platform separator, so compare against
        // the same join rather than a literal.
        let expected_path = format!("{:?}", Path::new("src").join("a.rs"));
        assert!(
            shown.contains(&expected_path) && shown.contains("[\"a\"]"),
            "{shown}"
        );
    }

    #[test]
    fn every_file_is_recorded_with_the_module_its_items_belong_to() {
        let t = tree(&[
            ("src/lib.rs", "mod a;"),
            ("src/a/mod.rs", "mod b;"),
            ("src/a/b.rs", ""),
        ])
        .unwrap();
        let recorded: Vec<(PathBuf, ModulePath)> = t
            .files
            .iter()
            .map(|f| (f.path.clone(), f.module.clone()))
            .collect();
        assert_eq!(
            recorded,
            [
                (PathBuf::from("src/lib.rs"), vec![]),
                (PathBuf::from("src/a/mod.rs"), path("a")),
                (PathBuf::from("src/a/b.rs"), path("a.b")),
            ]
        );
    }
}
