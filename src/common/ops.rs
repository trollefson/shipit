use std::env;
use std::time::Duration;

use git2::Repository;
use indicatif::{ProgressBar, ProgressStyle};

use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{
    categorize_commits, extract_repo_path, fetch_github_pr_info, fetch_gitlab_mr_info,
    format_categorized_commits, lookup_github_identifier, lookup_gitlab_project_id,
    parse_github_pr_number, parse_gitlab_mr_iid, summarize_with_agent, OllamaAgent,
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
    println!(
        "Found a git repository at {}",
        repo.path().to_str().unwrap_or("NOT FOUND")
    );
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
    messages: &[String],
    args_prompt: Option<String>,
    spinner_msg: &str,
) -> Result<String, ShipItError> {
    if !ctx.settings.shipit.agent.is_empty() {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );

        match ctx.settings.shipit.agent.as_str() {
            "ollama" => {
                let mut ollama = ctx.settings.ollama.clone();
                if let Some(prompt) = args_prompt {
                    ollama.prompt = prompt;
                }
                spinner.set_message(format!("{} {}...", spinner_msg, ollama.model));
                spinner.enable_steady_tick(Duration::from_millis(80));

                let agent = OllamaAgent::new(ollama);
                let result = summarize_with_agent(description, &agent)
                    .await
                    .map_err(|_e| ShipItError::Error("Failed to summarize with agent!".to_string()));

                spinner.finish_and_clear();

                Ok(result?)
            }
            "shipit" => {
                let refs: Vec<&str> = messages.iter().map(|s| s.as_str()).collect();
                let categorized = categorize_commits(&refs);
                let formatted = format_categorized_commits(&categorized);
                Ok(formatted)
            }
            unknown => {
                Err(ShipItError::Error(format!("Unknown ai agent: '{}'", unknown)))
            }
        }
    } else {
        Ok(description.to_string())
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
                println!("Auto-detected GitLab project id: {}", id);
                Ok(id.to_string())
            } else {
                Err(ShipItError::Error("Could not determine platform from remote url. Ensure the remote url contains 'github' or 'gitlab'.".to_string()))
            }
        }
    }
}
