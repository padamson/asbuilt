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
fn survey_with_an_absolute_output_inside_the_root_links_by_its_real_depth() {
    let ws = scratch_copy();
    let out = ws.root().join("docs/arch/m.c4");

    let result = run(&[
        "survey",
        ws.root().to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);

    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    let written = std::fs::read_to_string(&out).unwrap();
    assert!(
        written.contains("link ../../app/src/server.rs\n"),
        "{written}"
    );
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
fn survey_with_a_theme_styles_the_kinds_in_the_specification_and_check_agrees() {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let mut toml = std::fs::read_to_string(&config).unwrap();
    toml.push_str("\n[theme]\ncontainer = { light = \"#f0a884\", dark = \"#b5673f\" }\n");
    std::fs::write(&config, toml).unwrap();

    let out = run(&["survey", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let model = std::fs::read_to_string(ws.root().join("docs/architecture/model.c4")).unwrap();
    assert!(
        model.contains("  color theme_container #f0a884\n"),
        "{model}"
    );
    assert!(
        model.contains(
            "  element container {\n    style {\n      color theme_container\n    }\n  }\n"
        ),
        "{model}"
    );
    assert!(model.contains("  element process\n"), "{model}");
    let check = run(&["check", ws.root().to_str().unwrap()]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check.stderr));
}

#[test]
fn a_theme_for_a_kind_the_model_lacks_exits_two_naming_it() {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let mut toml = std::fs::read_to_string(&config).unwrap();
    toml.push_str("\n[theme]\nbrowser = \"#000000\"\n");
    std::fs::write(&config, toml).unwrap();

    let out = run(&["survey", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"browser\"") && stderr.contains("process"),
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

/// The fixture with `asbuilt = "<pin>"` above its first table.
fn pinned_copy(pin: &str) -> Workspace {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let toml = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("asbuilt = \"{pin}\"\n\n{toml}")).unwrap();
    ws
}

#[test]
fn check_under_a_pin_naming_this_release_passes() {
    let ws = pinned_copy(env!("CARGO_PKG_VERSION"));

    let out = run(&["check", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
}

/// A release other than this one. The two tests under it fail on the pin
/// alone: `check_under_a_pin_naming_this_release_passes` is the same
/// fixture with the same key, passing.
const OTHER_RELEASE: &str = "0.0.1";

#[test]
fn check_under_a_pin_naming_another_release_stops_with_exit_two_and_no_diff() {
    let ws = pinned_copy(OTHER_RELEASE);

    let out = run(&["check", ws.root().to_str().unwrap()]);

    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (Some(2), String::new())
    );
}

#[test]
fn survey_under_a_pin_naming_another_release_writes_nothing() {
    let ws = pinned_copy(OTHER_RELEASE);
    std::fs::remove_dir_all(ws.root().join("docs")).unwrap();

    let out = run(&["survey", ws.root().to_str().unwrap()]);

    assert_eq!(
        (out.status.code(), ws.root().join("docs").exists()),
        (Some(2), false)
    );
}

#[test]
fn render_rejects_a_misspelled_table_before_running_likec4() {
    let ws = scratch_copy();
    ws.write("asbuilt.toml", "[outptu]\npath = \"arch/model.c4\"\n");

    let out = without_path(&["render", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("[outptu]"),
        "{}",
        text(&out.stderr)
    );
}

// The LikeC4 and Graphviz wrappers without Node on the PATH: every one
// must report that `npx` could not start and exit 2, which is what
// separates a real call from a stub without needing Node at all. The
// `likec4_` tests cover the calls themselves.

fn without_path(args: &[&str]) -> Output {
    let empty = Workspace::new();
    asbuilt()
        .args(args)
        .env("PATH", empty.root())
        .output()
        .unwrap()
}

#[test]
fn validate_without_npx_exits_two_and_says_it_could_not_run() {
    let out = without_path(&["validate", fixture().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("could not run"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn export_json_without_npx_exits_two_and_writes_nothing() {
    let ws = scratch_copy();
    let out = without_path(&["export", "json", ws.root().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(!ws.root().join("docs/architecture/model.json").exists());
}

#[test]
fn render_without_npx_exits_two_and_writes_no_svg() {
    let ws = scratch_copy();
    let out = without_path(&["render", ws.root().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(!ws.root().join("docs/architecture/views/index.svg").exists());
}

#[test]
fn render_without_npx_leaves_the_previous_render_as_it_was() {
    let ws = scratch_copy();
    let dot_text = "digraph { graph [likec4_viewId=index]; }";
    let dot = ws.write("docs/architecture/views/index.dot", dot_text);
    let svg = ws.write("docs/architecture/views/index.svg", "<svg/>");

    let out = without_path(&["render", ws.root().to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("npx"), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&svg).unwrap(), "<svg/>");
    assert_eq!(std::fs::read_to_string(&dot).unwrap(), dot_text);
    assert!(
        !ws.root()
            .join("docs/architecture/views/.asbuilt-render")
            .exists()
    );
}

#[test]
fn render_without_npx_into_a_new_directory_leaves_no_directory() {
    let ws = scratch_copy();
    let out = without_path(&["render", ws.root().to_str().unwrap(), "-o", "fresh"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(!ws.root().join("fresh").exists());
}

// `asbuilt docs` without Node: `--no-render` reuses whatever SVGs exist
// and the pages are proven here; the `likec4_docs_*` test covers the
// rendering path.

/// A directory holding an `npx` that exits 1, to put first on the PATH:
/// the survey (cargo) still works, a render cannot, and it fails at once
/// rather than fetching LikeC4. Every `docs` test runs with it so a docs
/// that ignored `--no-render` would exit 2 in milliseconds on any machine.
fn fake_npx() -> Workspace {
    let bin = Workspace::new();
    let fake = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let script = if cfg!(windows) {
        "@echo off\r\nexit /b 1\r\n"
    } else {
        "#!/bin/sh\nexit 1\n"
    };
    let path = bin.write(fake, script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

fn path_with(first: &Path) -> String {
    format!(
        "{}{}{}",
        first.display(),
        if cfg!(windows) { ";" } else { ":" },
        std::env::var("PATH").unwrap_or_default()
    )
}

fn docs_run(ws: &Workspace, extra: &[&str]) -> Output {
    let bin = fake_npx();
    let mut args = vec!["docs", ws.root().to_str().unwrap(), "--no-render"];
    args.extend_from_slice(extra);
    asbuilt()
        .args(&args)
        .env("PATH", path_with(bin.root()))
        .output()
        .unwrap()
}

#[test]
fn docs_no_render_writes_the_tree_with_placeholders_and_names_the_missing_views() {
    let ws = scratch_copy();

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let root = ws.root().join("out");
    for file in [
        "index.html",
        "style.css",
        "theme.css",
        "theme.js",
        "viewer.css",
        "viewer.js",
        "containers/app.html",
        "containers/e2e.html",
    ] {
        assert!(root.join(file).is_file(), "missing {file}");
    }
    assert!(!root.join("views.html").exists());
    let stderr = text(&out.stderr);
    for view in ["index", "view_app", "view_e2e"] {
        assert!(
            stderr.contains(&format!("no SVG for view {view} (views/{view}.svg)")),
            "{stderr}"
        );
    }
    assert!(stderr.contains("wrote 8 pages to"), "{stderr}");
    let index = std::fs::read_to_string(root.join("index.html")).unwrap();
    assert!(
        index.contains("No diagram for <code>index</code>"),
        "{index}"
    );
    assert!(index.contains("href=\"containers/app.html\""), "{index}");
    assert!(index.contains("<tr id=\"node_driver\">"), "{index}");
}

#[test]
fn docs_no_viewer_leaves_out_the_viewer_and_its_script() {
    let ws = scratch_copy();

    let out = docs_run(&ws, &["-o", "out", "--no-viewer"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!ws.root().join("out/viewer.js").exists());
    assert!(!ws.root().join("out/viewer.css").exists());
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(!index.contains("viewer.js"), "{index}");
    assert!(!index.contains("viewer.css"), "{index}");
}

#[test]
fn docs_no_viewer_removes_an_earlier_viewer_script() {
    let ws = scratch_copy();
    assert_eq!(docs_run(&ws, &["-o", "out"]).status.code(), Some(0));
    assert!(ws.root().join("out/viewer.js").is_file());

    let out = docs_run(&ws, &["-o", "out", "--no-viewer"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!ws.root().join("out/viewer.js").exists());
}

#[test]
fn docs_viewer_off_in_the_config_leaves_out_the_viewer_and_its_script() {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let mut toml = std::fs::read_to_string(&config).unwrap();
    toml.push_str("\n[docs]\nviewer = false\n");
    std::fs::write(&config, toml).unwrap();

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!ws.root().join("out/viewer.js").exists());
}

#[test]
fn docs_no_render_copies_the_svgs_render_left_and_not_the_dot_files() {
    let ws = scratch_copy();
    ws.write(
        "docs/architecture/views/index.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    );
    ws.write("docs/architecture/views/index.dot", "digraph {}");

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let copied = ws.root().join("out/views/index.svg");
    assert_eq!(
        std::fs::read_to_string(&copied).unwrap(),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
    );
    assert!(!ws.root().join("out/views/index.dot").exists());
    let stderr = text(&out.stderr);
    assert!(!stderr.contains("no SVG for view index "), "{stderr}");
    assert!(stderr.contains("no SVG for view view_app "), "{stderr}");
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(index.contains("<img src=\"views/index.svg\""), "{index}");
}

#[test]
fn docs_into_the_model_directory_writes_beside_the_model_and_keeps_its_renders() {
    let ws = scratch_copy();
    let svg = ws.write("docs/architecture/views/index.svg", "<svg/>");

    for _ in 0..2 {
        let out = docs_run(&ws, &["-o", "docs/architecture"]);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        assert_eq!(std::fs::read_to_string(&svg).unwrap(), "<svg/>");
        assert!(ws.root().join("docs/architecture/index.html").is_file());
        assert!(ws.root().join("docs/architecture/model.c4").is_file());
    }
}

#[test]
fn docs_writes_beside_files_it_does_not_own_without_force() {
    let ws = scratch_copy();
    ws.write("out/.gitkeep", "");
    ws.write("out/notes.txt", "mine");

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(ws.root().join("out/index.html").is_file());
    assert!(ws.root().join("out/.gitkeep").is_file());
    assert_eq!(
        std::fs::read_to_string(ws.root().join("out/notes.txt")).unwrap(),
        "mine"
    );
}

#[test]
fn docs_refuses_to_replace_an_index_it_did_not_write_and_names_the_flag() {
    let ws = scratch_copy();
    ws.write("out/index.html", "<html>host</html>");

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    let stderr = text(&out.stderr);
    assert!(stderr.contains("--force"), "{stderr}");
    assert!(stderr.contains("did not write"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(ws.root().join("out/index.html")).unwrap(),
        "<html>host</html>"
    );
}

#[test]
fn docs_refuses_before_rendering() {
    let ws = scratch_copy();
    ws.write("out/index.html", "<html>host</html>");
    let bin = fake_npx();

    let out = asbuilt()
        .args(["docs", ws.root().to_str().unwrap(), "-o", "out"])
        .env("PATH", path_with(bin.root()))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    let stderr = text(&out.stderr);
    assert!(stderr.contains("--force"), "{stderr}");
    assert!(!stderr.contains("npx"), "{stderr}");
}

#[test]
fn docs_with_a_failing_render_keeps_the_previous_tree() {
    let ws = scratch_copy();
    assert_eq!(docs_run(&ws, &["-o", "out"]).status.code(), Some(0));
    let bin = fake_npx();

    let out = asbuilt()
        .args(["docs", ws.root().to_str().unwrap(), "-o", "out"])
        .env("PATH", path_with(bin.root()))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(ws.root().join("out/index.html").is_file());
    assert!(ws.root().join("out/containers/app.html").is_file());
}

#[test]
fn docs_force_replaces_the_tree_s_own_file_names_and_keeps_the_rest() {
    let ws = scratch_copy();
    ws.write("out/index.html", "<html>host</html>");
    ws.write("out/404.html", "not found");
    ws.write("out/containers/old.html", "host");

    let out = docs_run(&ws, &["-o", "out", "--force"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(
        index.contains(&format!(
            "content=\"asbuilt docs {}",
            env!("CARGO_PKG_VERSION")
        )),
        "{index}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root().join("out/404.html")).unwrap(),
        "not found"
    );
    assert!(!ws.root().join("out/containers/old.html").exists());
}

#[test]
fn docs_replaces_a_tree_whose_generator_meta_names_no_release() {
    let ws = scratch_copy();
    ws.write(
        "out/index.html",
        "<meta name=\"generator\" content=\"asbuilt docs\">",
    );

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
}

#[test]
fn docs_run_twice_removes_what_the_first_run_wrote_and_nothing_else() {
    let ws = scratch_copy();
    assert_eq!(docs_run(&ws, &["-o", "out"]).status.code(), Some(0));
    // What a renamed crate and a dropped curated view would leave behind,
    // and a host's own page and placeholder beside the tree.
    ws.write("out/containers/old.html", "stale");
    ws.write("out/views.html", "stale");
    ws.write("out/views/stale.svg", "<svg/>");
    ws.write("out/extra.html", "host");
    ws.write("out/.gitkeep", "");

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let root = ws.root().join("out");
    assert!(!root.join("containers/old.html").exists());
    assert!(!root.join("views.html").exists());
    assert!(!root.join("views/stale.svg").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("extra.html")).unwrap(),
        "host"
    );
    assert!(root.join(".gitkeep").is_file());
    assert!(root.join("containers/app.html").is_file());
}

#[test]
fn docs_without_the_scheme_toggle_writes_no_script_and_removes_an_earlier_one() {
    let ws = scratch_copy();
    assert_eq!(docs_run(&ws, &["-o", "out"]).status.code(), Some(0));
    assert!(ws.root().join("out/theme.js").is_file());

    let out = docs_run(&ws, &["-o", "out", "--no-scheme-toggle"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!ws.root().join("out/theme.js").exists());
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(!index.contains("theme.js"), "{index}");
    assert!(!index.contains("class=\"scheme\""), "{index}");
}

#[test]
fn docs_color_scheme_comes_from_the_flag_over_the_config() {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let mut toml = std::fs::read_to_string(&config).unwrap();
    toml.push_str("\n[docs]\ncolor_scheme = \"light\"\nscheme_toggle = false\n");
    std::fs::write(&config, toml).unwrap();

    let configured = docs_run(&ws, &["-o", "configured"]);
    assert_eq!(
        configured.status.code(),
        Some(0),
        "{}",
        text(&configured.stderr)
    );
    let index = std::fs::read_to_string(ws.root().join("configured/index.html")).unwrap();
    assert!(
        index.contains("<html lang=\"en\" data-theme=\"light\">"),
        "{index}"
    );
    assert!(!ws.root().join("configured/theme.js").exists());

    let flagged = docs_run(&ws, &["-o", "flagged", "--color-scheme", "dark"]);
    assert_eq!(flagged.status.code(), Some(0), "{}", text(&flagged.stderr));
    let index = std::fs::read_to_string(ws.root().join("flagged/index.html")).unwrap();
    assert!(
        index.contains("<html lang=\"en\" data-theme=\"dark\">"),
        "{index}"
    );
}

#[test]
fn docs_rejects_a_color_scheme_it_does_not_know_naming_the_three() {
    let ws = scratch_copy();
    let out = docs_run(&ws, &["-o", "out", "--color-scheme", "sepia"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("sepia") && stderr.contains("system, light or dark"),
        "{stderr}"
    );
}

#[test]
fn docs_inlines_a_view_whose_dot_likec4_wrote() {
    let ws = scratch_copy();
    ws.write(
        "docs/architecture/views/index.svg",
        "<svg viewBox=\"0 0 1 1\" xmlns=\"http://www.w3.org/2000/svg\"><g id=\"node1\" class=\"node\"><title>app</title></g></svg>",
    );
    ws.write(
        "docs/architecture/views/index.dot",
        "digraph {\n    graph [likec4_viewId=index];\n    app [likec4_id=app];\n}\n",
    );

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(index.contains("data-view=\"index\""), "{index}");
    assert!(
        index.contains("<g id=\"index-node1\" class=\"node c4-k-container\">"),
        "{index}"
    );
}

#[test]
fn docs_home_and_stylesheet_flags_reach_every_page_resolved_from_its_depth() {
    let ws = scratch_copy();

    let out = docs_run(
        &ws,
        &[
            "-o",
            "out",
            "--home-url",
            "../",
            "--home-title",
            "Site",
            "--stylesheet",
            "../site.css",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let index = std::fs::read_to_string(ws.root().join("out/index.html")).unwrap();
    assert!(
        index.contains("<a class=\"home\" href=\"../\">Site</a>"),
        "{index}"
    );
    assert!(
        index.contains("<link rel=\"stylesheet\" href=\"../site.css\">\n</head>"),
        "{index}"
    );
    let app = std::fs::read_to_string(ws.root().join("out/containers/app.html")).unwrap();
    assert!(
        app.contains("<a class=\"home\" href=\"../../\">Site</a>"),
        "{app}"
    );
    assert!(app.contains("href=\"../../site.css\">\n</head>"), "{app}");
}

#[test]
fn docs_defaults_to_site_under_the_model_directory() {
    let ws = scratch_copy();

    let out = docs_run(&ws, &[]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        ws.root()
            .join("docs/architecture/site/index.html")
            .is_file()
    );
    assert!(
        ws.root()
            .join("docs/architecture/site/containers/app.html")
            .is_file()
    );
}

#[test]
fn docs_on_a_stale_model_exits_one_and_writes_nothing() {
    let ws = scratch_copy();
    ws.write("app/src/extra.rs", "");
    let lib = ws.root().join("app/src/lib.rs");
    let mut src = std::fs::read_to_string(&lib).unwrap();
    src.push_str("pub mod extra;\n");
    std::fs::write(&lib, src).unwrap();

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("stale"), "{}", text(&out.stderr));
    assert!(!ws.root().join("out").exists());
}

#[test]
fn docs_without_a_committed_model_exits_two_naming_the_path() {
    let ws = scratch_copy();
    std::fs::remove_file(ws.root().join("docs/architecture/model.c4")).unwrap();

    let out = docs_run(&ws, &["-o", "out"]);

    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("docs/architecture/model.c4"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn docs_with_a_failing_npx_exits_two_and_writes_nothing() {
    let ws = scratch_copy();
    let bin = fake_npx();

    let out = asbuilt()
        .args(["docs", ws.root().to_str().unwrap(), "-o", "out"])
        .env("PATH", path_with(bin.root()))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("npx"), "{}", text(&out.stderr));
    assert!(!ws.root().join("out").exists());
}

#[test]
fn docs_title_and_source_url_come_from_flags_over_the_config_over_the_root_name() {
    let ws = scratch_copy();
    let config = ws.root().join("asbuilt.toml");
    let mut toml = std::fs::read_to_string(&config).unwrap();
    toml.push_str("\n[docs]\ntitle = \"From config\"\nsource_url = \"https://cfg/\"\n");
    std::fs::write(&config, toml).unwrap();

    let flagged = docs_run(
        &ws,
        &[
            "-o",
            "flagged",
            "--title",
            "From flag",
            "--source-url",
            "https://flag/",
        ],
    );
    assert_eq!(flagged.status.code(), Some(0), "{}", text(&flagged.stderr));
    let app = std::fs::read_to_string(ws.root().join("flagged/containers/app.html")).unwrap();
    assert!(app.contains("<h1>app</h1>"), "{app}");
    assert!(
        app.contains("href=\"https://flag/app/src/server.rs\""),
        "{app}"
    );
    assert!(app.contains("<title>From flag · app</title>"), "{app}");

    let configured = docs_run(&ws, &["-o", "configured"]);
    assert_eq!(configured.status.code(), Some(0));
    let index = std::fs::read_to_string(ws.root().join("configured/index.html")).unwrap();
    assert!(index.contains("<h1>From config</h1>"), "{index}");
    let app = std::fs::read_to_string(ws.root().join("configured/containers/app.html")).unwrap();
    assert!(
        app.contains("href=\"https://cfg/app/src/server.rs\""),
        "{app}"
    );

    std::fs::write(
        &config,
        std::fs::read_to_string(&config)
            .unwrap()
            .split("[docs]")
            .next()
            .unwrap(),
    )
    .unwrap();
    let bare = docs_run(&ws, &["-o", "bare"]);
    assert_eq!(bare.status.code(), Some(0));
    let index = std::fs::read_to_string(ws.root().join("bare/index.html")).unwrap();
    let root_name = ws
        .root()
        .canonicalize()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(index.contains(&format!("<h1>{root_name}</h1>")), "{index}");
    let app = std::fs::read_to_string(ws.root().join("bare/containers/app.html")).unwrap();
    assert!(!app.contains("app/src/server.rs\""), "{app}");
}
