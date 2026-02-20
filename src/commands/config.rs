use crate::error::ShipItError;
use crate::settings::Settings;

/// Write the default config to the platform config directory. This will overwrite existing config.
pub fn generate() -> Result<(), ShipItError> {
    let default_settings = Settings::default();
    confy::store("shipit", None, &default_settings)
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
