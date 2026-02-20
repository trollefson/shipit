use crate::cli::{Cli, Commands};
use crate::settings::Settings;

pub struct Context {
    pub settings: Settings,
}

impl Context {
    pub fn from_cli(args: &Cli) -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings: Settings = confy::load("shipit", None)?;
        match &args.command {
            Commands::B2b { ai, dryrun, .. } => {
                settings.shipit.ai = *ai;
                settings.shipit.dryrun = *dryrun;
            }
            Commands::Config { .. } => {}
        }
        Ok(Self { settings })
    }
}
