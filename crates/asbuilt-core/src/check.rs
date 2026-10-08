//! The drift check: the committed model against a fresh survey, as a
//! unified diff when they differ, and as the changes behind it (an
//! element added, a relation's items) when the committed model is one
//! asbuilt wrote. Line endings are normalized first so a consumer with
//! `autocrlf` gets a clean check.

use similar::TextDiff;

use crate::read::{Written, WrittenElement, WrittenRelation, read};

/// What the comparison found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Current,
    /// A unified diff from the committed text to the fresh one.
    Drift(String),
}

pub(crate) fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Compare the committed text with the fresh survey; `label` is the
/// path shown in the diff header.
pub fn compare(committed: &str, fresh: &str, label: &str) -> Outcome {
    let committed = normalize(committed);
    let fresh = normalize(fresh);
    if committed == fresh {
        return Outcome::Current;
    }
    let diff = TextDiff::from_lines(&committed, &fresh)
        .unified_diff()
        .header(&format!("a/{label}"), &format!("b/{label}"))
        .to_string();
    Outcome::Drift(diff)
}

/// One difference between the committed model and a fresh survey, named
/// by what changed rather than which lines did. Ids are as the file
/// writes them; kinds are the specification's keywords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    ElementAdded {
        id: String,
        kind: String,
    },
    ElementRemoved {
        id: String,
        kind: String,
    },
    /// An element in both, with the fields that differ in the order emit
    /// writes them; `kind` is the fresh one.
    ElementChanged {
        id: String,
        kind: String,
        fields: Vec<ElementField>,
    },
    RelationAdded {
        from: String,
        to: String,
        kind: String,
    },
    RelationRemoved {
        from: String,
        to: String,
        kind: String,
    },
    /// A relation in both: its kind when that changed (committed, then
    /// fresh), whether its label changed and the names it gained and lost,
    /// and whether its technology changed. The names are the label split
    /// on `, ` (a surveyed relation's items, an external's title in its
    /// pieces), so they describe a label change; a reorder or a repeat
    /// changes the label with none gained or lost.
    RelationChanged {
        from: String,
        to: String,
        kind: Option<(String, String)>,
        label: bool,
        added: Vec<String>,
        removed: Vec<String>,
        technology: bool,
    },
}

/// An element field that can change, in the order emit writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementField {
    Kind,
    Title,
    Description,
    Technology,
    Path,
    Tags,
}

impl ElementField {
    /// The field's name in the `.c4` file.
    pub fn name(self) -> &'static str {
        match self {
            ElementField::Kind => "kind",
            ElementField::Title => "title",
            ElementField::Description => "description",
            ElementField::Technology => "technology",
            ElementField::Path => "path",
            ElementField::Tags => "tags",
        }
    }
}

/// The changes from the committed text to the fresh one, or `None` when
/// either is not a model asbuilt wrote (the diff is then all there is).
pub fn summarize(committed: &str, fresh: &str) -> Option<Vec<Change>> {
    Some(changes(&read(committed)?, &read(fresh)?))
}

/// Every element added, removed or changed, then every relation, each in
/// id order.
pub fn changes(committed: &Written, fresh: &Written) -> Vec<Change> {
    let mut out = Vec::new();
    for (id, element) in &fresh.elements {
        match committed.elements.get(id) {
            None => out.push(Change::ElementAdded {
                id: id.clone(),
                kind: element.kind.clone(),
            }),
            Some(before) => {
                let fields = element_fields(before, element);
                if !fields.is_empty() {
                    out.push(Change::ElementChanged {
                        id: id.clone(),
                        kind: element.kind.clone(),
                        fields,
                    });
                }
            }
        }
    }
    for (id, element) in &committed.elements {
        if !fresh.elements.contains_key(id) {
            out.push(Change::ElementRemoved {
                id: id.clone(),
                kind: element.kind.clone(),
            });
        }
    }
    sort_by_id(&mut out);
    let mut relations = Vec::new();
    for ((from, to), relation) in &fresh.relations {
        let pair = (from.clone(), to.clone());
        match committed.relations.get(&pair) {
            None => relations.push(Change::RelationAdded {
                from: from.clone(),
                to: to.clone(),
                kind: relation.kind.clone(),
            }),
            Some(before) => {
                if let Some(change) = relation_change(from, to, before, relation) {
                    relations.push(change);
                }
            }
        }
    }
    for ((from, to), relation) in &committed.relations {
        if !fresh.relations.contains_key(&(from.clone(), to.clone())) {
            relations.push(Change::RelationRemoved {
                from: from.clone(),
                to: to.clone(),
                kind: relation.kind.clone(),
            });
        }
    }
    sort_by_id(&mut relations);
    out.extend(relations);
    out
}

/// Element changes by id, relation changes by source then target.
fn sort_by_id(changes: &mut [Change]) {
    changes.sort_by(|a, b| key(a).cmp(&key(b)));
}

fn key(change: &Change) -> (&str, &str) {
    match change {
        Change::ElementAdded { id, .. }
        | Change::ElementRemoved { id, .. }
        | Change::ElementChanged { id, .. } => (id, ""),
        Change::RelationAdded { from, to, .. }
        | Change::RelationRemoved { from, to, .. }
        | Change::RelationChanged { from, to, .. } => (from, to),
    }
}

fn element_fields(before: &WrittenElement, after: &WrittenElement) -> Vec<ElementField> {
    [
        (ElementField::Kind, before.kind != after.kind),
        (ElementField::Title, before.title != after.title),
        (
            ElementField::Description,
            before.description != after.description,
        ),
        (
            ElementField::Technology,
            before.technology != after.technology,
        ),
        (ElementField::Path, before.path != after.path),
        (ElementField::Tags, before.tags != after.tags),
    ]
    .into_iter()
    .filter_map(|(field, differs)| differs.then_some(field))
    .collect()
}

fn relation_change(
    from: &str,
    to: &str,
    before: &WrittenRelation,
    after: &WrittenRelation,
) -> Option<Change> {
    let names = |relation: &WrittenRelation| -> Vec<String> {
        relation
            .label
            .as_deref()
            .map(|label| label.split(", ").map(str::to_string).collect())
            .unwrap_or_default()
    };
    let (old, new) = (names(before), names(after));
    let added: Vec<String> = new.iter().filter(|n| !old.contains(n)).cloned().collect();
    let removed: Vec<String> = old.iter().filter(|n| !new.contains(n)).cloned().collect();
    let kind = (before.kind != after.kind).then(|| (before.kind.clone(), after.kind.clone()));
    let label = before.label != after.label;
    let technology = before.technology != after.technology;
    if kind.is_none() && !label && !technology {
        return None;
    }
    Some(Change::RelationChanged {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        label,
        added,
        removed,
        technology,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_is_current() {
        assert_eq!(compare("a\nb\n", "a\nb\n", "m.c4"), Outcome::Current);
    }

    #[test]
    fn crlf_in_the_committed_text_is_not_drift() {
        assert_eq!(compare("a\r\nb\r\n", "a\nb\n", "m.c4"), Outcome::Current);
    }

    #[test]
    fn a_changed_line_is_drift_with_both_versions_and_the_label_in_the_header() {
        match compare("a\nb\nc\n", "a\nB\nc\n", "docs/model.c4") {
            Outcome::Drift(diff) => {
                assert!(
                    diff.starts_with("--- a/docs/model.c4\n+++ b/docs/model.c4\n"),
                    "{diff}"
                );
                assert!(diff.contains("\n-b\n"), "{diff}");
                assert!(diff.contains("\n+B\n"), "{diff}");
            }
            other => panic!("expected Drift, got {other:?}"),
        }
    }

    #[test]
    fn an_added_line_is_drift() {
        assert!(matches!(compare("a\n", "a\nb\n", "m"), Outcome::Drift(_)));
    }

    fn element(kind: &str, title: &str) -> WrittenElement {
        WrittenElement {
            kind: kind.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    fn relation(kind: &str, label: Option<&str>) -> WrittenRelation {
        WrittenRelation {
            kind: kind.into(),
            label: label.map(str::to_string),
            technology: None,
        }
    }

    fn written(
        elements: &[(&str, WrittenElement)],
        relations: &[(&str, &str, WrittenRelation)],
    ) -> Written {
        Written {
            elements: elements
                .iter()
                .map(|(id, e)| (id.to_string(), e.clone()))
                .collect(),
            relations: relations
                .iter()
                .map(|(from, to, r)| ((from.to_string(), to.to_string()), r.clone()))
                .collect(),
        }
    }

    fn base() -> Written {
        written(
            &[
                ("app", element("container", "app")),
                ("app.server", element("component", "server")),
            ],
            &[(
                "app",
                "app.server",
                relation("calls", Some("Server, serve")),
            )],
        )
    }

    #[test]
    fn identical_models_have_no_changes() {
        assert_eq!(changes(&base(), &base()), vec![]);
    }

    #[test]
    fn an_element_only_in_the_fresh_model_is_added() {
        let mut fresh = base();
        fresh
            .elements
            .insert("app.client".into(), element("component", "client"));
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::ElementAdded {
                id: "app.client".into(),
                kind: "component".into()
            }]
        );
    }

    #[test]
    fn an_element_only_in_the_committed_model_is_removed() {
        let mut committed = base();
        committed
            .elements
            .insert("app.old".into(), element("component", "old"));
        assert_eq!(
            changes(&committed, &base()),
            vec![Change::ElementRemoved {
                id: "app.old".into(),
                kind: "component".into()
            }]
        );
    }

    #[test]
    fn an_element_s_changed_fields_are_named_in_emit_s_order() {
        let mut fresh = base();
        let server = fresh.elements.get_mut("app.server").unwrap();
        server.path = Some("app/src/server/mod.rs".into());
        server.description = Some("Serves.".into());
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::ElementChanged {
                id: "app.server".into(),
                kind: "component".into(),
                fields: vec![ElementField::Description, ElementField::Path]
            }]
        );
    }

    #[test]
    fn an_element_s_kind_change_names_the_fresh_kind() {
        let mut fresh = base();
        fresh.elements.get_mut("app.server").unwrap().kind = "bin".into();
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::ElementChanged {
                id: "app.server".into(),
                kind: "bin".into(),
                fields: vec![ElementField::Kind]
            }]
        );
    }

    #[test]
    fn a_relation_only_in_the_fresh_model_is_added() {
        let mut fresh = base();
        fresh.relations.insert(
            ("app.server".into(), "app".into()),
            relation("names", Some("App")),
        );
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::RelationAdded {
                from: "app.server".into(),
                to: "app".into(),
                kind: "names".into()
            }]
        );
    }

    #[test]
    fn a_relation_only_in_the_committed_model_is_removed() {
        let mut fresh = base();
        fresh.relations.clear();
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::RelationRemoved {
                from: "app".into(),
                to: "app.server".into(),
                kind: "calls".into()
            }]
        );
    }

    #[test]
    fn a_relation_s_label_change_names_what_it_gained_and_lost() {
        let mut fresh = base();
        fresh.relations.insert(
            ("app".into(), "app.server".into()),
            relation("calls", Some("Server, start")),
        );
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::RelationChanged {
                from: "app".into(),
                to: "app.server".into(),
                kind: None,
                label: true,
                added: vec!["start".into()],
                removed: vec!["serve".into()],
                technology: false
            }]
        );
    }

    #[test]
    fn a_relation_s_kind_change_names_both_kinds() {
        let mut fresh = base();
        fresh.relations.insert(
            ("app".into(), "app.server".into()),
            relation("constructs", Some("Server, serve")),
        );
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::RelationChanged {
                from: "app".into(),
                to: "app.server".into(),
                kind: Some(("calls".into(), "constructs".into())),
                label: false,
                added: vec![],
                removed: vec![],
                technology: false
            }]
        );
    }

    #[test]
    fn a_relation_s_technology_change_is_flagged() {
        let mut fresh = base();
        fresh
            .relations
            .get_mut(&("app".to_string(), "app.server".to_string()))
            .unwrap()
            .technology = Some("stdio".into());
        assert_eq!(
            changes(&base(), &fresh),
            vec![Change::RelationChanged {
                from: "app".into(),
                to: "app.server".into(),
                kind: None,
                label: false,
                added: vec![],
                removed: vec![],
                technology: true
            }]
        );
    }

    #[test]
    fn element_changes_come_first_in_id_order_then_relations() {
        // The removed id sorts before the added one, which the order the
        // two are found in would not give.
        let mut fresh = base();
        fresh.elements.remove("app.server");
        fresh
            .elements
            .insert("app.z".into(), element("component", "z"));
        fresh.relations.clear();
        assert_eq!(
            changes(&base(), &fresh),
            vec![
                Change::ElementRemoved {
                    id: "app.server".into(),
                    kind: "component".into()
                },
                Change::ElementAdded {
                    id: "app.z".into(),
                    kind: "component".into()
                },
                Change::RelationRemoved {
                    from: "app".into(),
                    to: "app.server".into(),
                    kind: "calls".into()
                },
            ]
        );
    }

    #[test]
    fn a_committed_text_asbuilt_did_not_write_has_no_summary() {
        assert_eq!(summarize("not a model\n", "not a model either\n"), None);
    }

    #[test]
    fn a_label_that_changes_without_gaining_or_losing_a_name_is_a_change() {
        let mut committed = base();
        committed.relations.insert(
            ("app".into(), "app.server".into()),
            relation("uses", Some("reads, writes, writes")),
        );
        let mut fresh = base();
        fresh.relations.insert(
            ("app".into(), "app.server".into()),
            relation("uses", Some("reads, writes")),
        );
        assert_eq!(
            changes(&committed, &fresh),
            vec![Change::RelationChanged {
                from: "app".into(),
                to: "app.server".into(),
                kind: None,
                label: true,
                added: vec![],
                removed: vec![],
                technology: false
            }]
        );
    }

    #[test]
    fn every_element_field_is_named_as_the_file_writes_it() {
        assert_eq!(
            [
                ElementField::Kind,
                ElementField::Title,
                ElementField::Description,
                ElementField::Technology,
                ElementField::Path,
                ElementField::Tags,
            ]
            .map(ElementField::name),
            ["kind", "title", "description", "technology", "path", "tags"]
        );
    }
}
