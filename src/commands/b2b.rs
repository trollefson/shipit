use std::env;
use std::time::Duration;

use git2::Repository;
use indicatif::{ProgressBar, ProgressStyle};

use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{categorize_commits, format_categorized_commits, lookup_github_identifier, lookup_gitlab_project_id, open_merge_request, Github, Gitlab, summarize_with_agent, OllamaAgent};

pub async fn branch_to_branch(
    ctx: &Context,
    args_source: String,
    args_target: String,
    args_dir: Option<String>,
    args_id: Option<String>,
    args_remote: String,
    args_prompt: Option<String>,
    args_description: Option<String>,
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

    // if a description is provided, skip commit discovery and summary generation
    let mut summary = if let Some(provided) = args_description {
        provided
    } else {
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
        if ctx.settings.shipit.ai {
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
                    spinner.set_message(format!("Generating merge request description with {}...", ollama.model));
                    spinner.enable_steady_tick(Duration::from_millis(80));

                    let agent = OllamaAgent::new(ollama);
                    let result = summarize_with_agent(&description, &agent)
                        .await
                        .map_err(|_e| ShipItError::Error("Failed to summarize with agent!".to_string()));

                    spinner.finish_and_clear();

                    let result = result?;
                    println!("The merge request description is:\n\n{}", result);
                    result
                }
                "shipit" => {
                    let refs: Vec<&str> = messages.iter().map(|s| s.as_str()).collect();
                    let categorized = categorize_commits(&refs);
                    let formatted = format_categorized_commits(&categorized);
                    println!("The merge request description is:\n\n{}", formatted);
                    formatted
                }
                unknown => {
                    return Err(ShipItError::Error(format!("Unknown ai agent: '{}'", unknown)));
                }
            }
        } else {
            description
        }
    };
    summary += "\n\n\n*This request was generated by [Shipit](https://gitshipit.net)* 🚢";

    if ctx.settings.shipit.dryrun {
        println!("\n\nDry run complete! Re-run without the dry-run flag to open a request.");
        return Ok(());
    }

    // always fetch the remote url — needed both for platform detection and id auto-lookup
    let remote_url = {
        let remote = repo.find_remote(&args_remote).map_err(|e| ShipItError::Git(e))?;
        remote.url()
            .ok_or_else(|| ShipItError::Error(format!("The '{}' remote has no url.", args_remote)))?
            .to_string()
    };

    // detect platform from remote url
    let (is_github, is_gitlab) = (remote_url.contains("github"), remote_url.contains("gitlab"));

    // resolve the project identifier:
    // use --id if provided, otherwise look it up from the remote url via the platform api
    let resolved_id: String = match args_id {
        Some(id) => id,
        None => {
            if is_github {
                lookup_github_identifier(&remote_url)
                    .map_err(|e| ShipItError::Error(format!("Failed to detect GitHub owner/repo from remote url: {}", e)))?
            } else if is_gitlab {
                let token = ctx.settings.gitlab.token.as_deref()
                    .ok_or_else(|| ShipItError::Error("GitLab token is required to look up the project id.".to_string()))?;
                let id = lookup_gitlab_project_id(&remote_url, &ctx.settings.gitlab.domain, token).await
                    .map_err(|e| ShipItError::Error(format!("Failed to look up GitLab project id from remote url: {}", e)))?;
                println!("Auto-detected GitLab project id: {}", id);
                id.to_string()
            } else {
                return Err(ShipItError::Error("Could not determine platform from remote url. Ensure the remote url contains 'github' or 'gitlab'.".to_string()));
            }
        }
    };

    // check if the local source branch is ahead of its remote tracking branch
    let needs_push = {
        let local_oid = source.get().target()
            .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to get source branch oid")))?;
        let remote_tracking_ref = format!("refs/remotes/{}/{}", args_remote, args_source);
        match repo.find_reference(&remote_tracking_ref) {
            Ok(remote_ref) => match remote_ref.target() {
                Some(remote_oid) => {
                    let (ahead, _) = repo.graph_ahead_behind(local_oid, remote_oid)
                        .map_err(|e| ShipItError::Git(e))?;
                    ahead > 0
                }
                None => true,
            },
            Err(_) => true,
        }
    };

    if needs_push {
        println!(
            "\n\nYour local source branch is ahead of the remote. Please push it, then press Enter to continue:\n\n  git push {} {}\n",
            args_remote, args_source
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
    }

    let url = if is_github {
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
        open_merge_request(&platform, &args_source, &args_target, &summary)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to open a GitHub pr: {}", e)))?
    } else if is_gitlab {
        let project_id: u64 = resolved_id.parse()
            .map_err(|_| ShipItError::Error(format!("GitLab project identifier '{}' must be a numeric project id.", resolved_id)))?;
        let platform = Gitlab {
            domain: ctx.settings.gitlab.domain.clone(),
            token: ctx.settings.gitlab.token.as_deref().unwrap().to_string(),
            project_id,
        };
        open_merge_request(&platform, &args_source, &args_target, &summary)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to open a GitLab mr: {}", e)))?
    } else {
        return Err(ShipItError::Error("Could not determine platform from remote url. Ensure the remote url contains 'github' or 'gitlab'.".to_string()));
    };
    println!("\n\nThe request is available at:\n\n{}", url);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::settings::{
        GithubSettings, GitlabSettings, OllamaSettings, Settings, ShipitSettings,
    };
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    fn make_ctx(
        dryrun: bool,
        ai: bool,
        agent: &str,
        gitlab_token: Option<&str>,
        github_token: Option<&str>,
    ) -> Context {
        Context {
            settings: Settings {
                shipit: ShipitSettings {
                    agent: agent.to_string(),
                    ai,
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

        let ctx = make_ctx(false, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "does-not-exist".to_string(),
            "main".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            None,
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

        let ctx = make_ctx(false, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "does-not-exist".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            None, // no description → code must look up the target branch
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

        let ctx = make_ctx(true, false, "ollama", None, None); // dryrun → exits before remote
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "nonexistent-target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            Some("My custom description".to_string()),
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

        let ctx = make_ctx(false, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            None,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dryrun_exits_after_commit_discovery_without_reaching_remote() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(true, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            None,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dryrun_with_description_exits_without_reaching_remote() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(true, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            Some("Provided description".to_string()),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shipit_agent_categorizes_commits_and_returns_ok() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(true, true, "shipit", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            None,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_agent_name_returns_shipit_error() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(false, true, "unknown_agent", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            None,
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(msg)) if msg.contains("unknown_agent")));
    }

    #[tokio::test]
    async fn test_missing_remote_returns_git_error() {
        let (dir, _repo, _, _) = setup_diverged_repo("source", "target");

        let ctx = make_ctx(false, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            Some("desc".to_string()),
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_url_without_github_or_gitlab_returns_error() {
        let (dir, repo, _, _) = setup_diverged_repo("source", "target");
        repo.remote("origin", "https://bitbucket.org/owner/repo.git")
            .unwrap();

        let ctx = make_ctx(false, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            Some("desc".to_string()),
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_auto_detect_gitlab_url_without_token_returns_error() {
        let (dir, repo, _, _) = setup_diverged_repo("source", "target");
        repo.remote("origin", "https://gitlab.com/owner/repo.git")
            .unwrap();

        let ctx = make_ctx(false, false, "ollama", None, None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            None,
            "origin".to_string(),
            None,
            Some("desc".to_string()),
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

        let ctx = make_ctx(false, false, "ollama", None, Some("fake-token"));
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            Some("noslash".to_string()),
            "origin".to_string(),
            None,
            Some("desc".to_string()),
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

        let ctx = make_ctx(false, false, "ollama", Some("fake-token"), None);
        let result = branch_to_branch(
            &ctx,
            "source".to_string(),
            "target".to_string(),
            Some(dir.path().to_str().unwrap().to_string()),
            Some("not-a-number".to_string()),
            "origin".to_string(),
            None,
            Some("desc".to_string()),
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(msg)) if msg.contains("numeric")));
    }
}
