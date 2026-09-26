// Tests that run the real LikeC4 parser over what the emitter produces.
// They need `npx` (Node) and, the first time, the network to fetch the
// pinned likec4; a fresh clone may lack both, so they are gated, not
// skipped. The `LikeC4 validate` CI job runs them on ubuntu.

mod common;

use asbuilt::likec4;
use asbuilt_core::model::{Element, ElementKind, Id, Model, Relation, RelationKind};
use asbuilt_core::{EmitOptions, emit};
use common::Workspace;

fn id(s: &str) -> Id {
    s.split('.').map(str::to_string).collect()
}

fn element(s: &str, kind: ElementKind) -> Element {
    Element {
        id: id(s),
        kind,
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

/// Every shape the emitter can produce, plus the ids most likely to
/// collide with LikeC4 keywords: a module named `model`, one named
/// `view`, one named `element`, and a crate named `views`.
fn probe_model() -> Model {
    let mut app = element("app", ElementKind::Container);
    app.technology = Some("library crate".into());
    app.path = Some("crates/app".into());
    app.description = Some("An app. It's the one with a quote.\n\n- a list\n- of two".into());
    let mut model_mod = element("app.model", ElementKind::Component);
    model_mod.path = Some("crates/app/src/model.rs".into());
    let mut view_mod = element("app.view", ElementKind::Component);
    view_mod.path = Some("crates/app/src/view.rs".into());
    let mut inner = element("app.view.element", ElementKind::Component);
    inner.path = Some("crates/app/src/view/element.rs".into());
    let mut tests = element("app.tests", ElementKind::Component);
    tests.tags = vec!["tests".into()];
    tests.path = Some("crates/app/tests".into());
    let mut views = element("views", ElementKind::Container);
    views.technology = Some("binary".into());
    let mut digit = element("3d-lib", ElementKind::Container);
    digit.technology = Some("library crate".into());
    let mut driver = element("node_driver", ElementKind::External("process".into()));
    driver.tags = vec!["external".into()];
    driver.technology = Some("Node.js process".into());
    let mut browsers = element("browsers", ElementKind::External("browser".into()));
    browsers.tags = vec!["external".into()];

    let mut spawns = relation("app.model", "node_driver", RelationKind::Uses, &["spawns"]);
    spawns.technology = Some("stdio".into());
    Model {
        elements: vec![
            app, model_mod, view_mod, inner, tests, views, digit, driver, browsers,
        ],
        relations: vec![
            relation(
                "app.model",
                "app.view",
                RelationKind::Implements,
                &["Render"],
            ),
            relation(
                "app.view",
                "app.model",
                RelationKind::Constructs,
                &["Model", "Thing"],
            ),
            relation(
                "app.tests",
                "app.view.element",
                RelationKind::Calls,
                &["render"],
            ),
            relation("views", "app.model", RelationKind::NamesType, &["Model"]),
            relation("3d-lib", "app", RelationKind::Uses, &[]),
            spawns,
            relation("node_driver", "browsers", RelationKind::Uses, &["drives"]),
        ],
        ..Default::default()
    }
}

#[test]
#[ignore = "needs npx (Node) and network; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_validate_accepts_an_emitted_model() {
    let text = emit(
        &probe_model(),
        &EmitOptions::for_output_path("docs/architecture/model.c4"),
    );
    let ws = Workspace::new();
    let path = ws.write("docs/architecture/model.c4", &text);

    let result = likec4::validate(path.parent().unwrap());

    assert!(
        result.is_ok(),
        "{}\n--- emitted ---\n{text}",
        result.unwrap_err()
    );
}

#[test]
#[ignore = "needs npx (Node) and network; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_validate_rejects_a_relation_to_an_unknown_element() {
    let ws = Workspace::new();
    let path = ws.write(
        "model.c4",
        "specification {\n  element container\n}\nmodel {\n  a = container 'a'\n  a -> nope\n}\n",
    );

    let result = likec4::validate(path.parent().unwrap());

    match result {
        Err(likec4::LikeC4Error::Failed { output, .. }) => {
            assert!(
                output.contains("nope"),
                "output should name the bad id:\n{output}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}
