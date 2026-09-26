//! The pinned `npx likec4` calls. Everything downstream of the model is
//! LikeC4's; this module runs it and reports what it said.

use std::path::Path;
use std::process::Command;

use crate::likec4_package;

/// Why a LikeC4 call did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum LikeC4Error {
    /// The program could not be started at all (no Node on this machine).
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

/// The arguments of `likec4 validate` over the `.c4` files under `dir`.
/// LikeC4 merges every file under a path into one model, so validate
/// one model's directory at a time.
pub fn validate_args(dir: &Path) -> Vec<&str> {
    vec!["validate", "--no-layout", dir_str(dir)]
}

fn dir_str(dir: &Path) -> &str {
    dir.to_str().expect("a UTF-8 path to hand to npx")
}

/// `likec4 validate` over `dir`.
pub fn validate(dir: &Path) -> Result<(), LikeC4Error> {
    run(&validate_args(dir)).map(|_| ())
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
}
