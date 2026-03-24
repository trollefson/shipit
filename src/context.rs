use std::path::PathBuf;

use crate::cli::{B2bSubcommand, B2tSubcommand, Cli, Commands};
use crate::settings::Settings;

pub struct Context {
    pub settings: Settings,
}

impl Context {
    pub fn from_cli(args: &Cli) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = match &args.command {
            Some(Commands::B2b(b2b)) => match &b2b.command {
                B2bSubcommand::Plan(a) => a.dir.as_deref().map(PathBuf::from),
                B2bSubcommand::Apply(a) => a.dir.as_deref().map(PathBuf::from),
            },
            Some(Commands::B2t(b2t)) => match &b2t.command {
                B2tSubcommand::Plan(a) => a.dir.as_deref().map(PathBuf::from),
                B2tSubcommand::Apply(a) => a.dir.as_deref().map(PathBuf::from),
            },
            _ => None,
        }
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

        let config_path = dir.join("shipit.toml");
        if !config_path.exists() {
            return Err(format!(
                "No config file found at '{}'. Run `shipit init` to generate one.",
                config_path.display()
            )
            .into());
        }
        let settings: Settings = confy::load_path(&config_path)?;
        Ok(Self { settings })
    }
}
