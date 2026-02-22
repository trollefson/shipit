use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Platform {
    Github,
    Gitlab,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Agent {
    Ollama,
}

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Open a merge/pull request from a source branch to a target branch
    B2b {
        source: String,
        target: String,
        #[arg(
            long,
            value_enum,
            help = "Use ai to generate the merge/pull request title and description (e.g., ollama)"
        )]
        ai: Option<Agent>,
        #[arg(
            long,
            help = "Print the merge/pull request details without creating it"
        )]
        dryrun: bool,
        #[arg(
            long,
            help = "Path to the git repository (defaults to current directory)"
        )]
        dir: Option<String>,
        #[arg(
            long,
            help = "GitLab project id or GitHub 'owner/repo' (auto-detected from remote url if not provided)"
        )]
        id: Option<String>,
        #[arg(
            long,
            value_enum,
            help = "Platform to open the merge/pull request on (overrides auto-detection)"
        )]
        platform: Option<Platform>,
        #[arg(long, default_value = "origin", help = "Name of the git remote to use")]
        remote: String,
        #[arg(
            long,
            help = "Prompt prefix to send to Ollama (overrides the config file value)"
        )]
        prompt: Option<String>,
        #[arg(
            long,
            help = "Description to use for the merge/pull request (skips commit discovery and ai summary)"
        )]
        description: Option<String>,
    },
    /// Manage shipit configuration
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Write the default config to the platform config directory (overwrites existing config)
    Generate,
    /// Print the current config and its file path
    Show,
}
