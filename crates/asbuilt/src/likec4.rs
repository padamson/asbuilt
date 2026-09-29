//! The pinned `npx likec4` calls, plus Graphviz for rendering. Everything
//! downstream of the model is LikeC4's; this module runs it and reports
//! what it said.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::likec4_package;

/// Why a LikeC4 or Graphviz call did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum LikeC4Error {
    /// The program could not be started at all (no Node, no Graphviz).
    #[error("could not run `{command}`: {source}")]
    NotRunnable {
        command: String,
        #[source]
        source: std::io::Error,
    },
    /// It ran and reported a problem; `output` is what it printed.
    #[error("`{command}` failed with {status}:\n{output}")]
    Failed {
        command: String,
        status: String,
        output: String,
    },
    #[error("reading or writing {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not the JSON likec4 was expected to write: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// The `npx` executable for this platform.
pub fn npx_program() -> &'static str {
    if cfg!(windows) { "npx.cmd" } else { "npx" }
}

/// The arguments that select the pinned LikeC4: `--yes likec4@<pinned>`
/// followed by `args`.
pub fn npx_args(args: &[&str]) -> Vec<String> {
    let mut all = vec!["--yes".to_string(), likec4_package()];
    all.extend(args.iter().map(|s| s.to_string()));
    all
}

/// Run `program args...` and fold a non-zero exit into an error carrying
/// the combined output. Public so the tests can drive it with a program
/// every machine has.
pub fn run_program(program: &str, args: &[String]) -> Result<String, LikeC4Error> {
    let command = format!("{program} {}", args.join(" "));
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|source| LikeC4Error::NotRunnable {
            command: command.clone(),
            source,
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(LikeC4Error::Failed {
            command,
            status: output.status.to_string(),
            output: format!("{stdout}{stderr}"),
        });
    }
    Ok(format!("{stdout}{stderr}"))
}

/// Run `npx --yes likec4@<pinned> <args>`.
pub fn run(args: &[&str]) -> Result<String, LikeC4Error> {
    run_program(npx_program(), &npx_args(args))
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path to hand to a subprocess")
}

/// The arguments of `likec4 validate` over the `.c4` files under `dir`.
/// LikeC4 merges every file under a path into one model, so validate
/// one model's directory at a time.
pub fn validate_args(dir: &Path) -> Vec<&str> {
    vec!["validate", "--no-layout", path_str(dir)]
}

/// `likec4 validate` over `dir`.
pub fn validate(dir: &Path) -> Result<(), LikeC4Error> {
    run(&validate_args(dir)).map(|_| ())
}

/// The arguments of `likec4 export json`: no layout (layout is not
/// deterministic and not the point), pretty-printed, to `out`.
pub fn export_json_args<'a>(dir: &'a Path, out: &'a Path) -> Vec<&'a str> {
    vec![
        "export",
        "json",
        "--skip-layout",
        "--pretty",
        "-o",
        path_str(out),
        path_str(dir),
    ]
}

/// Strip every `links[].relative`, an absolute `file://` URL of the
/// exporting checkout, which is the one thing in the export that
/// differs between machines. Everything else is left as is.
///
/// ```
/// let mut value = serde_json::json!({
///     "elements": {
///         "app": { "links": [{ "url": "./crates/app", "relative": "file:///home/x/crates/app" }] }
///     }
/// });
/// asbuilt::likec4::normalize(&mut value);
/// assert_eq!(value["elements"]["app"]["links"][0], serde_json::json!({ "url": "./crates/app" }));
/// ```
pub fn normalize(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(links)) = map.get_mut("links") {
                for link in links.iter_mut() {
                    if let Value::Object(link) = link {
                        link.remove("relative");
                    }
                }
            }
            for child in map.values_mut() {
                normalize(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                normalize(child);
            }
        }
        _ => {}
    }
}

/// `likec4 export json` over `dir` into `out`, normalized so the file is
/// the same on every machine.
pub fn export_json(dir: &Path, out: &Path) -> Result<(), LikeC4Error> {
    run(&export_json_args(dir, out))?;
    let text = std::fs::read_to_string(out).map_err(|source| LikeC4Error::Io {
        path: out.to_path_buf(),
        source,
    })?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| LikeC4Error::Json {
            path: out.to_path_buf(),
            source,
        })?;
    normalize(&mut value);
    let mut pretty = serde_json::to_string_pretty(&value).expect("a Value serializes");
    pretty.push('\n');
    std::fs::write(out, pretty).map_err(|source| LikeC4Error::Io {
        path: out.to_path_buf(),
        source,
    })
}

/// The arguments of `likec4 gen dot`: one Graphviz file per view into
/// `out_dir`.
pub fn gen_dot_args<'a>(dir: &'a Path, out_dir: &'a Path) -> Vec<&'a str> {
    vec!["gen", "dot", "-o", path_str(out_dir), path_str(dir)]
}

/// `likec4 gen dot` over `dir` into `out_dir`; returns the `.dot` files
/// written, sorted.
pub fn gen_dot(dir: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, LikeC4Error> {
    std::fs::create_dir_all(out_dir).map_err(|source| LikeC4Error::Io {
        path: out_dir.to_path_buf(),
        source,
    })?;
    run(&gen_dot_args(dir, out_dir))?;
    dot_files_in(out_dir)
}

/// The `.dot` files directly under `dir`, sorted.
pub fn dot_files_in(dir: &Path) -> Result<Vec<PathBuf>, LikeC4Error> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| LikeC4Error::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "dot"))
        .collect();
    files.sort();
    Ok(files)
}

/// The arguments of `dot -Tsvg` for one Graphviz file, writing beside it.
pub fn svg_args(dot_file: &Path) -> (PathBuf, Vec<String>) {
    let svg = dot_file.with_extension("svg");
    let args = vec![
        "-Tsvg".to_string(),
        path_str(dot_file).to_string(),
        "-o".to_string(),
        path_str(&svg).to_string(),
    ];
    (svg, args)
}

/// Remove the `.dot` and `.svg` files directly under `dir`, so a view
/// that no longer exists leaves no render behind. A missing directory is
/// nothing to clear.
pub fn clear_renders(dir: &Path) -> Result<(), LikeC4Error> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_some_and(|e| e == "dot" || e == "svg") {
            std::fs::remove_file(&path).map_err(|source| LikeC4Error::Io { path, source })?;
        }
    }
    Ok(())
}

/// Render every view of the model under `dir` to an SVG in `out_dir`:
/// clear the previous renders, `likec4 gen dot`, then Graphviz `dot
/// -Tsvg` per file. Returns the SVGs written, sorted. The `.dot` files
/// stay beside them.
pub fn render(dir: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, LikeC4Error> {
    clear_renders(out_dir)?;
    let mut svgs = Vec::new();
    for dot_file in gen_dot(dir, out_dir)? {
        let (svg, args) = svg_args(&dot_file);
        run_program("dot", &args)?;
        svgs.push(svg);
    }
    Ok(svgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    // `cargo` is on every machine that runs these tests, and it is the
    // only program the driver for this crate can count on. It stands in
    // for `npx` here so the exit-status handling is tested without Node;
    // the `likec4_` tests cover the real call.

    #[test]
    fn a_program_that_exits_zero_yields_its_output() {
        let out = run_program("cargo", &["--version".to_string()]).unwrap();
        assert!(out.starts_with("cargo "), "got {out:?}");
    }

    #[test]
    fn a_program_that_exits_non_zero_is_failed_with_its_output_and_command() {
        let args = vec!["no-such-subcommand-xyzzy".to_string()];
        match run_program("cargo", &args) {
            Err(LikeC4Error::Failed {
                command,
                status,
                output,
            }) => {
                assert_eq!(command, "cargo no-such-subcommand-xyzzy");
                assert!(!status.is_empty());
                assert!(
                    output.contains("no-such-subcommand-xyzzy"),
                    "got {output:?}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn a_program_that_does_not_exist_is_not_runnable_and_names_the_command() {
        match run_program("no-such-program-xyzzy", &[]) {
            Err(LikeC4Error::NotRunnable { command, .. }) => {
                assert_eq!(command, "no-such-program-xyzzy ");
            }
            other => panic!("expected NotRunnable, got {other:?}"),
        }
    }

    #[test]
    fn the_npx_program_is_npx_with_the_platform_suffix() {
        let expected = if cfg!(windows) { "npx.cmd" } else { "npx" };
        assert_eq!(npx_program(), expected);
    }

    #[test]
    fn npx_args_pin_the_release_before_the_subcommand() {
        assert_eq!(
            npx_args(&["validate", "x"]),
            ["--yes", "likec4@1.59.3", "validate", "x"]
        );
    }

    #[test]
    fn validate_args_disable_layout_and_end_with_the_directory() {
        assert_eq!(
            validate_args(Path::new("docs/arch")),
            ["validate", "--no-layout", "docs/arch"]
        );
    }

    #[test]
    fn export_json_args_skip_layout_pretty_print_and_name_the_output_before_the_directory() {
        assert_eq!(
            export_json_args(Path::new("docs/arch"), Path::new("out.json")),
            [
                "export",
                "json",
                "--skip-layout",
                "--pretty",
                "-o",
                "out.json",
                "docs/arch"
            ]
        );
    }

    #[test]
    fn clear_renders_drops_dot_and_svg_files_and_leaves_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["old.dot", "old.svg", "keep.txt", ".gitkeep"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        clear_renders(dir.path()).unwrap();

        assert!(!dir.path().join("old.dot").exists());
        assert!(!dir.path().join("old.svg").exists());
        assert!(dir.path().join("keep.txt").exists());
        assert!(dir.path().join(".gitkeep").exists());
        assert!(dir.path().join("sub").is_dir());
    }

    #[test]
    fn clear_renders_on_a_missing_directory_is_nothing_to_do() {
        let dir = tempfile::tempdir().unwrap();
        assert!(clear_renders(&dir.path().join("nowhere")).is_ok());
    }

    #[test]
    fn gen_dot_args_name_the_output_directory_before_the_model_directory() {
        assert_eq!(
            gen_dot_args(Path::new("docs/arch"), Path::new("docs/arch/views")),
            ["gen", "dot", "-o", "docs/arch/views", "docs/arch"]
        );
    }

    #[test]
    fn svg_args_write_beside_the_dot_file_with_the_svg_extension() {
        let (svg, args) = svg_args(Path::new("views/index.dot"));
        assert_eq!(svg, PathBuf::from("views/index.svg"));
        assert_eq!(args, ["-Tsvg", "views/index.dot", "-o", "views/index.svg"]);
    }

    #[test]
    fn normalize_removes_relative_from_every_link_and_nothing_else() {
        let mut value = serde_json::json!({
            "elements": {
                "a": {
                    "title": "a",
                    "links": [
                        { "url": "./x", "relative": "file:///abs/x", "title": "t" },
                        { "url": "./y", "relative": "file:///abs/y" }
                    ]
                }
            },
            "views": [ { "nodes": [ { "links": [ { "url": "u", "relative": "r" } ] } ] } ],
            "relative": "not a link, stays"
        });
        normalize(&mut value);
        assert_eq!(
            value["elements"]["a"]["links"],
            serde_json::json!([{ "url": "./x", "title": "t" }, { "url": "./y" }])
        );
        assert_eq!(
            value["views"][0]["nodes"][0]["links"],
            serde_json::json!([{ "url": "u" }])
        );
        assert_eq!(value["relative"], "not a link, stays");
        assert_eq!(value["elements"]["a"]["title"], "a");
    }

    #[test]
    fn normalize_leaves_scalars_and_link_less_objects_alone() {
        let mut value = serde_json::json!({ "n": 1, "s": "x", "o": { "k": [1, 2] } });
        let before = value.clone();
        normalize(&mut value);
        assert_eq!(value, before);
    }

    #[test]
    fn dot_files_are_the_dot_extension_only_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.dot", "a.svg", "c.dot", "a.dot", "notes.txt"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let names: Vec<String> = dot_files_in(dir.path())
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.dot", "b.dot", "c.dot"]);
    }

    #[test]
    fn gen_dot_on_a_directory_that_cannot_be_created_is_an_io_error() {
        // A path beneath a regular file cannot be created on any
        // platform; `/no/such/...` can, on Windows, under the drive root.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a-file");
        std::fs::write(&file, "").unwrap();
        let out = file.join("dots");
        match gen_dot(Path::new("."), &out) {
            Err(LikeC4Error::Io { path, .. }) => assert_eq!(path, out),
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
