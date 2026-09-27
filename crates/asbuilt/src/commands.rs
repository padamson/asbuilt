//! The subcommands, as functions that return an exit code, so the binary
//! is a thin `main` and the tests can drive each path. `survey` and
//! `check` need no Node; `validate`, `export json` and `render` shell
//! out to the pinned LikeC4 (and `render` to Graphviz).

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use asbuilt_core::{Config, EmitOptions, Model, Outcome, compare, emit};
use asbuilt_rust::RustFrontend;

use crate::likec4::{self, LikeC4Error};

pub const EXIT_OK: i32 = 0;
/// The committed model no longer matches a fresh survey, or LikeC4
/// rejects the model directory.
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

    #[error(transparent)]
    LikeC4(#[from] LikeC4Error),
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

/// `root` canonicalized, with the Windows verbatim prefix dropped.
fn canonical_root(root: &Path) -> Result<PathBuf, CliError> {
    let canonical = root.canonicalize().map_err(|source| CliError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    let text = canonical.to_string_lossy();
    Ok(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text)))
}

/// Canonicalize the deepest ancestor of `path` that exists and re-append
/// the rest, so an output file that is not written yet still compares
/// against the canonical root (`/tmp` is a symlink on macOS).
fn canonicalize_partial(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
    let Ok(canonical) = existing.canonicalize() else {
        return path.to_path_buf();
    };
    let text = canonical.to_string_lossy();
    let mut out = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text));
    for name in tail.into_iter().rev() {
        out.push(name);
    }
    out
}

/// Resolve `.` and `..` lexically, for a path that may not exist yet.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The `link` prefix that takes a file in `out_dir` back to `root`:
/// `../` per directory below the common ancestor, then the root's own
/// path below it; `./` when the file sits at the root.
fn link_prefix(out_dir: &Path, root: &Path) -> String {
    let out: Vec<Component> = out_dir.components().collect();
    let base: Vec<Component> = root.components().collect();
    let common = out
        .iter()
        .zip(base.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut prefix = "../".repeat(out.len() - common);
    for component in &base[common..] {
        prefix.push_str(&component.as_os_str().to_string_lossy());
        prefix.push('/');
    }
    if prefix.is_empty() {
        "./".to_string()
    } else {
        prefix
    }
}

/// Where the model goes and how its links get back to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputTarget {
    /// The path as shown in messages and the diff header: relative to
    /// the root when under it, otherwise absolute.
    pub label: String,
    pub path: PathBuf,
    pub link_prefix: String,
}

/// The `-o` override (relative to the root, or absolute) or the config's
/// path, resolved against the canonical root so `..`, symlinks and a
/// `.` root all count directories correctly.
pub fn output_target(
    root: &Path,
    config: &Config,
    override_path: Option<&Path>,
) -> Result<OutputTarget, CliError> {
    let root = canonical_root(root)?;
    let requested = override_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&config.output.path));
    // `join` keeps an absolute `requested` as is.
    let path = canonicalize_partial(&normalize_lexically(&root.join(requested)));
    let out_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.clone());
    let label = match path.strip_prefix(&root) {
        Ok(rel) => slashes(rel),
        Err(_) => path.to_string_lossy().into_owned(),
    };
    Ok(OutputTarget {
        label,
        link_prefix: link_prefix(&out_dir, &root),
        path,
    })
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
    let target = output_target(root, &config, override_path)?;
    let model = survey_model(root, &config)?;
    let text = emit(
        &model,
        &EmitOptions {
            link_prefix: target.link_prefix,
        },
    );
    let path = target.path;
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
    let target = output_target(root, &config, None)?;
    let (rel, path) = (target.label, target.path);
    // Errors name the root-relative label, which is what the user
    // configured and reads the same on every platform.
    let committed = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            CliError::NoModel {
                path: PathBuf::from(&rel),
            }
        } else {
            CliError::Read {
                path: PathBuf::from(&rel),
                source,
            }
        }
    })?;
    let model = survey_model(root, &config)?;
    let fresh = emit(
        &model,
        &EmitOptions {
            link_prefix: target.link_prefix,
        },
    );
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

/// The directory holding the committed model and any curated `.c4`
/// files beside it: the configured output path's parent.
fn model_dir(root: &Path, config_path: Option<&Path>) -> Result<(String, PathBuf), CliError> {
    let config = load_config(root, config_path)?;
    let target = output_target(root, &config, None)?;
    let dir = target
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| target.path.clone());
    let label = Path::new(&target.label)
        .parent()
        .map(slashes)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());
    Ok((label, dir))
}

/// A path for a LikeC4 output: the override (relative to the root, or
/// absolute, which `join` keeps as is) or `model_dir/<default>`.
fn under_model_dir(
    root: &Path,
    dir: &Path,
    override_path: Option<&Path>,
    default: &str,
) -> PathBuf {
    match override_path {
        Some(p) => root.join(p),
        None => dir.join(default),
    }
}

/// `likec4 validate` over the model directory. Exit 1 when LikeC4
/// rejects it (the diagnostics go to `err`), 2 when it cannot run.
pub fn validate(
    root: &Path,
    config_path: Option<&Path>,
    err: &mut dyn Write,
) -> Result<i32, CliError> {
    let (label, dir) = model_dir(root, config_path)?;
    match likec4::validate(&dir) {
        Ok(()) => {
            let _ = writeln!(err, "{label}: valid");
            Ok(EXIT_OK)
        }
        Err(LikeC4Error::Failed { output, .. }) => {
            let _ = write!(err, "{output}");
            let _ = writeln!(err, "asbuilt: {label}: likec4 rejected the model");
            Ok(EXIT_DRIFT)
        }
        Err(other) => Err(other.into()),
    }
}

/// `likec4 export json` over the model directory, normalized, to the
/// override or `model_dir/model.json`.
pub fn export_json(
    root: &Path,
    config_path: Option<&Path>,
    override_path: Option<&Path>,
    err: &mut dyn Write,
) -> Result<i32, CliError> {
    let (_, dir) = model_dir(root, config_path)?;
    let out = under_model_dir(root, &dir, override_path, "model.json");
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CliError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    likec4::export_json(&dir, &out)?;
    let _ = writeln!(err, "wrote {}", out.display());
    Ok(EXIT_OK)
}

/// Render every view to an SVG under the override or `model_dir/views`.
pub fn render(
    root: &Path,
    config_path: Option<&Path>,
    override_path: Option<&Path>,
    err: &mut dyn Write,
) -> Result<i32, CliError> {
    let (_, dir) = model_dir(root, config_path)?;
    let out_dir = under_model_dir(root, &dir, override_path, "views");
    for svg in likec4::render(&dir, &out_dir)? {
        let _ = writeln!(err, "wrote {}", svg.display());
    }
    Ok(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn target(dir: &tempfile::TempDir, override_path: Option<&str>) -> OutputTarget {
        output_target(dir.path(), &Config::default(), override_path.map(Path::new)).unwrap()
    }

    #[test]
    fn the_default_output_is_the_config_path_under_the_root_two_levels_down() {
        let dir = root();
        let t = target(&dir, None);
        assert_eq!(t.label, "docs/architecture/model.c4");
        assert_eq!(t.link_prefix, "../../");
        assert!(t.path.ends_with("docs/architecture/model.c4"));
    }

    #[test]
    fn a_relative_override_is_under_the_root_and_counts_its_own_depth() {
        let dir = root();
        let t = target(&dir, Some("out/m.c4"));
        assert_eq!(t.label, "out/m.c4");
        assert_eq!(t.link_prefix, "../");
    }

    #[test]
    fn a_file_at_the_root_links_with_a_dot_prefix() {
        let dir = root();
        let t = target(&dir, Some("m.c4"));
        assert_eq!(t.label, "m.c4");
        assert_eq!(t.link_prefix, "./");
    }

    #[test]
    fn a_relative_override_that_climbs_out_of_the_root_links_back_through_the_root_name() {
        let dir = root();
        let t = target(&dir, Some("../out/m.c4"));
        let root_name = canonical_root(dir.path())
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(t.link_prefix, format!("../{root_name}/"));
        assert!(Path::new(&t.label).is_absolute(), "{}", t.label);
        assert!(
            t.path.ends_with("out/m.c4") && !t.path.to_string_lossy().contains(".."),
            "{:?}",
            t.path
        );
    }

    #[test]
    fn an_absolute_override_inside_the_root_keeps_its_relative_spelling() {
        let dir = root();
        let inside = canonical_root(dir.path()).unwrap().join("a/m.c4");
        let t = output_target(dir.path(), &Config::default(), Some(&inside)).unwrap();
        assert_eq!(t.label, "a/m.c4");
        assert_eq!(t.link_prefix, "../");
    }

    #[test]
    fn an_absolute_override_through_an_uncanonical_root_still_counts_its_real_depth() {
        // `dir.path()` is not canonical on macOS (`/tmp` is a symlink);
        // the output file does not exist yet either.
        let dir = root();
        let inside = dir.path().join("docs/arch/m.c4");
        let t = output_target(dir.path(), &Config::default(), Some(&inside)).unwrap();
        assert_eq!(t.label, "docs/arch/m.c4");
        assert_eq!(t.link_prefix, "../../");
    }

    #[test]
    fn the_deepest_existing_ancestor_is_canonicalized_and_the_tail_reattached() {
        // `.` exists and canonicalizes to the absolute current directory;
        // `x/nowhere` does not exist and rides along. Holds on every
        // platform, unlike a symlinked temp root.
        let got = canonicalize_partial(Path::new("./x/nowhere"));
        let cwd = canonical_root(Path::new(".")).unwrap();
        assert_eq!(got, cwd.join("x").join("nowhere"));
    }

    #[test]
    fn a_path_with_no_existing_ancestor_is_returned_as_is() {
        assert_eq!(
            canonicalize_partial(Path::new("relative/nowhere")),
            PathBuf::from("relative/nowhere")
        );
    }

    #[test]
    fn lexical_normalization_resolves_dots_without_touching_the_disk() {
        assert_eq!(
            normalize_lexically(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(
            normalize_lexically(Path::new("../x")),
            PathBuf::from("../x")
        );
    }

    #[test]
    fn a_missing_root_is_a_read_error_naming_it() {
        match output_target(Path::new("/no/such/root"), &Config::default(), None) {
            Err(CliError::Read { path, .. }) => assert_eq!(path, PathBuf::from("/no/such/root")),
            other => panic!("expected Read, got {other:?}"),
        }
    }

    #[test]
    fn the_model_directory_is_the_output_path_parent_with_a_relative_label() {
        let dir = root();
        let (label, path) = model_dir(dir.path(), None).unwrap();
        assert_eq!(label, "docs/architecture");
        assert!(path.ends_with("docs/architecture"), "{path:?}");
    }

    #[test]
    fn a_model_at_the_root_has_the_dot_label() {
        let dir = root();
        std::fs::write(
            dir.path().join("asbuilt.toml"),
            "[output]\npath = \"model.c4\"\n",
        )
        .unwrap();
        let (label, path) = model_dir(dir.path(), None).unwrap();
        assert_eq!(label, ".");
        assert_eq!(path, canonical_root(dir.path()).unwrap());
    }

    #[test]
    fn likec4_outputs_default_under_the_model_directory_and_honor_an_override() {
        let root = Path::new("/r");
        let dir = Path::new("/r/docs/architecture");
        assert_eq!(
            under_model_dir(root, dir, None, "views"),
            PathBuf::from("/r/docs/architecture/views")
        );
        assert_eq!(
            under_model_dir(root, dir, Some(Path::new("out/v")), "views"),
            PathBuf::from("/r/out/v")
        );
        assert_eq!(
            under_model_dir(root, dir, Some(Path::new("/abs/v")), "views"),
            PathBuf::from("/abs/v")
        );
    }

    #[test]
    fn a_missing_config_file_given_explicitly_is_an_error() {
        assert!(matches!(
            load_config(Path::new("."), Some(Path::new("/no/such/asbuilt.toml"))),
            Err(CliError::Core(asbuilt_core::Error::Io { .. }))
        ));
    }
}
