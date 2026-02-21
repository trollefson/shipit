mod cli;
mod commands;
mod common;
mod context;
mod error;
mod settings;

use clap::Parser;

use crate::context::Context;
use crate::error::ShipItError;

#[tokio::main]
async fn main() -> Result<(), ShipItError> {
    let args = cli::Cli::parse();

    // Handle commands related to config generation and reading first
    if let cli::Commands::Config { subcommand } = args.command {
        return match subcommand {
            cli::ConfigCommands::Generate => commands::config::generate(),
            cli::ConfigCommands::Show => commands::config::show(),
        };
    }

    let ctx = Context::from_cli(&args).map_err(|_e| ShipItError::Error("Failed to parse CLI context!".to_string()))?;
    match args.command {
        cli::Commands::B2b { source, target, dir, id, .. } => {
            commands::b2b::branch_to_branch(&ctx, source, target, dir, id).await?;
        }
        cli::Commands::Config { .. } => unreachable!(),
    }
    Ok(())
}
