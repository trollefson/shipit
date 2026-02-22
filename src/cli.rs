use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Platform {
    Github,
    Gitlab,
}

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
        #[arg(long, required_unless_present = "dryrun", help = "GitLab project ID or GitHub 'owner/repo'")]
        id: Option<String>,
        #[arg(long, value_enum, help = "Platform to open the merge/pull request on (overrides auto-detection)")]
        platform: Option<Platform>,
        #[arg(long, default_value = "origin", help = "Name of the git remote to use")]
        remote: String,
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
