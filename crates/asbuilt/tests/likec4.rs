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

/// Every fixture snapshot, one directory at a time (likec4 merges every
/// `.c4` under a path into one model). The count is asserted so an
/// empty glob cannot pass.
#[test]
#[ignore = "needs npx (Node) and network; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_validate_accepts_every_fixture_snapshot() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../asbuilt-rust/tests/fixtures");
    let ws = Workspace::new();
    let mut validated = 0;
    let mut cases: Vec<_> = std::fs::read_dir(&fixtures)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    cases.sort();
    for case in &cases {
        let snapshot = case.join("expected.c4");
        if !snapshot.is_file() {
            continue;
        }
        let name = case.file_name().unwrap().to_str().unwrap();
        let text = std::fs::read_to_string(&snapshot).unwrap();
        let path = ws.write(&format!("{name}/model.c4"), &text);
        let result = likec4::validate(path.parent().unwrap());
        assert!(result.is_ok(), "{name}: {}", result.unwrap_err());
        validated += 1;
    }
    assert_eq!(
        validated,
        cases.len(),
        "every fixture directory must carry an expected.c4"
    );
    assert!(validated > 0);
}

#[test]
#[ignore = "needs npx (Node) and network; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_validate_accepts_the_consumer_fixture_model() {
    let committed = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/consumer/docs/architecture/model.c4");
    let text = std::fs::read_to_string(&committed).unwrap();
    let ws = Workspace::new();
    let path = ws.write("model.c4", &text);

    let result = likec4::validate(path.parent().unwrap());

    assert!(result.is_ok(), "{}", result.unwrap_err());
}

fn consumer_copy() -> Workspace {
    let ws = Workspace::new();
    ws.copy_from(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/consumer"));
    ws
}

#[test]
#[ignore = "needs npx (Node) and network; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_export_json_is_free_of_relative_links_and_keeps_the_elements() {
    let ws = consumer_copy();
    let dir = ws.root().join("docs/architecture");
    let out = dir.join("model.json");

    likec4::export_json(&dir, &out).unwrap();

    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        !text.contains("\"relative\""),
        "relative links survived:\n{text}"
    );
    assert!(
        !text.contains("file://"),
        "an absolute file URL survived:\n{text}"
    );
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(value["elements"]["app.server"].is_object(), "{text}");
    assert!(value["elements"]["node_driver"].is_object(), "{text}");
    assert!(
        text.ends_with("}\n"),
        "pretty-printed with a trailing newline"
    );
}

#[test]
#[ignore = "needs npx (Node), network and Graphviz dot; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_render_writes_one_svg_per_view_and_replaces_only_its_own_files() {
    let ws = consumer_copy();
    let dir = ws.root().join("docs/architecture");
    let out = dir.join("views");
    // A view an earlier render made that the model no longer has, and a
    // file no render made.
    ws.write("docs/architecture/views/gone.svg", "<svg/>");
    ws.write(
        "docs/architecture/views/gone.dot",
        "digraph { graph [likec4_viewId=gone]; }",
    );
    ws.write("docs/architecture/views/logo.svg", "<svg id=\"logo\"/>");

    let svgs = likec4::render(&dir, &out).unwrap();

    assert!(
        !out.join("gone.svg").exists(),
        "a stale SVG survived render"
    );
    assert!(
        !out.join("gone.dot").exists(),
        "a stale dot survived render"
    );
    assert!(out.join("logo.svg").is_file(), "render removed a stranger");
    assert!(!out.join(likec4::RENDER_SCRATCH).exists());

    let names: Vec<String> = svgs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["index.svg", "view_app.svg", "view_e2e.svg"]);
    for svg in &svgs {
        let text = std::fs::read_to_string(svg).unwrap();
        assert!(text.contains("<svg"), "{} is not an SVG", svg.display());
    }
}

#[test]
#[ignore = "needs npx (Node), network and Graphviz dot; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_a_themed_kind_validates_and_renders_in_its_color() {
    let ws = consumer_copy();
    let config = ws.root().join("asbuilt.toml");
    let mut toml = std::fs::read_to_string(&config).unwrap();
    toml.push_str("\n[theme]\ncontainer = \"#f0a884\"\n");
    std::fs::write(&config, toml).unwrap();
    let root = ws.root().to_str().unwrap();
    for args in [["survey", root], ["validate", root], ["render", root]] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_asbuilt"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let dot = std::fs::read_to_string(ws.root().join("docs/architecture/views/index.dot")).unwrap();
    assert!(dot.contains("fillcolor=\"#f0a884\""), "{dot}");
}

#[test]
#[ignore = "needs npx (Node), network and Graphviz dot; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_the_cli_validate_export_and_render_subcommands_exit_zero() {
    let ws = consumer_copy();
    let root = ws.root().to_str().unwrap();
    for args in [
        vec!["validate", root],
        vec!["export", "json", root],
        vec!["render", root, "-o", "out/views"],
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_asbuilt"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(ws.root().join("docs/architecture/model.json").is_file());
    assert!(ws.root().join("out/views/index.svg").is_file());
}

#[test]
#[ignore = "needs npx (Node) and network; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_the_cli_validate_exits_one_on_a_broken_curated_view() {
    let ws = consumer_copy();
    ws.write(
        "docs/architecture/views.c4",
        "views {\n  view broken {\n    include app.nope\n  }\n}\n",
    );

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_asbuilt"))
        .args(["validate", ws.root().to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("nope") && stderr.contains("rejected"),
        "{stderr}"
    );
}

#[test]
#[ignore = "needs npx (Node), network and Graphviz dot; run with: cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'"]
fn likec4_docs_renders_every_view_and_embeds_them() {
    let ws = consumer_copy();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_asbuilt"))
        .args(["docs", ws.root().to_str().unwrap(), "-o", "out"])
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("no SVG"), "{stderr}");
    for view in ["index", "view_app", "view_e2e"] {
        assert!(
            ws.root().join(format!("out/views/{view}.svg")).is_file(),
            "{view}.svg"
        );
    }
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(index.contains("<img src=\"views/index.svg\""), "{index}");
    let app = std::fs::read_to_string(ws.root().join("out/containers/app.html")).unwrap();
    assert!(app.contains("<img src=\"../views/view_app.svg\""), "{app}");
}
