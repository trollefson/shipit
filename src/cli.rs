use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    B2b {
        source: String,
        target: String,
        #[arg(long)]
        ai: bool,
        #[arg(long)]
        dryrun: bool,
        #[arg(long)]
        dir: Option<String>,
        #[arg(long, required_unless_present = "dryrun", help = "GitLab/GitHub Project ID")]
        id: Option<u64>,
    },
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    Generate,
    Show,
}
