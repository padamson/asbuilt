// Each fixture under tests/fixtures/<case>/ is a small workspace that
// exists to prove one resolver rule. Two tests per case: a claim on the
// model (the relation the case is about, by source and target), and a
// byte-exact comparison of the emitted `.c4` against `expected.c4`
// beside the fixture. `UPDATE_EXPECT=1` rewrites a stale snapshot and
// passes; CI never sets it.

use std::path::{Path, PathBuf};

use asbuilt_core::model::{Element, Model, Relation, RelationKind};
use asbuilt_core::{Config, EmitOptions, emit};
use asbuilt_rust::{RustConfig, RustFrontend, analyze};

fn fixture_root(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(case)
}

fn survey_with(case: &str, rust: &RustConfig) -> (Model, String) {
    let root = fixture_root(case);
    let model = analyze(&root, rust).unwrap_or_else(|e| panic!("{case}: {e}"));
    let text = emit(&model, &EmitOptions::for_output_path("expected.c4"));
    (model, text)
}

fn survey(case: &str) -> (Model, String) {
    let root = fixture_root(case);
    let config = Config::load(&root).unwrap();
    let rust = RustConfig::from_config(&config).unwrap();
    survey_with(case, &rust)
}

fn id(s: &str) -> Vec<String> {
    s.split('.').map(str::to_string).collect()
}

fn relation<'a>(model: &'a Model, source: &str, target: &str) -> &'a Relation {
    model
        .relations
        .iter()
        .find(|r| r.source == id(source) && r.target == id(target))
        .unwrap_or_else(|| {
            let all: Vec<String> = model
                .relations
                .iter()
                .map(|r| {
                    format!(
                        "{} -[{}]-> {} {:?}",
                        r.source.join("."),
                        r.kind.keyword(),
                        r.target.join("."),
                        r.items
                    )
                })
                .collect();
            panic!(
                "no relation {source} -> {target}; have:\n{}",
                all.join("\n")
            )
        })
}

fn no_relation(model: &Model, source: &str, target: &str) {
    assert!(
        !model
            .relations
            .iter()
            .any(|r| r.source == id(source) && r.target == id(target)),
        "unexpected relation {source} -> {target}"
    );
}

fn element<'a>(model: &'a Model, path: &str) -> &'a Element {
    model.element(&id(path)).unwrap_or_else(|| {
        let all: Vec<String> = model.elements.iter().map(|e| e.id.join(".")).collect();
        panic!("no element {path}; have {all:?}")
    })
}

fn first_difference(expected: &str, actual: &str) -> String {
    for (n, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
        if e != a {
            return format!("line {}:\n  expected: {e}\n  actual:   {a}", n + 1);
        }
    }
    format!(
        "line counts differ: expected {} lines, actual {}",
        expected.lines().count(),
        actual.lines().count()
    )
}

fn assert_snapshot(case: &str, actual: &str) {
    let path = fixture_root(case).join("expected.c4");
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if expected == actual {
        return;
    }
    if std::env::var_os("UPDATE_EXPECT").is_some_and(|v| v == "1") {
        std::fs::write(&path, actual).unwrap();
        eprintln!("rewrote {}", path.display());
        return;
    }
    panic!(
        "{} is stale; run with UPDATE_EXPECT=1 to rewrite it\n{}",
        path.display(),
        first_difference(&expected, actual)
    );
}

// reexport_chain

#[test]
fn a_chain_of_pub_use_lands_on_the_defining_module_with_the_original_names() {
    let (model, _) = survey("reexport_chain");
    let r = relation(&model, "app.consumer", "app.protocol.page");
    assert_eq!(r.kind, RelationKind::Constructs);
    assert_eq!(r.items, ["Frame", "Page"]);
}

#[test]
fn a_reexport_by_a_parent_is_structure_not_coupling() {
    let (model, _) = survey("reexport_chain");
    no_relation(&model, "app", "app.protocol");
    no_relation(&model, "app.protocol", "app.protocol.page");
}

#[test]
fn a_description_is_the_first_doc_paragraph() {
    let (model, _) = survey("reexport_chain");
    assert_eq!(
        element(&model, "app.protocol.page").description.as_deref(),
        Some("Page objects.")
    );
    assert_eq!(element(&model, "app").description.as_deref(), Some("App."));
    assert_eq!(element(&model, "app").path.as_deref(), Some("."));
}

#[test]
fn a_root_level_test_target_resolves_through_the_crate_root_reexport_and_sits_in_tests() {
    let (model, _) = survey("reexport_chain");
    let r = relation(&model, "app.tests", "app.protocol.page");
    assert_eq!(r.kind, RelationKind::Constructs);
    assert_eq!(r.items, ["Page"]);
    assert_eq!(element(&model, "app.tests").path.as_deref(), Some("tests"));
}

#[test]
fn the_reexport_chain_snapshot_is_current() {
    let (_, text) = survey("reexport_chain");
    assert_snapshot("reexport_chain", &text);
}

// glob_reexport

#[test]
fn a_glob_reexport_is_followed_to_the_defining_module() {
    let (model, _) = survey("glob_reexport");
    let r = relation(&model, "app.b", "app.a.inner");
    assert_eq!(r.kind, RelationKind::Calls);
    assert_eq!(r.items, ["Thing", "make"]);
    no_relation(&model, "app.b", "app.a");
}

#[test]
fn a_bare_name_from_a_glob_import_lands_where_the_glob_points() {
    let (model, _) = survey("glob_reexport");
    let r = relation(&model, "app.c", "app.a.inner");
    assert_eq!(r.items, ["Thing"]);
}

#[test]
fn surveying_one_member_scopes_the_model_to_the_packages_under_it() {
    let root = fixture_root("workspace_crates").join("consumer");
    let model = analyze(&root, &RustConfig::default()).unwrap();
    let ids: Vec<String> = model.elements.iter().map(|e| e.id.join(".")).collect();
    assert_eq!(ids, ["consumer"]);
    assert!(model.relations.is_empty(), "{:?}", model.relations);
}

#[test]
fn the_glob_reexport_snapshot_is_current() {
    let (_, text) = survey("glob_reexport");
    assert_snapshot("glob_reexport", &text);
}

// super_inline_kinds

#[test]
fn super_paths_resolve_from_a_file_module() {
    let (model, _) = survey("super_inline_kinds");
    let r = relation(&model, "app.outer.deep", "app.util");
    assert_eq!(r.items, ["Helper"]);
}

#[test]
fn an_inline_module_is_its_own_component_and_an_impl_is_the_strongest_evidence() {
    let (model, _) = survey("super_inline_kinds");
    let r = relation(&model, "app.outer.deep.nested", "app.util");
    assert_eq!(r.kind, RelationKind::Implements);
    assert_eq!(r.items, ["Helper", "Render"]);
    assert_eq!(
        element(&model, "app.outer.deep.nested").path.as_deref(),
        Some("src/outer/deep.rs")
    );
    assert_eq!(
        element(&model, "app.outer.deep.nested")
            .description
            .as_deref(),
        Some("Nested inline.")
    );
}

#[test]
fn an_ancestor_reference_and_a_cfg_test_module_leave_no_trace() {
    let (model, _) = survey("super_inline_kinds");
    no_relation(&model, "app.outer.deep.nested", "app.outer");
    assert!(model.element(&id("app.outer.deep.tests")).is_none());
}

#[test]
fn the_super_inline_kinds_snapshot_is_current() {
    let (_, text) = survey("super_inline_kinds");
    assert_snapshot("super_inline_kinds", &text);
}

// path_attr

#[test]
fn a_path_attribute_module_and_its_children_resolve_and_carry_their_real_paths() {
    let (model, _) = survey("path_attr");
    assert_eq!(relation(&model, "app.b", "app.a").items, ["A"]);
    assert_eq!(relation(&model, "app.b.c", "app.a.sub").items, ["Sub"]);
    assert_eq!(
        element(&model, "app.a").path.as_deref(),
        Some("src/support/impl_a.rs")
    );
    assert_eq!(
        element(&model, "app.a.sub").path.as_deref(),
        Some("src/support/sub.rs")
    );
}

#[test]
fn the_path_attr_snapshot_is_current() {
    let (_, text) = survey("path_attr");
    assert_snapshot("path_attr", &text);
}

// workspace_crates

#[test]
fn a_crate_name_segment_enters_that_crate_and_follows_its_reexports() {
    let (model, _) = survey("workspace_crates");
    assert_eq!(
        relation(&model, "consumer", "core_lib.model.item").items,
        ["Item"]
    );
}

#[test]
fn a_tests_only_member_and_an_extra_manifest_are_tests_components() {
    let (model, _) = survey("workspace_crates");
    assert_eq!(
        relation(&model, "e2e.tests", "core_lib.model.item").items,
        ["Item"]
    );
    assert_eq!(
        relation(&model, "standalone.tests", "core_lib.model.item").items,
        ["Item"]
    );
    assert_eq!(element(&model, "standalone.tests").tags, ["tests"]);
    assert_eq!(
        element(&model, "standalone.tests").path.as_deref(),
        Some("standalone/tests")
    );
}

#[test]
fn an_unreferenced_member_is_an_element_with_no_relation() {
    let (model, _) = survey("workspace_crates");
    assert_eq!(
        element(&model, "unused_lib").technology.as_deref(),
        Some("library crate")
    );
    assert!(
        !model
            .relations
            .iter()
            .any(|r| r.target == id("unused_lib") || r.source == id("unused_lib"))
    );
}

#[test]
fn the_workspace_crates_snapshot_is_current() {
    let (_, text) = survey("workspace_crates");
    assert_snapshot("workspace_crates", &text);
}

// targets

#[test]
fn a_bin_beside_a_lib_is_a_tagged_component_that_uses_the_lib() {
    let (model, _) = survey("targets");
    assert_eq!(element(&model, "tool.cli").tags, ["bin"]);
    assert_eq!(
        element(&model, "tool.cli").path.as_deref(),
        Some("tool/src/bin/cli.rs")
    );
    assert_eq!(relation(&model, "tool.cli", "tool.api").items, ["call"]);
}

#[test]
fn examples_and_tests_collapse_to_one_component_each() {
    let (model, _) = survey("targets");
    assert_eq!(
        relation(&model, "tool.examples", "tool.api").kind,
        RelationKind::Calls
    );
    let tests = relation(&model, "tool.tests", "tool.api");
    assert_eq!(tests.kind, RelationKind::Calls);
    assert_eq!(tests.items, ["api", "call"]);
    assert_eq!(
        element(&model, "tool.examples").path.as_deref(),
        Some("tool/examples")
    );
    // tests/ and benches/ are siblings, so the component is at the crate.
    assert_eq!(element(&model, "tool.tests").path.as_deref(), Some("tool"));
}

#[test]
fn a_reference_from_a_synthetic_component_to_its_own_crate_root_is_lineal() {
    // benches/b.rs uses `tool::Api`, defined in the crate root; the
    // tests component sits under that root, so nothing is recorded.
    let (model, _) = survey("targets");
    no_relation(&model, "tool.tests", "tool");
}

#[test]
fn a_bin_only_crate_is_a_binary_whose_modules_sit_under_it() {
    let (model, _) = survey("targets");
    assert_eq!(
        element(&model, "runner").technology.as_deref(),
        Some("binary crate")
    );
    assert_eq!(
        element(&model, "runner").description.as_deref(),
        Some("Runs jobs.")
    );
    assert_eq!(
        element(&model, "runner.jobs").path.as_deref(),
        Some("runner/src/jobs.rs")
    );
    assert_eq!(
        element(&model, "pm").technology.as_deref(),
        Some("proc-macro crate")
    );
}

#[test]
fn examples_can_be_left_out_by_config() {
    let rust = RustConfig {
        include_examples: false,
        ..Default::default()
    };
    let (model, _) = survey_with("targets", &rust);
    assert!(model.element(&id("tool.examples")).is_none());
    assert!(model.element(&id("tool.tests")).is_some());
}

#[test]
fn the_targets_snapshot_is_current() {
    let (_, text) = survey("targets");
    assert_snapshot("targets", &text);
}

#[test]
fn a_bin_named_like_its_crate_beside_a_lib_is_still_a_bin_component() {
    // tool/src/main.rs is the package's default bin; with a lib present
    // it must not become the crate's main namespace.
    let (model, _) = survey("targets");
    assert_eq!(element(&model, "tool.tool").tags, ["bin"]);
    assert_eq!(relation(&model, "tool.tool", "tool.api").items, ["call"]);
}

#[test]
fn the_front_end_trait_surveys_the_same_model_as_analyze() {
    let root = fixture_root("targets");
    let config = Config::load(&root).unwrap();
    let through_trait = asbuilt_core::survey(&root, &config, &[&RustFrontend]).unwrap();
    let direct = analyze(&root, &RustConfig::default()).unwrap();
    assert_eq!(through_trait, direct);
    assert!(!through_trait.elements.is_empty());
}

#[test]
fn a_survey_is_the_same_twice() {
    let (a, _) = survey("workspace_crates");
    let (b, _) = survey("workspace_crates");
    assert_eq!(a, b);
}
