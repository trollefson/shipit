use octocrab::OctocrabBuilder;

use crate::error::ShipItError;
use super::common::{extract_repo_path, GitPlatform};

pub(crate) struct Github {
    pub domain: String,
    pub token: String,
    pub owner: String,
    pub repo: String,
}

impl GitPlatform for Github {
    async fn open(&self, source: &str, target: &str, title: &str, description: &str) -> Result<String, ShipItError> {
        let mut builder = OctocrabBuilder::new().personal_token(self.token.clone());

        if self.domain != "github.com" {
            let base_uri = format!("https://{}/api/v3/", self.domain);
            builder = builder.base_uri(base_uri)
                .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
        }

        let octo = builder.build().map_err(|e| ShipItError::Github(e))?;

        let pr = octo
            .pulls(&self.owner, &self.repo)
            .create(title, source, target)
            .body(description)
            .send()
            .await
            .map_err(|e| ShipItError::Github(e))?;

        let url = pr.html_url
            .ok_or_else(|| ShipItError::Error("Failed to get pr url from GitHub response".to_string()))?;

        Ok(url.to_string())
    }
}

/// Creates a GitHub release for the given tag name, targeting the given branch.
pub(crate) async fn create_github_release(
    domain: &str,
    token: &str,
    owner: &str,
    repo_name: &str,
    tag_name: &str,
    branch: &str,
    notes: &str,
) -> Result<String, ShipItError> {
    let mut builder = OctocrabBuilder::new().personal_token(token.to_string());
    if domain != "github.com" {
        let base_uri = format!("https://{}/api/v3/", domain);
        builder = builder
            .base_uri(base_uri)
            .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
    }
    let octo = builder.build().map_err(|e| ShipItError::Github(e))?;

    let release = octo
        .repos(owner, repo_name)
        .releases()
        .create(tag_name)
        .body(notes)
        .target_commitish(branch)
        .send()
        .await
        .map_err(|e| ShipItError::Github(e))?;

    Ok(release.html_url.to_string())
}

/// Returns the GitHub `owner/repo` identifier by parsing it directly from the remote url.
pub(crate) fn lookup_github_identifier(remote_url: &str) -> Result<String, ShipItError> {
    extract_repo_path(remote_url).ok_or_else(|| {
        ShipItError::Error(format!(
            "Failed to parse GitHub owner/repo from remote url: {}",
            remote_url
        ))
    })
}

/// Tries to parse a GitHub PR number from a merge commit message.
///
/// Recognizes:
/// - `"Merge pull request #123 from owner/branch"` (standard GitHub merge commit)
/// - `"feat: something (#123)"` (squash-merge title)
pub(crate) fn parse_github_pr_number(message: &str) -> Option<u64> {
    // Standard merge commit: "Merge pull request #NNN"
    if let Some(idx) = message.find("pull request #") {
        let rest = &message[idx + "pull request #".len()..];
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num_str.is_empty() {
            return num_str.parse().ok();
        }
    }
    // Squash-merge title: "(#NNN)"
    if let Some(idx) = message.find("(#") {
        let rest = &message[idx + 2..];
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num_str.is_empty() {
            if rest.chars().nth(num_str.len()) == Some(')') {
                return num_str.parse().ok();
            }
        }
    }
    None
}

/// Fetches the title and HTML URL of a GitHub pull request.
///
/// Builds an Octocrab client from the supplied credentials, then calls
/// `GET /repos/{owner}/{repo}/pulls/{number}`.
pub(crate) async fn fetch_github_pr_info(
    domain: &str,
    token: &str,
    owner: &str,
    repo_name: &str,
    number: u64,
) -> Result<(String, String), ShipItError> {
    let mut builder = OctocrabBuilder::new().personal_token(token.to_string());
    if domain != "github.com" {
        let base_uri = format!("https://{}/api/v3/", domain);
        builder = builder
            .base_uri(base_uri)
            .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
    }
    let octo = builder.build().map_err(|e| ShipItError::Github(e))?;

    let pr = octo
        .pulls(owner, repo_name)
        .get(number)
        .await
        .map_err(|e| ShipItError::Github(e))?;

    let title = pr
        .title
        .ok_or_else(|| ShipItError::Error("PR missing title".to_string()))?;
    let url = pr
        .html_url
        .ok_or_else(|| ShipItError::Error("PR missing url".to_string()))?;

    Ok((title, url.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ShipItError;

    // ── parse_github_pr_number ────────────────────────────────────────────────

    #[test]
    fn test_parse_github_pr_number_standard_merge_commit() {
        assert_eq!(
            parse_github_pr_number("Merge pull request #123 from owner/feature-branch"),
            Some(123)
        );
    }

    #[test]
    fn test_parse_github_pr_number_squash_merge_title() {
        assert_eq!(
            parse_github_pr_number("feat: add new feature (#456)"),
            Some(456)
        );
    }

    #[test]
    fn test_parse_github_pr_number_no_match() {
        assert_eq!(
            parse_github_pr_number("Merge branch 'main' into feature"),
            None
        );
    }

    #[test]
    fn test_parse_github_pr_number_squash_no_closing_paren() {
        // "(#123" without a closing paren should not match
        assert_eq!(parse_github_pr_number("fix: something (#123 without paren"), None);
    }

    // ── lookup_github_identifier ──────────────────────────────────────────────

    #[test]
    fn test_lookup_github_identifier_success() {
        let result = lookup_github_identifier("git@github.com:owner/repo.git");
        assert_eq!(result.unwrap(), "owner/repo");
    }

    #[test]
    fn test_lookup_github_identifier_returns_error_for_unparseable_url() {
        let result = lookup_github_identifier("");
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }
}
