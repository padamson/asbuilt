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

// `asbuilt docs` without Node: `--no-render` reuses whatever SVGs exist
// and the pages are proven here; the `likec4_docs_*` test covers the
// rendering path.

fn docs_run(ws: &Workspace, extra: &[&str]) -> Output {
    let mut args = vec!["docs", ws.root().to_str().unwrap(), "--no-render"];
    args.extend_from_slice(extra);
    run(&args)
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
    assert!(stderr.contains("wrote 4 pages to"), "{stderr}");
    let index = std::fs::read_to_string(root.join("index.html")).unwrap();
    assert!(
        index.contains("No diagram for <code>index</code>"),
        "{index}"
    );
    assert!(index.contains("href=\"containers/app.html\""), "{index}");
    assert!(index.contains("<tr id=\"node_driver\">"), "{index}");
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
fn docs_into_the_model_directory_keeps_an_existing_svg_intact() {
    let ws = scratch_copy();
    let svg = ws.write("docs/architecture/views/index.svg", "<svg/>");

    let out = docs_run(&ws, &["-o", "docs/architecture"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&svg).unwrap(), "<svg/>");
    assert!(ws.root().join("docs/architecture/index.html").is_file());
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
    // A fake `npx` first on the PATH: the survey (cargo) still works, the
    // render does not, and nothing may be written.
    let ws = scratch_copy();
    let bin = Workspace::new();
    let fake = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let script = if cfg!(windows) {
        "@echo off\r\nexit /b 1\r\n"
    } else {
        "#!/bin/sh\nexit 1\n"
    };
    let fake_path = bin.write(fake, script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}{}{}",
        bin.root().display(),
        if cfg!(windows) { ";" } else { ":" },
        std::env::var("PATH").unwrap_or_default()
    );

    let out = asbuilt()
        .args(["docs", ws.root().to_str().unwrap(), "-o", "out"])
        .env("PATH", path)
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
    assert!(app.contains("<title>app · From flag</title>"), "{app}");

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
    assert!(!app.contains("href=\"https://"), "{app}");
}
