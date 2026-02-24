use crate::cli::T2rArgs;
use crate::context::Context;
use crate::error::ShipItError;
use crate::common::{
    create_platform_release, fetch_latest_platform_tag, open_repo, read_tag_annotation,
    resolve_project_id, resolve_remote_url,
};

pub async fn tag_to_release(ctx: &Context, args: T2rArgs) -> Result<(), ShipItError> {
    let repo = open_repo(args.dir);

    // When --tag is not provided we must fetch from the platform.
    // Cache the resolution so we don't repeat API calls after the dryrun check.
    let (tag_name, platform_info) = match args.tag {
        Some(t) => (t, None),
        None => {
            let remote_url = resolve_remote_url(&repo, &args.remote)?;
            let (is_github, is_gitlab) =
                (remote_url.contains("github"), remote_url.contains("gitlab"));
            let resolved_id =
                resolve_project_id(ctx, &remote_url, args.id.clone(), is_github, is_gitlab)
                    .await?;
            let tag =
                fetch_latest_platform_tag(ctx, &resolved_id, is_github, is_gitlab).await?;
            tracing::info!("Auto-detected latest tag: {}", tag);
            (tag, Some((resolved_id, is_github, is_gitlab)))
        }
    };

    let annotation = read_tag_annotation(&repo, &tag_name)?;

    crate::output::print_content("The release name is:", &tag_name);
    crate::output::print_content("The release notes are:", &annotation);

    if ctx.settings.shipit.dryrun {
        crate::output::print_dryrun("create the release");
        return Ok(());
    }

    let (resolved_id, is_github, is_gitlab) = match platform_info {
        Some(info) => info,
        None => {
            let remote_url = resolve_remote_url(&repo, &args.remote)?;
            let (is_github, is_gitlab) =
                (remote_url.contains("github"), remote_url.contains("gitlab"));
            let resolved_id =
                resolve_project_id(ctx, &remote_url, args.id, is_github, is_gitlab).await?;
            (resolved_id, is_github, is_gitlab)
        }
    };

    let url =
        create_platform_release(ctx, &resolved_id, &tag_name, &annotation, is_github, is_gitlab)
            .await?;
    crate::output::print_url(&url);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::T2rArgs;
    use crate::context::Context;
    use crate::settings::{
        GithubSettings, GitlabSettings, OllamaSettings, Settings, ShipitSettings,
    };
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    fn make_ctx(dryrun: bool) -> Context {
        Context {
            settings: Settings {
                shipit: ShipitSettings {
                    agent: "".to_string(),
                    commits: "custom".to_string(),
                    dryrun,
                },
                ollama: OllamaSettings::default(),
                gitlab: GitlabSettings {
                    domain: "gitlab.com".to_string(),
                    token: None,
                },
                github: GithubSettings {
                    domain: "github.com".to_string(),
                    token: None,
                },
            },
        }
    }

    fn setup_annotated_tag_repo(tag_name: &str, annotation: &str) -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let tagger = Signature::now("tagger", "tagger@example.com").unwrap();

        {
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let oid = repo
                .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                .unwrap();
            let obj = repo.find_object(oid, Some(git2::ObjectType::Commit)).unwrap();
            repo.tag(tag_name, &obj, &tagger, annotation, false).unwrap();
        }

        (dir, repo)
    }

    #[tokio::test]
    async fn test_missing_tag_locally_returns_git_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();

        let ctx = make_ctx(false);
        let result = tag_to_release(
            &ctx,
            T2rArgs {
                tag: Some("v9.9.9".to_string()),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Git(_))));
    }

    #[tokio::test]
    async fn test_lightweight_tag_returns_error() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("t", "t@t.com").unwrap();

        {
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let oid = repo
                .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
            // Create a lightweight tag (no annotation)
            let obj = repo.find_object(oid, Some(git2::ObjectType::Commit)).unwrap();
            repo.tag_lightweight("v1.0.0-light", &obj, false).unwrap();
        }

        let ctx = make_ctx(false);
        let result = tag_to_release(
            &ctx,
            T2rArgs {
                tag: Some("v1.0.0-light".to_string()),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_dryrun_with_annotated_tag_exits_without_creating_release() {
        let (dir, _repo) = setup_annotated_tag_repo("v1.0.0", "Initial release\n\nGreat stuff.");

        let ctx = make_ctx(true);
        let result = tag_to_release(
            &ctx,
            T2rArgs {
                tag: Some("v1.0.0".to_string()),
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_tag_and_no_platform_token_returns_error() {
        let (dir, repo) = setup_annotated_tag_repo("v1.0.0", "notes");
        repo.remote("origin", "https://github.com/owner/repo.git").unwrap();

        let ctx = make_ctx(false); // no token - fetch_latest_platform_tag will fail
        let result = tag_to_release(
            &ctx,
            T2rArgs {
                tag: None, // triggers platform fetch
                dir: Some(dir.path().to_str().unwrap().to_string()),
                remote: "origin".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(matches!(result, Err(ShipItError::Error(_))));
    }
}
