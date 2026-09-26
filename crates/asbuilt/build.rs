// `--version` names the commit for a build that is not a tagged release:
// no git (a crates.io install) prints the bare crate version; HEAD at the
// tag `v<version>` prints the bare version; anything else prints
// `<version> (<short sha>)`. No dirty-tree marker: nothing re-runs this
// script on an uncommitted edit, so it would be wrong as often as right.

fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let build_id = match git_short_sha() {
        Some(sha) if !at_release_tag(&version) => format!("{version} ({sha})"),
        _ => version,
    };
    println!("cargo:rustc-env=CRATE_VERSION_WITH_BUILD={build_id}");

    // Freshness: `.git/HEAD` covers checkouts and branch switches; the
    // branch's ref file covers commits. The build script runs with the
    // package directory as cwd, two levels below the repo root.
    let git_dir = std::path::Path::new("../../.git");
    if !git_dir.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.strip_prefix("ref: ")
        && git_dir.join(reference.trim()).exists()
    {
        println!("cargo:rerun-if-changed=../../.git/{}", reference.trim());
    }
}

fn git_short_sha() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

fn at_release_tag(version: &str) -> bool {
    std::process::Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|tag| tag.trim() == format!("v{version}"))
}
