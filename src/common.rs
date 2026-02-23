use std::collections::HashMap;

use gitlab::api::{projects, AsyncQuery};
use gitlab::Gitlab as GitlabClient;
use octocrab::OctocrabBuilder;
use reqwest::Client;
use serde_json::json;

use crate::error::ShipItError;
use crate::settings::OllamaSettings;

pub(crate) trait Agent {
    async fn send_prompt(&self, text: &str) -> Result<String, ShipItError>;
}

pub(crate) struct OllamaAgent {
    settings: OllamaSettings,
}

impl OllamaAgent {
    pub(crate) fn new(settings: OllamaSettings) -> Self {
        Self { settings }
    }
}

impl Agent for OllamaAgent {
    async fn send_prompt(&self, text: &str) -> Result<String, ShipItError> {
        let client = Client::new();

        let prompt = format!("{}\n\n{}", self.settings.prompt, text);
        let url = format!(
            "http://{}:{}{}",
            self.settings.domain, self.settings.port, self.settings.endpoint
        );

        let response = client
            .post(&url)
            .json(&json!({
                "model": self.settings.model,
                "prompt": prompt,
                "stream": false,
                "options": self.settings.options,
            }))
            .send()
            .await
            .map_err(|e| ShipItError::Http(e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| ShipItError::Http(e))?;

        let summary = response["response"]
            .as_str()
            .ok_or_else(|| ShipItError::Error("Failed to parse Ollama response!".to_string()))?;

        Ok(summary.to_string())
    }
}

pub(crate) async fn summarize_with_agent<A: Agent>(
    text: &str,
    agent: &A,
) -> Result<String, ShipItError> {
    agent.send_prompt(text).await
}

/// Extracts the repository path from a git remote url.
pub(crate) fn extract_repo_path(url: &str) -> Option<String> { let path = if url.starts_with("git@") {
        // SSH: git@host:path.git
        url.split(':').nth(1)?
    } else {
        // HTTPS: https://host/path.git  or  ssh://git@host/path.git
        let without_scheme = url.splitn(2, "//").nth(1)?;
        without_scheme.splitn(2, '/').nth(1)?
    };
    let path = path.trim_end_matches(".git");
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
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

/// Parses the project path from the remote url and queries the GitLab API
/// to resolve the numeric project id.
pub(crate) async fn lookup_gitlab_project_id(
    remote_url: &str,
    domain: &str,
    token: &str,
) -> Result<u64, ShipItError> {
    let path = extract_repo_path(remote_url).ok_or_else(|| {
        ShipItError::Error(format!(
            "Failed to parse GitLab project path from remote url: {}",
            remote_url
        ))
    })?;

    let client = GitlabClient::builder(domain, token)
        .build_async()
        .await
        .map_err(|e| ShipItError::Gitlab(e))?;

    let endpoint = projects::Project::builder()
        .project(path.as_str())
        .build()
        .map_err(|_| ShipItError::Error("Failed to build GitLab project query".to_string()))?;

    let project: serde_json::Value = endpoint
        .query_async(&client)
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to look up GitLab project '{}': {}", path, e)))?;

    project["id"]
        .as_u64()
        .ok_or_else(|| ShipItError::Error("GitLab project response missing 'id' field".to_string()))
}


pub(crate) trait GitPlatform {
    async fn open(&self, source: &str, target: &str, description: &str) -> Result<String, ShipItError>;
}

pub(crate) struct Github {
    pub domain: String,
    pub token: String,
    pub owner: String,
    pub repo: String,
}

impl GitPlatform for Github {
    async fn open(&self, source: &str, target: &str, description: &str) -> Result<String, ShipItError> {
        let mut builder = OctocrabBuilder::new().personal_token(self.token.clone());

        if self.domain != "github.com" {
            let base_uri = format!("https://{}/api/v3/", self.domain);
            builder = builder.base_uri(base_uri)
                .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
        }

        let octo = builder.build().map_err(|e| ShipItError::Github(e))?;

        let pr = octo
            .pulls(&self.owner, &self.repo)
            .create(format!("{} to {}", source, target), source, target)
            .body(description)
            .send()
            .await
            .map_err(|e| ShipItError::Github(e))?;

        let url = pr.html_url
            .ok_or_else(|| ShipItError::Error("Failed to get pr url from GitHub response".to_string()))?;

        Ok(url.to_string())
    }
}

pub(crate) struct Gitlab {
    pub domain: String,
    pub token: String,
    pub project_id: u64,
}

impl GitPlatform for Gitlab {
    async fn open(&self, source: &str, target: &str, description: &str) -> Result<String, ShipItError> {
        let client = GitlabClient::builder(&self.domain, &self.token)
            .build_async()
            .await
            .map_err(|e| ShipItError::Gitlab(e))?;

        let create_mr = projects::merge_requests::CreateMergeRequest::builder()
            .project(self.project_id)
            .source_branch(source)
            .target_branch(target)
            .title(format!("{} to {}", source, target))
            .description(description)
            .remove_source_branch(true)
            .build()
            .map_err(|e| ShipItError::Error(format!("Failed to build a Gitlab mr: {}", e)))?;

        let merge_request: serde_json::Value = create_mr
            .query_async(&client)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to create a Gitlab merge request: {}", e)))?;

        merge_request["web_url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ShipItError::Error("Failed to get mr url from GitLab response".to_string()))
    }
}

pub(crate) async fn open_merge_request<P: GitPlatform>(
    platform: &P,
    source: &str,
    target: &str,
    description: &str,
) -> Result<String, ShipItError> {
    platform.open(source, target, description).await
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

/// Tries to parse a GitLab MR IID from a merge commit message.
///
/// Recognizes `"See merge request group/project!123"` which GitLab appends to
/// the body of every merge commit.
pub(crate) fn parse_gitlab_mr_iid(message: &str) -> Option<u64> {
    if let Some(idx) = message.find("See merge request ") {
        let rest = &message[idx + "See merge request ".len()..];
        if let Some(bang_idx) = rest.find('!') {
            let after_bang = &rest[bang_idx + 1..];
            let num_str: String = after_bang
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !num_str.is_empty() {
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

/// Fetches the title and web URL of a GitLab merge request by IID.
///
/// Builds a GitLab async client from the supplied credentials, then calls
/// `GET /projects/{project}/merge_requests/{iid}`.
pub(crate) async fn fetch_gitlab_mr_info(
    domain: &str,
    token: &str,
    project_path: &str,
    iid: u64,
) -> Result<(String, String), ShipItError> {
    use gitlab::api::projects::merge_requests::MergeRequest;

    let client = GitlabClient::builder(domain, token)
        .build_async()
        .await
        .map_err(|e| ShipItError::Gitlab(e))?;

    let endpoint = MergeRequest::builder()
        .project(project_path)
        .merge_request(iid)
        .build()
        .map_err(|_| ShipItError::Error("Failed to build GitLab MR query".to_string()))?;

    let mr: serde_json::Value = endpoint
        .query_async(&client)
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to fetch GitLab MR: {}", e)))?;

    let title = mr["title"]
        .as_str()
        .ok_or_else(|| ShipItError::Error("GitLab MR missing title".to_string()))?
        .to_string();
    let url = mr["web_url"]
        .as_str()
        .ok_or_else(|| ShipItError::Error("GitLab MR missing url".to_string()))?
        .to_string();

    Ok((title, url))
}

/// Formats a map of categorized commits into a markdown string.
/// Each key becomes a `##` subheading with its commits listed as bullet points.
/// Keys are sorted alphabetically and empty categories are omitted.
pub(crate) fn format_categorized_commits(commits: &HashMap<String, Vec<String>>) -> String {
    let mut keys: Vec<&String> = commits.keys().collect();
    keys.sort();

    let mut sections: Vec<String> = Vec::new();
    for key in keys {
        let entries = &commits[key];
        if entries.is_empty() {
            continue;
        }
        // replace '_' with ' ' and uppercase each heading
        let heading = key
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let mut section = format!("## {}\n", heading);
        for commit in entries {
            section.push_str(&format!("- {}\n", commit));
        }
        sections.push(section);
    }

    sections.join("\n")
}

/// Scans every whitespace-separated token in `text` for the first recognised
/// conventional-commit type and returns the corresponding category key.
///
/// A leading `[` is stripped from each token before matching so that enriched
/// merge-commit messages of the form `[feat: add something (#123)](url)` are
/// handled correctly.  Multiline messages are also supported because
/// `split_whitespace` spans newlines.
fn find_commit_category(text: &str) -> &'static str {
    for token in text.split_whitespace() {
        let token = token.trim_start_matches('[');
        let commit_type = String::from(token.split(['(', ':']).next().unwrap_or("").trim()).to_lowercase();
        match commit_type.as_str() {
            "feat" => return "features",
            "fix" | "bug"  => return "bug_fixes",
            "ci" | "infra" | "build" | "chore" | "perf" | "refactor" | "style" | "test" => {
                return "infrastructure"
            }
            "docs" => return "docs",
            _ => {}
        }
    }
    "misc"
}

/// Categorizes conventional commits into features, bug fixes, infrastructure, docs, and misc.
///
/// The key for each category in the returned map is one of:
/// `"features"`, `"bug_fixes"`, `"infrastructure"`, `"docs"`, `"misc"`.
///
/// The conventional-commit type is searched across the **entire** message, not
/// just its first token, so enriched merge-commit messages (e.g.
/// `[feat: add something (#123)](url)`) and multiline messages are handled
/// correctly.
pub(crate) fn categorize_commits(commits: &[&str]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = [
        "features",
        "bug_fixes",
        "infrastructure",
        "docs",
        "misc",
    ]
    .iter()
    .map(|&k| (k.to_string(), Vec::new()))
    .collect();

    for &commit in commits {
        let category = find_commit_category(commit);
        map.entry(category.to_string()).or_default().push(commit.to_string());
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ShipItError;
    use crate::settings::{OllamaOptions, OllamaSettings};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    // ── parse_gitlab_mr_iid ───────────────────────────────────────────────────

    #[test]
    fn test_parse_gitlab_mr_iid_standard_body() {
        assert_eq!(
            parse_gitlab_mr_iid(
                "Merge branch 'feature' into 'main'\n\nSee merge request group/project!789"
            ),
            Some(789)
        );
    }

    #[test]
    fn test_parse_gitlab_mr_iid_no_match() {
        assert_eq!(
            parse_gitlab_mr_iid("Merge branch 'feature' into 'main'"),
            None
        );
    }

    #[test]
    fn test_parse_gitlab_mr_iid_multiline_body() {
        assert_eq!(
            parse_gitlab_mr_iid(
                "Merge branch 'feat/thing' into 'main'\n\nAdds the thing.\n\nSee merge request myorg/myrepo!42"
            ),
            Some(42)
        );
    }

    // ── extract_repo_path ─────────────────────────────────────────────────────

    #[test]
    fn test_extract_repo_path_ssh_url() {
        assert_eq!(
            extract_repo_path("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_extract_repo_path_returns_none_for_empty_string() {
        assert_eq!(extract_repo_path(""), None);
    }

    #[test]
    fn test_extract_repo_path_returns_none_for_empty_ssh_path() {
        assert_eq!(extract_repo_path("git@github.com:"), None);
    }

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

    #[tokio::test]
    async fn test_lookup_gitlab_project_id_returns_error_for_unparseable_url() {
        let result = lookup_gitlab_project_id("", "gitlab.com", "token").await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_lookup_gitlab_project_id_returns_gitlab_error_for_invalid_domain() {
        let result = lookup_gitlab_project_id(
            "git@host:owner/repo.git",
            "://invalid",
            "token",
        )
        .await;
        assert!(matches!(result, Err(ShipItError::Gitlab(_))));
    }

    struct MockPlatformSuccess;
    struct MockPlatformError;

    impl GitPlatform for MockPlatformSuccess {
        async fn open(&self, _: &str, _: &str, _: &str) -> Result<String, ShipItError> {
            Ok("https://github.com/owner/repo/pull/1".to_string())
        }
    }

    impl GitPlatform for MockPlatformError {
        async fn open(&self, _: &str, _: &str, _: &str) -> Result<String, ShipItError> {
            Err(ShipItError::Error("platform returned an error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_open_merge_request_success() {
        let result = open_merge_request(&MockPlatformSuccess, "feat", "main", "desc").await;
        assert_eq!(result.unwrap(), "https://github.com/owner/repo/pull/1");
    }

    #[tokio::test]
    async fn test_open_merge_request_propagates_platform_error() {
        let result = open_merge_request(&MockPlatformError, "feat", "main", "desc").await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    fn ollama_settings(port: u16) -> OllamaSettings {
        OllamaSettings {
            model: "test-model".to_string(),
            domain: "127.0.0.1".to_string(),
            port,
            endpoint: "/api/generate".to_string(),
            prompt: "Summarize:".to_string(),
            options: OllamaOptions::default(),
        }
    }

    struct MockAgentSuccess;
    struct MockAgentError;

    impl Agent for MockAgentSuccess {
        async fn send_prompt(&self, _text: &str) -> Result<String, ShipItError> {
            Ok("mock summary".to_string())
        }
    }

    impl Agent for MockAgentError {
        async fn send_prompt(&self, _text: &str) -> Result<String, ShipItError> {
            Err(ShipItError::Error("agent error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_summarize_with_agent_delegates_to_agent() {
        let result = summarize_with_agent("some commits", &MockAgentSuccess).await;
        assert_eq!(result.unwrap(), "mock summary");
    }

    #[tokio::test]
    async fn test_summarize_with_agent_propagates_agent_error() {
        let result = summarize_with_agent("text", &MockAgentError).await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_ollama_agent_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "response": "Generated summary" })),
            )
            .mount(&server)
            .await;

        let agent = OllamaAgent::new(ollama_settings(server.address().port()));
        let result = summarize_with_agent("some commits", &agent).await;
        assert_eq!(result.unwrap(), "Generated summary");
    }

    #[tokio::test]
    async fn test_ollama_agent_returns_http_error_on_connection_failure() {
        let closed_port = {
            let server = MockServer::start().await;
            server.address().port()
        };
        let agent = OllamaAgent::new(ollama_settings(closed_port));
        let result = summarize_with_agent("text", &agent).await;
        assert!(matches!(result, Err(ShipItError::Http(_))));
    }

    #[tokio::test]
    async fn test_ollama_agent_returns_http_error_on_invalid_json_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&server)
            .await;

        let agent = OllamaAgent::new(ollama_settings(server.address().port()));
        let result = summarize_with_agent("text", &agent).await;
        assert!(matches!(result, Err(ShipItError::Http(_))));
    }

    #[tokio::test]
    async fn test_ollama_agent_returns_error_when_response_field_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "not_response": "oops" })),
            )
            .mount(&server)
            .await;

        let agent = OllamaAgent::new(ollama_settings(server.address().port()));
        let result = summarize_with_agent("text", &agent).await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[test]
    fn test_find_commit_category_bug_fixes() {
        let _result = find_commit_category("See merge request endpoint/project_name!1 ec42387abc\n- Merge branch 'feat' into 'main'\n\nBUG: format the message\n\nmore info");
        assert!(matches!("bug_fixes", _result));
    }

    #[test]
    fn test_find_commit_category_infra() {
        let _result = find_commit_category("See merge request endpoint/project_name!1 ec42387abc\n- Merge branch 'feat' into 'main'\n\nINFRA: format the message\n\nmore info");
        assert!(matches!("infrastructure", _result));
    }

    #[test]
    fn test_categorize_commits_empty_input() {
        let result = categorize_commits(&[]);
        assert!(result["features"].is_empty());
        assert!(result["bug_fixes"].is_empty());
        assert!(result["infrastructure"].is_empty());
        assert!(result["docs"].is_empty());
        assert!(result["misc"].is_empty());
    }

    #[test]
    fn test_categorize_commits_features() {
        let commits = vec!["feat: add login page", "feat(auth): add OAuth support"];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"], vec!["feat: add login page", "feat(auth): add OAuth support"]);
        assert!(result["bug_fixes"].is_empty());
        assert!(result["infrastructure"].is_empty());
        assert!(result["docs"].is_empty());
        assert!(result["misc"].is_empty());
    }

    #[test]
    fn test_categorize_commits_bug_fixes() {
        let commits = vec!["fix: resolve null pointer", "fix(ui): correct button alignment"];
        let result = categorize_commits(&commits);
        assert_eq!(result["bug_fixes"], vec!["fix: resolve null pointer", "fix(ui): correct button alignment"]);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_infrastructure() {
        let commits = vec![
            "ci: add github actions",
            "build: update dependencies",
            "chore: clean up temp files",
            "perf: cache expensive query",
            "refactor: extract helper",
            "style: fix trailing whitespace",
            "test: add unit tests",
        ];
        let result = categorize_commits(&commits);
        assert_eq!(result["infrastructure"].len(), 7);
        assert!(result["features"].is_empty());
        assert!(result["bug_fixes"].is_empty());
    }

    #[test]
    fn test_categorize_commits_docs() {
        let commits = vec!["docs: update README", "docs(api): add endpoint docs"];
        let result = categorize_commits(&commits);
        assert_eq!(result["docs"], vec!["docs: update README", "docs(api): add endpoint docs"]);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_misc() {
        let commits = vec!["wip: half done feature", "unknown: some commit"];
        let result = categorize_commits(&commits);
        assert_eq!(result["misc"], vec!["wip: half done feature", "unknown: some commit"]);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_mixed() {
        let commits = vec![
            "feat: add feature",
            "fix: fix bug",
            "docs: update docs",
            "ci: add workflow",
            "wip: in progress",
        ];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"], vec!["feat: add feature"]);
        assert_eq!(result["bug_fixes"], vec!["fix: fix bug"]);
        assert_eq!(result["docs"], vec!["docs: update docs"]);
        assert_eq!(result["infrastructure"], vec!["ci: add workflow"]);
        assert_eq!(result["misc"], vec!["wip: in progress"]);
    }

    // ── categorize_commits – enriched / nested conventional-commit headings ──

    #[test]
    fn test_categorize_commits_enriched_github_feat() {
        // Enriched GitHub merge-commit: "[Title (#N)](url)" where title starts with feat:
        let commits = vec!["[feat: add login page (#42)](https://github.com/owner/repo/pull/42)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"].len(), 1);
        assert!(result["bug_fixes"].is_empty());
    }

    #[test]
    fn test_categorize_commits_enriched_github_fix() {
        let commits = vec!["[fix: resolve null pointer (#7)](https://github.com/owner/repo/pull/7)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["bug_fixes"].len(), 1);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_enriched_gitlab_feat() {
        // Enriched GitLab merge-commit: "[Title (!N)](url)"
        let commits = vec!["[feat(api): expose new endpoint (!99)](https://gitlab.com/g/p/-/merge_requests/99)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"].len(), 1);
    }

    #[test]
    fn test_categorize_commits_enriched_ci_infrastructure() {
        let commits = vec!["[ci: add release workflow (#5)](https://github.com/owner/repo/pull/5)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["infrastructure"].len(), 1);
    }

    #[test]
    fn test_categorize_commits_multiline_merge_commit_with_nested_type() {
        // Multiline message: first line is the merge subject, conventional type appears later.
        let msg = "Merge pull request #10 from owner/feature\n\nfeat: add something cool abc123";
        let result = categorize_commits(&[msg]);
        assert_eq!(result["features"].len(), 1);
        assert!(result["misc"].is_empty());
    }

    #[test]
    fn test_categorize_commits_multiline_fix_nested() {
        let msg = "Merge pull request #11 from owner/bugfix\n\nfix(ui): correct button alignment abc123";
        let result = categorize_commits(&[msg]);
        assert_eq!(result["bug_fixes"].len(), 1);
    }

    #[test]
    fn test_categorize_commits_no_conventional_type_falls_back_to_misc() {
        let msg = "Merge branch 'main' into develop abc123";
        let result = categorize_commits(&[msg]);
        assert_eq!(result["misc"].len(), 1);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_format_categorized_commits_empty_map() {
        let map: HashMap<String, Vec<String>> = HashMap::new();
        assert_eq!(format_categorized_commits(&map), "");
    }

    #[test]
    fn test_format_categorized_commits_skips_empty_categories() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec![]);
        map.insert("bug_fixes".to_string(), vec![]);
        assert_eq!(format_categorized_commits(&map), "");
    }

    #[test]
    fn test_format_categorized_commits_single_category_single_commit() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec!["feat: add login".to_string()]);
        assert_eq!(
            format_categorized_commits(&map),
            "## Features\n- feat: add login\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_single_category_multiple_commits() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert(
            "bug_fixes".to_string(),
            vec![
                "fix: resolve null pointer".to_string(),
                "fix(ui): correct alignment".to_string(),
            ],
        );
        assert_eq!(
            format_categorized_commits(&map),
            "## Bug Fixes\n- fix: resolve null pointer\n- fix(ui): correct alignment\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_single_category_three_commits() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert(
            "infrastructure".to_string(),
            vec![
                "ci: add github actions".to_string(),
                "build: update dependencies".to_string(),
                "chore: remove unused files".to_string(),
            ],
        );
        assert_eq!(
            format_categorized_commits(&map),
            "## Infrastructure\n- ci: add github actions\n- build: update dependencies\n- chore: remove unused files\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_multiple_categories_sorted() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec!["feat: add search".to_string()]);
        map.insert("bug_fixes".to_string(), vec!["fix: crash on startup".to_string()]);
        map.insert("docs".to_string(), vec!["docs: update README".to_string()]);
        assert_eq!(
            format_categorized_commits(&map),
            "## Bug Fixes\n- fix: crash on startup\n\n## Docs\n- docs: update README\n\n## Features\n- feat: add search\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_mixed_empty_and_populated() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec!["feat: new flag".to_string()]);
        map.insert("misc".to_string(), vec![]);
        map.insert("docs".to_string(), vec!["docs: fix typo".to_string()]);
        assert_eq!(
            format_categorized_commits(&map),
            "## Docs\n- docs: fix typo\n\n## Features\n- feat: new flag\n"
        );
    }
}
