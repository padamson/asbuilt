//! The subcommands, as functions that return an exit code, so the binary
//! is a thin `main` and the tests can drive each path. `survey` and
//! `check` need no Node; `validate`, `export json` and `render` shell
//! out to the pinned LikeC4 (and `render` to Graphviz).

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use std::collections::{BTreeMap, BTreeSet};

use asbuilt_core::docs::{DocsOptions, GENERATOR_META, generate};
use asbuilt_core::svg::ViewSource;
use asbuilt_core::{ColorScheme, Config, EmitOptions, Frontend, Model, Outcome, compare, emit};
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

    #[error(
        "output directory {path} has an index.html, stylesheet, container page or copied SVG that `asbuilt docs` did not write; move it or pass --force to replace it"
    )]
    OutputNotOurs { path: PathBuf },

    #[error(transparent)]
    LikeC4(#[from] LikeC4Error),
}

/// The config every command reads, with any top-level entry no
/// front-end claims rejected before the command does anything.
fn load_config(root: &Path, config_path: Option<&Path>) -> Result<Config, CliError> {
    let config = match config_path {
        Some(path) => Config::load_file(path)?,
        None => Config::load(root)?,
    };
    config.check_tables(&[RustFrontend.name()])?;
    Ok(config)
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
            theme: config.light_theme(),
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

/// A fresh survey of `root` and how it compares with the committed
/// model: the shared half of `check` and `docs`. Errors name the
/// root-relative label, which is what the user configured and reads the
/// same on every platform.
fn drift(
    root: &Path,
    config: &Config,
    target: &OutputTarget,
) -> Result<(Model, Outcome), CliError> {
    let rel = &target.label;
    let committed = std::fs::read_to_string(&target.path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            CliError::NoModel {
                path: PathBuf::from(rel),
            }
        } else {
            CliError::Read {
                path: PathBuf::from(rel),
                source,
            }
        }
    })?;
    let model = survey_model(root, config)?;
    let fresh = emit(
        &model,
        &EmitOptions {
            link_prefix: target.link_prefix.clone(),
            theme: config.light_theme(),
        },
    );
    let outcome = compare(&committed, &fresh, rel);
    Ok((model, outcome))
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
    let rel = target.label.clone();
    match drift(root, &config, &target)?.1 {
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

/// The files directly under `dir` with extension `ext`, sorted; empty
/// when `dir` does not exist.
fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == ext))
        .collect();
    files.sort();
    files
}

/// File stems of the `.svg` files directly under `dir`; empty when the
/// directory does not exist.
pub fn svg_stems(dir: &Path) -> BTreeSet<String> {
    files_with_extension(dir, "svg")
        .iter()
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect()
}

/// Each rendered view under `dir` by file stem: the SVG, and the `.dot`
/// beside it when there is one.
fn view_sources(dir: &Path) -> Result<BTreeMap<String, ViewSource>, CliError> {
    let mut views = BTreeMap::new();
    for stem in svg_stems(dir) {
        let svg_path = dir.join(format!("{stem}.svg"));
        let svg = std::fs::read_to_string(&svg_path).map_err(|source| CliError::Read {
            path: svg_path.clone(),
            source,
        })?;
        let dot = std::fs::read_to_string(dir.join(format!("{stem}.dot"))).ok();
        views.insert(stem, ViewSource { svg, dot });
    }
    Ok(views)
}

/// `rel`, a `/`-separated page path, under `dir` with the platform's
/// own separators.
fn page_path(dir: &Path, rel: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    for segment in rel.split('/') {
        path.push(segment);
    }
    path
}

/// The pages, stylesheets and scripts `asbuilt docs` writes at the root of its tree.
const DOCS_ROOT_FILES: [&str; 6] = [
    "index.html",
    "views.html",
    "style.css",
    "theme.css",
    "theme.js",
    "viewer.js",
];

/// The files in `out` that `asbuilt docs` writes, by name: the root pages
/// and stylesheets, the container pages, and the copied SVGs, unless
/// `views_are_renders` says the tree's `views/` is the render directory
/// itself. Anything else in `out` belongs to someone else.
fn docs_files_in(out: &Path, views_are_renders: bool) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = DOCS_ROOT_FILES
        .iter()
        .map(|name| out.join(name))
        .filter(|path| path.is_file())
        .collect();
    files.extend(files_with_extension(&out.join("containers"), "html"));
    if !views_are_renders {
        files.extend(files_with_extension(&out.join("views"), "svg"));
    }
    files
}

/// Refuse to write where the run would replace files an earlier `docs`
/// run did not write. A directory holding none of the tree's files (absent,
/// empty, a `.gitkeep`, the model itself) is fine, and so is a tree whose
/// `index.html` carries the generator meta. `force` accepts anything.
fn check_output(out: &Path, force: bool, views_are_renders: bool) -> Result<(), CliError> {
    if force || docs_files_in(out, views_are_renders).is_empty() {
        return Ok(());
    }
    let ours = std::fs::read_to_string(out.join("index.html"))
        .is_ok_and(|index| index.contains(GENERATOR_META));
    if ours {
        Ok(())
    } else {
        Err(CliError::OutputNotOurs {
            path: out.to_path_buf(),
        })
    }
}

/// Remove the files an earlier run wrote (see [`docs_files_in`]), so a
/// renamed crate's page or a dropped view cannot outlive the run that
/// replaces it. Nothing else in `out` is touched.
fn clear_docs_files(out: &Path, views_are_renders: bool) -> Result<(), CliError> {
    for path in docs_files_in(out, views_are_renders) {
        std::fs::remove_file(&path).map_err(|source| CliError::Write { path, source })?;
    }
    Ok(())
}

/// The inputs of `asbuilt docs` beyond the root.
#[derive(Debug, Clone, Default)]
pub struct DocsArgs<'a> {
    pub config_path: Option<&'a Path>,
    /// Write here instead of `<model dir>/site`, relative to the root.
    pub output: Option<&'a Path>,
    /// Reuse the SVGs under `<model dir>/views` instead of rendering.
    pub no_render: bool,
    /// Replace pages, stylesheets or SVGs in the output directory that no
    /// earlier run wrote.
    pub force: bool,
    pub title: Option<&'a str>,
    pub source_url: Option<&'a str>,
    pub home_url: Option<&'a str>,
    pub home_title: Option<&'a str>,
    pub stylesheet: Option<&'a str>,
    /// The scheme the pages show before a visitor chooses one.
    pub color_scheme: Option<ColorScheme>,
    /// Leave out the visitor's scheme control and the script behind it.
    pub no_scheme_toggle: bool,
    /// Leave out the diagram viewer and its script.
    pub no_viewer: bool,
}

/// Write the documentation tree: survey, refuse to write when the
/// committed model is stale (the SVGs come from it, the pages from the
/// survey) or when the output directory holds tree files no earlier run
/// wrote, render the views unless told not to, generate, remove what an
/// earlier run wrote, write, copy the SVGs beside the pages, and report
/// the views that have none.
pub fn docs(root: &Path, args: &DocsArgs<'_>, err: &mut dyn Write) -> Result<i32, CliError> {
    let DocsArgs {
        config_path,
        output: override_path,
        no_render,
        force,
        title,
        source_url,
        home_url,
        home_title,
        stylesheet,
        color_scheme,
        no_scheme_toggle,
        no_viewer,
    } = *args;
    let config = load_config(root, config_path)?;
    let target = output_target(root, &config, None)?;
    let (model, outcome) = drift(root, &config, &target)?;
    if let Outcome::Drift(_) = outcome {
        let _ = writeln!(
            err,
            "asbuilt: {} is stale; run `asbuilt survey` and commit the result before `asbuilt docs`",
            target.label
        );
        return Ok(EXIT_DRIFT);
    }
    let model_dir = target
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| target.path.clone());
    let svg_dir = model_dir.join("views");
    let out = under_model_dir(root, &model_dir, override_path, "site");
    let out_views = out.join("views");
    // Compared as resolved paths, not by canonicalizing what exists:
    // before a first render neither directory is there yet.
    let resolve = |path: &Path| normalize_lexically(&canonicalize_partial(path));
    let views_are_renders = resolve(&svg_dir) == resolve(&out_views);
    // Refuse before rendering, and clear only once the render and the
    // pages are ready, so neither a refusal nor a failed render costs the
    // previous tree.
    check_output(&out, force, views_are_renders)?;
    if !no_render {
        likec4::render(&model_dir, &svg_dir)?;
    }
    let root_name = canonical_root(root)?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asbuilt".to_string());
    let flag_or_config = |flag: Option<&str>, configured: &Option<String>| {
        flag.map(str::to_string).or_else(|| configured.clone())
    };
    let options = DocsOptions {
        title: flag_or_config(title, &config.docs.title).unwrap_or(root_name),
        source_url: flag_or_config(source_url, &config.docs.source_url),
        home_url: flag_or_config(home_url, &config.docs.home_url),
        home_title: flag_or_config(home_title, &config.docs.home_title),
        stylesheet: flag_or_config(stylesheet, &config.docs.stylesheet),
        views: view_sources(&svg_dir)?,
        theme: config.theme.clone(),
        color_scheme: color_scheme
            .or(config.docs.color_scheme)
            .unwrap_or_default(),
        scheme_toggle: !no_scheme_toggle && config.docs.scheme_toggle.unwrap_or(true),
        viewer: !no_viewer && config.docs.viewer.unwrap_or(true),
    };
    let site = generate(&model, &options);

    clear_docs_files(&out, views_are_renders)?;
    for (rel, contents) in &site.pages {
        let path = page_path(&out, rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CliError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(&path, contents).map_err(|source| CliError::Write { path, source })?;
    }
    if svg_dir.is_dir() && !views_are_renders {
        std::fs::create_dir_all(&out_views).map_err(|source| CliError::Write {
            path: out_views.clone(),
            source,
        })?;
        for stem in options.views.keys() {
            let name = format!("{stem}.svg");
            std::fs::copy(svg_dir.join(&name), out_views.join(&name)).map_err(|source| {
                CliError::Write {
                    path: out_views.join(&name),
                    source,
                }
            })?;
        }
    }
    for view in &site.missing_views {
        let _ = writeln!(
            err,
            "asbuilt: no SVG for view {view} (views/{view}.svg); run `asbuilt render`"
        );
    }
    let _ = writeln!(err, "wrote {} pages to {}", site.pages.len(), out.display());
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
    fn svg_stems_are_the_svg_files_only_and_an_absent_directory_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.svg", "a.svg", "a.dot", "notes.txt"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let stems: Vec<String> = svg_stems(dir.path()).into_iter().collect();
        assert_eq!(stems, ["a", "b"]);
        assert!(svg_stems(&dir.path().join("nowhere")).is_empty());
    }

    #[test]
    fn a_page_path_is_joined_segment_by_segment() {
        assert_eq!(
            page_path(Path::new("out"), "containers/app.html"),
            Path::new("out").join("containers").join("app.html")
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
