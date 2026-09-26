//! The `asbuilt` command line: `survey` writes the model, `check` fails
//! when the committed one is stale.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use asbuilt::commands::{self, EXIT_ERROR};

#[derive(Parser)]
#[command(
    name = "asbuilt",
    version = env!("CRATE_VERSION_WITH_BUILD"),
    about = "Keep a LikeC4 architecture model that describes the code as built"
)]
struct Cli {
    /// Read this config instead of `<root>/asbuilt.toml`.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Survey the code base and write the model.
    Survey {
        /// The repository root; the current directory by default.
        root: Option<PathBuf>,
        /// Write here instead of the configured path (relative to the root).
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Survey in memory and exit 1 with a diff if the committed model differs.
    Check {
        /// The repository root; the current directory by default.
        root: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let config = cli.config.as_deref();
    let result = match &cli.command {
        Command::Survey { root, output } => commands::survey(
            root.as_deref().unwrap_or(".".as_ref()),
            config,
            output.as_deref(),
        ),
        Command::Check { root } => commands::check(
            root.as_deref().unwrap_or(".".as_ref()),
            config,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        ),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("asbuilt: {e}");
            std::process::exit(EXIT_ERROR);
        }
    }
}
