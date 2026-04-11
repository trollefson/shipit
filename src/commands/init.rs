use chrono::Utc;
use owo_colors::OwoColorize;

use crate::cli::InitArgs;
use crate::error::ShipItError;
use crate::output::{prompt_line, prompt_line_with_default, prompt_line_with_env_default};
use crate::settings::Settings;

/// Return the first set environment variable for the platform inferred from `domain`.
///
/// - GitHub (`github`): checks `GITHUB_TOKEN`, then `GH_TOKEN`
/// - GitLab (`gitlab`): checks `GITLAB_TOKEN`, then `GITLAB_PRIVATE_TOKEN`
///
/// Returns `(variable_name, value)` for the first variable that is set and non-empty,
/// or `None` if the domain is unrecognised or no variable is set.
fn token_env_var_for_domain(domain: &str) -> Option<(&'static str, String)> {
    token_env_var_for_domain_with_lookup(domain, |var| std::env::var(var).ok())
}

fn token_env_var_for_domain_with_lookup<F>(domain: &str, lookup: F) -> Option<(&'static str, String)>
where
    F: Fn(&str) -> Option<String>,
{
    token_env_var_candidates(domain).and_then(|candidates| {
        candidates.iter().find_map(|&var| {
            lookup(var).filter(|v| !v.is_empty()).map(|val| (var, val))
        })
    })
}

/// Returns the candidate environment variable names for the given platform domain,
/// or `None` if the domain is unrecognised.
fn token_env_var_candidates(domain: &str) -> Option<&'static [&'static str]> {
    if domain.contains("github") {
        Some(&["GITHUB_TOKEN", "GH_TOKEN"])
    } else if domain.contains("gitlab") {
        Some(&["GITLAB_TOKEN", "GITLAB_PRIVATE_TOKEN"])
    } else {
        None
    }
}

/// Extract the hostname from a git remote URL.
///
/// Handles both SSH (`git@host:owner/repo.git`) and HTTPS (`https://host/owner/repo.git`) formats.
fn domain_from_remote_url(url: &str) -> Option<String> {
    if url.starts_with("git@") {
        // git@host:owner/repo.git
        url.strip_prefix("git@")
            .and_then(|s| s.split(':').next())
            .map(|s| s.to_string())
    } else {
        // https://host/owner/repo.git
        url.split_once("//")
            .and_then(|(_, rest)| rest.split('/').next())
            .map(|s| s.to_string())
    }
}

/// Try to infer the platform domain from the git remote URL.
fn infer_domain(dir: &std::path::Path, remote: &str) -> Option<String> {
    let repo = git2::Repository::open(dir).ok()?;
    let remote_obj = repo.find_remote(remote).ok()?;
    let url = remote_obj.url()?.to_string();
    domain_from_remote_url(&url)
}

pub fn init(args: InitArgs) -> Result<(), ShipItError> {
    let dir = args
        .dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let path = dir.join("shipit.toml");

    if path.exists() {
        eprintln!(
            "{} Config already exists at: {}",
            "!".yellow().bold(),
            path.display().bold()
        );
        return Ok(());
    }

    let mut settings = Settings::default();

    // --- Platform Domain ---
    eprintln!();
    eprintln!("{}", "Platform Domain".bold().cyan());
    eprintln!("  A platform is a remote Git repository service (e.g. GitHub or GitLab).");
    eprintln!("  Only {} and {} are currently supported.", "github.com".bold(), "gitlab.com".bold());

    let remote_name = args.remote.as_deref().unwrap_or("origin");
    let inferred_domain = infer_domain(&dir, remote_name);

    let domain = if let Some(domain) = args.platform_domain {
        domain
    } else if let Some(ref default) = inferred_domain {
        eprintln!();
        prompt_line_with_default("Platform domain:", default)?
    } else {
        eprintln!();
        prompt_line("Platform domain (e.g. github.com):")?
    };

    if !domain.trim().is_empty() {
        settings.platform.domain = domain.trim().to_string();
        crate::output::print_success("Platform domain saved.");
    } else {
        crate::output::print_skipped("Platform domain skipped.");
    }

    // --- Platform Token ---
    eprintln!();
    eprintln!("{}", "Platform Personal Access Token".bold().cyan());

    let env_token = token_env_var_for_domain(domain.trim());

    let token = if let Some(token) = args.platform_token {
        token
    } else if let Some((env_var_name, ref env_var_value)) = env_token {
        eprintln!();
        eprintln!("  Found token in {}.", format!("${env_var_name}").bold());
        prompt_line_with_env_default("Platform token:", env_var_name, env_var_value)?
    } else if let Some(candidates) = token_env_var_candidates(domain.trim()) {
        let vars = candidates
            .iter()
            .map(|v| format!("${v}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ShipItError::Error(format!(
            "No platform token found. Set {} or provide --platform-token.",
            vars
        )));
    } else {
        eprintln!();
        prompt_line("Platform token (leave blank to skip):")?
    };

    if !token.trim().is_empty() {
        settings.platform.token = token.trim().to_string();
        crate::output::print_success("Platform token saved.");
    } else {
        crate::output::print_skipped("Platform token skipped.");
    }

    eprintln!();
    confy::store_path(&path, &settings)
        .map_err(|e| ShipItError::Error(format!("Failed to write config: {}", e)))?;

    let existing = std::fs::read_to_string(&path)
        .map_err(|e| ShipItError::Error(format!("Failed to read config: {}", e)))?;
    let versioned = format!(
        "# Generated by shipit v{} on {}\n{}",
        env!("CARGO_PKG_VERSION"),
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        existing
    );
    std::fs::write(&path, versioned)
        .map_err(|e| ShipItError::Error(format!("Failed to write config: {}", e)))?;

    crate::output::print_success(&format!("Config written to: {}", path.display().bold()));

    let plans_dir = dir.join(".shipit").join("plans");
    std::fs::create_dir_all(&plans_dir)
        .map_err(|e| ShipItError::Error(format!("Failed to create .shipit/plans directory: {}", e)))?;
    crate::output::print_success(&format!("Plans directory created at: {}", plans_dir.display().bold()));

    update_gitignore(&dir)?;

    Ok(())
}

/// Append shipit entries to `.gitignore` if they are not already present.
///
/// Adds `shipit.toml` (contains the platform token) and `.shipit/` (ephemeral
/// plan files) under a clearly marked shipit block so users can easily find and
/// remove them if they prefer to commit these files.
fn update_gitignore(dir: &std::path::Path) -> Result<(), ShipItError> {
    const ENTRIES: &[&str] = &["shipit.toml", ".shipit/"];

    let gitignore_path = dir.join(".gitignore");
    let existing = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path)
            .map_err(|e| ShipItError::Error(format!("Failed to read .gitignore: {}", e)))?
    } else {
        String::new()
    };

    let missing: Vec<&str> = ENTRIES
        .iter()
        .copied()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let block = format!(
        "\n# shipit\n{}\n",
        missing.join("\n")
    );

    let updated = format!("{}{}", existing.trim_end_matches('\n'), block);
    std::fs::write(&gitignore_path, updated)
        .map_err(|e| ShipItError::Error(format!("Failed to write .gitignore: {}", e)))?;

    crate::output::print_success(&format!(
        ".gitignore updated with: {}",
        missing.join(", ").bold()
    ));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{domain_from_remote_url, infer_domain, token_env_var_candidates, token_env_var_for_domain_with_lookup, update_gitignore};
    use tempfile::TempDir;

    #[test]
    fn test_creates_gitignore_when_absent() {
        let dir = TempDir::new().unwrap();
        update_gitignore(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("shipit.toml"));
        assert!(content.contains(".shipit/"));
        assert!(content.contains("# shipit"));
    }

    #[test]
    fn test_appends_to_existing_gitignore() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n.env\n").unwrap();

        update_gitignore(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".env"));
        assert!(content.contains("shipit.toml"));
        assert!(content.contains(".shipit/"));
    }

    #[test]
    fn test_idempotent_when_entries_already_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "# shipit\nshipit.toml\n.shipit/\n").unwrap();

        update_gitignore(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches("shipit.toml").count(), 1, "shipit.toml should not be duplicated");
        assert_eq!(content.matches(".shipit/").count(), 1, ".shipit/ should not be duplicated");
    }

    #[test]
    fn test_only_missing_entries_are_added() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "shipit.toml\n").unwrap();

        update_gitignore(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches("shipit.toml").count(), 1, "shipit.toml should not be duplicated");
        assert!(content.contains(".shipit/"), ".shipit/ should have been added");
    }

    // --- domain_from_remote_url ---

    #[test]
    fn test_domain_from_ssh_url() {
        assert_eq!(
            domain_from_remote_url("git@github.com:owner/repo.git"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_ssh_url_gitlab() {
        assert_eq!(
            domain_from_remote_url("git@gitlab.com:owner/repo.git"),
            Some("gitlab.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_https_url() {
        assert_eq!(
            domain_from_remote_url("https://github.com/owner/repo.git"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_https_url_gitlab() {
        assert_eq!(
            domain_from_remote_url("https://gitlab.com/owner/repo.git"),
            Some("gitlab.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_https_url_no_dotgit() {
        assert_eq!(
            domain_from_remote_url("https://github.com/owner/repo"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_ssh_url_custom_host() {
        assert_eq!(
            domain_from_remote_url("git@git.example.internal:team/project.git"),
            Some("git.example.internal".to_string())
        );
    }

    #[test]
    fn test_domain_from_empty_url() {
        assert_eq!(domain_from_remote_url(""), None);
    }

    // --- infer_domain ---

    #[test]
    fn test_infer_domain_from_https_remote() {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "https://github.com/owner/repo.git").unwrap();

        assert_eq!(
            infer_domain(dir.path(), "origin"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn test_infer_domain_from_ssh_remote() {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "git@gitlab.com:owner/repo.git").unwrap();

        assert_eq!(
            infer_domain(dir.path(), "origin"),
            Some("gitlab.com".to_string())
        );
    }

    #[test]
    fn test_infer_domain_named_remote() {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("upstream", "https://github.com/org/project.git").unwrap();

        assert_eq!(
            infer_domain(dir.path(), "upstream"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn test_infer_domain_missing_remote_returns_none() {
        let dir = TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();

        assert_eq!(infer_domain(dir.path(), "origin"), None);
    }

    #[test]
    fn test_infer_domain_not_a_git_repo_returns_none() {
        let dir = TempDir::new().unwrap();

        assert_eq!(infer_domain(dir.path(), "origin"), None);
    }

    // --- token_env_var_for_domain ---
    //
    // Tests use the injectable `_with_lookup` variant to avoid touching the
    // process environment (which would require `unsafe` in Rust 2024 and could
    // race with parallel tests).

    fn lookup_from<'a>(pairs: &'a [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |var| {
            pairs.iter()
                .find(|(k, _)| *k == var)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn test_token_github_domain_uses_github_token() {
        let result = token_env_var_for_domain_with_lookup(
            "github.com",
            lookup_from(&[("GITHUB_TOKEN", "ghp_test123")]),
        );
        assert_eq!(result, Some(("GITHUB_TOKEN", "ghp_test123".to_string())));
    }

    #[test]
    fn test_token_github_domain_falls_back_to_gh_token() {
        let result = token_env_var_for_domain_with_lookup(
            "github.com",
            lookup_from(&[("GH_TOKEN", "ghp_fallback")]),
        );
        assert_eq!(result, Some(("GH_TOKEN", "ghp_fallback".to_string())));
    }

    #[test]
    fn test_token_github_prefers_github_token_over_gh_token() {
        let result = token_env_var_for_domain_with_lookup(
            "github.com",
            lookup_from(&[("GITHUB_TOKEN", "ghp_primary"), ("GH_TOKEN", "ghp_secondary")]),
        );
        assert_eq!(result, Some(("GITHUB_TOKEN", "ghp_primary".to_string())));
    }

    #[test]
    fn test_token_gitlab_domain_uses_gitlab_token() {
        let result = token_env_var_for_domain_with_lookup(
            "gitlab.com",
            lookup_from(&[("GITLAB_TOKEN", "glpat_test456")]),
        );
        assert_eq!(result, Some(("GITLAB_TOKEN", "glpat_test456".to_string())));
    }

    #[test]
    fn test_token_gitlab_domain_falls_back_to_private_token() {
        let result = token_env_var_for_domain_with_lookup(
            "gitlab.com",
            lookup_from(&[("GITLAB_PRIVATE_TOKEN", "glpat_private")]),
        );
        assert_eq!(result, Some(("GITLAB_PRIVATE_TOKEN", "glpat_private".to_string())));
    }

    #[test]
    fn test_token_unknown_domain_returns_none() {
        let result = token_env_var_for_domain_with_lookup("bitbucket.org", lookup_from(&[]));
        assert_eq!(result, None);
    }

    #[test]
    fn test_token_empty_domain_returns_none() {
        let result = token_env_var_for_domain_with_lookup("", lookup_from(&[]));
        assert_eq!(result, None);
    }

    #[test]
    fn test_token_no_env_var_set_returns_none() {
        let result = token_env_var_for_domain_with_lookup("github.com", lookup_from(&[]));
        assert_eq!(result, None);
    }

    #[test]
    fn test_token_empty_env_var_ignored() {
        let result = token_env_var_for_domain_with_lookup(
            "github.com",
            lookup_from(&[("GITHUB_TOKEN", "")]),
        );
        assert_eq!(result, None);
    }

    // --- token_env_var_candidates ---
    //
    // These tests cover the logic that drives the "no token found" error in
    // `init`: a recognised platform domain should always return candidates so
    // the caller can surface a helpful error message listing which variables to
    // set.

    #[test]
    fn test_candidates_github_returns_expected_vars() {
        let candidates = token_env_var_candidates("github.com").expect("should return candidates for github.com");
        assert_eq!(candidates, &["GITHUB_TOKEN", "GH_TOKEN"]);
    }

    #[test]
    fn test_candidates_gitlab_returns_expected_vars() {
        let candidates = token_env_var_candidates("gitlab.com").expect("should return candidates for gitlab.com");
        assert_eq!(candidates, &["GITLAB_TOKEN", "GITLAB_PRIVATE_TOKEN"]);
    }

    #[test]
    fn test_candidates_unknown_domain_returns_none() {
        assert!(token_env_var_candidates("bitbucket.org").is_none());
    }

    #[test]
    fn test_candidates_empty_domain_returns_none() {
        assert!(token_env_var_candidates("").is_none());
    }

    // Verify that a recognised domain with no env vars set produces `None` from
    // the lookup (triggering the error path in init) while an unknown domain
    // also produces `None` (falling through to the interactive prompt instead).
    // The distinction is made via `token_env_var_candidates`, not the lookup.
    #[test]
    fn test_candidates_distinguishes_recognised_from_unknown_domain() {
        // Both return None from the value lookup when no vars are set …
        assert_eq!(token_env_var_for_domain_with_lookup("github.com", lookup_from(&[])), None);
        assert_eq!(token_env_var_for_domain_with_lookup("bitbucket.org", lookup_from(&[])), None);

        // … but only the recognised domain has candidates, enabling the error path.
        assert!(token_env_var_candidates("github.com").is_some());
        assert!(token_env_var_candidates("bitbucket.org").is_none());
    }
}
