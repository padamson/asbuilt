// The binary as a user runs it. Each test spawns the built `asbuilt`
// and asserts on its exit status and output.

use std::process::Command;

fn asbuilt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asbuilt"))
}

// Shape, not environment: the same claim holds in a dev checkout (a
// short sha follows the version) and in a tagged or crates.io build
// (nothing follows it).
#[test]
fn version_is_the_crate_version_with_at_most_a_short_sha_suffix() {
    let out = asbuilt().arg("--version").output().unwrap();
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
fn help_exits_zero_and_names_the_binary() {
    let out = asbuilt().arg("--help").output().unwrap();
    assert!(out.status.success(), "status {:?}", out.status);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("asbuilt"), "got {text:?}");
}
