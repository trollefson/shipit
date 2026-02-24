use std::collections::HashMap;
use std::env;

use git2::Repository;

use crate::common::agent::Agent;
use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{
    extract_repo_path, fetch_github_pr_info, fetch_gitlab_mr_info,
    fetch_latest_github_tag, fetch_latest_gitlab_tag,
    lookup_github_identifier, lookup_gitlab_project_id, next_version, open_merge_request,
    parse_github_pr_number, parse_gitlab_mr_iid, Github, Gitlab, OllamaAgent, ShipitAgent,
};

pub(crate) fn open_repo(args_dir: Option<String>) -> Repository {
    let dir = match args_dir {
        Some(path) => std::path::PathBuf::from(path),
        None => match env::current_dir() {
            Ok(path) => path,
            Err(e) => panic!("Failed to get the source directory: {}", e),
        },
    };
    let repo = match Repository::init(dir) {
        Ok(repo) => repo,
        Err(e) => panic!("Failed to find a git repo at: {}", e),
    };
    tracing::debug!("Found a git repository at {}", repo.path().to_str().unwrap_or("NOT FOUND"));
    repo
}

pub(crate) fn collect_messages(repo: &Repository, commits: Vec<git2::Oid>) -> Result<Vec<String>, ShipItError> {
    let mut messages = Vec::new();
    for commit in commits {
        let release_oid = repo.find_commit(commit).unwrap();
        let msg = release_oid
            .message()
            .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to unwrap the message of a release commit!")))?
            .to_string();
        messages.push(format!("{} {}", msg, release_oid.id().to_string()));
    }
    Ok(messages)
}

pub(crate) async fn enrich_messages(
    ctx: &Context,
    repo: &Repository,
    remote_name: &str,
    messages: Vec<String>,
) -> Vec<String> {
    let enrichment_remote_url: Option<String> = {
        repo.find_remote(remote_name)
            .ok()
            .and_then(|r| r.url().map(|u| u.to_string()))
    };

    if let Some(ref remote_url) = enrichment_remote_url {
        let is_github = remote_url.contains("github");
        let is_gitlab = remote_url.contains("gitlab");
        let repo_path = extract_repo_path(remote_url);

        let mut enriched = Vec::with_capacity(messages.len());
        for msg in messages {
            let replacement = 'enrich: {
                if is_github {
                    if let Some(pr_num) = parse_github_pr_number(&msg) {
                        if let (Some(token), Some(path)) =
                            (ctx.settings.github.token.as_deref(), &repo_path)
                        {
                            let parts: Vec<&str> = path.splitn(2, '/').collect();
                            if parts.len() == 2 {
                                if let Ok((title, link)) = fetch_github_pr_info(
                                    &ctx.settings.github.domain,
                                    token,
                                    parts[0],
                                    parts[1],
                                    pr_num,
                                )
                                .await
                                {
                                    break 'enrich format!(
                                        "{} - [#{}]({})",
                                        title, pr_num, link
                                    );
                                }
                            }
                        }
                    }
                } else if is_gitlab {
                    if let Some(mr_iid) = parse_gitlab_mr_iid(&msg) {
                        if let (Some(token), Some(path)) =
                            (ctx.settings.gitlab.token.as_deref(), &repo_path)
                        {
                            if let Ok((title, link)) = fetch_gitlab_mr_info(
                                &ctx.settings.gitlab.domain,
                                token,
                                path,
                                mr_iid,
                            )
                            .await
                            {
                                break 'enrich format!(
                                    "{} - [!{}]({})",
                                    title, mr_iid, link
                                );
                            }
                        }
                    }
                }
                msg
            };
            enriched.push(replacement);
        }
        enriched
    } else {
        messages
    }
}

pub(crate) async fn generate_summary(
    ctx: &Context,
    description: &str,
    categorized: &HashMap<String, Vec<String>>,
    args_prompt: Option<String>,
) -> Result<String, ShipItError> {
    match ctx.settings.shipit.agent.as_str() {
        "ollama" => {
            let mut ollama = ctx.settings.ollama.clone();
            if let Some(prompt) = args_prompt {
                ollama.prompt = prompt;
            }
            OllamaAgent::new(ollama).generate_summary(description, categorized).await
        }
        "shipit" => ShipitAgent.generate_summary(description, categorized).await,
        "" => Ok(description.to_string()),
        unknown => Err(ShipItError::Error(format!("Unknown ai agent: '{}'", unknown))),
    }
}

pub(crate) fn resolve_remote_url(repo: &Repository, remote_name: &str) -> Result<String, ShipItError> {
    let remote = repo.find_remote(remote_name).map_err(|e| ShipItError::Git(e))?;
    Ok(remote.url()
        .ok_or_else(|| ShipItError::Error(format!("The '{}' remote has no url.", remote_name)))?
        .to_string())
}

pub(crate) async fn resolve_project_id(
    ctx: &Context,
    remote_url: &str,
    args_id: Option<String>,
    is_github: bool,
    is_gitlab: bool,
) -> Result<String, ShipItError> {
    match args_id {
        Some(id) => Ok(id),
        None => {
            if is_github {
                lookup_github_identifier(remote_url)
                    .map_err(|e| ShipItError::Error(format!("Failed to detect GitHub owner/repo from remote url: {}", e)))
            } else if is_gitlab {
                let token = ctx.settings.gitlab.token.as_deref()
                    .ok_or_else(|| ShipItError::Error("GitLab token is required to look up the project id.".to_string()))?;
                let id = lookup_gitlab_project_id(remote_url, &ctx.settings.gitlab.domain, token).await
                    .map_err(|e| ShipItError::Error(format!("Failed to look up GitLab project id from remote url: {}", e)))?;
                tracing::info!("Auto-detected GitLab project id: {}", id);
                Ok(id.to_string())
            } else {
                Err(ShipItError::Error("Could not determine platform from remote url. Ensure the remote url contains 'github' or 'gitlab'.".to_string()))
            }
        }
    }
}

/// Attempts to auto-generate a merge request title by fetching the latest tag
/// and computing the next semantic version from the categorized commits.
///
/// `resolved_id` must be the value already returned by `resolve_project_id`:
/// `"owner/repo"` for GitHub or a numeric project-id string for GitLab.
///
/// Returns `None` on any failure (missing token, no tags, API error, etc.) so
/// callers can fall back to a default title without surfacing errors.
pub(crate) async fn suggest_title(
    ctx: &Context,
    is_github: bool,
    is_gitlab: bool,
    resolved_id: &str,
    categorized: &HashMap<String, Vec<String>>,
) -> Option<String> {
    let latest_tag = if is_github {
        let token = ctx.settings.github.token.as_deref()?;
        let parts: Vec<&str> = resolved_id.splitn(2, '/').collect();
        if parts.len() != 2 {
            return None;
        }
        fetch_latest_github_tag(&ctx.settings.github.domain, token, parts[0], parts[1])
            .await
            .ok()?
    } else if is_gitlab {
        let token = ctx.settings.gitlab.token.as_deref()?;
        let project_id: u64 = resolved_id.parse().ok()?;
        fetch_latest_gitlab_tag(&ctx.settings.gitlab.domain, token, project_id)
            .await
            .ok()?
    } else {
        return None;
    };

    let version = next_version(&categorized, &latest_tag)?;
    Some(format!("Release {}", version))
}

pub(crate) fn collect_commits(
    repo: &Repository,
    source: &git2::Branch<'_>,
    args_target: &str,
    only_merges: bool,
) -> Result<Vec<git2::Oid>, ShipItError> {
    let target = repo.find_branch(args_target, git2::BranchType::Local).map_err(|e| ShipItError::Git(e))?;
    let target_oid = target
        .get()
        .target()
        .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to find a valid commit for the target branch!")))?;

    let target_oid_on_source = repo.find_commit(target_oid).unwrap();

    let mut revwalk = repo.revwalk().map_err(|e| ShipItError::Git(e))?;
    let root_ref = "refs/heads/";
    let branch_ref = source
        .name().map_err(|e| ShipItError::Git(e))?
        .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to unwrap the name of the source branch!")))?;
    let full_ref = root_ref.to_string() + branch_ref;
    revwalk.push_ref(&full_ref).map_err(|e| ShipItError::Git(e))?;

    let target_oid_hash = target_oid_on_source.id();
    revwalk.hide(target_oid_hash).map_err(|e| ShipItError::Git(e))?;

    let mut commits = Vec::new();
    for oid in revwalk {
        commits.push(oid.map_err(|e| ShipItError::Git(e))?);
    }

    let commits: Vec<_> = if only_merges {
        commits.into_iter().filter(|oid| {
            repo.find_commit(*oid)
                .map(|c| c.parent_count() > 1)
                .unwrap_or(false)
        }).collect()
    } else {
        commits
    };

    Ok(commits)
}

pub(crate) fn check_needs_push(
    repo: &Repository,
    source: &git2::Branch<'_>,
    remote_name: &str,
    source_branch: &str,
) -> Result<bool, ShipItError> {
    let local_oid = source.get().target()
        .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to get source branch oid")))?;
    let remote_tracking_ref = format!("refs/remotes/{}/{}", remote_name, source_branch);
    match repo.find_reference(&remote_tracking_ref) {
        Ok(remote_ref) => match remote_ref.target() {
            Some(remote_oid) => {
                let (ahead, _) = repo.graph_ahead_behind(local_oid, remote_oid)
                    .map_err(|e| ShipItError::Git(e))?;
                Ok(ahead > 0)
            }
            None => Ok(true),
        },
        Err(_) => Ok(true),
    }
}

pub(crate) async fn open_platform_mr(
    ctx: &Context,
    resolved_id: &str,
    source: &str,
    target: &str,
    title: &str,
    summary: &str,
    is_github: bool,
    is_gitlab: bool,
) -> Result<String, ShipItError> {
    if is_github {
        let parts: Vec<&str> = resolved_id.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(ShipItError::Error(format!("GitHub project identifier '{}' must be in 'owner/repo' format.", resolved_id)));
        }
        let (owner, gh_repo) = (parts[0], parts[1]);
        let platform = Github {
            domain: ctx.settings.github.domain.clone(),
            token: ctx.settings.github.token.as_deref().unwrap().to_string(),
            owner: owner.to_string(),
            repo: gh_repo.to_string(),
        };
        open_merge_request(&platform, source, target, title, summary)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to open a GitHub pr: {}", e)))
    } else if is_gitlab {
        let project_id: u64 = resolved_id.parse()
            .map_err(|_| ShipItError::Error(format!("GitLab project identifier '{}' must be a numeric project id.", resolved_id)))?;
        let platform = Gitlab {
            domain: ctx.settings.gitlab.domain.clone(),
            token: ctx.settings.gitlab.token.as_deref().unwrap().to_string(),
            project_id,
        };
        open_merge_request(&platform, source, target, title, summary)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to open a GitLab mr: {}", e)))
    } else {
        Err(ShipItError::Error("Could not determine platform from remote url. Ensure the remote url contains 'github' or 'gitlab'.".to_string()))
    }
}
