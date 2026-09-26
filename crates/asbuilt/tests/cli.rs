// The binary as a user runs it. Each test spawns the built `asbuilt`
// over the consumer fixture (or a scratch copy of it, when the test
// changes anything) and asserts on exit status and output.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Workspace;

fn asbuilt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asbuilt"))
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/consumer")
}

fn scratch_copy() -> Workspace {
    let ws = Workspace::new();
    ws.copy_from(&fixture());
    ws
}

fn run(args: &[&str]) -> Output {
    asbuilt().args(args).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

// Shape, not environment: the same claim holds in a dev checkout (a
// short sha follows the version) and in a tagged or crates.io build
// (nothing follows it).
#[test]
fn version_is_the_crate_version_with_at_most_a_short_sha_suffix() {
    let out = run(&["--version"]);
    assert!(out.status.success(), "status {:?}", out.status);
    let text = String::from_utf8(out.stdout).unwrap();
    let text = text.trim_end();

    let rest = text
        .strip_prefix("asbuilt ")
        .and_then(|t| t.strip_prefix(env!("CARGO_PKG_VERSION")))
        .unwrap_or_else(|| panic!("expected `asbuilt <version>...`, got {text:?}"));
    if rest.is_empty() {
        return;
    }
    let sha = rest
        .strip_prefix(" (")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("suffix must be ` (<sha>)`, got {rest:?}"));
    assert!(
        sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "sha must be 7+ hex digits, got {sha:?}"
    );
}

#[test]
fn help_exits_zero_and_lists_the_subcommands() {
    let out = run(&["--help"]);
    assert!(out.status.success(), "status {:?}", out.status);
    let help = text(&out.stdout);
    assert!(help.contains("survey") && help.contains("check"), "{help}");
}

#[test]
fn survey_writes_the_model_at_the_configured_path_and_prints_nothing() {
    let ws = scratch_copy();
    std::fs::remove_dir_all(ws.root().join("docs")).unwrap();

    let out = run(&["survey", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    let written = ws.root().join("docs/architecture/model.c4");
    let committed = std::fs::read_to_string(fixture().join("docs/architecture/model.c4")).unwrap();
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        committed.replace("\r\n", "\n"),
        "a survey from a different root must be byte-identical"
    );
}

#[test]
fn survey_writes_to_an_output_override_relative_to_the_root() {
    let ws = scratch_copy();

    let out = run(&["survey", ws.root().to_str().unwrap(), "-o", "out/m.c4"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let written = std::fs::read_to_string(ws.root().join("out/m.c4")).unwrap();
    assert!(written.contains("link ../app/src/server.rs\n"), "{written}");
}

#[test]
fn survey_reads_a_config_from_outside_the_root() {
    let ws = scratch_copy();
    let elsewhere = Workspace::new();
    let config = elsewhere.write("custom.toml", "[output]\npath = \"custom.c4\"\n");

    let out = run(&[
        "--config",
        config.to_str().unwrap(),
        "survey",
        ws.root().to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(ws.root().join("custom.c4").is_file());
}

#[test]
fn check_on_a_current_model_exits_zero_and_says_so() {
    let out = run(&["check", fixture().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "docs/architecture/model.c4 is current\n");
}

#[test]
fn check_defaults_to_the_current_directory() {
    let out = asbuilt()
        .arg("check")
        .current_dir(fixture())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
}

#[test]
fn check_on_a_stale_model_exits_one_and_prints_the_diff() {
    let ws = scratch_copy();
    ws.write(
        "app/src/extra.rs",
        "use crate::server::Driver;\n\npub fn d() -> Driver {\n    Driver\n}\n",
    );
    let lib = ws.root().join("app/src/lib.rs");
    let mut src = std::fs::read_to_string(&lib).unwrap();
    src.push_str("pub mod extra;\n");
    std::fs::write(&lib, src).unwrap();

    let out = run(&["check", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.starts_with("--- a/docs/architecture/model.c4\n+++ b/docs/architecture/model.c4\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\n+  app.extra -[names]-> app.server 'Driver'\n"),
        "{stdout}"
    );
    assert!(text(&out.stderr).contains("stale"), "{}", text(&out.stderr));
}

#[test]
fn check_without_a_committed_model_exits_two_naming_the_path() {
    let ws = scratch_copy();
    std::fs::remove_file(ws.root().join("docs/architecture/model.c4")).unwrap();

    let out = run(&["check", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("docs/architecture/model.c4") && stderr.contains("asbuilt survey"),
        "{stderr}"
    );
}

#[test]
fn a_root_without_a_supported_stack_exits_two() {
    let empty = Workspace::new();

    let out = run(&["survey", empty.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("no supported stack"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_bad_externals_from_exits_two_naming_the_external_and_the_from() {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let toml = std::fs::read_to_string(&config)
        .unwrap()
        .replace("app.server", "app.nope");
    std::fs::write(&config, toml).unwrap();

    let out = run(&["survey", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("app.nope") && stderr.contains("node_driver"),
        "{stderr}"
    );
}

#[test]
fn a_config_typo_exits_two_naming_the_file() {
    let ws = scratch_copy();
    ws.write("asbuilt.toml", "[rust]\ninclude_test = true\n");

    let out = run(&["survey", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("asbuilt.toml"),
        "{}",
        text(&out.stderr)
    );
}
