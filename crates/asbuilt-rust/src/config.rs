//! The `[rust]` table of `asbuilt.toml`.

use std::path::PathBuf;

use asbuilt_core::Config;
use serde::Deserialize;

/// What the Rust front-end reads from `[rust]`.
///
/// ```
/// use asbuilt_core::Config;
/// use asbuilt_rust::RustConfig;
///
/// let config: Config = "[rust]\ninclude_tests = false\n".parse().unwrap();
/// let rust = RustConfig::from_config(&config).unwrap();
///
/// assert!(!rust.include_tests);
/// assert!(rust.include_examples);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RustConfig {
    /// Manifests of crates inside the repo but outside the workspace
    /// (each with its own lockfile), relative to the surveyed root.
    pub extra_manifests: Vec<PathBuf>,
    /// Emit one `tests` component per crate for its test and bench
    /// targets. A crate with only tests is a real consumer.
    pub include_tests: bool,
    /// Emit one `examples` component per crate for its example targets.
    pub include_examples: bool,
}

impl Default for RustConfig {
    fn default() -> Self {
        Self {
            extra_manifests: Vec::new(),
            include_tests: true,
            include_examples: true,
        }
    }
}

impl RustConfig {
    /// The `[rust]` table, or the defaults when there is none. An
    /// unknown key is a config error naming the file.
    pub fn from_config(config: &Config) -> asbuilt_core::Result<Self> {
        Ok(config.section("rust")?.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_include_tests_and_examples_and_no_extra_manifests() {
        let rust = RustConfig::from_config(&Config::default()).unwrap();
        assert_eq!(rust, RustConfig::default());
        assert!(rust.include_tests && rust.include_examples);
        assert!(rust.extra_manifests.is_empty());
    }

    #[test]
    fn the_brief_example_parses() {
        let config: Config =
            "[rust]\nextra_manifests = [\"crates/site/Cargo.toml\"]\ninclude_tests = true\n"
                .parse()
                .unwrap();
        let rust = RustConfig::from_config(&config).unwrap();
        assert_eq!(
            rust.extra_manifests,
            [PathBuf::from("crates/site/Cargo.toml")]
        );
    }

    #[test]
    fn a_misspelled_key_is_a_config_error() {
        let config: Config = "[rust]\ninclude_test = true\n".parse().unwrap();
        assert!(matches!(
            RustConfig::from_config(&config),
            Err(asbuilt_core::Error::Config { .. })
        ));
    }
}
