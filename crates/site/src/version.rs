//! The build's version identity and the one place the site's path prefix
//! is spelled. `build.rs` injects `SITE_VERSION`; the deploy serves every
//! snapshot under `/asbuilt/<dev|vX.Y.Z>/` (project Pages), and only the
//! version switcher needs to know that: every other asset path is relative.

/// The snapshot identifier: `"dev"` for the main-HEAD build, or the
/// release version (e.g. `"0.1.0"`).
pub const SITE_VERSION: &str = env!("SITE_VERSION");

/// The path the site is served under. `""` would be a custom domain at
/// the apex; project Pages is `/asbuilt`.
pub const SITE_PREFIX: &str = "/asbuilt";

/// Whether this is the unreleased main-HEAD (`dev`) build.
pub fn is_dev() -> bool {
    SITE_VERSION == "dev"
}

/// Where the deploy writes the manifest of published versions.
pub fn manifest_url() -> String {
    format!("{SITE_PREFIX}/versions.json")
}

/// The root of a snapshot: `dev` or a version number.
pub fn snapshot_url(value: &str) -> String {
    match value {
        "dev" => format!("{SITE_PREFIX}/dev/"),
        v => format!("{SITE_PREFIX}/v{v}/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_and_snapshots_live_under_the_prefix() {
        assert_eq!(manifest_url(), "/asbuilt/versions.json");
        assert_eq!(snapshot_url("dev"), "/asbuilt/dev/");
        assert_eq!(snapshot_url("0.1.0"), "/asbuilt/v0.1.0/");
    }
}
