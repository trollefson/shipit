use gitlab::api::{projects, AsyncQuery};
use gitlab::Gitlab as GitlabClient;
use octocrab::OctocrabBuilder;
use reqwest::Client;
use serde_json::json;

use crate::error::ShipItError;
use crate::settings::OllamaSettings;

/// Extracts the repository path from a git remote URL.
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

/// Returns the GitHub `owner/repo` identifier by parsing it directly from the remote URL.
pub(crate) fn lookup_github_identifier(remote_url: &str) -> Result<String, ShipItError> {
    extract_repo_path(remote_url).ok_or_else(|| {
        ShipItError::Error(format!(
            "Failed to parse GitHub owner/repo from remote URL: {}",
            remote_url
        ))
    })
}

/// Parses the project path from the remote URL and queries the GitLab API
/// to resolve the numeric project ID.
pub(crate) async fn lookup_gitlab_project_id(
    remote_url: &str,
    domain: &str,
    token: &str,
) -> Result<u64, ShipItError> {
    let path = extract_repo_path(remote_url).ok_or_else(|| {
        ShipItError::Error(format!(
            "Failed to parse GitLab project path from remote URL: {}",
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
            .ok_or_else(|| ShipItError::Error("Failed to get PR URL from GitHub response".to_string()))?;

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
            .map_err(|e| ShipItError::Error(format!("Failed to build a Gitlab MR: {}", e)))?;

        let merge_request: serde_json::Value = create_mr
            .query_async(&client)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to create a Gitlab merge request: {}", e)))?;

        merge_request["web_url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ShipItError::Error("Failed to get MR URL from GitLab response".to_string()))
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


pub(crate) async fn summarize_with_ollama(text: &str, ollama: &OllamaSettings) -> Result<String, ShipItError> {
    let client = Client::new();

    let prompt = format!("{}\n\n{}", ollama.prompt, text);

    let url = format!("http://{}:{}{}", ollama.domain, ollama.port, ollama.endpoint);

    let response = client.post(&url)
        .json(&json!({
            "model": ollama.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": ollama.options.temperature,
                "top_p": ollama.options.top_p,
                "seed": ollama.options.seed
            }
        }))
        .send()
        .await.map_err(|e| ShipItError::Http(e))?
        .json::<serde_json::Value>()
        .await.map_err(|e| ShipItError::Http(e))?;

    let summary = response["response"]
        .as_str()
        .ok_or_else(|| ShipItError::Error("Failed to parse Ollama response!".to_string()))?;

    Ok(summary.to_string())
}
