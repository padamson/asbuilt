//! The `asbuilt` command line. Subcommands arrive with the survey.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "asbuilt",
    version = env!("CRATE_VERSION_WITH_BUILD"),
    about = "Keep a LikeC4 architecture model that describes the code as built"
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
