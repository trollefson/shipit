use owo_colors::OwoColorize;

use crate::cli::ClaudeArgs;
use crate::error::ShipItError;

const SKILL_MD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.claude/skills/shipit/SKILL.md"));

/// Install the shipit Claude Code skill to `~/.claude/skills/shipit/SKILL.md`.
///
/// Creates the directory if it does not already exist. If a file is already
/// present it is overwritten so that re-running after a shipit upgrade refreshes
/// the embedded skill content.
pub fn write_skill() -> Result<std::path::PathBuf, ShipItError> {
    let home = std::env::var("HOME")
        .map_err(|_| ShipItError::Error("Cannot determine home directory: $HOME is not set".to_string()))?;

    let skill_dir = std::path::PathBuf::from(home)
        .join(".claude")
        .join("skills")
        .join("shipit");

    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| ShipItError::Error(format!("Failed to create skill directory: {}", e)))?;

    let skill_path = skill_dir.join("SKILL.md");
    std::fs::write(&skill_path, SKILL_MD)
        .map_err(|e| ShipItError::Error(format!("Failed to write SKILL.md: {}", e)))?;

    Ok(skill_path)
}

pub fn claude(_args: ClaudeArgs) -> Result<(), ShipItError> {
    let skill_path = write_skill()?;
    crate::output::print_success(&format!(
        "Claude Code skill installed to: {}",
        skill_path.display().bold()
    ));
    Ok(())
}
