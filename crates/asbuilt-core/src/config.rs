//! `asbuilt.toml`: the output path, the externals nothing static in the
//! code can state, and every front-end's own table kept raw for it to
//! parse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// Where `survey` writes and `check` reads when the config says nothing.
pub const DEFAULT_OUTPUT_PATH: &str = "docs/architecture/model.c4";

/// The config file name, looked up at the surveyed root.
pub const FILE_NAME: &str = "asbuilt.toml";

/// The parsed `asbuilt.toml`.
///
/// Unknown top-level tables are not an error: they are a front-end's
/// (`[rust]`, say) and stay in `sections` for it to deserialize with its
/// own schema. Unknown keys *inside* `[output]` or an `[[externals]]`
/// entry are rejected.
///
/// ```
/// use asbuilt_core::Config;
///
/// let config: Config = r#"
/// [output]
/// path = "arch/model.c4"
///
/// [rust]
/// include_tests = false
/// "#
/// .parse()
/// .unwrap();
///
/// assert_eq!(config.output.path, "arch/model.c4");
/// assert!(config.sections.contains_key("rust"));
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub docs: DocsConfig,
    #[serde(default)]
    pub externals: Vec<External>,
    #[serde(flatten)]
    pub sections: BTreeMap<String, toml::Value>,
    /// Where this config was read from, for error messages. `None` for a
    /// config parsed from a string or defaulted.
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    /// Relative to the surveyed root.
    #[serde(default = "default_output_path")]
    pub path: String,
}

fn default_output_path() -> String {
    DEFAULT_OUTPUT_PATH.to_string()
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            path: default_output_path(),
        }
    }
}

/// What `asbuilt docs` cannot read off the code.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DocsConfig {
    /// The site title; the root directory's name when absent.
    #[serde(default)]
    pub title: Option<String>,
    /// A URL prefix that turns a repo-relative path into a link, e.g.
    /// `https://github.com/owner/repo/blob/main/`. Paths stay plain text
    /// when absent.
    #[serde(default)]
    pub source_url: Option<String>,
}

/// Something outside the code that a module talks to: a spawned
/// process, a browser, a service. Nothing static says it exists, so the
/// config does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct External {
    /// A LikeC4 identifier; must not collide with a generated id.
    pub id: String,
    /// The LikeC4 element kind (`process`, `browser`, ...).
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub technology: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub relations: Vec<ExternalRelation>,
}

/// An edge from a generated element (or another external) to this one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRelation {
    /// A dotted id that must name a surveyed element or a sibling external.
    pub from: String,
    /// The label.
    pub title: String,
    #[serde(default)]
    pub technology: Option<String>,
}

impl FromStr for Config {
    type Err = toml::de::Error;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        toml::from_str(text)
    }
}

impl Config {
    /// The config at `root/asbuilt.toml`, or the defaults when there is
    /// no such file.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(FILE_NAME);
        if !path.is_file() {
            return Ok(Self::default());
        }
        Self::load_file(&path)
    }

    /// A config from an explicit path (`--config`); a missing file is an
    /// error here, unlike [`Config::load`].
    pub fn load_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut config: Config = text.parse().map_err(|source| Error::Config {
            path: path.to_path_buf(),
            source,
        })?;
        config.path = Some(path.to_path_buf());
        Ok(config)
    }

    /// A front-end's own table, deserialized with its schema. `Ok(None)`
    /// when the table is absent; a `Config` error naming the file when
    /// it is present and does not fit.
    pub fn section<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let Some(value) = self.sections.get(name) else {
            return Ok(None);
        };
        value
            .clone()
            .try_into()
            .map(Some)
            .map_err(|source| Error::Config {
                path: self
                    .path
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(FILE_NAME)),
                source,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIEF_EXAMPLE: &str = r#"
[output]
path = "docs/architecture/model.c4"

[rust]
extra_manifests = ["crates/site/Cargo.toml", "crates/site-e2e/Cargo.toml"]
include_tests = true

[[externals]]
id = "node_driver"
kind = "process"
title = "Playwright driver"
technology = "Node.js process"
description = "The official Playwright server, assembled by build.rs."

[[externals.relations]]
from = "playwright_rs.server.playwright_server"
title = "spawns"
technology = "stdio"
"#;

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RustLike {
        extra_manifests: Vec<String>,
        include_tests: bool,
    }

    #[test]
    fn the_brief_example_parses_to_its_values() {
        let config: Config = BRIEF_EXAMPLE.parse().unwrap();
        assert_eq!(config.output.path, "docs/architecture/model.c4");
        assert_eq!(config.externals.len(), 1);
        let ext = &config.externals[0];
        assert_eq!(ext.id, "node_driver");
        assert_eq!(ext.kind, "process");
        assert_eq!(ext.technology.as_deref(), Some("Node.js process"));
        assert_eq!(
            ext.relations,
            vec![ExternalRelation {
                from: "playwright_rs.server.playwright_server".into(),
                title: "spawns".into(),
                technology: Some("stdio".into()),
            }]
        );
    }

    #[test]
    fn a_front_end_table_round_trips_through_its_own_schema() {
        let config: Config = BRIEF_EXAMPLE.parse().unwrap();
        let rust: Option<RustLike> = config.section("rust").unwrap();
        assert_eq!(
            rust,
            Some(RustLike {
                extra_manifests: vec![
                    "crates/site/Cargo.toml".into(),
                    "crates/site-e2e/Cargo.toml".into()
                ],
                include_tests: true,
            })
        );
    }

    #[test]
    fn an_absent_section_is_none() {
        let config: Config = "".parse().unwrap();
        let rust: Option<RustLike> = config.section("rust").unwrap();
        assert_eq!(rust, None);
    }

    #[test]
    fn a_section_that_does_not_fit_its_schema_is_a_config_error_naming_the_file() {
        let mut config: Config = "[rust]\ninclude_test = true\n".parse().unwrap();
        config.path = Some(PathBuf::from("here/asbuilt.toml"));
        match config.section::<RustLike>("rust") {
            Err(Error::Config { path, .. }) => assert_eq!(path, PathBuf::from("here/asbuilt.toml")),
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_config_has_the_default_output_path() {
        let config: Config = "".parse().unwrap();
        assert_eq!(config.output.path, DEFAULT_OUTPUT_PATH);
    }

    #[test]
    fn an_unknown_key_under_output_is_rejected() {
        let result: std::result::Result<Config, _> = "[output]\npth = \"x\"\n".parse();
        assert!(result.is_err(), "got {result:?}");
    }

    #[test]
    fn an_unknown_key_in_an_external_is_rejected() {
        let result: std::result::Result<Config, _> =
            "[[externals]]\nid = \"x\"\nkind = \"k\"\ntitle = \"t\"\nlabel = \"no\"\n".parse();
        assert!(result.is_err(), "got {result:?}");
    }

    #[test]
    fn the_docs_table_parses_and_defaults_to_nothing() {
        let config: Config = "[docs]\ntitle = \"asbuilt\"\nsource_url = \"https://x/blob/main/\"\n"
            .parse()
            .unwrap();
        assert_eq!(config.docs.title.as_deref(), Some("asbuilt"));
        assert_eq!(
            config.docs.source_url.as_deref(),
            Some("https://x/blob/main/")
        );
        let empty: Config = "".parse().unwrap();
        assert_eq!(empty.docs, DocsConfig::default());
    }

    #[test]
    fn an_unknown_key_under_docs_is_rejected() {
        let result: std::result::Result<Config, _> = "[docs]\ntitel = \"x\"\n".parse();
        assert!(result.is_err(), "got {result:?}");
    }

    #[test]
    fn an_external_without_relations_has_none() {
        let config: Config = "[[externals]]\nid = \"x\"\nkind = \"k\"\ntitle = \"t\"\n"
            .parse()
            .unwrap();
        assert!(config.externals[0].relations.is_empty());
    }

    #[test]
    fn a_missing_file_at_the_root_loads_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path()).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_file_at_the_root_is_loaded_and_remembers_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "[output]\npath = \"m.c4\"\n").unwrap();
        let config = Config::load(dir.path()).unwrap();
        assert_eq!(config.output.path, "m.c4");
        assert_eq!(config.path, Some(path));
    }

    #[test]
    fn a_malformed_file_is_a_config_error_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "[output\n").unwrap();
        match Config::load(dir.path()) {
            Err(Error::Config { path: p, .. }) => assert_eq!(p, path),
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn an_explicit_path_that_does_not_exist_is_an_io_error_naming_it() {
        let missing = PathBuf::from("/no/such/asbuilt.toml");
        match Config::load_file(&missing) {
            Err(Error::Io { path, .. }) => assert_eq!(path, missing),
            other => panic!("expected Io error, got {other:?}"),
        }
    }
}
