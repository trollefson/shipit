use gitlab::api::{projects, AsyncQuery};
use gitlab::Gitlab as GitlabClient;

use crate::error::ShipItError;
use super::common::{extract_repo_path, GitPlatform};

pub(crate) struct Gitlab {
    pub domain: String,
    pub token: String,
    pub project_id: u64,
}

impl GitPlatform for Gitlab {
    async fn open(&self, source: &str, target: &str, title: &str, description: &str) -> Result<String, ShipItError> {
        let client = GitlabClient::builder(&self.domain, &self.token)
            .build_async()
            .await
            .map_err(|e| ShipItError::Gitlab(e))?;

        let create_mr = projects::merge_requests::CreateMergeRequest::builder()
            .project(self.project_id)
            .source_branch(source)
            .target_branch(target)
            .title(title)
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

/// Creates an annotated GitLab tag for the given tag name on the given branch/ref.
pub(crate) async fn create_gitlab_tag(
    domain: &str,
    token: &str,
    project_id: u64,
    tag_name: &str,
    branch: &str,
    notes: &str,
) -> Result<String, ShipItError> {
    use gitlab::api::projects::repository::tags::CreateTag;

    let client = GitlabClient::builder(domain, token)
        .build_async()
        .await
        .map_err(|e| ShipItError::Gitlab(e))?;

    let create_tag = CreateTag::builder()
        .project(project_id)
        .tag_name(tag_name)
        .ref_(branch)
        .message(notes)
        .build()
        .map_err(|e| ShipItError::Error(format!("Failed to build GitLab tag: {}", e)))?;

    let tag: serde_json::Value = create_tag
        .query_async(&client)
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to create GitLab tag: {}", e)))?;

    tag["web_url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ShipItError::Error("Failed to get tag url from GitLab response".to_string()))
}

/// Creates a GitLab release for an already-existing tag.
///
/// Uses the tag name as the release name and `notes` as the release description.
pub(crate) async fn create_gitlab_release(
    domain: &str,
    token: &str,
    project_id: u64,
    tag_name: &str,
    notes: &str,
) -> Result<String, ShipItError> {
    use gitlab::api::projects::releases::CreateRelease;

    let client = GitlabClient::builder(domain, token)
        .build_async()
        .await
        .map_err(|e| ShipItError::Gitlab(e))?;

    let create_release = CreateRelease::builder()
        .project(project_id)
        .tag_name(tag_name)
        .name(tag_name)
        .description(notes)
        .build()
        .map_err(|e| ShipItError::Error(format!("Failed to build GitLab release: {}", e)))?;

    let release: serde_json::Value = create_release
        .query_async(&client)
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to create GitLab release: {}", e)))?;

    release["_links"]["self"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ShipItError::Error("Failed to get release url from GitLab response".to_string()))
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

/// Fetches the name of the most recent tag in a GitLab project.
///
/// Tags are requested ordered by `version` descending so the first item is the
/// latest semver tag; falls back to the first item if no versioned tags exist.
pub(crate) async fn fetch_latest_gitlab_tag(
    domain: &str,
    token: &str,
    project_id: u64,
) -> Result<String, ShipItError> {
    use gitlab::api::projects::repository::tags::Tags;

    let client = GitlabClient::builder(domain, token)
        .build_async()
        .await
        .map_err(|e| ShipItError::Gitlab(e))?;

    let endpoint = Tags::builder()
        .project(project_id)
        .build()
        .map_err(|e| ShipItError::Error(format!("Failed to build GitLab tags query: {}", e)))?;

    let tags: Vec<serde_json::Value> = gitlab::api::paged(endpoint, gitlab::api::Pagination::Limit(1))
        .query_async(&client)
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to fetch GitLab tags: {}", e)))?;

    tags.into_iter()
        .next()
        .and_then(|t| t["name"].as_str().map(|s| s.to_string()))
        .ok_or_else(|| ShipItError::Error("No tags found in GitLab project".to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ShipItError;

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

    // ── lookup_gitlab_project_id ──────────────────────────────────────────────

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
}
