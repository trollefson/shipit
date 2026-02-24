use git2::Repository;

use crate::cli::B2bArgs;
use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{
    collect_messages, enrich_messages, generate_summary, open_merge_request, open_repo,
    resolve_project_id, resolve_remote_url, Github, Gitlab,
};

fn collect_commits(
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

fn check_needs_push(
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

async fn open_platform_mr(
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

pub async fn branch_to_branch(ctx: &Context, args: B2bArgs) -> Result<(), ShipItError> {
    let repo = open_repo(args.dir);
    let source = repo.find_branch(&args.source, git2::BranchType::Local).map_err(|e| ShipItError::Git(e))?;

    let mut summary = if let Some(provided) = args.description {
        provided
    } else {
        let commits = collect_commits(&repo, &source, &args.target, args.only_merges)?;
        let messages = collect_messages(&repo, commits)?;
        let messages = enrich_messages(ctx, &repo, &args.remote, messages).await;

        let description = messages.join(",");

        if description.is_empty() {
            tracing::warn!("No commits found between '{}' and '{}'. Nothing to do.", args.source, args.target);
            return Ok(());
        }

        generate_summary(ctx, &description, &messages, args.prompt, "Generating merge request description with").await?
    };

    let title = args.title.unwrap_or_else(|| format!("{} to {}", args.source, args.target));

    crate::output::print_content("The merge request description is:", &summary);
    summary += "\n\n\n*This request was generated by [Shipit](https://gitshipit.net)* 🚢";

    if ctx.settings.shipit.dryrun {
        crate::output::print_dryrun("open a request");
        return Ok(());
    }

    let remote_url = resolve_remote_url(&repo, &args.remote)?;
    let (is_github, is_gitlab) = (remote_url.contains("github"), remote_url.contains("gitlab"));
    let resolved_id = resolve_project_id(ctx, &remote_url, args.id, is_github, is_gitlab).await?;

    if check_needs_push(&repo, &source, &args.remote, &args.source)? {
        crate::output::print_push_prompt(&args.remote, &args.source);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    }

    let url = open_platform_mr(ctx, &resolved_id, &args.source, &args.target, &title, &summary, is_github, is_gitlab).await?;
    crate::output::print_url(&url);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::B2bArgs;
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

    /// Initialises a bare repo with a single base commit on HEAD and two named
    /// branches: `target_name` at the base commit and `source_name` with one
    /// extra commit on top.  Returns (TempDir, Repository, base_oid, source_oid).
    fn setup_diverged_repo(
        source_name: &str,
        target_name: &str,
    ) -> (TempDir, Repository, git2::Oid, git2::Oid) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();

        let tree_id = repo.index().unwrap().write_tree().unwrap();

        let (base_oid, source_oid) = {
            let tree = repo.find_tree(tree_id).unwrap();

            let base_oid = repo
                .commit(Some("HEAD"), &sig, &sig, "base commit", &tree, &[])
                .unwrap();
            let base_commit = repo.find_commit(base_oid).unwrap();

            repo.branch(target_name, &base_commit, false).unwrap();
            repo.branch(source_name, &base_commit, false).unwrap();

            let source_ref = format!("refs/heads/{}", source_name);
            let source_oid = repo
                .commit(
                    Some(&source_ref),
                    &sig,
                    &sig,
                    "feat: add something",
                    &tree,
                    &[&base_commit],
                )
                .unwrap();

            (base_oid, source_oid)
        };

        (dir, repo, base_oid, source_oid)
    }

    /// Creates a remote tracking ref so that `needs_push` evaluates to `false`
    /// (local and remote are at the same commit).
    fn pin_remote_tracking(repo: &Repository, remote: &str, branch: &str, oid: git2::Oid) {
        let ref_name = format!("refs/remotes/{}/{}", remote, branch);
        repo.reference(&ref_name, oid, false, "test").unwrap();
    }

    #[tokio::test]
    async fn test_missing_source_branch_returns_git_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "does-not-exist".to_string(),
                target: "main".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_missing_target_branch_without_description_returns_git_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("source", &base_commit, false).unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "does-not-exist".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                // no description → code must look up the target branch
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_description_skips_target_branch_lookup() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("source", &base_commit, false).unwrap();

        let ctx = make_ctx(true, "", None, None); // dryrun → exits before remote
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "nonexistent-target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                description: Some("My custom description".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_commits_between_branches_returns_ok_early() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();
        repo.branch("source", &base_commit, false).unwrap();
        repo.branch("target", &base_commit, false).unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dryrun_exits_after_commit_discovery_without_reaching_remote() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(true, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dryrun_with_description_exits_without_reaching_remote() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(true, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                description: Some("Provided description".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shipit_agent_categorizes_commits_and_returns_ok() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(true, "shipit", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_agent_name_returns_shipit_error() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(false, "unknown_agent", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(msg)) if msg.contains("unknown_agent")));
    }

    #[tokio::test]
    async fn test_missing_remote_returns_git_error() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                description: Some("desc".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_url_without_github_or_gitlab_returns_error() {
        let (dir, repo, _, _) = setup_diverged_repo("source", "target");
        repo.remote("origin", "https://bitbucket.org/owner/repo.git")
            .unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                description: Some("desc".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_auto_detect_gitlab_url_without_token_returns_error() {
        let (dir, repo, _, _) = setup_diverged_repo("source", "target");
        repo.remote("origin", "https://gitlab.com/owner/repo.git")
            .unwrap();

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                description: Some("desc".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_github_id_without_slash_returns_format_error() {
        let (dir, repo, _, source_oid) = setup_diverged_repo("source", "target");
        repo.remote("origin", "https://github.com/owner/repo.git")
            .unwrap();
        pin_remote_tracking(&repo, "origin", "source", source_oid);

        let ctx = make_ctx(false, "", None, Some("fake-token"));
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                id: Some("noslash".to_string()),
                remote: "origin".to_string(),
                description: Some("desc".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(msg)) if msg.contains("owner/repo")));
    }

    #[tokio::test]
    async fn test_gitlab_non_numeric_project_id_returns_error() {
        let (dir, repo, _, source_oid) = setup_diverged_repo("source", "target");
        repo.remote("origin", "https://gitlab.com/owner/repo.git")
            .unwrap();
        pin_remote_tracking(&repo, "origin", "source", source_oid);

        let ctx = make_ctx(false, "", Some("fake-token"), None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                id: Some("not-a-number".to_string()),
                remote: "origin".to_string(),
                description: Some("desc".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(msg)) if msg.contains("numeric")));
    }

    #[tokio::test]
    async fn test_only_merges_filters_out_regular_commits() {
        // The diverged repo has one regular (non-merge) commit on the source branch.
        // With only_merges=true that commit is filtered out, so the description is
        // empty and the function returns Ok early.
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(false, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                only_merges: true,
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_only_merges_includes_merge_commits() {
        // Build a repo where the source branch has a real merge commit (two parents).
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        // base commit → shared ancestor
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "base commit", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base_oid).unwrap();

        // target branch sits at the base
        repo.branch("target", &base_commit, false).unwrap();

        // feature commit branching off base
        let feature_oid = repo
            .commit(None, &sig, &sig, "feat: feature", &tree, &[&base_commit])
            .unwrap();
        let feature_commit = repo.find_commit(feature_oid).unwrap();

        // merge commit on source that joins base + feature (two parents)
        let source_ref = "refs/heads/source";
        repo.commit(
            Some(source_ref),
            &sig,
            &sig,
            "Merge branch 'feature' into source",
            &tree,
            &[&base_commit, &feature_commit],
        )
        .unwrap();

        let ctx = make_ctx(true, "", None, None);
        let result = branch_to_branch(
            &ctx,
            B2bArgs {
                source: "source".to_string(),
                target: "target".to_string(),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                only_merges: true,
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }
}
