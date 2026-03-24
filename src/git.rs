use std::collections::HashMap;
use std::path::{Path};

use async_trait::async_trait;
use git2::Repository;
use gitlab::api::{projects, AsyncQuery};
use gitlab::Gitlab as GitLabClient;
use octocrab::OctocrabBuilder;
use regex::Regex;

use crate::error::ShipItError;

/// Computes the next semantic version given a categorized-commit map and the
/// current version string. The version digits are extracted via regex, so
/// prefixes like `v`, `version`, `v `, etc. are ignored.
///
/// Bump rules (first match wins):
///   - any `features`     - **minor** bump, patch reset to 0
///   - any `bug_fixes`    - **patch** bump
///   - any other non-empty category (`infrastructure`, `docs`, `misc`) - **patch** bump
///   - nothing            - version unchanged
///
/// Always returns a bare `"MAJOR.MINOR.PATCH"` string, or `None` if no
/// `MAJOR.MINOR.PATCH` digits can be found in `current`.
pub(crate) fn next_version(
    commits: &HashMap<String, Vec<String>>,
    current: &str,
) -> Option<String> {
    let re = Regex::new(r"(\d+)\.(\d+)\.(\d+)").unwrap();
    let caps = re.captures(current)?;
    let major: u64 = caps[1].parse().ok()?;
    let minor: u64 = caps[2].parse().ok()?;
    let patch: u64 = caps[3].parse().ok()?;

    let has = |key: &str| commits.get(key).is_some_and(|v| !v.is_empty());

    let (new_minor, new_patch) = if has("features") {
        (minor + 1, 0)
    } else if has("bug_fixes") || has("infrastructure") || has("docs") || has("misc") {
        (minor, patch + 1)
    } else {
        (minor, patch)
    };

    Some(format!("{}.{}.{}", major, new_minor, new_patch))
}

/// Abstraction over the git hosting platform used by command functions.
///
/// Implemented by [`GitPlatform`] in production and by `MockPlatform` in tests.
/// The two methods cover every platform operation the commands need.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub(crate) trait Platform: Send + Sync {
    /// Opens a merge/pull request from `source` into `target` and returns the URL
    /// of the newly created request.
    async fn open_request(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
    ) -> Result<String, ShipItError>;

    /// Attempts to replace raw commit messages with richer descriptions fetched
    /// from the platform API. Messages that cannot be enriched are returned unchanged.
    async fn enrich_messages(&self, messages: &[String]) -> Vec<String>;
}

/// A git repository paired with its hosting platform client and a named source branch.
///
/// Created via [`TetheredGit::new`], which opens the repository, detects the platform
/// from the remote URL, verifies the source branch exists, and runs a preflight
/// `refresh` check (fetch + dirty/ahead/behind guards).
pub(crate) struct TetheredGit {
    /// Filesystem path to the working-tree root (used when shelling out to `git`).
    pub(crate) path: std::path::PathBuf,
    /// The open git2 repository handle.
    pub(crate) repo: Repository,
    /// Name of the git remote (e.g. `"origin"`).
    pub(crate) remote_name: String,
    /// Platform client used for API calls (GitHub or GitLab).
    pub(crate) platform: Box<dyn Platform>,
    /// Name of the source branch this instance is anchored to.
    pub(crate) source: String,
}

impl TetheredGit {

    /// Opens the repository at `path`, resolves the hosting platform from the remote URL,
    /// verifies `source` exists as a local branch, and performs a preflight [`refresh`].
    ///
    /// Returns an error if the repo, remote, or branch cannot be found, if the platform
    /// cannot be detected, or if the working tree is dirty / out of sync with the remote.
    pub(crate) async fn new(path: &Path, remote: &str, source: &str, domain: &str, token: &str, allow_dirty: bool, yes: bool) -> Result<TetheredGit, ShipItError> {
        let repo = Repository::open(path)
            .map_err(|e| ShipItError::Error(format!("Failed to find a git repo: {}", e)))?;
        tracing::debug!("Found a git repository at {:?}", path);

        // verify the remote exists and capture its URL before dropping the Remote object
        let remote_obj = repo.find_remote(remote)
            .map_err(|e| ShipItError::Error(format!("Failed to find a remote in the git repo: {}", e)))?;
        tracing::debug!("Found {:?} as a remote", remote_obj.name());
        let remote_url = remote_obj.url().map(|u| u.to_string()).unwrap();
        drop(remote_obj);

        // extract the owner/repo path from the remote URL for platform initialization
        let repo_path = {
            let p = if remote_url.starts_with("git@") {
                // SSH: git@host:owner/repo.git
                remote_url.split(':').nth(1).unwrap_or("")
            } else {
                // HTTPS: https://host/owner/repo.git
                let without_scheme = remote_url.split_once("//").map(|x| x.1).unwrap_or("");
                without_scheme.split_once('/').map(|x| x.1).unwrap_or("")
            };
            p.trim_end_matches(".git").to_string()
        };

        // use the remote URL to detect and construct the git platform
        let platform = GitPlatform::new(&remote_url, domain, token, &repo_path).await?;

        // verify the branch exists locally
        let branch = repo.find_branch(source, git2::BranchType::Local)
            .map_err(|e| ShipItError::Error(format!("Branch not found: {}", e)))?;
        tracing::debug!("Found {:?} as a branch", branch.name());
        drop(branch);

        let tethered_git = Self {
            path: path.to_path_buf(),
            repo,
            platform: Box::new(platform),
            remote_name: remote.to_string(),
            source: source.to_string(),
        };
        tethered_git.refresh(allow_dirty, yes)?;
        Ok(tethered_git)
    }

    /// Runs `git fetch <remote> <source>` as a subprocess to update the remote-tracking ref.
    /// Fetch failures are intentionally soft-ignored by [`refresh`] so an offline run still works.
    fn fetch(&self) -> Result<(), ShipItError> {
        tracing::info!("Fetching {} from {}", self.source, self.remote_name);
        let output = std::process::Command::new("git")
            .args(["fetch", &self.remote_name, &self.source])
            .current_dir(&self.path)
            .output()
            .map_err(|e| ShipItError::Error(format!("Failed to run git fetch: {}", e)))?;

        if !output.status.success() {
            return Err(ShipItError::Error(format!(
                "git fetch failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Fetches from the remote (best-effort), then verifies the working tree is in a safe
    /// state to proceed. The dirty check is skipped when `allow_dirty` or `yes` is true.
    /// When the branch is ahead or behind the remote the user is prompted to push/pull;
    /// if `yes` is true the prompt is skipped and the operation proceeds automatically.
    fn refresh(&self, allow_dirty: bool, yes: bool) -> Result<(), ShipItError> {
        let _ = self.fetch();

        if self.is_dirty()? && !allow_dirty && !yes {
            tracing::warn!("Working directory has uncommitted changes. Unsafe to continue!");
            return Err(ShipItError::Error("Working directory has uncommitted changes. Unsafe to continue! Clean up your uncommitted changes or add the --allow-dirty flag.".to_string()));
        }

        if self.needs_push()? {
            tracing::warn!("Local source branch is ahead of remote!");
            let confirmed = yes || crate::output::prompt_push(&self.source)
                .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
            if confirmed {
                self.push_branch()?;
            } else {
                return Err(ShipItError::Error("Aborted: push your local branch before continuing.".to_string()));
            }
        }

        if self.needs_pull()? {
            tracing::warn!("Remote is ahead of local source branch!");
            let confirmed = yes || crate::output::prompt_pull(&self.source)
                .map_err(|e| ShipItError::Error(format!("Failed to read input: {}", e)))?;
            if confirmed {
                self.pull_branch()?;
            } else {
                return Err(ShipItError::Error("Aborted: pull the latest changes before continuing.".to_string()));
            }
        }

        Ok(())
    }

    /// Pushes `self.source` to `self.remote_name`.
    fn push_branch(&self) -> Result<(), ShipItError> {
        let output = std::process::Command::new("git")
            .args(["push", &self.remote_name, &self.source])
            .current_dir(&self.path)
            .output()
            .map_err(|e| ShipItError::Error(format!("Failed to run git push: {}", e)))?;
        if !output.status.success() {
            return Err(ShipItError::Error(format!(
                "git push failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Pulls `self.source` from `self.remote_name`.
    fn pull_branch(&self) -> Result<(), ShipItError> {
        let output = std::process::Command::new("git")
            .args(["pull", &self.remote_name, &self.source])
            .current_dir(&self.path)
            .output()
            .map_err(|e| ShipItError::Error(format!("Failed to run git pull: {}", e)))?;
        if !output.status.success() {
            return Err(ShipItError::Error(format!(
                "git pull failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Returns `true` if the working tree has any uncommitted changes (`git status --porcelain`
    /// produces output).
    pub(crate) fn is_dirty(&self) -> Result<bool, ShipItError> {
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.path)
            .output()
            .map_err(|e| ShipItError::Error(format!("Failed to run git status: {}", e)))?;

        Ok(!output.stdout.is_empty())
    }

    /// Returns the remote-tracking ref path and the current OID of `self.source`.
    /// Used internally by [`needs_push`] and [`needs_pull`] to compute ahead/behind counts.
    fn _get_remote_ref(
        &self,
    ) -> Result<(String, git2::Oid), ShipItError> {
        let branch = self.repo.find_branch(&self.source, git2::BranchType::Local)
            .map_err(ShipItError::Git)?;
        let local_oid = branch.get().target()
            .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to get source branch oid")))?;
        Ok((format!("refs/remotes/{}/{}", self.remote_name, self.source), local_oid))
    }

    /// Returns `true` if `self.source` has commits not yet present on the remote-tracking ref,
    /// meaning a push is required before the branch can be used as a PR/tag source.
    pub(crate) fn needs_push(
        &self,
    ) -> Result<bool, ShipItError> {
        let (remote_ref, local_oid) = self._get_remote_ref()?;
        match self.repo.find_reference(&remote_ref) {
            Ok(remote_ref) => match remote_ref.target() {
                Some(remote_oid) => {
                    let (ahead, _) = self.repo.graph_ahead_behind(local_oid, remote_oid)
                        .map_err(ShipItError::Git)?;
                    Ok(ahead > 0)
                }
                None => Ok(true),
            },
            Err(_) => Ok(true),
        }
    }

    /// Returns `true` if the remote-tracking ref has commits not yet present on `self.source`,
    /// meaning a pull is required to bring the local branch up to date.
    pub(crate) fn needs_pull(
        &self,
    ) -> Result<bool, ShipItError> {
        let (remote_ref, local_oid) = self._get_remote_ref()?;
        match self.repo.find_reference(&remote_ref) {
            Ok(remote_ref) => match remote_ref.target() {
                Some(remote_oid) => {
                    let (_, behind) = self.repo.graph_ahead_behind(local_oid, remote_oid)
                        .map_err(ShipItError::Git)?;
                    Ok(behind > 0)
                }
                None => Ok(false),
            },
            Err(_) => Ok(false),
        }
    }

    // POST MVP TODO: refactor only_merges in to merges and commits bools for more flexible
    // message parsing
    /// Collects commits on `self.source` since `target`, optionally filtering to merge commits only.
    pub(crate) fn collect_commits(&self, target: &str, only_merges: &bool) -> Result<Vec<git2::Oid>, ShipItError> {
        let target = self.repo.find_branch(target, git2::BranchType::Local).map_err(ShipItError::Git)?;
        let target_oid = target
            .get()
            .target()
            .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to find a valid commit for the target branch!")))?;

        let target_oid_on_source = self.repo.find_commit(target_oid).unwrap();

        let mut revwalk = self.repo.revwalk().map_err(ShipItError::Git)?;
        let full_ref = format!("refs/heads/{}", self.source);
        revwalk.push_ref(&full_ref).map_err(ShipItError::Git)?;

        let target_oid_hash = target_oid_on_source.id();
        revwalk.hide(target_oid_hash).map_err(ShipItError::Git)?;

        let mut commits = Vec::new();
        for oid in revwalk {
            commits.push(oid.map_err(ShipItError::Git)?);
        }

        let commits: Vec<_> = if *only_merges {
            commits.into_iter().filter(|oid| {
                self.repo.find_commit(*oid)
                    .map(|c| c.parent_count() > 1)
                    .unwrap_or(false)
            }).collect()
        } else {
            commits
        };

        Ok(commits)
    }

    /// Collects commits on `branch` since `tag_name`, optionally filtering to merge commits only.
    pub(crate) fn collect_commits_since_tag(
        &self,
        tag_name: &str,
        only_merges: bool,
    ) -> Result<Vec<git2::Oid>, ShipItError> {
        let tag_ref = format!("refs/tags/{}", tag_name);
        let tag_reference = self.repo.find_reference(&tag_ref).map_err(ShipItError::Git)?;
        let tag_commit = tag_reference.peel_to_commit().map_err(ShipItError::Git)?;
        let tag_oid = tag_commit.id();

        let mut revwalk = self.repo.revwalk().map_err(ShipItError::Git)?;
        let branch_ref = format!("refs/heads/{}", self.source);
        revwalk.push_ref(&branch_ref).map_err(ShipItError::Git)?;
        revwalk.hide(tag_oid).map_err(ShipItError::Git)?;

        let mut commits = Vec::new();
        for oid in revwalk {
            commits.push(oid.map_err(ShipItError::Git)?);
        }

        let commits = if only_merges {
            commits
                .into_iter()
                .filter(|oid| {
                    self.repo.find_commit(*oid)
                        .map(|c| c.parent_count() > 1)
                        .unwrap_or(false)
                })
                .collect()
        } else {
            commits
        };

        Ok(commits)
    }

    /// Collects commit messages on `self.source` since `target`, optionally filtering to merge
    /// commits only. Each entry is `"<message> <oid>"` so callers can correlate messages back
    /// to commits when enriching via the platform API.
    pub(crate) fn collect_messages(&self, target: &str, only_merges: &bool) -> Result<Vec<String>, ShipItError> {
        let commits = self.collect_commits(target, only_merges)?;
        let mut messages = Vec::new();
        for commit in commits {
            let release_oid = self.repo.find_commit(commit).unwrap();
            let msg = release_oid
                .message()
                .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to unwrap the message of a release commit!")))?
                .to_string();
            messages.push(format!("{} {}", msg, release_oid.id()));
        }
        Ok(messages)
    }

    /// Collects commit messages on `self.source` since `tag_name`, optionally filtering to merges.
    pub(crate) fn collect_messages_since_tag(
        &self,
        tag_name: &str,
        only_merges: bool,
    ) -> Result<Vec<String>, ShipItError> {
        let commits = self.collect_commits_since_tag(tag_name, only_merges)?;
        let mut messages = Vec::new();
        for oid in commits {
            let commit = self.repo.find_commit(oid).unwrap();
            let msg = commit
                .message()
                .ok_or_else(|| ShipItError::Git(git2::Error::from_str("Failed to unwrap commit message")))?
                .to_string();
            messages.push(format!("{} {}", msg, commit.id()));
        }
        Ok(messages)
    }

    /// Pushes a local tag to the remote.
    pub(crate) fn push_tag(&self, tag_name: &str) -> Result<(), ShipItError> {
        tracing::info!("Pushing tag {} to {}", tag_name, self.remote_name);
        let refspec = format!("refs/tags/{}", tag_name);
        let output = std::process::Command::new("git")
            .args(["push", &self.remote_name, &refspec])
            .current_dir(&self.path)
            .output()
            .map_err(|e| ShipItError::Error(format!("Failed to run git push: {}", e)))?;

        if !output.status.success() {
            return Err(ShipItError::Error(format!(
                "git push failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Creates an annotated local tag pointing at `branch_oid`.
    pub(crate) fn create_local_tag(
        &self,
        tag_name: &str,
        branch_oid: git2::Oid,
        notes: &str,
    ) -> Result<(), ShipItError> {
        let obj = self.repo
            .find_object(branch_oid, Some(git2::ObjectType::Commit))
            .map_err(ShipItError::Git)?;
        let sig = git2::Signature::now("shipit", "shipit@gitshipit.net")
            .map_err(ShipItError::Git)?;
        self.repo.tag(tag_name, &obj, &sig, notes, false)
            .map_err(ShipItError::Git)?;
        Ok(())
    }

    /// Finds the most recent tag reachable from (and an ancestor of) `branch_oid`.
    pub(crate) fn get_latest_tag(&self) -> Result<String, ShipItError> {
        let mut best: Option<(i64, String)> = None;
        let branch = self.repo.revparse_single(&self.source).map_err(ShipItError::Git)?;

        let refs = self.repo.references().map_err(ShipItError::Git)?;
        for reference in refs {
            let reference = reference.map_err(ShipItError::Git)?;
            if !reference.is_tag() {
                continue;
            }

            let tag_commit = match reference.peel_to_commit() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let tag_oid = tag_commit.id();

            let merge_base = match self.repo.merge_base(branch.id(), tag_oid) {
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

}

/// Scans each commit message for a conventional commit type prefix and groups
/// commits into `"features"`, `"bug_fixes"`, `"infrastructure"`, `"docs"`, or
/// `"misc"`. All five keys are always present in the returned map.
pub(crate) fn categorize_commits(commits: &[&str]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = ["features", "bug_fixes", "infrastructure", "docs", "misc"]
        .iter()
        .map(|&k| (k.to_string(), Vec::new()))
        .collect();

    for &commit in commits {
        let category = find_commit_category(commit);
        map.entry(category.to_string()).or_default().push(commit.to_string());
    }

    map
}

fn find_commit_category(text: &str) -> &'static str {
    for token in text.split_whitespace() {
        let token = token.trim_start_matches('[');
        let commit_type = token.split(['(', ':']).next().unwrap_or("").trim().to_lowercase();
        match commit_type.as_str() {
            "feat" => return "features",
            "fix" | "bug" => return "bug_fixes",
            "ci" | "infra" | "build" | "chore" | "perf" | "refactor" | "style" | "test" => {
                return "infrastructure"
            }
            "docs" => return "docs",
            _ => {}
        }
    }
    "misc"
}

/// Formats a categorized commit map into a markdown string with `##` headings.
/// Empty categories are omitted. Keys are sorted alphabetically.
pub(crate) fn generate_summary(commits: &HashMap<String, Vec<String>>) -> String {
    let mut keys: Vec<&String> = commits.keys().collect();
    keys.sort();

    let mut sections: Vec<String> = Vec::new();
    for key in keys {
        let entries = &commits[key];
        if entries.is_empty() {
            continue;
        }
        let heading = key
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let mut section = format!("## {}\n", heading);
        for commit in entries {
            section.push_str(&format!("- {}\n", commit));
        }
        sections.push(section);
    }

    sections.join("\n")
}

/// Concrete git hosting platform, detected from the remote URL.
///
/// Wraps either a [`GitHub`] or [`GitLab`] client and implements [`Platform`]
/// by delegating to the appropriate variant.
pub(crate) enum GitPlatform {
    GitHub(GitHub),
    GitLab(GitLab),
}

impl GitPlatform {

    /// Detects the platform from `remote_url` (looks for `"github"` or `"gitlab"` in the URL),
    /// parses the `owner/repo` path, and constructs the matching client.
    ///
    /// Returns an error if the URL does not match a supported platform.
    pub(crate) async fn new(remote_url: &str, domain: &str, token: &str, path: &str) -> Result<Self, ShipItError> {
        if remote_url.contains("github") {
            Ok(GitPlatform::GitHub(GitHub::new(domain, token, path).await))
        } else if remote_url.contains("gitlab") {
            Ok(GitPlatform::GitLab(GitLab::new(domain, token, path).await))
        } else {
            Err(ShipItError::Error("Unable to detect a supported git platform!".to_string()))
        }
    }

    /// Delegates to the active platform variant to open a merge/pull request.
    pub(crate) async fn open_request(&self, source: &str, target: &str, title: &str, description: &str) -> Result<String, ShipItError> {
        match self {
            GitPlatform::GitHub(gh) => gh.open_request(source, target, title, description).await,
            GitPlatform::GitLab(gl) => gl.open_request(source, target, title, description).await,
        }
    }

    /// Fetches the title and URL for a merge/pull request by its numeric id.
    pub(crate) async fn get_request_info(&self, id: u64) -> Result<(String, String), ShipItError> {
        match self {
            GitPlatform::GitHub(gh) => gh.get_request_info(id).await,
            GitPlatform::GitLab(gl) => gl.get_request_info(id).await,
        }
    }

    /// Delegates to the active platform variant to parse a request ID from a commit message.
    pub(crate) fn parse_request_id(&self, message: &str) -> Option<u64> {
        match self {
            GitPlatform::GitHub(_) => GitHub::parse_request_id(message),
            GitPlatform::GitLab(_) => GitLab::parse_request_id(message),
        }
    }

    /// Attempts to replace each message with a richer title and link fetched via the platform
    /// API. If a message contains a recognizable request ID and the API call succeeds, the
    /// message is replaced with `"<title> - [#<id>](<url>)"`. Otherwise it is left unchanged.
    pub(crate) async fn enrich_messages(&self, messages: &[String]) -> Vec<String> {
        let mut enriched = Vec::with_capacity(messages.len());

        for msg in messages {
            let replacement = 'enrich: {
                if let Some(id) = self.parse_request_id(msg)
                    && let Ok((title, link)) = self.get_request_info(id).await {
                        break 'enrich format!("{} - [#{}]({})", title, id, link);
                    }
                msg.to_string()
            };
            enriched.push(replacement);
        }
        enriched
    }

}

/// GitHub platform client, wrapping the Octocrab library.
pub(crate) struct GitHub {
    /// GitHub instance hostname (e.g. `"github.com"` or a GitHub Enterprise host).
    pub domain: String,
    /// Personal access token used for API authentication.
    pub token: String,
    /// Numeric GitHub repository ID, resolved from `owner/repo` during construction.
    pub project_id: u64,
}

impl GitHub {

    /// Constructs a `GitHub` client by resolving the numeric repo ID from `path` (`"owner/repo"`).
    async fn new(domain: &str, token: &str, path: &str) -> Self {
        let mut platform = Self {
            domain: domain.to_string(),
            token: token.to_string(),
            project_id: 0
        };
        platform.project_id = platform.parse_project_id(path).await.expect("Project existence already verified!");

        platform
    }

    /// Creates a GitHub pull request from `source` into `target` and returns its HTML URL.
    /// Supports GitHub Enterprise by configuring a custom base URI when `domain` is not
    /// `"github.com"`.
    async fn open_request(&self, source: &str, target: &str, title: &str, description: &str) -> Result<String, ShipItError> {
        let mut builder = OctocrabBuilder::new().personal_token(self.token.clone());

        if self.domain != "github.com" {
            let base_uri = format!("https://{}/api/v3/", self.domain);
            builder = builder.base_uri(base_uri)
                .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
        }

        let octo = builder.build().map_err(|e| ShipItError::GitHub(Box::new(e)))?;

        let repo = octo.repos_by_id(self.project_id).get().await.map_err(|e| ShipItError::GitHub(Box::new(e)))?;
        let owner = repo.owner.ok_or("No owner found").map_err(|e| ShipItError::Error(e.to_string()))?.login;
        let pr = octo
            .pulls(&owner, &repo.name)
            .create(title, source, target)
            .body(description)
            .send()
            .await
            .map_err(|e| ShipItError::GitHub(Box::new(e)))?;

        let url = pr.html_url
            .ok_or_else(|| ShipItError::Error("Failed to get pr url from GitHub response".to_string()))?;

        Ok(url.to_string())
    }

    /// Fetches the title and HTML URL of a GitHub pull request.
    ///
    /// Builds an Octocrab client from the supplied credentials, then calls
    /// `GET /repos/{self.owner}/{self.repo}/pulls/{id}`.
    async fn get_request_info(&self, id: u64) -> Result<(String, String), ShipItError> {
        let mut builder = OctocrabBuilder::new().personal_token(self.token.to_string());
        if self.domain != "github.com" {
            let base_uri = format!("https://{}/api/v3/", self.domain);
            builder = builder
                .base_uri(base_uri)
                .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
        }
        let octo = builder.build().map_err(|e| ShipItError::GitHub(Box::new(e)))?;
        let repo = octo.repos_by_id(self.project_id).get().await.map_err(|e| ShipItError::GitHub(Box::new(e)))?;
        let owner = repo.owner.ok_or("No owner found").map_err(|e| ShipItError::Error(e.to_string()))?.login;

        let pr = octo
            .pulls(&owner, &repo.name)
            .get(id)
            .await
            .map_err(|e| ShipItError::GitHub(Box::new(e)))?;

        let title = pr
            .title
            .ok_or_else(|| ShipItError::Error("PR missing title".to_string()))?;
        let url = pr
            .html_url
            .ok_or_else(|| ShipItError::Error("PR missing url".to_string()))?;

        Ok((title, url.to_string()))
    }

    /// Tries to parse a GitHub PR number from a merge commit message.
    ///
    /// Recognizes:
    /// - `"Merge pull request #123 from owner/branch"` (standard GitHub merge commit)
    /// - `"feat: something (#123)"` (squash-merge title)
    fn parse_request_id(message: &str) -> Option<u64> {
        // Standard merge commit: "Merge pull request #NNN"
        if let Some(idx) = message.find("pull request #") {
            let rest = &message[idx + "pull request #".len()..];
            let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !num_str.is_empty() {
                return num_str.parse().ok();
            }
        }
        // Squash-merge title: "(#NNN)"
        if let Some(idx) = message.find("(#") {
            let rest = &message[idx + 2..];
            let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !num_str.is_empty()
                && rest.chars().nth(num_str.len()) == Some(')') {
                    return num_str.parse().ok();
                }
        }
        None
    }

    /// Queries the GitHub API to resolve an `"owner/repo"` path to its numeric repository ID.
    async fn parse_project_id(&self, path: &str) -> Result<u64, ShipItError> {
        let octo = OctocrabBuilder::new().personal_token(self.token.clone()).build().map_err(|e| ShipItError::GitHub(Box::new(e)))?;
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 2 {
            return Err(ShipItError::Error("Path must be in 'owner/repo' format".to_string()));
        }
        let owner = parts[0];
        let repo_name = parts[1];

        let repo = octo.repos(owner, repo_name).get().await.map_err(|e| ShipItError::GitHub(Box::new(e)))?;

        Ok(repo.id.0)
    }

}

/// GitLab platform client, wrapping the `gitlab` crate.
pub(crate) struct GitLab {
    /// GitLab instance hostname (e.g. `"gitlab.com"` or a self-hosted instance).
    pub domain: String,
    /// Personal access token used for API authentication.
    pub token: String,
    /// Numeric GitLab project ID, resolved from the project path during construction.
    pub project_id: u64,
}

impl GitLab {

    /// Constructs a `GitLab` client by resolving the numeric project ID from `path`.
    async fn new(domain: &str, token: &str, path: &str) -> Self {
        let mut platform = Self {
            domain: domain.to_string(),
            token: token.to_string(),
            project_id: 0
        };
        platform.project_id = platform.parse_project_id(path).await.expect("Project existence already verified!");

        platform
    }

    /// Creates a GitLab merge request from `source` into `target` and returns its web URL.
    /// The source branch is configured with `remove_source_branch: true`.
    async fn open_request(&self, source: &str, target: &str, title: &str, description: &str) -> Result<String, ShipItError> {
        let client = GitLabClient::builder(&self.domain, &self.token)
            .build_async()
            .await
            .map_err(|e| ShipItError::Gitlab(Box::new(e)))?;

        let create_mr = projects::merge_requests::CreateMergeRequest::builder()
            .project(self.project_id)
            .source_branch(source)
            .target_branch(target)
            .title(title)
            .description(description)
            .remove_source_branch(true)
            .build()
            .map_err(|e| ShipItError::Error(format!("Failed to build a GitLab mr: {}", e)))?;

        let merge_request: serde_json::Value = create_mr
            .query_async(&client)
            .await
            .map_err(|e| ShipItError::Error(format!("Failed to create a GitLab merge request: {}", e)))?;

        merge_request["web_url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ShipItError::Error("Failed to get mr url from GitLab response".to_string()))
    }

    /// Fetches the title and web URL of a GitLab merge request by id.
    ///
    /// Builds a GitLab async client from the supplied credentials, then calls
    /// `GET /projects/{self.project_id}/merge_requests/{id}`.
    async fn get_request_info(&self, id: u64) -> Result<(String, String), ShipItError> {
        use gitlab::api::projects::merge_requests::MergeRequest;

        let client = GitLabClient::builder(&self.domain, &self.token)
            .build_async()
            .await
            .map_err(|e| ShipItError::Gitlab(Box::new(e)))?;

        let endpoint = MergeRequest::builder()
            .project(self.project_id)
            .merge_request(id)
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

    /// Tries to parse a GitLab MR ID from a merge commit message.
    ///
    /// Recognizes `"See merge request group/project!123"` which GitLab appends to
    /// the body of every merge commit.
    fn parse_request_id(message: &str) -> Option<u64> {
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

    /// Parses the project path from the remote url and queries the GitLab API
    /// to resolve the numeric project id.
    async fn parse_project_id(
        &self,
        path: &str,
    ) -> Result<u64, ShipItError> {
        let client = GitLabClient::builder(&self.domain, &self.token)
            .build_async()
            .await
            .map_err(|e| ShipItError::Gitlab(Box::new(e)))?;

        let endpoint = projects::Project::builder()
            .project(path.to_string())
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

}

#[async_trait]
impl Platform for GitPlatform {
    async fn open_request(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
    ) -> Result<String, ShipItError> {
        GitPlatform::open_request(self, source, target, title, description).await
    }

    async fn enrich_messages(&self, messages: &[String]) -> Vec<String> {
        GitPlatform::enrich_messages(self, messages).await
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod test_helpers {
    use std::path::Path;
    use git2::{Repository, Signature, Time};

    /// Initialise a bare repo at `bare_path` and a plain work repo at `work_path`
    /// with `bare_path` registered as the "origin" remote.
    ///
    /// HEAD is explicitly set to `refs/heads/master` so tests are consistent
    /// regardless of the system's `init.defaultBranch` setting.
    pub(crate) fn init_repo_with_remote(work_path: &Path, bare_path: &Path) -> Repository {
        Repository::init_bare(bare_path).unwrap();
        let repo = Repository::init(work_path).unwrap();
        repo.set_head("refs/heads/master").unwrap();
        repo.remote("origin", bare_path.to_str().unwrap()).unwrap();
        repo
    }

    /// Create a commit on HEAD (advancing the current branch) and return its OID.
    pub(crate) fn make_commit(repo: &Repository, message: &str) -> git2::Oid {
        let sig = Signature::new("test", "test@test.com", &Time::new(1_000_000, 0)).unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parents: Vec<git2::Commit> = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| repo.find_commit(oid).ok())
            .into_iter()
            .collect();
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs).unwrap()
    }

    /// Create an annotated tag pointing at the commit with the given OID.
    pub(crate) fn make_tag(repo: &Repository, tag_name: &str, oid: git2::Oid) {
        let sig = Signature::new("test", "test@test.com", &Time::new(1_000_000, 0)).unwrap();
        let obj = repo.find_object(oid, Some(git2::ObjectType::Commit)).unwrap();
        repo.tag(tag_name, &obj, &sig, "release", false).unwrap();
    }

    /// Construct a `TetheredGit` directly, bypassing `::new` (no network, no refresh).
    pub(crate) fn make_tethered_git(
        repo: Repository,
        path: std::path::PathBuf,
        source: &str,
        platform: Box<dyn crate::git::Platform>,
    ) -> crate::git::TetheredGit {
        crate::git::TetheredGit {
            path,
            repo,
            remote_name: "origin".to_string(),
            platform,
            source: source.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── categorize_commits ────────────────────────────────────────────────────

    #[test]
    fn test_categorize_all_keys_always_present() {
        let map = categorize_commits(&[]);
        for key in &["features", "bug_fixes", "infrastructure", "docs", "misc"] {
            assert!(map.contains_key(*key), "missing key: {key}");
        }
    }

    #[test]
    fn test_categorize_feat_prefix() {
        let map = categorize_commits(&["feat: add login"]);
        assert_eq!(map["features"], vec!["feat: add login"]);
        assert!(map["misc"].is_empty());
    }

    #[test]
    fn test_categorize_fix_and_bug_prefix() {
        let map = categorize_commits(&["fix: crash on start", "bug: off-by-one"]);
        assert_eq!(map["bug_fixes"].len(), 2);
    }

    #[test]
    fn test_categorize_infrastructure_prefixes() {
        let commits = ["ci: update pipeline", "chore: bump deps", "build: makefile", "perf: cache"];
        let map = categorize_commits(&commits);
        assert_eq!(map["infrastructure"].len(), 4);
    }

    #[test]
    fn test_categorize_docs_prefix() {
        let map = categorize_commits(&["docs: update readme"]);
        assert_eq!(map["docs"], vec!["docs: update readme"]);
    }

    #[test]
    fn test_categorize_unknown_prefix_goes_to_misc() {
        let map = categorize_commits(&["random commit message", "wip: something"]);
        assert_eq!(map["misc"].len(), 2);
        assert!(map["features"].is_empty());
    }

    // ── generate_summary ─────────────────────────────────────────────────────

    #[test]
    fn test_generate_summary_formats_sections() {
        let map = categorize_commits(&["feat: login", "fix: crash"]);
        let out = generate_summary(&map);
        assert!(out.contains("## Bug Fixes"), "missing Bug Fixes heading");
        assert!(out.contains("## Features"), "missing Features heading");
        assert!(out.contains("- feat: login"));
        assert!(out.contains("- fix: crash"));
    }

    #[test]
    fn test_generate_summary_omits_empty_categories() {
        let map = categorize_commits(&["feat: only features"]);
        let out = generate_summary(&map);
        assert!(!out.contains("## Bug Fixes"));
        assert!(!out.contains("## Misc"));
    }

    #[test]
    fn test_generate_summary_empty_input_is_empty_string() {
        let map = categorize_commits(&[]);
        assert_eq!(generate_summary(&map), "");
    }

    // ── next_version ─────────────────────────────────────────────────────────

    #[test]
    fn test_next_version_feature_bumps_minor_resets_patch() {
        let map = categorize_commits(&["feat: new thing"]);
        assert_eq!(next_version(&map, "v1.2.3"), Some("1.3.0".to_string()));
    }

    #[test]
    fn test_next_version_bug_fix_bumps_patch() {
        let map = categorize_commits(&["fix: crash"]);
        assert_eq!(next_version(&map, "v1.2.3"), Some("1.2.4".to_string()));
    }

    #[test]
    fn test_next_version_infrastructure_bumps_patch() {
        let map = categorize_commits(&["ci: update pipeline"]);
        assert_eq!(next_version(&map, "v1.2.3"), Some("1.2.4".to_string()));
    }

    #[test]
    fn test_next_version_no_commits_unchanged() {
        let map = categorize_commits(&[]);
        assert_eq!(next_version(&map, "v1.2.3"), Some("1.2.3".to_string()));
    }

    #[test]
    fn test_next_version_feature_beats_bug_fix() {
        let map = categorize_commits(&["feat: something", "fix: crash"]);
        assert_eq!(next_version(&map, "v2.1.5"), Some("2.2.0".to_string()));
    }

    #[test]
    fn test_next_version_returns_none_for_invalid_tag() {
        let map = categorize_commits(&[]);
        assert_eq!(next_version(&map, "not-a-version"), None);
    }

    #[test]
    fn test_next_version_ignores_v_prefix() {
        let map = categorize_commits(&["fix: patch"]);
        assert_eq!(next_version(&map, "version 0.9.1"), Some("0.9.2".to_string()));
    }
}
