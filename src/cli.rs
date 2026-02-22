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
        #[arg(long, help = "Use AI to generate the merge/pull request title and description")]
        ai: bool,
        #[arg(long, help = "Print the merge/pull request details without creating it")]
        dryrun: bool,
        #[arg(long, help = "Path to the git repository (defaults to current directory)")]
        dir: Option<String>,
        #[arg(long, help = "GitLab project ID or GitHub 'owner/repo' (auto-detected from remote URL if not provided)")]
        id: Option<String>,
        #[arg(long, value_enum, help = "Platform to open the merge/pull request on (overrides auto-detection)")]
        platform: Option<Platform>,
        #[arg(long, default_value = "origin", help = "Name of the git remote to use")]
        remote: String,
        #[arg(long, help = "Prompt prefix to send to Ollama (overrides the config file value)")]
        prompt: Option<String>,
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
