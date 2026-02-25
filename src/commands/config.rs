use std::io;

use owo_colors::OwoColorize;

use crate::cli::ConfigGenerateArgs;
use crate::error::ShipItError;
use crate::settings::Settings;

fn prompt_token(label: &str) -> Result<String, ShipItError> {
    crate::output::print_token_prompt(label);
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    Ok(input.trim().to_string())
}

/// Write the default config to the platform config directory. This will overwrite existing config.
pub fn generate(args: ConfigGenerateArgs) -> Result<(), ShipItError> {
    let mut settings = Settings::default();

    // --- GitHub ---
    println!();
    println!("{}", "GitHub Personal Access Token".bold().cyan());

    let github_token = if let Some(token) = args.github_token {
        token
    } else {
        crate::output::print_token_scope_label("Required token scopes (classic token)");
        crate::output::print_token_scope_item("repo", "(push branches, create tags, releases, and pull requests)");
        crate::output::print_token_scope_label("Required permissions (fine-grained token)");
        crate::output::print_token_scope_item("Contents: Read and write", "(push branches, create tags and releases)");
        crate::output::print_token_scope_item("Pull requests: Read and write", "(create pull requests)");
        println!();
        prompt_token("GitHub token (leave blank to skip):")?
    };

    if !github_token.trim().is_empty() {
        settings.github.token = Some(github_token.trim().to_string());
        crate::output::print_success("GitHub token saved.");
    } else {
        crate::output::print_skipped("GitHub token skipped.");
    }

    if let Some(domain) = args.github_domain {
        settings.github.domain = domain;
        crate::output::print_success("GitHub domain saved.");
    }

    // --- GitLab ---
    println!();
    println!("{}", "GitLab Personal Access Token".bold().cyan());

    let gitlab_token = if let Some(token) = args.gitlab_token {
        token
    } else {
        crate::output::print_token_scope_label("Required token scopes");
        crate::output::print_token_scope_item("api", "(create / update merge requests, tags and releases)");
        crate::output::print_token_scope_item("write_repository", "(push branches)");
        println!();
        prompt_token("GitLab token (leave blank to skip):")?
    };

    if !gitlab_token.trim().is_empty() {
        settings.gitlab.token = Some(gitlab_token.trim().to_string());
        crate::output::print_success("GitLab token saved.");
    } else {
        crate::output::print_skipped("GitLab token skipped.");
    }

    if let Some(domain) = args.gitlab_domain {
        settings.gitlab.domain = domain;
        crate::output::print_success("GitLab domain saved.");
    }

    println!();
    confy::store("shipit", None, &settings)
        .map_err(|e| ShipItError::Error(format!("Failed to write config: {}", e)))?;

    let path = confy::get_configuration_file_path("shipit", None)
        .map_err(|e| ShipItError::Error(format!("Failed to resolve config path: {}", e)))?;

    crate::output::print_success(&format!("Config written to: {}", path.display().bold()));
    Ok(())
}

/// Load and pretty-print the current config as TOML.
pub fn show() -> Result<(), ShipItError> {
    let settings: Settings = confy::load("shipit", None)
        .map_err(|e| ShipItError::Error(format!("Failed to load config: {}", e)))?;

    let path = confy::get_configuration_file_path("shipit", None)
        .map_err(|e| ShipItError::Error(format!("Failed to resolve config path: {}", e)))?;

    let toml_str = toml::to_string_pretty(&settings)
        .map_err(|e| ShipItError::Error(format!("Failed to serialize config: {}", e)))?;

    println!("# {}\n\n{}", path.display(), toml_str);
    Ok(())
}
