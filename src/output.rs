use owo_colors::OwoColorize;

use crate::error::ShipItError;

/// Serialize `plan` to YAML and inject a `plan_file` key containing `filename`.
///
/// This is the value emitted to stdout when `--yaml` is passed. The `plan_file`
/// field is not present in the plan written to disk — it exists only in this
/// stdout form so agents can extract the filename for use with `apply` without
/// resorting to filesystem globbing.
pub fn build_plan_yaml<T: serde::Serialize>(plan: &T, filename: &str) -> Result<String, ShipItError> {
    let mut val = serde_yaml::to_value(plan)
        .map_err(|e| ShipItError::Error(format!("Failed to serialize plan: {}", e)))?;
    if let serde_yaml::Value::Mapping(ref mut map) = val {
        map.insert(
            serde_yaml::Value::String("plan_file".into()),
            serde_yaml::Value::String(filename.to_string()),
        );
    }
    serde_yaml::to_string(&val)
        .map_err(|e| ShipItError::Error(format!("Failed to serialize plan: {}", e)))
}

/// Print the AI-generated content (PR description, tag notes) with a styled header.
pub fn print_content(header: &str, body: &str) {
    eprintln!("{}\n\n{}", header.bold(), body);
}

/// Print a final resource URL after a successful remote action.
pub fn print_url(url: &str) {
    println!("\n\nAvailable at:\n\n  {}", url.cyan().underline());
}

/// Print the path to a saved plan file.
pub fn print_plan_saved(path: &std::path::Path) {
    eprintln!("{} Plan saved to: {}", "✓".green().bold(), path.display().bold());
}

/// Print a success confirmation line with a green checkmark.
pub fn print_success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

/// Print a dimmed notice for a skipped or no-op action.
pub fn print_skipped(msg: &str) {
    println!("{}", msg.dimmed());
}

/// Print an interactive token prompt (no newline) and flush stderr.
pub fn print_token_prompt(label: &str) {
    use std::io::Write;
    eprint!("  {} ", label.bold());
    std::io::stderr().flush().ok();
}

/// Print a merge request title prompt with a default suggestion, flush stdout, and read
/// the response. Returns the entered string, or the suggestion if the user presses Enter.
pub fn prompt_mr_title(suggested: &str) -> std::io::Result<String> {
    use std::io::Write;
    eprint!("\n\nMerge request title [{}]: ", suggested);
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    Ok(if trimmed.is_empty() {
        suggested.to_string()
    } else {
        trimmed
    })
}

/// Prompt the user to push the local branch to the remote. Returns `true` if the user
/// confirmed with `y` or `Y`, `false` otherwise.
pub fn prompt_push(branch: &str) -> std::io::Result<bool> {
    use std::io::Write;
    eprint!("\nLocal branch '{}' is ahead of remote. Push now? [y/N]: ", branch);
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Prompt the user to pull from the remote into the local branch. Returns `true` if
/// the user confirmed with `y` or `Y`, `false` otherwise.
pub fn prompt_pull(branch: &str) -> std::io::Result<bool> {
    use std::io::Write;
    eprint!("\nRemote branch '{}' is ahead of local. Pull now? [y/N]: ", branch);
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Print a tag name prompt with an optional suggestion, flush stdout, and read the
/// response. Returns the entered string, or the suggestion if the user presses Enter.
pub fn prompt_tag_name(suggested: Option<&str>) -> std::io::Result<String> {
    use std::io::Write;
    match suggested {
        Some(s) => eprint!("\n\nTag name [{}]: ", s),
        None => eprint!("\n\nTag name: "),
    }
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    Ok(if trimmed.is_empty() {
        suggested.unwrap_or("").to_string()
    } else {
        trimmed
    })
}

#[cfg(test)]
mod tests {
    use super::build_plan_yaml;
    use serde::Serialize;

    #[derive(Serialize)]
    struct DummyPlan {
        title: String,
        commits: Vec<String>,
    }

    fn dummy_plan() -> DummyPlan {
        DummyPlan {
            title: "Release v1.0.0".to_string(),
            commits: vec![
                "feat: add payments abc123".to_string(),
                "fix: handle timeout def456".to_string(),
            ],
        }
    }

    #[test]
    fn test_plan_file_field_is_injected() {
        let yaml = build_plan_yaml(&dummy_plan(), "abc123.yml").unwrap();
        assert!(yaml.contains("plan_file: abc123.yml"));
    }

    #[test]
    fn test_plan_fields_are_present() {
        let yaml = build_plan_yaml(&dummy_plan(), "abc123.yml").unwrap();
        assert!(yaml.contains("title: Release v1.0.0"));
        assert!(yaml.contains("feat: add payments abc123"));
        assert!(yaml.contains("fix: handle timeout def456"));
    }

    #[test]
    fn test_output_is_valid_yaml() {
        let yaml = build_plan_yaml(&dummy_plan(), "abc123.yml").unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml)
            .expect("output should be valid YAML");
        assert_eq!(parsed["plan_file"].as_str().unwrap(), "abc123.yml");
        assert_eq!(parsed["title"].as_str().unwrap(), "Release v1.0.0");
    }

    #[test]
    fn test_commits_accessible_by_index() {
        let yaml = build_plan_yaml(&dummy_plan(), "abc123.yml").unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let commits = parsed["commits"].as_sequence().unwrap();

        // newest-first: index 0 is last commit, index -1 (last) is first commit
        let last_sha = commits[0].as_str().unwrap().split_whitespace().last().unwrap();
        let first_sha = commits[commits.len() - 1].as_str().unwrap().split_whitespace().last().unwrap();

        assert_eq!(last_sha, "abc123");
        assert_eq!(first_sha, "def456");
    }

    #[test]
    fn test_plan_file_not_duplicated_in_other_fields() {
        let yaml = build_plan_yaml(&dummy_plan(), "abc123.yml").unwrap();
        assert_eq!(yaml.matches("plan_file").count(), 1);
    }
}
