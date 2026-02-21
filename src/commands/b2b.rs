use std::env;

use git2::Repository;

use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{open_gitlab_mr, summarize_with_ollama};

pub async fn branch_to_branch(
    ctx: &Context,
    args_source: String,
    args_target: String,
    args_dir: Option<String>,
    args_id: Option<u64>,
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

    // open an mr
    if args_id.is_some() {
        let project_id = args_id.as_ref().unwrap();
        let token = ctx.settings.gitlab.token.as_deref()
            .ok_or_else(|| ShipItError::Error("GitLab token not configured. Set gitlab.token in your shipit config.".to_string()))?;
        let mr_url = open_gitlab_mr(
            &args_source, &args_target, &ctx.settings.gitlab.domain,
            token, project_id, &summary
        ).await.or_else(|_e| Err(ShipItError::Error("Failed to open a Gitlab MR!".to_string())))?;
        println!("\n\nThe merge request is available at:\n\n{}", mr_url["web_url"]);
    } else {
        return Err(ShipItError::Error("Unable to open a Gitlab MR without a project id specified with '--id'".to_string()));
    }

    Ok(())
}
