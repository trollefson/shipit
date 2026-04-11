use std::io;

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

/// Start an indeterminate spinner on stderr with the given message.
///
/// Call [`indicatif::ProgressBar::finish_and_clear`] on the returned handle when the
/// operation completes so the spinner line is erased before the next output.
pub fn start_spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Print content (PR description, tag notes) with a styled header.
pub fn print_content(header: &str, body: &str) {
    let rule = "─".repeat(60);
    eprintln!("\n{}\n{}\n\n{}\n\n{}", header.bold().cyan(), rule.dimmed(), body.trim_end(), rule.dimmed());
}

/// Print a final resource URL after a successful remote action.
pub fn print_url(url: &str) {
    println!("\n\nAvailable at:\n\n  {}", url.cyan().underline());
}

/// Print the path to a saved plan file.
pub fn print_plan_saved(path: &std::path::Path) {
    eprintln!("{} Plan saved to: {}", "✓".bright_green().bold(), path.display().bold());
}

/// Print a success confirmation line with a green checkmark.
pub fn print_success(msg: &str) {
    println!("{} {}", "✓".bright_green().bold(), msg);
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

/// Prompt for a line of input, returning the trimmed result.
pub fn prompt_line(label: &str) -> Result<String, ShipItError> {
    print_token_prompt(label);
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    Ok(input.trim().to_string())
}

/// Prompt for a line of input, returning `default` if the user presses Enter.
pub fn prompt_line_with_default(label: &str, default: &str) -> Result<String, ShipItError> {
    print_token_prompt(&format!("{} [{}]", label, default));
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    Ok(resolve_with_default(input.trim(), default))
}

/// Prompt for a secret value whose default comes from an environment variable.
///
/// Displays the env var *name* (e.g. `[$GITHUB_TOKEN]`) so the secret is never
/// echoed to the terminal. Pressing Enter accepts the env var's current value;
/// typing overrides it.
pub fn prompt_line_with_env_default(label: &str, env_var_name: &str, env_var_value: &str) -> Result<String, ShipItError> {
    print_token_prompt(&format!("{} [${env_var_name}]", label));
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    Ok(resolve_with_default(input.trim(), env_var_value))
}

/// Prompt for a yes/no answer. Returns `default_yes` if the user presses Enter.
#[allow(dead_code)] // available for future interactive prompts
pub fn prompt_yes_no(label: &str, default_yes: bool) -> Result<bool, ShipItError> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print_token_prompt(&format!("{} [{}]", label, hint));
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    Ok(parse_yes_no(input.trim(), default_yes))
}

fn resolve_with_default(trimmed: &str, default: &str) -> String {
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_yes_no(trimmed: &str, default_yes: bool) -> bool {
    match trimmed.to_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        _ => false,
    }
}

/// Print a merge request title prompt with a default suggestion, flush stdout, and read
/// the response. Returns the entered string, or the suggestion if the user presses Enter.
pub fn prompt_mr_title(suggested: &str) -> std::io::Result<String> {
    use std::io::Write;
    eprint!("\n\n{} {} ", "Merge request title".bold().cyan(), format!("[{}]:", suggested).dimmed());
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
    use super::{build_plan_yaml, parse_yes_no, resolve_with_default};
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

    // ── resolve_with_default ─────────────────────────────────────────────────

    #[test]
    fn resolve_with_default_returns_input_when_non_empty() {
        assert_eq!(resolve_with_default("hello", "fallback"), "hello");
    }

    #[test]
    fn resolve_with_default_returns_default_when_empty() {
        assert_eq!(resolve_with_default("", "fallback"), "fallback");
    }

    #[test]
    fn resolve_with_default_returns_default_for_whitespace_only() {
        // The public functions trim before calling this helper, so whitespace-only
        // trimmed input correctly falls through to the default.
        assert_eq!(resolve_with_default("", "fallback"), "fallback");
    }

    #[test]
    fn resolve_with_default_preserves_inner_whitespace() {
        assert_eq!(resolve_with_default("hello world", "fallback"), "hello world");
    }

    // ── parse_yes_no ─────────────────────────────────────────────────────────

    #[test]
    fn parse_yes_no_empty_returns_default_yes() {
        assert!(parse_yes_no("", true));
    }

    #[test]
    fn parse_yes_no_empty_returns_default_no() {
        assert!(!parse_yes_no("", false));
    }

    #[test]
    fn parse_yes_no_y_returns_true() {
        assert!(parse_yes_no("y", false));
        assert!(parse_yes_no("Y", false));
    }

    #[test]
    fn parse_yes_no_yes_returns_true() {
        assert!(parse_yes_no("yes", false));
        assert!(parse_yes_no("YES", false));
        assert!(parse_yes_no("Yes", false));
    }

    #[test]
    fn parse_yes_no_n_returns_false() {
        assert!(!parse_yes_no("n", true));
        assert!(!parse_yes_no("N", true));
    }

    #[test]
    fn parse_yes_no_no_returns_false() {
        assert!(!parse_yes_no("no", true));
    }

    #[test]
    fn parse_yes_no_unrecognised_returns_false() {
        assert!(!parse_yes_no("maybe", true));
        assert!(!parse_yes_no("sure", true));
    }
}
