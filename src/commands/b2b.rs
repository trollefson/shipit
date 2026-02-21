use std::env;

use git2::Repository;

use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{open_github_pr, open_gitlab_mr, summarize_with_ollama};

pub async fn branch_to_branch(
    ctx: &Context,
    args_source: String,
    args_target: String,
    args_dir: Option<String>,
    args_id: Option<String>,
) -> Result<(), ShipItError> {
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

    // get branch and most recent commit structs for the target and source branches
    let source = repo.find_branch(&args_source, git2::BranchType::Local).map_err(|e| ShipItError::Git(e))?;
    let target = repo.find_branch(&args_target, git2::BranchType::Local).map_err(|e| ShipItError::Git(e))?;
    let target_oid = target
        .get()
        .target()
        .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to find a valid commit for the target branch!")))?;

    // find the most recent target commit on the source branch
    // this will help determine which commits are not present on the target branch
    let target_oid_on_source = repo.find_commit(target_oid).unwrap();

    // create a vector of the commit ids that are on the source, but not the
    // target branch.  display the messages for those commit ids
    // and create a revision walk for the source branch
    let mut revwalk = repo.revwalk().map_err(|e| ShipItError::Git(e))?;
    let root_ref = "refs/heads/";
    let branch_ref = source
        .name().map_err(|e| ShipItError::Git(e))?
        .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to unwrap the name of the source branch!")))?;
    let full_ref = root_ref.to_string() + branch_ref;
    revwalk.push_ref(&full_ref).map_err(|e| ShipItError::Git(e))?;
    let target_oid_hash = target_oid_on_source.id();

    // hide commits that are on both branches
    // essentially tells the walker to stop here
    revwalk.hide(target_oid_hash).map_err(|e| ShipItError::Git(e))?;
    let mut commits = Vec::new();
    for oid in revwalk {
        commits.push(oid.map_err(|e| ShipItError::Git(e))?);
    }

    // display the messages of the discovered commits
    let mut messages = Vec::new();
    for commit in commits {
        let release_oid = repo.find_commit(commit).unwrap();
        let msg = release_oid
            .message()
            .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to unwrap the message of a release commit!")))?
            .to_string();
        messages.push(format!("{} {}", msg, release_oid.id().to_string()));
    }
    let description = messages.join(",");

    if description.is_empty() {
        println!("No commits found between '{}' and '{}'. Nothing to do.", args_source, args_target);
        return Ok(());
    }

    // ask a local llm to summarize these commit messages
    let mut summary = if ctx.settings.shipit.ai {
        let result = summarize_with_ollama(
            &description, &ctx.settings.ollama
        ).await.or_else(|_e| Err(ShipItError::Error("Failed to summarize with Ollama!".to_string())))?;
        println!("The merge request description is:\n\n{}", result);
        result
    } else {
        description
    };
    summary += "\n\n\n*This request was opened by Shipit* 🚢";

    if ctx.settings.shipit.dryrun {
        println!("\n\nDry run complete! Re-run without the dry-run flag to open a request.");
        return Ok(());
    }

    // handle opening a github pr or gitlab mr
    // defaults to github if both are configured
    let id = args_id.as_deref()
        .ok_or_else(|| ShipItError::Error("A project identifier is required via '--id'.".to_string()))?;

    let use_github = ctx.settings.github.token.is_some();
    let use_gitlab = ctx.settings.gitlab.token.is_some() && !use_github;

    if use_github {
        let parts: Vec<&str> = id.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(ShipItError::Error("'--id' must be in 'owner/repo' format for GitHub.".to_string()));
        }
        let (owner, repo) = (parts[0], parts[1]);
        let token = ctx.settings.github.token.as_deref().unwrap();
        let pr_url = open_github_pr(
            &args_source, &args_target, &ctx.settings.github.domain,
            token, owner, repo, &summary,
        ).await.map_err(|e| ShipItError::Error(format!("Failed to open a GitHub PR: {}", e)))?;
        println!("\n\nThe pull request is available at:\n\n{}", pr_url);
    } else if use_gitlab {
        let project_id: u64 = id.parse()
            .map_err(|_| ShipItError::Error("'--id' must be a numeric project ID for GitLab.".to_string()))?;
        let token = ctx.settings.gitlab.token.as_deref().unwrap();
        let mr_url = open_gitlab_mr(
            &args_source, &args_target, &ctx.settings.gitlab.domain,
            token, &project_id, &summary,
        ).await.map_err(|e| ShipItError::Error(format!("Failed to open a GitLab MR: {}", e)))?;
        println!("\n\nThe merge request is available at:\n\n{}", mr_url["web_url"]);
    } else {
        return Err(ShipItError::Error("No platform token configured. Set github.token or gitlab.token in your shipit config.".to_string()));
    }

    Ok(())
}
