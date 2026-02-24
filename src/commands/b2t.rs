use git2::Repository;

use crate::cli::B2tArgs;
use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{
    categorize_commits, collect_messages, create_github_release, create_gitlab_tag,
    enrich_messages, generate_summary, open_repo, resolve_project_id, resolve_remote_url,
};

/// Finds the most recent tag reachable from (and an ancestor of) `branch_oid`.
fn find_most_recent_tag(repo: &Repository, branch_oid: git2::Oid) -> Result<String, ShipItError> {
    let mut best: Option<(i64, String)> = None;

    let refs = repo.references().map_err(|e| ShipItError::Git(e))?;
    for reference in refs {
        let reference = reference.map_err(|e| ShipItError::Git(e))?;
        if !reference.is_tag() {
            continue;
        }

        let tag_commit = match reference.peel_to_commit() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let tag_oid = tag_commit.id();

        let merge_base = match repo.merge_base(branch_oid, tag_oid) {
            Ok(oid) => oid,
            Err(_) => continue,
        };

        if merge_base != tag_oid {
            continue;
        }

        let name = match reference.shorthand() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let seconds = tag_commit.time().seconds();
        match best {
            None => best = Some((seconds, name)),
            Some((best_seconds, _)) if seconds > best_seconds => {
                best = Some((seconds, name));
            }
            _ => {}
        }
    }

    best.map(|(_, name)| name).ok_or_else(|| {
        ShipItError::Error(
            "No tags found on this branch. Use --latest-tag to specify a tag to compare against."
                .to_string(),
        )
    })
}

/// Collects commits on `branch` since `tag_name`, optionally filtering to merge commits only.
fn collect_commits_since_tag(
    repo: &Repository,
    branch: &str,
    tag_name: &str,
    only_merges: bool,
) -> Result<Vec<git2::Oid>, ShipItError> {
    let tag_ref = format!("refs/tags/{}", tag_name);
    let tag_reference = repo.find_reference(&tag_ref).map_err(|e| ShipItError::Git(e))?;
    let tag_commit = tag_reference.peel_to_commit().map_err(|e| ShipItError::Git(e))?;
    let tag_oid = tag_commit.id();

    let mut revwalk = repo.revwalk().map_err(|e| ShipItError::Git(e))?;
    let branch_ref = format!("refs/heads/{}", branch);
    revwalk.push_ref(&branch_ref).map_err(|e| ShipItError::Git(e))?;
    revwalk.hide(tag_oid).map_err(|e| ShipItError::Git(e))?;

    let mut commits = Vec::new();
    for oid in revwalk {
        commits.push(oid.map_err(|e| ShipItError::Git(e))?);
    }

    let commits = if only_merges {
        commits
            .into_iter()
            .filter(|oid| {
                repo.find_commit(*oid)
                    .map(|c| c.parent_count() > 1)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        commits
    };

    Ok(commits)
}

/// Creates an annotated local tag pointing at `branch_oid`.
fn create_local_tag(
    repo: &Repository,
    tag_name: &str,
    branch_oid: git2::Oid,
    notes: &str,
) -> Result<(), ShipItError> {
    let obj = repo
        .find_object(branch_oid, Some(git2::ObjectType::Commit))
        .map_err(|e| ShipItError::Git(e))?;
    let sig = git2::Signature::now("shipit", "shipit@gitshipit.net")
        .map_err(|e| ShipItError::Git(e))?;
    repo.tag(tag_name, &obj, &sig, notes, false)
        .map_err(|e| ShipItError::Git(e))?;
    Ok(())
}

/// Creates the tag on the remote platform (GitHub release or GitLab tag).
async fn create_platform_tag(
    ctx: &Context,
    resolved_id: &str,
    tag_name: &str,
    branch: &str,
    notes: &str,
    is_github: bool,
    is_gitlab: bool,
) -> Result<String, ShipItError> {
    if is_github {
        let parts: Vec<&str> = resolved_id.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(ShipItError::Error(format!(
                "GitHub project identifier '{}' must be in 'owner/repo' format.",
                resolved_id
            )));
        }
        let (owner, gh_repo) = (parts[0], parts[1]);
        let token = ctx
            .settings
            .github
            .token
            .as_deref()
            .ok_or_else(|| ShipItError::Error("GitHub token is required to create a release.".to_string()))?;
        create_github_release(
            &ctx.settings.github.domain,
            token,
            owner,
            gh_repo,
            tag_name,
            branch,
            notes,
        )
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to create a GitHub release: {}", e)))
    } else if is_gitlab {
        let project_id: u64 = resolved_id.parse().map_err(|_| {
            ShipItError::Error(format!(
                "GitLab project identifier '{}' must be a numeric project id.",
                resolved_id
            ))
        })?;
        let token = ctx
            .settings
            .gitlab
            .token
            .as_deref()
            .ok_or_else(|| ShipItError::Error("GitLab token is required to create a tag.".to_string()))?;
        create_gitlab_tag(
            &ctx.settings.gitlab.domain,
            token,
            project_id,
            tag_name,
            branch,
            notes,
        )
        .await
        .map_err(|e| ShipItError::Error(format!("Failed to create a GitLab tag: {}", e)))
    } else {
        Err(ShipItError::Error(
            "Could not determine platform from remote url. Ensure the remote url contains 'github' or 'gitlab'.".to_string(),
        ))
    }
}

pub async fn branch_to_tag(ctx: &Context, args: B2tArgs) -> Result<(), ShipItError> {
    let repo = open_repo(args.dir);
    let branch = repo
        .find_branch(&args.branch, git2::BranchType::Local)
        .map_err(|e| ShipItError::Git(e))?;
    let branch_oid = branch
        .get()
        .target()
        .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to find a valid commit for the branch!")))?;

    let mut summary = if let Some(provided) = args.description {
        provided
    } else {
        let latest_tag_name = match args.latest_tag {
            Some(t) => t,
            None => find_most_recent_tag(&repo, branch_oid)?,
        };
        tracing::info!("Comparing against tag: {}", latest_tag_name);

        let commits = collect_commits_since_tag(&repo, &args.branch, &latest_tag_name, args.only_merges)?;
        let messages = collect_messages(&repo, commits)?;
        let messages = enrich_messages(ctx, &repo, &args.remote, messages).await;

        if messages.is_empty() {
            tracing::warn!("No commits found since tag '{}'. Nothing to do.", latest_tag_name);
            return Ok(());
        }

        let description = messages.join(",");
        let refs: Vec<&str> = messages.iter().map(|s| s.as_str()).collect();
        let categorized = categorize_commits(&refs);
        generate_summary(ctx, &description, &categorized, args.prompt).await?
    };

    crate::output::print_content("The tag notes are:", &summary);
    summary += "\n\n\n*This tag was generated by [Shipit](https://gitshipit.net)* 🚢";

    if ctx.settings.shipit.dryrun {
        crate::output::print_dryrun("create the tag");
        return Ok(());
    }

    create_local_tag(&repo, &args.tag, branch_oid, &summary)?;
    crate::output::print_success(&format!("Created local tag '{}'", args.tag));

    let remote_url = resolve_remote_url(&repo, &args.remote)?;
    let (is_github, is_gitlab) = (remote_url.contains("github"), remote_url.contains("gitlab"));
    let resolved_id = resolve_project_id(ctx, &remote_url, args.id, is_github, is_gitlab).await?;

    let url = create_platform_tag(
        ctx,
        &resolved_id,
        &args.tag,
        &args.branch,
        &summary,
        is_github,
        is_gitlab,
    )
    .await?;
    crate::output::print_url(&url);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::B2tArgs;
    use crate::context::Context;
    use crate::settings::{
        GithubSettings, GitlabSettings, OllamaSettings, Settings, ShipitSettings,
    };
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    fn make_ctx(
        dryrun: bool,
        agent: &str,
        gitlab_token: Option<&str>,
        github_token: Option<&str>,
    ) -> Context {
        Context {
            settings: Settings {
                shipit: ShipitSettings {
                    agent: agent.to_string(),
                    commits: "custom".to_string(),
                    dryrun,
                },
                ollama: OllamaSettings::default(),
                gitlab: GitlabSettings {
                    domain: "gitlab.com".to_string(),
                    token: gitlab_token.map(|t| t.to_string()),
                },
                github: GithubSettings {
                    domain: "github.com".to_string(),
                    token: github_token.map(|t| t.to_string()),
                },
            },
        }
    }

    /// Creates a repo with a base commit, a tag on that commit, and one extra
    /// commit on `branch_name` beyond the tag. Returns (TempDir, Repository,
    /// tag_oid, extra_commit_oid).
    fn setup_tagged_repo(branch_name: &str, tag_name: &str) -> (TempDir, Repository, git2::Oid, git2::Oid) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();

        let (base_oid, extra_oid) = {
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();

            // Base commit (tagged)
            let base_oid = repo
                .commit(Some("HEAD"), &sig, &sig, "base commit", &tree, &[])
                .unwrap();
            let base_commit = repo.find_commit(base_oid).unwrap();

            // Create branch at base
            repo.branch(branch_name, &base_commit, false).unwrap();

            // Tag the base commit
            let tag_obj = repo.find_object(base_oid, Some(git2::ObjectType::Commit)).unwrap();
            let tagger = Signature::now("tagger", "tagger@example.com").unwrap();
            repo.tag(tag_name, &tag_obj, &tagger, "initial release", false).unwrap();

            // Extra commit on branch beyond the tag
            let branch_ref = format!("refs/heads/{}", branch_name);
            let extra_oid = repo
                .commit(
                    Some(&branch_ref),
                    &sig,
                    &sig,
                    "feat: add something",
                    &tree,
                    &[&base_commit],
                )
                .unwrap();

            (base_oid, extra_oid)
        };

        (dir, repo, base_oid, extra_oid)
    }

    #[tokio::test]
    async fn test_missing_branch_returns_git_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "does-not-exist".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_no_tags_in_repo_returns_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "feature".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                // no --latest-tag → auto-detection must find a tag
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_latest_tag_not_found_returns_git_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "feature".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                latest_tag: Some("nonexistent-tag".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_no_commits_since_tag_returns_ok_early() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();

        // Tag at the same commit as the branch tip
        let tag_obj = repo.find_object(base_oid, Some(git2::ObjectType::Commit)).unwrap();
        let tagger = Signature::now("tagger", "tagger@example.com").unwrap();
        repo.tag("v0.1.0", &tag_obj, &tagger, "initial", false).unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "feature".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                latest_tag: Some("v0.1.0".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dryrun_exits_without_creating_tag() {
        let (dir, repo, _, _) = setup_tagged_repo("feature", "v0.1.0");

        let ctx = make_ctx(true, "", None, None);
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "feature".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                latest_tag: Some("v0.1.0".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
        // Local tag must NOT have been created
        assert!(repo.find_reference("refs/tags/v1.0.0").is_err());
    }

    #[tokio::test]
    async fn test_description_skips_tag_lookup() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("feature", &base_commit, false).unwrap();

        // No tags in this repo — but description bypasses tag lookup
        let ctx = make_ctx(true, "", None, None); // dryrun so no remote call
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "feature".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                description: Some("My release notes".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_only_merges_filters_regular_commits() {
        // setup_tagged_repo adds one regular (non-merge) commit on top of the tag.
        // With only_merges=true that commit is filtered → nothing to do.
        let (dir, _repo, _, _) = setup_tagged_repo("feature", "v0.1.0");

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_tag(
            &ctx,
            B2tArgs {
                branch: "feature".to_string(),
                tag: "v1.0.0".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                latest_tag: Some("v0.1.0".to_string()),
                only_merges: true,
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }
}
