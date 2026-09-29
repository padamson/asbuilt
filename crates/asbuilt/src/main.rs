//! The `asbuilt` command line: `survey` writes the model, `check` fails
//! when the committed one is stale, `docs` writes the documentation
//! tree, and `validate`, `export json` and `render` hand the model to
//! the pinned LikeC4.

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
    /// Run `likec4 validate` over the model directory (needs Node).
    Validate {
        /// The repository root; the current directory by default.
        root: Option<PathBuf>,
    },
    /// Export the model through LikeC4 (needs Node).
    Export {
        #[command(subcommand)]
        format: ExportFormat,
    },
    /// Write the documentation tree: pages from a fresh survey, diagrams
    /// from `render` (needs Node and Graphviz unless `--no-render`).
    Docs {
        /// The repository root; the current directory by default.
        root: Option<PathBuf>,
        /// Write here instead of `<model dir>/site` (relative to the root).
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,
        /// Reuse the SVGs under `<model dir>/views` instead of rendering.
        #[arg(long)]
        no_render: bool,
        /// The site title; `[docs] title`, else the root directory's name.
        #[arg(long, value_name = "TEXT")]
        title: Option<String>,
        /// A URL prefix that turns each path into a link; `[docs] source_url` by default.
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,
        /// A link back to the hosting site on every page; `[docs] home_url` by default.
        #[arg(long, value_name = "URL")]
        home_url: Option<String>,
        /// The text of that link; `[docs] home_title`, else the URL.
        #[arg(long, value_name = "TEXT")]
        home_title: Option<String>,
        /// A stylesheet linked last on every page; `[docs] stylesheet` by default.
        #[arg(long, value_name = "URL")]
        stylesheet: Option<String>,
        /// Replace pages, stylesheets or SVGs in the output directory that no earlier run wrote.
        #[arg(long)]
        force: bool,
    },
    /// Render every view to an SVG (needs Node and Graphviz `dot`).
    Render {
        /// The repository root; the current directory by default.
        root: Option<PathBuf>,
        /// Write here instead of `<model dir>/views` (relative to the root).
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ExportFormat {
    /// The computed model as JSON, with the machine-specific link fields removed.
    Json {
        /// The repository root; the current directory by default.
        root: Option<PathBuf>,
        /// Write here instead of `<model dir>/model.json` (relative to the root).
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
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
        Command::Validate { root } => commands::validate(
            root.as_deref().unwrap_or(".".as_ref()),
            config,
            &mut std::io::stderr().lock(),
        ),
        Command::Export {
            format: ExportFormat::Json { root, output },
        } => commands::export_json(
            root.as_deref().unwrap_or(".".as_ref()),
            config,
            output.as_deref(),
            &mut std::io::stderr().lock(),
        ),
        Command::Docs {
            root,
            output,
            no_render,
            title,
            source_url,
            home_url,
            home_title,
            stylesheet,
            force,
        } => commands::docs(
            root.as_deref().unwrap_or(".".as_ref()),
            &commands::DocsArgs {
                config_path: config,
                output: output.as_deref(),
                no_render: *no_render,
                force: *force,
                title: title.as_deref(),
                source_url: source_url.as_deref(),
                home_url: home_url.as_deref(),
                home_title: home_title.as_deref(),
                stylesheet: stylesheet.as_deref(),
            },
            &mut std::io::stderr().lock(),
        ),
        Command::Render { root, output } => commands::render(
            root.as_deref().unwrap_or(".".as_ref()),
            config,
            output.as_deref(),
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
