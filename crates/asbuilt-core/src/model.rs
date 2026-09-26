//! The architecture model every front-end produces and the emitter
//! consumes.
//!
//! Ids are segment lists (`["playwright_rs", "server", "connection"]`),
//! which LikeC4 renders as `playwright_rs.server.connection`. The model
//! keeps real names; mapping them to LikeC4 identifiers happens in the
//! emitter, and [`Model::validate`] is what says two names would
//! collide there.

use std::collections::BTreeMap;

use crate::error::{Error, Result};

/// A fully qualified element id: crate, then module path.
pub type Id = Vec<String>;

/// What a front-end found, plus the externals the config adds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Model {
    pub elements: Vec<Element>,
    pub relations: Vec<Relation>,
    pub deployment: Deployment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub id: Id,
    pub kind: ElementKind,
    pub title: String,
    /// The first paragraph of the module's own docs, markdown kept.
    pub description: Option<String>,
    /// `library crate`, `proc-macro crate`, `binary`, or whatever the
    /// config says for an external.
    pub technology: Option<String>,
    /// Relative to the surveyed root, `/`-separated on every platform.
    /// `None` for an external.
    pub path: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementKind {
    /// A crate or package.
    Container,
    /// A module, or a synthetic `tests` / `examples` / bin component.
    Component,
    /// Something outside the code, from `[[externals]]`; carries the
    /// LikeC4 element kind name (`process`, `browser`, ...).
    External(String),
}

/// Evidence for a relation, strongest first. Aggregation keeps the
/// minimum, so the declared order is the precedence and must not be
/// reordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelationKind {
    Implements,
    Constructs,
    Calls,
    NamesType,
    Uses,
}

impl RelationKind {
    /// The relationship kind declared in the LikeC4 specification.
    pub fn keyword(self) -> &'static str {
        match self {
            RelationKind::Implements => "implements",
            RelationKind::Constructs => "constructs",
            RelationKind::Calls => "calls",
            RelationKind::NamesType => "names",
            RelationKind::Uses => "uses",
        }
    }

    /// Every kind, in precedence order; the emitter declares them all.
    pub const ALL: [RelationKind; 5] = [
        RelationKind::Implements,
        RelationKind::Constructs,
        RelationKind::Calls,
        RelationKind::NamesType,
        RelationKind::Uses,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub source: Id,
    pub target: Id,
    pub kind: RelationKind,
    /// The referenced item names, sorted and deduplicated; the label.
    pub items: Vec<String>,
    /// Externals only, from the config.
    pub technology: Option<String>,
}

/// Deployment nodes and instances. Empty until an infrastructure
/// front-end exists; it is here from day one because LikeC4 keeps a
/// separate deployment model and retrofitting it would rework every
/// front-end.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Deployment {
    pub nodes: Vec<DeploymentNode>,
    pub instances: Vec<DeploymentInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentNode {
    pub id: Id,
    pub kind: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentInstance {
    pub id: Id,
    /// The model element deployed on the node this instance sits in.
    pub element: Id,
}

/// Whether one id is an ancestor of, a descendant of, or equal to the
/// other. `mod x;` and `pub use x::Y` are structure, not coupling, so a
/// relation between lineal elements is never recorded.
pub fn is_lineal(a: &[String], b: &[String]) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// An id as LikeC4 renders it, before sanitizing: segments joined by `.`.
pub fn dotted(id: &[String]) -> String {
    id.join(".")
}

/// Words likec4 1.59.3 refuses as an element id, found by validating a
/// model with each one as a nested element (`model`, `element`,
/// `deployment` and `relationship` are accepted and so are not here).
/// A module named one of these gets a trailing `_`.
pub const RESERVED: &[&str] = &[
    "BottomTop",
    "LeftRight",
    "RightLeft",
    "TopBottom",
    "and",
    "autoLayout",
    "border",
    "color",
    "deploymentNode",
    "description",
    "dynamic",
    "exclude",
    "extend",
    "extends",
    "false",
    "from",
    "global",
    "icon",
    "import",
    "include",
    "instanceOf",
    "is",
    "it",
    "kind",
    "likec4lib",
    "link",
    "metadata",
    "multiple",
    "navigateTo",
    "not",
    "notation",
    "notes",
    "of",
    "opacity",
    "or",
    "padding",
    "shape",
    "size",
    "specification",
    "style",
    "tag",
    "technology",
    "textSize",
    "this",
    "title",
    "true",
    "view",
    "views",
    "where",
    "with",
];

/// One segment as a LikeC4 identifier: `-` and any other character
/// outside `[A-Za-z0-9_]` become `_`, a leading digit gets a `_` in
/// front of it, and a [`RESERVED`] word gets one after it.
pub fn sanitize_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len() + 1);
    if segment.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.push('_');
    }
    for c in segment.chars() {
        out.push(if c.is_ascii_alphanumeric() || c == '_' {
            c
        } else {
            '_'
        });
    }
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// The whole id as a LikeC4 identifier path.
pub fn sanitize_id(id: &[String]) -> String {
    id.iter()
        .map(|s| sanitize_segment(s))
        .collect::<Vec<_>>()
        .join(".")
}

impl Model {
    /// Sort elements by id, merge relations on `(source, target)`
    /// (strongest kind, union of items, first technology), and sort
    /// them. Every path to the emitter goes through here, so output
    /// order never depends on discovery order.
    pub fn normalize(&mut self) {
        self.elements.sort_by(|a, b| a.id.cmp(&b.id));

        let mut merged: BTreeMap<(Id, Id), Relation> = BTreeMap::new();
        for rel in self.relations.drain(..) {
            let key = (rel.source.clone(), rel.target.clone());
            match merged.get_mut(&key) {
                None => {
                    merged.insert(key, rel);
                }
                Some(existing) => {
                    existing.kind = existing.kind.min(rel.kind);
                    existing.items.extend(rel.items);
                    if existing.technology.is_none() {
                        existing.technology = rel.technology;
                    }
                }
            }
        }
        self.relations = merged
            .into_values()
            .map(|mut rel| {
                rel.items.sort();
                rel.items.dedup();
                rel
            })
            .collect();
    }

    /// Append another front-end's model and normalize.
    pub fn merge(&mut self, other: Model) {
        self.elements.extend(other.elements);
        self.relations.extend(other.relations);
        self.deployment.nodes.extend(other.deployment.nodes);
        self.deployment.instances.extend(other.deployment.instances);
        self.normalize();
    }

    /// Fail if two elements have the same id, or two ids that become the
    /// same LikeC4 identifier.
    pub fn validate(&self) -> Result<()> {
        let mut seen: BTreeMap<String, &Id> = BTreeMap::new();
        for element in &self.elements {
            let key = sanitize_id(&element.id);
            if let Some(first) = seen.insert(key.clone(), &element.id) {
                return Err(Error::DuplicateId {
                    id: key,
                    first: dotted(first),
                    second: dotted(&element.id),
                });
            }
        }
        Ok(())
    }

    pub fn element(&self, id: &[String]) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Id {
        s.split('.').map(str::to_string).collect()
    }

    fn element(s: &str) -> Element {
        Element {
            id: id(s),
            kind: ElementKind::Component,
            title: s.rsplit('.').next().unwrap().to_string(),
            description: None,
            technology: None,
            path: None,
            tags: vec![],
        }
    }

    fn relation(source: &str, target: &str, kind: RelationKind, items: &[&str]) -> Relation {
        Relation {
            source: id(source),
            target: id(target),
            kind,
            items: items.iter().map(|s| s.to_string()).collect(),
            technology: None,
        }
    }

    #[test]
    fn precedence_is_the_declared_order_strongest_first() {
        assert!(RelationKind::Implements < RelationKind::Constructs);
        assert!(RelationKind::Constructs < RelationKind::Calls);
        assert!(RelationKind::Calls < RelationKind::NamesType);
        assert!(RelationKind::NamesType < RelationKind::Uses);
    }

    #[test]
    fn every_kind_has_a_distinct_keyword() {
        let mut keywords: Vec<_> = RelationKind::ALL.iter().map(|k| k.keyword()).collect();
        keywords.sort();
        keywords.dedup();
        assert_eq!(
            keywords,
            ["calls", "constructs", "implements", "names", "uses"]
        );
    }

    #[test]
    fn an_ancestor_and_its_descendant_are_lineal_in_both_directions() {
        assert!(is_lineal(&id("a"), &id("a.b.c")));
        assert!(is_lineal(&id("a.b.c"), &id("a")));
    }

    #[test]
    fn an_element_is_lineal_with_itself() {
        assert!(is_lineal(&id("a.b"), &id("a.b")));
    }

    #[test]
    fn siblings_are_not_lineal() {
        assert!(!is_lineal(&id("a.b"), &id("a.c")));
    }

    #[test]
    fn a_shared_prefix_string_is_not_lineage() {
        // `a.bc` is not under `a.b`; segments compare whole.
        assert!(!is_lineal(&id("a.b"), &id("a.bc")));
    }

    #[test]
    fn a_hyphen_becomes_an_underscore() {
        assert_eq!(sanitize_segment("playwright-rs"), "playwright_rs");
    }

    #[test]
    fn a_leading_digit_is_prefixed() {
        assert_eq!(sanitize_segment("3d"), "_3d");
    }

    #[test]
    fn a_reserved_word_gets_a_trailing_underscore() {
        assert_eq!(sanitize_segment("view"), "view_");
        assert_eq!(sanitize_segment("link"), "link_");
    }

    #[test]
    fn model_and_element_are_not_reserved() {
        // likec4 1.59.3 accepts both as ids; asbuilt-core's own `model`
        // module keeps its name.
        assert_eq!(sanitize_segment("model"), "model");
        assert_eq!(sanitize_segment("element"), "element");
    }

    #[test]
    fn the_reserved_list_is_sorted_and_unique() {
        let mut sorted = RESERVED.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, RESERVED);
    }

    #[test]
    fn a_plain_identifier_is_unchanged() {
        assert_eq!(sanitize_segment("server_v2"), "server_v2");
    }

    #[test]
    fn sanitizing_an_id_keeps_the_dots() {
        assert_eq!(sanitize_id(&id("play-wright.a.1x")), "play_wright.a._1x");
    }

    #[test]
    fn normalize_sorts_elements_by_id() {
        let mut model = Model {
            elements: vec![element("b"), element("a.z"), element("a")],
            ..Default::default()
        };
        model.normalize();
        let ids: Vec<_> = model.elements.iter().map(|e| dotted(&e.id)).collect();
        assert_eq!(ids, ["a", "a.z", "b"]);
    }

    #[test]
    fn normalize_merges_relations_on_the_pair_keeping_the_strongest_kind() {
        let mut model = Model {
            relations: vec![
                relation("a", "b", RelationKind::Uses, &["Y"]),
                relation("a", "b", RelationKind::Calls, &["X"]),
            ],
            ..Default::default()
        };
        model.normalize();
        assert_eq!(
            model.relations,
            vec![relation("a", "b", RelationKind::Calls, &["X", "Y"])]
        );
    }

    #[test]
    fn normalize_dedups_items_within_a_relation() {
        let mut model = Model {
            relations: vec![relation("a", "b", RelationKind::Uses, &["Y", "X", "Y"])],
            ..Default::default()
        };
        model.normalize();
        assert_eq!(model.relations[0].items, ["X", "Y"]);
    }

    #[test]
    fn normalize_sorts_relations_by_source_then_target() {
        let mut model = Model {
            relations: vec![
                relation("b", "a", RelationKind::Uses, &[]),
                relation("a", "c", RelationKind::Uses, &[]),
                relation("a", "b", RelationKind::Uses, &[]),
            ],
            ..Default::default()
        };
        model.normalize();
        let pairs: Vec<_> = model
            .relations
            .iter()
            .map(|r| (dotted(&r.source), dotted(&r.target)))
            .collect();
        assert_eq!(
            pairs,
            [
                ("a".into(), "b".into()),
                ("a".into(), "c".into()),
                ("b".into(), "a".into())
            ]
        );
    }

    #[test]
    fn normalize_keeps_the_first_technology_seen() {
        let mut first = relation("a", "b", RelationKind::Uses, &[]);
        first.technology = Some("stdio".into());
        let mut model = Model {
            relations: vec![
                relation("a", "b", RelationKind::Uses, &[]),
                first,
                relation("a", "b", RelationKind::Uses, &[]),
            ],
            ..Default::default()
        };
        model.normalize();
        assert_eq!(model.relations[0].technology.as_deref(), Some("stdio"));
    }

    #[test]
    fn merge_appends_and_normalizes() {
        let mut model = Model {
            elements: vec![element("b")],
            ..Default::default()
        };
        model.merge(Model {
            elements: vec![element("a")],
            relations: vec![relation("a", "b", RelationKind::Uses, &["X"])],
            ..Default::default()
        });
        let ids: Vec<_> = model.elements.iter().map(|e| dotted(&e.id)).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(model.relations.len(), 1);
    }

    #[test]
    fn two_ids_that_sanitize_alike_are_a_duplicate_naming_both() {
        let model = Model {
            elements: vec![element("foo-bar"), element("foo_bar")],
            ..Default::default()
        };
        match model.validate() {
            Err(Error::DuplicateId { id, first, second }) => {
                assert_eq!(id, "foo_bar");
                assert_eq!(first, "foo-bar");
                assert_eq!(second, "foo_bar");
            }
            other => panic!("expected DuplicateId, got {other:?}"),
        }
    }

    #[test]
    fn an_id_that_appears_twice_is_a_duplicate() {
        let model = Model {
            elements: vec![element("app"), element("app")],
            ..Default::default()
        };
        match model.validate() {
            Err(Error::DuplicateId { id, first, second }) => {
                assert_eq!(
                    (id.as_str(), first.as_str(), second.as_str()),
                    ("app", "app", "app")
                );
            }
            other => panic!("expected DuplicateId, got {other:?}"),
        }
    }

    #[test]
    fn distinct_ids_validate() {
        let model = Model {
            elements: vec![element("a"), element("a.b"), element("b")],
            ..Default::default()
        };
        assert!(model.validate().is_ok());
    }

    #[test]
    fn element_finds_by_exact_id() {
        let model = Model {
            elements: vec![element("a"), element("a.b")],
            ..Default::default()
        };
        assert_eq!(
            model.element(&id("a.b")).map(|e| e.title.as_str()),
            Some("b")
        );
        assert_eq!(model.element(&id("a.b.c")), None);
    }
}
