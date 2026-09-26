//! From resolved references to model relations: element ids per target
//! role, one relation per (source, target) pair with the strongest
//! kind and the sorted item names, and nothing between an element and
//! its own ancestors or descendants.

use std::collections::{BTreeMap, BTreeSet};

use asbuilt_core::model::{Id, Relation, RelationKind, is_lineal};

use crate::resolve::{Location, ResolveTree, TargetRole, resolve};
use crate::visit::ModuleFacts;
use crate::walk::ModulePath;

/// The tag on a crate's `tests` component.
pub const TESTS_COMPONENT: &str = "tests";
/// The tag on a crate's `examples` component.
pub const EXAMPLES_COMPONENT: &str = "examples";

/// The element a module of a target belongs to. Every test and bench
/// target of a crate collapses into `<crate>.tests`, every example
/// target into `<crate>.examples`, a bin beside a lib is
/// `<crate>.<bin>` with its modules under it, and the main target's
/// modules sit directly under the crate.
pub fn element_id(loc: &Location) -> Id {
    let krate = loc.target.crate_name.clone();
    match &loc.target.role {
        TargetRole::Main => [vec![krate], loc.module.clone()].concat(),
        TargetRole::Bin(name) => [vec![krate, name.clone()], loc.module.clone()].concat(),
        TargetRole::Tests(_) => vec![krate, TESTS_COMPONENT.to_string()],
        TargetRole::Examples(_) => vec![krate, EXAMPLES_COMPONENT.to_string()],
    }
}

/// The label for a path that ends on a module: its name, or the crate
/// name for a root.
fn module_label(loc: &Location) -> String {
    loc.module
        .last()
        .cloned()
        .unwrap_or_else(|| loc.target.crate_name.clone())
}

/// Relations from every module's facts, sorted by (source, target).
pub fn relations(tree: &ResolveTree, facts: &BTreeMap<Location, ModuleFacts>) -> Vec<Relation> {
    let mut acc: BTreeMap<(Id, Id), (RelationKind, BTreeSet<String>)> = BTreeMap::new();
    for (from, module_facts) in facts {
        let source = element_id(from);
        // Every non-glob `use`, then every other path; resolution
        // decides what a bare name means in this module (an import
        // alias, a glob import, a child, a local, or nothing).
        let candidates = module_facts
            .uses
            .iter()
            .filter(|u| !u.glob)
            .map(|u| (&u.path, RelationKind::Uses))
            .chain(module_facts.refs.iter().map(|r| (&r.path, r.kind)));
        for (path, kind) in candidates {
            let Some(resolved) = resolve(tree, from, path) else {
                continue;
            };
            let target = element_id(&resolved.module);
            if is_lineal(&source, &target) {
                continue;
            }
            let item = resolved
                .item
                .unwrap_or_else(|| module_label(&resolved.module));
            let entry = acc
                .entry((source.clone(), target))
                .or_insert((RelationKind::Uses, BTreeSet::new()));
            entry.0 = entry.0.min(kind);
            entry.1.insert(item);
        }
    }
    acc.into_iter()
        .map(|((source, target), (kind, items))| Relation {
            source,
            target,
            kind,
            items: items.into_iter().collect(),
            technology: None,
        })
        .collect()
}

/// A module path as an element id suffix; here so callers share one
/// spelling of the collapse rule.
pub fn location(target: &crate::resolve::TargetKey, module: &ModulePath) -> Location {
    Location {
        target: target.clone(),
        module: module.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::TargetKey;
    use crate::visit::{RawPath, RawRef, UseEntry};

    fn p(s: &str) -> RawPath {
        RawPath::parse(s).unwrap()
    }

    fn m(s: &str) -> ModulePath {
        if s.is_empty() {
            vec![]
        } else {
            s.split('.').map(str::to_string).collect()
        }
    }

    fn use_of(path: &str) -> UseEntry {
        let path = p(path);
        UseEntry {
            alias: path.segments.last().cloned().unwrap_or_default(),
            path,
            glob: false,
            reexport: false,
        }
    }

    fn r(path: &str, kind: RelationKind) -> RawRef {
        RawRef {
            path: p(path),
            kind,
        }
    }

    /// `app` with `a` (defining `Thing`, `make`), `b`, `b.c`; `app`'s
    /// tests target with a `common` module.
    fn tree() -> (ResolveTree, TargetKey, TargetKey) {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        let tests = TargetKey {
            crate_name: "app".into(),
            role: TargetRole::Tests("smoke".into()),
        };
        t.add_target(app.clone(), true);
        t.add_target(tests.clone(), false);
        t.module_mut(&app, &m("a"))
            .defines
            .extend(["Thing".to_string(), "make".to_string()]);
        t.module_mut(&app, &m("b.c"));
        t.module_mut(&tests, &m("common"));
        (t, app, tests)
    }

    fn rel(
        tree: &ResolveTree,
        facts: &BTreeMap<Location, ModuleFacts>,
    ) -> Vec<(String, String, RelationKind, Vec<String>)> {
        relations(tree, facts)
            .into_iter()
            .map(|r| (r.source.join("."), r.target.join("."), r.kind, r.items))
            .collect()
    }

    #[test]
    fn element_ids_follow_the_target_role() {
        let main = location(&TargetKey::main("app"), &m("a.b"));
        assert_eq!(element_id(&main), ["app", "a", "b"]);
        let bin = location(
            &TargetKey {
                crate_name: "app".into(),
                role: TargetRole::Bin("cli".into()),
            },
            &m("x"),
        );
        assert_eq!(element_id(&bin), ["app", "cli", "x"]);
        let tests = location(
            &TargetKey {
                crate_name: "app".into(),
                role: TargetRole::Tests("t".into()),
            },
            &m("common"),
        );
        assert_eq!(element_id(&tests), ["app", "tests"]);
        let ex = location(
            &TargetKey {
                crate_name: "app".into(),
                role: TargetRole::Examples("e".into()),
            },
            &m("x"),
        );
        assert_eq!(element_id(&ex), ["app", "examples"]);
    }

    #[test]
    fn the_strongest_kind_wins_for_a_pair_and_items_are_sorted_and_deduped() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                uses: vec![use_of("crate::a::Thing")],
                refs: vec![
                    r("crate::a::make", RelationKind::Calls),
                    r("crate::a::Thing", RelationKind::NamesType),
                    r("crate::a::Thing", RelationKind::NamesType),
                ],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [(
                "app.b".to_string(),
                "app.a".to_string(),
                RelationKind::Calls,
                vec!["Thing".to_string(), "make".to_string()]
            )]
        );
    }

    #[test]
    fn an_edge_to_an_ancestor_or_descendant_is_never_recorded() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b.c")),
            ModuleFacts {
                refs: vec![
                    r("super::x", RelationKind::NamesType),
                    r("crate::b", RelationKind::NamesType),
                ],
                ..Default::default()
            },
        );
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                refs: vec![r("self::c::Y", RelationKind::NamesType)],
                ..Default::default()
            },
        );
        facts.insert(
            location(&app, &m("")),
            ModuleFacts {
                uses: vec![use_of("crate::a::Thing")],
                ..Default::default()
            },
        );
        assert_eq!(rel(&t, &facts), []);
    }

    #[test]
    fn a_bare_name_nothing_in_scope_explains_yields_nothing() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                refs: vec![
                    r("Vec", RelationKind::NamesType),
                    r("Vec::new", RelationKind::Constructs),
                ],
                ..Default::default()
            },
        );
        assert_eq!(rel(&t, &facts), []);
    }

    #[test]
    fn a_bare_import_alias_strengthens_the_kind() {
        let (mut t, app, _) = tree();
        t.module_mut(&app, &m("b"))
            .imports
            .insert("Thing".into(), p("crate::a::Thing"));
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                refs: vec![r("Thing", RelationKind::Constructs)],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [(
                "app.b".to_string(),
                "app.a".to_string(),
                RelationKind::Constructs,
                vec!["Thing".to_string()]
            )]
        );
    }

    #[test]
    fn a_bare_name_from_a_glob_import_is_a_relation() {
        let (mut t, app, _) = tree();
        t.module_mut(&app, &m("b"))
            .globs
            .push((p("crate::a"), false));
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                refs: vec![r("Thing", RelationKind::NamesType)],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [(
                "app.b".to_string(),
                "app.a".to_string(),
                RelationKind::NamesType,
                vec!["Thing".to_string()]
            )]
        );
    }

    #[test]
    fn a_single_segment_after_an_anchor_is_kept() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                refs: vec![
                    r("crate::a", RelationKind::NamesType),
                    r("super::a", RelationKind::NamesType),
                ],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [(
                "app.b".to_string(),
                "app.a".to_string(),
                RelationKind::NamesType,
                vec!["a".to_string()]
            )]
        );
    }

    #[test]
    fn a_bare_path_of_two_or_more_segments_is_kept() {
        let (t, _, tests) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&tests, &m("")),
            ModuleFacts {
                refs: vec![r("app::a::Thing", RelationKind::Constructs)],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [(
                "app.tests".to_string(),
                "app.a".to_string(),
                RelationKind::Constructs,
                vec!["Thing".to_string()]
            )]
        );
    }

    #[test]
    fn a_use_of_a_module_is_labeled_with_the_module_name_and_of_a_crate_with_the_crate_name() {
        let (mut t, app, _) = tree();
        t.add_target(TargetKey::main("other"), true);
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                uses: vec![use_of("crate::a"), use_of("other")],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [
                (
                    "app.b".to_string(),
                    "app.a".to_string(),
                    RelationKind::Uses,
                    vec!["a".to_string()]
                ),
                (
                    "app.b".to_string(),
                    "other".to_string(),
                    RelationKind::Uses,
                    vec!["other".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn a_glob_use_records_nothing_by_itself() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        let mut g = use_of("crate::a");
        g.glob = true;
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                uses: vec![g],
                ..Default::default()
            },
        );
        assert_eq!(rel(&t, &facts), []);
    }

    #[test]
    fn references_from_a_test_target_come_from_the_tests_component_and_stay_inside_it() {
        let (t, _, tests) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&tests, &m("")),
            ModuleFacts {
                uses: vec![use_of("app::a::Thing"), use_of("crate::common::Server")],
                ..Default::default()
            },
        );
        assert_eq!(
            rel(&t, &facts),
            [(
                "app.tests".to_string(),
                "app.a".to_string(),
                RelationKind::Uses,
                vec!["Thing".to_string()]
            )]
        );
    }

    #[test]
    fn unresolvable_paths_produce_no_relation() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b")),
            ModuleFacts {
                uses: vec![use_of("std::fs")],
                refs: vec![r("serde::Serialize", RelationKind::Implements)],
                ..Default::default()
            },
        );
        assert_eq!(rel(&t, &facts), []);
    }

    #[test]
    fn relations_come_out_sorted_by_source_then_target() {
        let (t, app, _) = tree();
        let mut facts = BTreeMap::new();
        facts.insert(
            location(&app, &m("b.c")),
            ModuleFacts {
                uses: vec![use_of("crate::a::Thing")],
                ..Default::default()
            },
        );
        facts.insert(
            location(&app, &m("a")),
            ModuleFacts {
                uses: vec![use_of("crate::b::c")],
                ..Default::default()
            },
        );
        let pairs: Vec<(String, String)> = rel(&t, &facts)
            .into_iter()
            .map(|(s, t, _, _)| (s, t))
            .collect();
        assert_eq!(
            pairs,
            [
                ("app.a".to_string(), "app.b.c".to_string()),
                ("app.b.c".to_string(), "app.a".to_string())
            ]
        );
    }
}
