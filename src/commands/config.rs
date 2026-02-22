use std::io::{self, Write};

use crate::error::ShipItError;
use crate::settings::Settings;

fn prompt_token(msg: &str) -> Result<String, ShipItError> {
    print!("{}", msg);
    io::stdout()
        .flush()
        .map_err(|e| ShipItError::Error(format!("Failed to flush stdout: {}", e)))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    Ok(input.trim().to_string())
}

/// Write the default config to the platform config directory. This will overwrite existing config.
pub fn generate() -> Result<(), ShipItError> {
    let mut settings = Settings::default();

    println!();
    println!("GitHub Personal Access Token (optional)");
    println!("  Providing a token now is optional, but one will be required to open pull");
    println!("  requests or push branches to GitHub.");
    println!("  Required token scopes (classic token):");
    println!("    - repo       (push branches and create pull requests)");
    println!("  Required permissions (fine-grained token):");
    println!("    - Contents: Read and write  (push branches)");
    println!("    - Pull requests: Read and write  (create pull requests)");
    println!();

    let github_token = prompt_token("  GitHub token (leave blank to skip): ")?;

    if !github_token.trim().is_empty() {
        settings.github.token = Some(github_token.trim().to_string());
        println!("  GitHub token saved.");
    } else {
        println!("  GitHub token skipped.");
    }

    println!();
    println!("GitLab Personal Access Token (optional)");
    println!("  Providing a token now is optional, but one will be required to open merge");
    println!("  requests or push branches to GitLab.");
    println!("  Required token scopes:");
    println!("    - api        (create / update merge requests)");
    println!("    - write_repository  (push branches)");
    println!();

    let gitlab_token = prompt_token("  GitLab token (leave blank to skip): ")?;

    if !gitlab_token.trim().is_empty() {
        settings.gitlab.token = Some(gitlab_token.trim().to_string());
        println!("  GitLab token saved.");
    } else {
        println!("  GitLab token skipped.");
    }

    println!();
    confy::store("shipit", None, &settings)
        .map_err(|e| ShipItError::Error(format!("Failed to write config: {}", e)))?;

    let path = confy::get_configuration_file_path("shipit", None)
        .map_err(|e| ShipItError::Error(format!("Failed to resolve config path: {}", e)))?;

    println!("Config written to: {}", path.display());
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
