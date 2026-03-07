use clap::{Parser, Subcommand};

mod com_test;

#[derive(Parser)]
#[command(name = "rig-test", about = "OmniRig diagnostic tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test OmniRig COM server connectivity
    ComTest {
        /// Show detailed output
        #[arg(long)]
        verbose: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::ComTest { verbose } => com_test::run(verbose),
    }
}
