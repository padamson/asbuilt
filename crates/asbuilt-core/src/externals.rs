//! Externals from `asbuilt.toml`: elements outside the code (a spawned
//! process, a browser) and the edges from surveyed modules to them.
//! Nothing static in the code states these, so the config does, and a
//! `from` that names no element is an error rather than a missing edge.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::{
    Element, ElementKind, Id, Model, Relation, RelationKind, dotted, sanitize_id, sanitize_segment,
};

/// The tag every external element carries.
pub const EXTERNAL_TAG: &str = "external";

/// Add the config's externals and their relations to a surveyed model,
/// then normalize it. A `from` may be written as the model spells the
/// id or as LikeC4 will (`-` as `_`); both resolve.
pub fn apply(model: &mut Model, config: &Config) -> Result<()> {
    for external in &config.externals {
        if sanitize_segment(&external.id) != external.id {
            return Err(Error::InvalidExternalId {
                id: external.id.clone(),
            });
        }
    }

    let mut by_name: BTreeMap<String, Id> = BTreeMap::new();
    for element in &model.elements {
        by_name.insert(dotted(&element.id), element.id.clone());
        by_name.insert(sanitize_id(&element.id), element.id.clone());
    }
    for external in &config.externals {
        by_name.insert(external.id.clone(), vec![external.id.clone()]);
    }

    for external in &config.externals {
        model.elements.push(Element {
            id: vec![external.id.clone()],
            kind: ElementKind::External(external.kind.clone()),
            title: external.title.clone(),
            description: external.description.clone(),
            technology: external.technology.clone(),
            path: None,
            tags: vec![EXTERNAL_TAG.to_string()],
        });
        for relation in &external.relations {
            let Some(source) = by_name.get(&relation.from) else {
                return Err(Error::UnknownRelationSource {
                    external: external.id.clone(),
                    from: relation.from.clone(),
                });
            };
            model.relations.push(Relation {
                source: source.clone(),
                target: vec![external.id.clone()],
                kind: RelationKind::Uses,
                items: vec![relation.title.clone()],
                technology: relation.technology.clone(),
            });
        }
    }
    model.normalize();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surveyed() -> Model {
        Model {
            elements: vec![Element {
                id: vec!["play-wright".into(), "server".into()],
                kind: ElementKind::Component,
                title: "server".into(),
                description: None,
                technology: None,
                path: Some("src/server.rs".into()),
                tags: vec![],
            }],
            ..Default::default()
        }
    }

    fn config(text: &str) -> Config {
        text.parse().unwrap()
    }

    const DRIVER: &str = r#"
[[externals]]
id = "node_driver"
kind = "process"
title = "Playwright driver"
technology = "Node.js process"
description = "The server."

[[externals.relations]]
from = "play-wright.server"
title = "spawns"
technology = "stdio"
"#;

    #[test]
    fn an_external_becomes_a_tagged_element_of_its_kind_with_no_path() {
        let mut model = surveyed();
        apply(&mut model, &config(DRIVER)).unwrap();
        let ext = model.element(&["node_driver".to_string()]).unwrap();
        assert_eq!(ext.kind, ElementKind::External("process".into()));
        assert_eq!(ext.tags, vec![EXTERNAL_TAG.to_string()]);
        assert_eq!(ext.path, None);
        assert_eq!(ext.technology.as_deref(), Some("Node.js process"));
        assert_eq!(ext.description.as_deref(), Some("The server."));
    }

    #[test]
    fn a_relation_from_a_surveyed_element_is_a_uses_edge_labeled_with_the_title() {
        let mut model = surveyed();
        apply(&mut model, &config(DRIVER)).unwrap();
        assert_eq!(
            model.relations,
            vec![Relation {
                source: vec!["play-wright".into(), "server".into()],
                target: vec!["node_driver".into()],
                kind: RelationKind::Uses,
                items: vec!["spawns".into()],
                technology: Some("stdio".into()),
            }]
        );
    }

    #[test]
    fn a_from_written_the_likec4_way_resolves_to_the_hyphenated_element() {
        let mut model = surveyed();
        apply(
            &mut model,
            &config(&DRIVER.replace("play-wright.server", "play_wright.server")),
        )
        .unwrap();
        assert_eq!(
            model.relations[0].source,
            vec!["play-wright".to_string(), "server".to_string()]
        );
    }

    #[test]
    fn a_from_naming_a_sibling_external_resolves() {
        let mut model = surveyed();
        let text = format!(
            "{DRIVER}\n[[externals]]\nid = \"browsers\"\nkind = \"browser\"\ntitle = \"Browsers\"\n\n[[externals.relations]]\nfrom = \"node_driver\"\ntitle = \"drives\"\n"
        );
        apply(&mut model, &config(&text)).unwrap();
        assert!(
            model.relations.iter().any(|r| r.source == ["node_driver"]
                && r.target == ["browsers"]
                && r.items == ["drives"]),
            "{:?}",
            model.relations
        );
    }

    #[test]
    fn a_from_naming_nothing_is_an_error_naming_the_external_and_the_from() {
        let mut model = surveyed();
        let text = DRIVER.replace("play-wright.server", "play-wright.nope");
        match apply(&mut model, &config(&text)) {
            Err(Error::UnknownRelationSource { external, from }) => {
                assert_eq!(external, "node_driver");
                assert_eq!(from, "play-wright.nope");
            }
            other => panic!("expected UnknownRelationSource, got {other:?}"),
        }
    }

    #[test]
    fn an_external_id_that_is_not_an_identifier_is_an_error_naming_it() {
        let mut model = surveyed();
        let text = DRIVER.replace("id = \"node_driver\"", "id = \"node-driver\"");
        match apply(&mut model, &config(&text)) {
            Err(Error::InvalidExternalId { id }) => assert_eq!(id, "node-driver"),
            other => panic!("expected InvalidExternalId, got {other:?}"),
        }
    }

    #[test]
    fn applying_no_externals_leaves_the_model_as_it_was() {
        let mut model = surveyed();
        let before = model.clone();
        apply(&mut model, &Config::default()).unwrap();
        assert_eq!(model, before);
    }
}
