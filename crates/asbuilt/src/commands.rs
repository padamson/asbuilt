//! `survey` and `check`, as functions that return an exit code, so the
//! binary is a thin `main` and the tests can drive either path.

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use asbuilt_core::{Config, EmitOptions, Model, Outcome, compare, emit};
use asbuilt_rust::RustFrontend;

pub const EXIT_OK: i32 = 0;
/// The committed model no longer matches a fresh survey.
pub const EXIT_DRIFT: i32 = 1;
/// Anything else that went wrong.
pub const EXIT_ERROR: i32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Core(#[from] asbuilt_core::Error),

    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("no model at {path}; run `asbuilt survey` to create it")]
    NoModel { path: PathBuf },
}

fn load_config(root: &Path, config_path: Option<&Path>) -> Result<Config, CliError> {
    Ok(match config_path {
        Some(path) => Config::load_file(path)?,
        None => Config::load(root)?,
    })
}

fn slashes(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Where the model goes: the `-o` override (relative to the root, or
/// absolute) or the config's path. Returns the root-relative spelling
/// used for links and messages, and the file to write.
pub fn output_path(
    root: &Path,
    config: &Config,
    override_path: Option<&Path>,
) -> (String, PathBuf) {
    match override_path {
        None => (config.output.path.clone(), root.join(&config.output.path)),
        Some(p) if p.is_absolute() => {
            let rel = p.strip_prefix(root).map(slashes).unwrap_or_else(|_| {
                p.file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            (rel, p.to_path_buf())
        }
        Some(p) => (slashes(p), root.join(p)),
    }
}

/// A fresh model of `root` with every front-end this binary carries.
pub fn survey_model(root: &Path, config: &Config) -> Result<Model, CliError> {
    Ok(asbuilt_core::survey(root, config, &[&RustFrontend])?)
}

/// Write the model at the configured (or overridden) path, creating
/// parent directories. Prints nothing on success.
pub fn survey(
    root: &Path,
    config_path: Option<&Path>,
    override_path: Option<&Path>,
) -> Result<i32, CliError> {
    let config = load_config(root, config_path)?;
    let (rel, path) = output_path(root, &config, override_path);
    let model = survey_model(root, &config)?;
    let text = emit(&model, &EmitOptions::for_output_path(&rel));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CliError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(&path, text).map_err(|source| CliError::Write { path, source })?;
    Ok(EXIT_OK)
}

/// Survey in memory and compare with the committed model. The diff, if
/// any, goes to `out`; the verdict line goes to `err`.
pub fn check(
    root: &Path,
    config_path: Option<&Path>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32, CliError> {
    let config = load_config(root, config_path)?;
    let (rel, path) = output_path(root, &config, None);
    let committed = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            CliError::NoModel { path: path.clone() }
        } else {
            CliError::Read {
                path: path.clone(),
                source,
            }
        }
    })?;
    let model = survey_model(root, &config)?;
    let fresh = emit(&model, &EmitOptions::for_output_path(&rel));
    match compare(&committed, &fresh, &rel) {
        Outcome::Current => {
            let _ = writeln!(out, "{rel} is current");
            Ok(EXIT_OK)
        }
        Outcome::Drift(diff) => {
            let _ = write!(out, "{diff}");
            let _ = writeln!(
                err,
                "asbuilt: {rel} is stale; run `asbuilt survey` and commit the result"
            );
            Ok(EXIT_DRIFT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_output_is_the_config_path_under_the_root() {
        let config = Config::default();
        let (rel, path) = output_path(Path::new("/r"), &config, None);
        assert_eq!(rel, "docs/architecture/model.c4");
        assert_eq!(path, PathBuf::from("/r/docs/architecture/model.c4"));
    }

    #[test]
    fn a_relative_override_is_under_the_root_and_spelled_with_slashes() {
        let (rel, path) = output_path(
            Path::new("/r"),
            &Config::default(),
            Some(Path::new("out/m.c4")),
        );
        assert_eq!(rel, "out/m.c4");
        assert_eq!(path, PathBuf::from("/r/out/m.c4"));
    }

    #[test]
    fn an_absolute_override_inside_the_root_keeps_its_relative_spelling() {
        let (rel, path) = output_path(
            Path::new("/r"),
            &Config::default(),
            Some(Path::new("/r/a/m.c4")),
        );
        assert_eq!(rel, "a/m.c4");
        assert_eq!(path, PathBuf::from("/r/a/m.c4"));
    }

    #[test]
    fn an_absolute_override_outside_the_root_is_labeled_by_its_file_name() {
        let (rel, path) = output_path(
            Path::new("/r"),
            &Config::default(),
            Some(Path::new("/elsewhere/m.c4")),
        );
        assert_eq!(rel, "m.c4");
        assert_eq!(path, PathBuf::from("/elsewhere/m.c4"));
    }

    #[test]
    fn a_missing_config_file_given_explicitly_is_an_error() {
        assert!(matches!(
            load_config(Path::new("."), Some(Path::new("/no/such/asbuilt.toml"))),
            Err(CliError::Core(asbuilt_core::Error::Io { .. }))
        ));
    }
}
