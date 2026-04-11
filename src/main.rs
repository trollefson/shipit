mod cli;
mod commands;
mod context;
mod error;
mod git;
mod output;
mod plan;
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

    if let Some(cli::Commands::Init(args)) = args.command {
        return commands::init::init(args);
    }

    if let Some(cli::Commands::Claude(args)) = args.command {
        return commands::claude::claude(args);
    }

    let ctx = Context::from_cli(&args).map_err(|e| ShipItError::Error(e.to_string()))?;
    match args.command {
        Some(cli::Commands::B2b(b2b)) => match b2b.command {
            cli::B2bSubcommand::Plan(args) => commands::b2b::plan(&ctx, args).await?,
            cli::B2bSubcommand::Apply(args) => commands::b2b::apply(&ctx, args).await?,
        },
        Some(cli::Commands::B2t(b2t)) => match b2t.command {
            cli::B2tSubcommand::Plan(args) => commands::b2t::plan(&ctx, args).await?,
            cli::B2tSubcommand::Apply(args) => commands::b2t::apply(&ctx, args).await?,
        },
        Some(cli::Commands::Init(_)) | Some(cli::Commands::Claude(_)) => unreachable!(),
        None => {}
    }
    Ok(())
}
