//! The front-end contract and the survey that runs every front-end
//! that recognizes a root, merges their models, and applies the config.

use std::path::Path;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::externals;
use crate::model::Model;

/// One language's reader. `asbuilt-rust` is the first.
pub trait Frontend {
    fn name(&self) -> &str;
    /// Whether `root` is a code base this front-end reads, decided from
    /// marker files (`Cargo.toml`, `package.json`, ...).
    fn detect(&self, root: &Path) -> bool;
    fn analyze(&self, root: &Path, config: &Config) -> Result<Model>;
}

/// The model of `root`: every front-end that detects it, merged, with
/// the config's externals applied and ids checked for collisions.
pub fn survey(root: &Path, config: &Config, frontends: &[&dyn Frontend]) -> Result<Model> {
    let mut model = Model::default();
    let mut detected = false;
    for frontend in frontends {
        if !frontend.detect(root) {
            continue;
        }
        detected = true;
        model.merge(frontend.analyze(root, config)?);
    }
    if !detected {
        return Err(Error::NoFrontend {
            root: root.to_path_buf(),
        });
    }
    externals::apply(&mut model, config)?;
    model.validate()?;
    check_theme_kinds(&model, config)?;
    Ok(model)
}

/// Every `[theme]` key names a kind the model has.
fn check_theme_kinds(model: &Model, config: &Config) -> Result<()> {
    let have: std::collections::BTreeSet<&str> =
        model.elements.iter().map(|e| e.kind.keyword()).collect();
    for kind in config.theme.keys() {
        if !have.contains(kind.as_str()) {
            return Err(Error::UnknownThemeKind {
                kind: kind.clone(),
                have: have.iter().copied().collect::<Vec<_>>().join(", "),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Element, ElementKind};
    use std::path::PathBuf;

    struct Fake {
        name: &'static str,
        detects: bool,
        elements: Vec<&'static str>,
        fails: bool,
    }

    impl Frontend for Fake {
        fn name(&self) -> &str {
            self.name
        }
        fn detect(&self, _: &Path) -> bool {
            self.detects
        }
        fn analyze(&self, _: &Path, _: &Config) -> Result<Model> {
            if self.fails {
                return Err(Error::Frontend {
                    frontend: self.name.into(),
                    source: "boom".into(),
                });
            }
            Ok(Model {
                elements: self
                    .elements
                    .iter()
                    .map(|id| Element {
                        id: vec![id.to_string()],
                        kind: ElementKind::Container,
                        title: id.to_string(),
                        description: None,
                        technology: None,
                        path: None,
                        tags: vec![],
                    })
                    .collect(),
                ..Default::default()
            })
        }
    }

    fn fake(name: &'static str, detects: bool, elements: &[&'static str]) -> Fake {
        Fake {
            name,
            detects,
            elements: elements.to_vec(),
            fails: false,
        }
    }

    #[test]
    fn no_front_end_recognizing_the_root_is_an_error_naming_it() {
        let none = fake("rust", false, &["a"]);
        match survey(Path::new("/some/root"), &Config::default(), &[&none]) {
            Err(Error::NoFrontend { root }) => assert_eq!(root, PathBuf::from("/some/root")),
            other => panic!("expected NoFrontend, got {other:?}"),
        }
    }

    #[test]
    fn a_theme_key_naming_no_kind_in_the_model_is_an_error_naming_it_and_the_kinds_there_are() {
        let rust = fake("rust", true, &["a"]);
        let config: Config = "[theme]\nbrowser = \"#000000\"\n".parse().unwrap();
        match survey(Path::new("/some/root"), &config, &[&rust]) {
            Err(Error::UnknownThemeKind { kind, have }) => {
                assert_eq!(kind, "browser");
                assert_eq!(have, "container");
            }
            other => panic!("expected UnknownThemeKind, got {other:?}"),
        }
        let config: Config = "[theme]\ncontainer = \"#000000\"\n".parse().unwrap();
        assert!(survey(Path::new("/some/root"), &config, &[&rust]).is_ok());
    }

    #[test]
    fn every_detecting_front_end_contributes_and_the_result_is_normalized() {
        let a = fake("a", true, &["zeta"]);
        let b = fake("b", true, &["alpha"]);
        let skipped = fake("c", false, &["never"]);
        let model = survey(Path::new("."), &Config::default(), &[&a, &skipped, &b]).unwrap();
        let ids: Vec<String> = model.elements.iter().map(|e| e.id.join(".")).collect();
        assert_eq!(ids, ["alpha", "zeta"]);
    }

    #[test]
    fn the_config_externals_are_applied_to_the_merged_model() {
        let a = fake("a", true, &["app"]);
        let config: Config = "[[externals]]\nid = \"driver\"\nkind = \"process\"\ntitle = \"D\"\n\n[[externals.relations]]\nfrom = \"app\"\ntitle = \"spawns\"\n"
            .parse()
            .unwrap();
        let model = survey(Path::new("."), &config, &[&a]).unwrap();
        assert!(model.element(&["driver".to_string()]).is_some());
        assert_eq!(model.relations.len(), 1);
    }

    #[test]
    fn a_front_end_error_is_returned_as_is() {
        let broken = Fake {
            name: "rust",
            detects: true,
            elements: vec![],
            fails: true,
        };
        match survey(Path::new("."), &Config::default(), &[&broken]) {
            Err(Error::Frontend { frontend, .. }) => assert_eq!(frontend, "rust"),
            other => panic!("expected Frontend, got {other:?}"),
        }
    }

    #[test]
    fn an_external_whose_id_is_a_surveyed_crate_is_a_duplicate() {
        let a = fake("a", true, &["app"]);
        let config: Config = "[[externals]]\nid = \"app\"\nkind = \"process\"\ntitle = \"D\"\n"
            .parse()
            .unwrap();
        assert!(matches!(
            survey(Path::new("."), &config, &[&a]),
            Err(Error::DuplicateId { .. })
        ));
    }

    #[test]
    fn colliding_ids_across_front_ends_are_an_error() {
        let a = fake("a", true, &["my-app"]);
        let b = fake("b", true, &["my_app"]);
        assert!(matches!(
            survey(Path::new("."), &Config::default(), &[&a, &b]),
            Err(Error::DuplicateId { .. })
        ));
    }
}
