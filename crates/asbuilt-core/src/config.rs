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

/// asbuilt's own top-level tables, as the file writes them.
const OWN_TABLES: [&str; 4] = ["[output]", "[docs]", "[theme]", "[[externals]]"];

/// The parsed `asbuilt.toml`.
///
/// Other top-level entries stay in `sections`, where a front-end's table
/// (`[rust]`, say) waits to be deserialized with that front-end's schema;
/// [`Config::check_tables`] rejects any that names no front-end. Unknown
/// keys *inside* `[output]` or an `[[externals]]` entry are rejected
/// when parsing.
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
    /// `[theme]`: a color per element kind (`container`, `component`,
    /// `bin`, `tests`, `examples`, or an external kind), emitted as
    /// LikeC4 element styles so the
    /// diagrams come out in the consumer's palette.
    #[serde(default)]
    pub theme: BTreeMap<String, ThemeColor>,
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

/// A kind's color: one value for every scheme, or a light and a dark
/// one. Written as `"#3b82f6"` or `{ light = "#3b82f6", dark = "#1e40af" }`.
/// Parsed by hand rather than as an untagged enum, whose only error is
/// "did not match any variant": a mistake here names the key or value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "toml::Value")]
pub struct ThemeColor {
    pub light: String,
    pub dark: Option<String>,
}

impl TryFrom<toml::Value> for ThemeColor {
    type Error = String;

    fn try_from(value: toml::Value) -> std::result::Result<Self, String> {
        let (light, dark) = match value {
            toml::Value::String(light) => (light, None),
            toml::Value::Table(table) => {
                if let Some(key) = table
                    .keys()
                    .find(|k| !matches!(k.as_str(), "light" | "dark"))
                {
                    return Err(format!(
                        "unknown key \"{key}\" in a theme color (expected light and optionally dark)"
                    ));
                }
                let text = |key: &str| match table.get(key) {
                    None => Ok(None),
                    Some(toml::Value::String(s)) => Ok(Some(s.clone())),
                    Some(other) => Err(format!(
                        "theme color {key} is a {}, not a \"#rrggbb\" string",
                        other.type_str()
                    )),
                };
                let light = text("light")?.ok_or_else(|| {
                    "a theme color table needs light (dark is optional)".to_string()
                })?;
                (light, text("dark")?)
            }
            other => {
                return Err(format!(
                    "a theme color is \"#rrggbb\" or {{ light = \"#rrggbb\", dark = \"#rrggbb\" }}, not a {}",
                    other.type_str()
                ));
            }
        };
        for value in std::iter::once(&light).chain(dark.iter()) {
            if !is_hex_color(value) {
                return Err(format!(
                    "theme color \"{value}\" is not #rrggbb (six hex digits after #)"
                ));
            }
        }
        Ok(ThemeColor { light, dark })
    }
}

/// `#rrggbb`, six hex digits after the `#`.
pub fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Config {
    /// The light color per themed kind, the emitter's input.
    pub fn light_theme(&self) -> BTreeMap<String, String> {
        self.theme
            .iter()
            .map(|(kind, color)| (kind.clone(), color.light.clone()))
            .collect()
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
    /// A link back to the site that hosts the tree, first in every
    /// page's header trail; relative values (`../`) resolve
    /// from each page's depth. No link when absent.
    #[serde(default)]
    pub home_url: Option<String>,
    /// The text of that link; the URL itself when absent. Every page's
    /// `<title>` starts with it.
    #[serde(default)]
    pub home_title: Option<String>,
    /// A stylesheet linked last in every page, after the tree's own, so
    /// the host can restate the page tokens in its palette. Resolved
    /// like `home_url`.
    #[serde(default)]
    pub stylesheet: Option<String>,
    /// The scheme the pages show before a visitor chooses one; the
    /// system's setting when absent.
    #[serde(default)]
    pub color_scheme: Option<ColorScheme>,
    /// Whether every page carries a System / Light / Dark control, and
    /// with it `theme.js`. On when absent.
    #[serde(default)]
    pub scheme_toggle: Option<bool>,
    /// Whether every inlined view gets the viewer (`viewer.js`): a frame
    /// at a readable scale with Fit, 1:1, Wide and Fullscreen controls.
    /// On when absent.
    #[serde(default)]
    pub viewer: Option<bool>,
}

/// The color scheme the documentation pages show before a visitor
/// chooses one: the system's setting, or light or dark regardless of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    #[default]
    System,
    Light,
    Dark,
}

impl ColorScheme {
    /// The name as `asbuilt.toml` and the `--color-scheme` flag spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            ColorScheme::System => "system",
            ColorScheme::Light => "light",
            ColorScheme::Dark => "dark",
        }
    }
}

impl FromStr for ColorScheme {
    type Err = String;

    fn from_str(text: &str) -> std::result::Result<Self, String> {
        match text {
            "system" => Ok(ColorScheme::System),
            "light" => Ok(ColorScheme::Light),
            "dark" => Ok(ColorScheme::Dark),
            other => Err(format!(
                "color scheme \"{other}\" is not system, light or dark"
            )),
        }
    }
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

    /// The file this config came from, for error messages: `asbuilt.toml`
    /// when it was parsed from a string or defaulted.
    pub fn file_path(&self) -> PathBuf {
        self.path
            .clone()
            .unwrap_or_else(|| PathBuf::from(FILE_NAME))
    }

    /// Every top-level entry is asbuilt's own or the table of one of
    /// `frontends` (by [`Frontend::name`](crate::Frontend::name)); the
    /// first that is neither is an error naming it as written.
    pub fn check_tables(&self, frontends: &[&str]) -> Result<()> {
        let Some((name, value)) = self
            .sections
            .iter()
            .find(|(name, _)| !frontends.contains(&name.as_str()))
        else {
            return Ok(());
        };
        let entry = match value {
            toml::Value::Table(_) => format!("[{name}]"),
            toml::Value::Array(items)
                if !items.is_empty() && items.iter().all(toml::Value::is_table) =>
            {
                format!("[[{name}]]")
            }
            _ => format!("{name} = ..."),
        };
        let known = OWN_TABLES
            .iter()
            .map(|t| t.to_string())
            .chain(frontends.iter().map(|name| format!("[{name}]")))
            .collect::<Vec<_>>()
            .join(", ");
        Err(Error::UnknownTopLevel {
            path: self.file_path(),
            entry,
            known,
        })
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
                path: self.file_path(),
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

    fn unknown(text: &str) -> (PathBuf, String, String) {
        let config: Config = text.parse().unwrap();
        match config.check_tables(&["rust"]) {
            Err(Error::UnknownTopLevel { path, entry, known }) => (path, entry, known),
            other => panic!("expected UnknownTopLevel, got {other:?}"),
        }
    }

    #[test]
    fn a_misspelled_table_is_named_as_written() {
        assert_eq!(unknown("[rsut]\ninclude_tests = false\n").1, "[rsut]");
    }

    #[test]
    fn a_singular_externals_table_is_named_as_an_array_of_tables() {
        let (_, entry, _) =
            unknown("[[external]]\nid = \"d\"\nkind = \"process\"\ntitle = \"D\"\n");
        assert_eq!(entry, "[[external]]");
    }

    #[test]
    fn a_stray_top_level_key_is_named_as_a_key() {
        assert_eq!(unknown("title = \"My docs\"\n").1, "title = ...");
    }

    #[test]
    fn a_stray_top_level_list_is_named_as_a_key() {
        assert_eq!(unknown("tags = [\"a\"]\n").1, "tags = ...");
    }

    #[test]
    fn a_stray_empty_list_is_named_as_a_key() {
        assert_eq!(unknown("tags = []\n").1, "tags = ...");
    }

    #[test]
    fn an_unknown_entry_lists_asbuilts_tables_then_the_front_ends() {
        assert_eq!(
            unknown("[rsut]\n").2,
            "[output], [docs], [theme], [[externals]], [rust]"
        );
    }

    #[test]
    fn an_unknown_entry_in_a_defaulted_config_names_the_default_file() {
        assert_eq!(unknown("[rsut]\n").0, PathBuf::from("asbuilt.toml"));
    }

    #[test]
    fn an_unknown_entry_in_a_loaded_config_names_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("elsewhere.toml");
        std::fs::write(&path, "[rsut]\n").unwrap();
        let config = Config::load_file(&path).unwrap();
        assert!(matches!(
            config.check_tables(&["rust"]),
            Err(Error::UnknownTopLevel { path: named, .. }) if named == path
        ));
    }

    #[test]
    fn a_front_ends_table_and_asbuilts_own_pass_the_check() {
        let config: Config = BRIEF_EXAMPLE.parse().unwrap();
        assert!(config.check_tables(&["rust"]).is_ok());
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
        let config: Config = "[docs]\ntitle = \"asbuilt\"\nsource_url = \"https://x/blob/main/\"\nhome_url = \"../\"\nhome_title = \"Home\"\nstylesheet = \"../site.css\"\n"
            .parse()
            .unwrap();
        assert_eq!(config.docs.title.as_deref(), Some("asbuilt"));
        assert_eq!(
            config.docs.source_url.as_deref(),
            Some("https://x/blob/main/")
        );
        assert_eq!(config.docs.home_url.as_deref(), Some("../"));
        assert_eq!(config.docs.home_title.as_deref(), Some("Home"));
        assert_eq!(config.docs.stylesheet.as_deref(), Some("../site.css"));
        let empty: Config = "".parse().unwrap();
        assert_eq!(empty.docs, DocsConfig::default());
    }

    #[test]
    fn a_theme_color_is_a_string_or_a_light_dark_pair() {
        let config: Config =
            "[theme]\ncontainer = \"#3b82f6\"\nprocess = { light = \"#c9c9c9\", dark = \"#444444\" }\n"
                .parse()
                .unwrap();
        assert_eq!(
            config.theme["container"],
            ThemeColor {
                light: "#3b82f6".into(),
                dark: None
            }
        );
        assert_eq!(
            config.theme["process"],
            ThemeColor {
                light: "#c9c9c9".into(),
                dark: Some("#444444".into())
            }
        );
        assert_eq!(
            config.light_theme(),
            BTreeMap::from([
                ("container".to_string(), "#3b82f6".to_string()),
                ("process".to_string(), "#c9c9c9".to_string()),
            ])
        );
    }

    #[test]
    fn a_theme_color_that_is_not_six_hex_digits_is_rejected_naming_the_value() {
        for bad in ["blue", "#fff", "#12345g", "3b82f6"] {
            let result: std::result::Result<Config, _> =
                format!("[theme]\ncontainer = \"{bad}\"\n").parse();
            let err = result.err().map(|e| e.to_string()).unwrap_or_default();
            assert!(err.contains(bad), "{bad}: {err}");
        }
        let result: std::result::Result<Config, _> =
            "[theme]\ncontainer = { light = \"#3b82f6\", dark = \"nope\" }\n".parse();
        assert!(result.is_err(), "{result:?}");
    }

    fn theme_error(color: &str) -> String {
        format!("[theme]\ncontainer = {color}\n")
            .parse::<Config>()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn an_unknown_key_in_a_theme_color_is_rejected_naming_it() {
        let err = theme_error("{ light = \"#f0a884\", drak = \"#b5673f\" }");
        assert!(err.contains("unknown key \"drak\""), "{err}");
    }

    #[test]
    fn a_theme_color_table_without_light_is_rejected_saying_so() {
        let err = theme_error("{ dark = \"#b5673f\" }");
        assert!(err.contains("needs light"), "{err}");
    }

    #[test]
    fn a_theme_color_of_another_type_is_rejected_naming_the_type() {
        assert!(
            theme_error("3").contains("not a integer"),
            "{}",
            theme_error("3")
        );
        let err = theme_error("{ light = 3 }");
        assert!(err.contains("light is a integer"), "{err}");
    }

    #[test]
    fn the_docs_viewer_key_parses_and_defaults_to_nothing() {
        let config: Config = "[docs]\nviewer = false\n".parse().unwrap();
        assert_eq!(config.docs.viewer, Some(false));
        let empty: Config = "[docs]\n".parse().unwrap();
        assert_eq!(empty.docs.viewer, None);
    }

    #[test]
    fn the_docs_color_scheme_and_toggle_parse_and_default_to_nothing() {
        let config: Config = "[docs]\ncolor_scheme = \"dark\"\nscheme_toggle = false\n"
            .parse()
            .unwrap();
        assert_eq!(config.docs.color_scheme, Some(ColorScheme::Dark));
        assert_eq!(config.docs.scheme_toggle, Some(false));
        let empty: Config = "[docs]\n".parse().unwrap();
        assert_eq!(empty.docs.color_scheme, None);
        assert_eq!(empty.docs.scheme_toggle, None);
    }

    #[test]
    fn a_color_scheme_that_is_not_one_of_the_three_is_rejected_naming_them() {
        let err = "[docs]\ncolor_scheme = \"sepia\"\n"
            .parse::<Config>()
            .unwrap_err()
            .to_string();
        assert!(err.contains("sepia") && err.contains("dark"), "{err}");
    }

    #[test]
    fn a_color_scheme_reads_from_its_name_and_prints_back_the_same() {
        for scheme in [ColorScheme::System, ColorScheme::Light, ColorScheme::Dark] {
            assert_eq!(scheme.as_str().parse::<ColorScheme>(), Ok(scheme));
        }
        assert_eq!(
            "sepia".parse::<ColorScheme>(),
            Err("color scheme \"sepia\" is not system, light or dark".to_string())
        );
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
