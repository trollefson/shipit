mod cli;
mod commands;
mod common;
mod context;
mod error;
mod output;
mod settings;

use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

use crate::cli::Cli;
use crate::context::Context;
use crate::error::ShipItError;

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    fmt()
        .with_env_filter(EnvFilter::new(level))
        .with_target(false)
        .with_ansi(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), ShipItError> {
    let args = cli::Cli::parse();

    if args.markdown_help {
        clap_markdown::print_help_markdown::<Cli>();
    }
    init_tracing(args.verbose);

    // Handle commands related to config generation and reading first
    if let Some(cli::Commands::Config { subcommand }) = args.command {
        return match subcommand {
            cli::ConfigCommands::Generate => commands::config::generate(),
            cli::ConfigCommands::Show => commands::config::show(),
        };
    }

    let ctx = Context::from_cli(&args).map_err(|_e| ShipItError::Error("Failed to parse CLI context!".to_string()))?;
    match args.command {
        Some(cli::Commands::B2b(args)) => {
            commands::b2b::branch_to_branch(&ctx, args).await?;
        }
        Some(cli::Commands::B2t(args)) => {
            commands::b2t::branch_to_tag(&ctx, args).await?;
        }
        Some(cli::Commands::T2r(args)) => {
            commands::t2r::tag_to_release(&ctx, args).await?;
        }
        Some(cli::Commands::Config { .. }) => unreachable!(),
        None => {}
    }
    Ok(())
}
