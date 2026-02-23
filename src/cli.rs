use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Agent {
    Ollama,
    Shipit,
}

#[derive(Args, Debug, Default)]
pub struct B2bArgs {
    pub source: String,
    pub target: String,
    #[arg(
        long,
        value_enum,
        help = "Use an agent to generate the merge/pull request title and description (e.g., ollama, shipit)"
    )]
    pub agent: Option<Agent>,
    #[arg(
        long,
        help = "Print the merge/pull request details without creating it"
    )]
    pub dryrun: bool,
    #[arg(
        long,
        help = "Path to the git repository (defaults to current directory)"
    )]
    pub dir: Option<String>,
    #[arg(
        long,
        help = "GitLab project id or GitHub 'owner/repo' (auto-detected from remote url if not provided)"
    )]
    pub id: Option<String>,
    #[arg(long, default_value = "origin", help = "Name of the git remote to use")]
    pub remote: String,
    #[arg(
        long,
        help = "Prompt prefix to send to Ollama (overrides the config file value)"
    )]
    pub prompt: Option<String>,
    #[arg(
        long,
        help = "Description to use for the merge/pull request (skips commit discovery and ai summary)"
    )]
    pub description: Option<String>,
    #[arg(
        long,
        help = "Only include merge commits in the discovered commits"
    )]
    pub only_merges: bool,
}

#[derive(Args, Debug, Default)]
pub struct B2tArgs {
    pub branch: String,
    pub tag: String,
    #[arg(
        long,
        value_enum,
        help = "Use an agent to generate the tag notes (e.g., ollama, shipit)"
    )]
    pub agent: Option<Agent>,
    #[arg(long, help = "Print the tag notes without creating it")]
    pub dryrun: bool,
    #[arg(
        long,
        help = "Path to the git repository (defaults to current directory)"
    )]
    pub dir: Option<String>,
    #[arg(
        long,
        help = "GitLab project id or GitHub 'owner/repo' (auto-detected from remote url if not provided)"
    )]
    pub id: Option<String>,
    #[arg(long, default_value = "origin", help = "Name of the git remote to use")]
    pub remote: String,
    #[arg(
        long,
        help = "Prompt prefix to send to Ollama (overrides the config file value)"
    )]
    pub prompt: Option<String>,
    #[arg(
        long,
        help = "Description to use for the tag notes (skips commit discovery and ai summary)"
    )]
    pub description: Option<String>,
    #[arg(
        long,
        help = "Only include merge commits in the discovered commits"
    )]
    pub only_merges: bool,
    #[arg(
        long,
        help = "The most recent tag to compare against (defaults to auto-detected most recent tag on the branch)"
    )]
    pub latest_tag: Option<String>,
}

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Open a merge/pull request from a source branch to a target branch
    B2b(B2bArgs),
    /// Create an annotated tag on a branch with formatted release notes
    B2t(B2tArgs),
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
