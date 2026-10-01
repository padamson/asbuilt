// Discovery against real `cargo metadata` over a workspace written into
// a scratch directory. `--no-deps --offline` reads manifests only, so
// the path dev-dependency below is never resolved and no lockfile is
// written; the last assertion of the first test is what holds that.

mod common;

use std::path::PathBuf;

use asbuilt_rust::config::RustConfig;
use asbuilt_rust::discover::{
    TECHNOLOGY_BINARY, TECHNOLOGY_EXAMPLES, TECHNOLOGY_LIBRARY, TECHNOLOGY_LIBRARY_AND_BINARY,
    TECHNOLOGY_PROC_MACRO, TECHNOLOGY_PROC_MACRO_AND_BINARY, TECHNOLOGY_TESTS, TargetKind,
    discover,
};
use asbuilt_rust::error::RustFrontendError;
use common::Workspace;

fn package(name: &str, extra: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{extra}")
}

fn two_members_and_an_extra() -> Workspace {
    let ws = Workspace::new();
    ws.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"core\", \"macros\"]\nresolver = \"2\"\n",
    );
    ws.write("core/Cargo.toml", &package("my-core", ""));
    ws.write("core/src/lib.rs", "");
    ws.write("core/src/bin/tool.rs", "fn main() {}");
    ws.write("core/tests/smoke.rs", "");
    ws.write("core/examples/demo.rs", "fn main() {}");
    ws.write(
        "macros/Cargo.toml",
        &package("my-macros", "[lib]\nproc-macro = true\n"),
    );
    ws.write("macros/src/lib.rs", "");
    ws.write(
        "e2e/Cargo.toml",
        &package(
            "site-e2e",
            "[dev-dependencies]\nmy-core = { path = \"../core\" }\n\n[workspace]\n",
        ),
    );
    ws.write("e2e/tests/it.rs", "");
    ws
}

#[test]
fn members_and_an_extra_manifest_are_discovered_in_crate_name_order_without_a_lockfile() {
    let ws = two_members_and_an_extra();
    let config = RustConfig {
        extra_manifests: vec![PathBuf::from("e2e/Cargo.toml")],
        ..Default::default()
    };

    let crates = discover(ws.root(), &config).unwrap();

    let names: Vec<&str> = crates.iter().map(|c| c.crate_name.as_str()).collect();
    assert_eq!(names, ["my_core", "my_macros", "site_e2e"]);
    assert_eq!(crates[0].package, "my-core");
    assert!(
        crates[0].manifest_dir.ends_with("core"),
        "{:?}",
        crates[0].manifest_dir
    );
    assert!(
        !ws.root().join("Cargo.lock").exists(),
        "a lockfile appeared at the root"
    );
    assert!(
        !ws.root().join("e2e/Cargo.lock").exists(),
        "a lockfile appeared in the extra manifest's crate"
    );
}

#[test]
fn a_crate_with_a_lib_lists_every_target_kind_sorted_with_the_lib_first() {
    let ws = two_members_and_an_extra();

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    let core = crates.iter().find(|c| c.crate_name == "my_core").unwrap();
    let targets: Vec<(TargetKind, &str)> = core
        .targets
        .iter()
        .map(|t| (t.kind, t.name.as_str()))
        .collect();
    assert_eq!(
        targets,
        [
            (TargetKind::Lib, "my_core"),
            (TargetKind::Bin, "tool"),
            (TargetKind::Test, "smoke"),
            (TargetKind::Example, "demo"),
        ]
    );
    assert!(
        core.targets[0].root.ends_with("core/src/lib.rs"),
        "{:?}",
        core.targets[0].root
    );
    assert!(
        core.targets[2].root.ends_with("core/tests/smoke.rs"),
        "{:?}",
        core.targets[2].root
    );
}

#[test]
fn a_lib_with_a_bin_beside_it_is_a_library_and_binary_crate() {
    let ws = two_members_and_an_extra();

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    let core = crates.iter().find(|c| c.crate_name == "my_core").unwrap();
    assert_eq!(core.technology, TECHNOLOGY_LIBRARY_AND_BINARY);
}

#[test]
fn a_lib_with_no_bin_is_a_library_crate() {
    let ws = Workspace::new();
    ws.write("Cargo.toml", &package("plain", ""));
    ws.write("src/lib.rs", "");
    ws.write("tests/it.rs", "");
    ws.write("examples/demo.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    assert_eq!(crates[0].technology, TECHNOLOGY_LIBRARY);
}

#[test]
fn a_proc_macro_with_a_bin_beside_it_is_a_proc_macro_and_binary_crate() {
    let ws = Workspace::new();
    ws.write(
        "Cargo.toml",
        &package("derive-it", "[lib]\nproc-macro = true\n"),
    );
    ws.write("src/lib.rs", "");
    ws.write("src/main.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    assert_eq!(crates[0].technology, TECHNOLOGY_PROC_MACRO_AND_BINARY);
}

#[test]
fn a_proc_macro_crate_says_so() {
    let ws = two_members_and_an_extra();

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    let macros = crates.iter().find(|c| c.crate_name == "my_macros").unwrap();
    assert_eq!(macros.technology, TECHNOLOGY_PROC_MACRO);
    assert_eq!(macros.targets[0].kind, TargetKind::ProcMacro);
}

#[test]
fn a_tests_only_crate_is_a_test_crate_named_after_its_package() {
    let ws = two_members_and_an_extra();
    let config = RustConfig {
        extra_manifests: vec![PathBuf::from("e2e/Cargo.toml")],
        ..Default::default()
    };

    let crates = discover(ws.root(), &config).unwrap();

    let e2e = crates.iter().find(|c| c.package == "site-e2e").unwrap();
    assert_eq!(e2e.crate_name, "site_e2e");
    assert_eq!(e2e.technology, TECHNOLOGY_TESTS);
    let kinds: Vec<TargetKind> = e2e.targets.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, [TargetKind::Test]);
}

#[test]
fn an_examples_only_package_is_an_example_crate() {
    let ws = Workspace::new();
    ws.write(
        "Cargo.toml",
        &package(
            "demos",
            "[[example]]\nname = \"demo\"\npath = \"examples/demo.rs\"\n",
        ),
    );
    ws.write("examples/demo.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    assert_eq!(crates[0].technology, TECHNOLOGY_EXAMPLES);
}

#[test]
fn a_package_with_tests_and_examples_but_no_lib_or_bin_is_a_test_crate() {
    let ws = Workspace::new();
    ws.write("Cargo.toml", &package("checks", ""));
    ws.write("tests/it.rs", "");
    ws.write("examples/demo.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    assert_eq!(crates[0].technology, TECHNOLOGY_TESTS);
}

#[test]
fn a_package_with_benches_and_examples_but_no_lib_or_bin_is_a_test_crate() {
    let ws = Workspace::new();
    ws.write("Cargo.toml", &package("timings", ""));
    ws.write("benches/b.rs", "fn main() {}");
    ws.write("examples/demo.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    assert_eq!(crates[0].technology, TECHNOLOGY_TESTS);
}

#[test]
fn a_bin_only_package_is_a_binary() {
    let ws = Workspace::new();
    ws.write("Cargo.toml", &package("run-it", ""));
    ws.write("src/main.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    assert_eq!(crates.len(), 1);
    assert_eq!(crates[0].crate_name, "run_it");
    assert_eq!(crates[0].technology, TECHNOLOGY_BINARY);
    assert_eq!(crates[0].targets[0].kind, TargetKind::Bin);
}

#[test]
fn a_build_script_is_not_a_target() {
    let ws = Workspace::new();
    ws.write("Cargo.toml", &package("built", ""));
    ws.write("src/lib.rs", "");
    ws.write("build.rs", "fn main() {}");

    let crates = discover(ws.root(), &RustConfig::default()).unwrap();

    let kinds: Vec<TargetKind> = crates[0].targets.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, [TargetKind::Lib]);
}

#[test]
fn two_packages_with_one_crate_name_are_an_error_naming_both_manifests() {
    let ws = Workspace::new();
    ws.write("Cargo.toml", &package("site", ""));
    ws.write("src/lib.rs", "");
    ws.write("other/Cargo.toml", &package("site", "[workspace]\n"));
    ws.write("other/src/lib.rs", "");
    let config = RustConfig {
        extra_manifests: vec![PathBuf::from("other/Cargo.toml")],
        ..Default::default()
    };

    match discover(ws.root(), &config) {
        Err(RustFrontendError::DuplicateCrateName {
            crate_name,
            manifests,
        }) => {
            assert_eq!(crate_name, "site");
            assert!(
                manifests[0].ends_with("Cargo.toml") && manifests[1].ends_with("other/Cargo.toml"),
                "{manifests:?}"
            );
        }
        other => panic!("expected DuplicateCrateName, got {other:?}"),
    }
}
