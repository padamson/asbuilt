//! Errors that name the offending input, so a message reads without a
//! rerun.

use std::path::PathBuf;

/// Anything the core, a front-end, or the config can get wrong.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `asbuilt.toml` did not parse or had a key the schema rejects.
    #[error("{path}: {source}")]
    Config {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    /// An `[[externals.relations]]` `from` that names no generated
    /// element and no sibling external. A typo here is an error, not a
    /// diagram that quietly omits an edge.
    #[error(
        "externals.relations: \"{from}\" (under external \"{external}\") names no element in the surveyed model"
    )]
    UnknownRelationSource { external: String, from: String },

    /// Two model ids that become the same LikeC4 identifier once `-`
    /// maps to `_` and a leading digit is prefixed.
    #[error("element ids \"{first}\" and \"{second}\" both become the LikeC4 id \"{id}\"")]
    DuplicateId {
        id: String,
        first: String,
        second: String,
    },

    /// No front-end recognized the root.
    #[error("no supported stack detected at {root}")]
    NoFrontend { root: PathBuf },

    /// A front-end failed; the source says which file or module.
    #[error("{frontend} front-end: {source}")]
    Frontend {
        frontend: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_relation_source_names_the_external_and_the_from() {
        let msg = Error::UnknownRelationSource {
            external: "node_driver".into(),
            from: "playwright_rs.nope".into(),
        }
        .to_string();
        assert!(msg.contains("node_driver"), "{msg}");
        assert!(msg.contains("playwright_rs.nope"), "{msg}");
    }

    #[test]
    fn a_duplicate_id_names_both_originals_and_the_collision() {
        let msg = Error::DuplicateId {
            id: "foo_bar".into(),
            first: "foo-bar".into(),
            second: "foo_bar".into(),
        }
        .to_string();
        assert!(
            msg.contains("\"foo-bar\"") && msg.contains("\"foo_bar\""),
            "{msg}"
        );
    }
}
