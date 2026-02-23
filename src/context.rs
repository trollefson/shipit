use crate::cli::{Agent, Cli, Commands};
use crate::settings::Settings;

pub struct Context {
    pub settings: Settings,
}

impl Context {
    pub fn from_cli(args: &Cli) -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings: Settings = confy::load("shipit", None)?;
        match &args.command {
            Commands::B2b(args) => {
                if let Some(selected) = &args.agent {
                    settings.shipit.agent = match selected {
                        Agent::Ollama => "ollama".to_string(),
                        Agent::Shipit => "shipit".to_string(),
                    };
                }
                settings.shipit.dryrun = args.dryrun;
            }
            Commands::Config { .. } => {}
        }
        Ok(Self { settings })
    }
}
