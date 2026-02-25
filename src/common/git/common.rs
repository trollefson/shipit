use std::collections::HashMap;

use regex::Regex;

use crate::error::ShipItError;

pub(crate) trait GitPlatform {
    async fn open(&self, source: &str, target: &str, title: &str, description: &str) -> Result<String, ShipItError>;
}

pub(crate) async fn open_merge_request<P: GitPlatform>(
    platform: &P,
    source: &str,
    target: &str,
    title: &str,
    description: &str,
) -> Result<String, ShipItError> {
    platform.open(source, target, title, description).await
}

/// Extracts the repository path from a git remote url.
pub(crate) fn extract_repo_path(url: &str) -> Option<String> {
    let path = if url.starts_with("git@") {
        // SSH: git@host:path.git
        url.split(':').nth(1)?
    } else {
        // HTTPS: https://host/path.git  or  ssh://git@host/path.git
        let without_scheme = url.splitn(2, "//").nth(1)?;
        without_scheme.splitn(2, '/').nth(1)?
    };
    let path = path.trim_end_matches(".git");
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Formats a map of categorized commits into a markdown string.
/// Each key becomes a `##` subheading with its commits listed as bullet points.
/// Keys are sorted alphabetically and empty categories are omitted.
pub(crate) fn format_categorized_commits(commits: &HashMap<String, Vec<String>>) -> String {
    let mut keys: Vec<&String> = commits.keys().collect();
    keys.sort();

    let mut sections: Vec<String> = Vec::new();
    for key in keys {
        let entries = &commits[key];
        if entries.is_empty() {
            continue;
        }
        // replace '_' with ' ' and uppercase each heading
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

/// Scans every whitespace-separated token in `text` for the first recognised
/// conventional-commit type and returns the corresponding category key.
///
/// A leading `[` is stripped from each token before matching so that enriched
/// merge-commit messages of the form `[feat: add something (#123)](url)` are
/// handled correctly.  Multiline messages are also supported because
/// `split_whitespace` spans newlines.
fn find_commit_category(text: &str) -> &'static str {
    for token in text.split_whitespace() {
        let token = token.trim_start_matches('[');
        let commit_type = String::from(token.split(['(', ':']).next().unwrap_or("").trim()).to_lowercase();
        match commit_type.as_str() {
            "feat" => return "features",
            "fix" | "bug"  => return "bug_fixes",
            "ci" | "infra" | "build" | "chore" | "perf" | "refactor" | "style" | "test" => {
                return "infrastructure"
            }
            "docs" => return "docs",
            _ => {}
        }
    }
    "misc"
}

/// Categorizes conventional commits into features, bug fixes, infrastructure, docs, and misc.
///
/// The key for each category in the returned map is one of:
/// `"features"`, `"bug_fixes"`, `"infrastructure"`, `"docs"`, `"misc"`.
///
/// The conventional-commit type is searched across the **entire** message, not
/// just its first token, so enriched merge-commit messages (e.g.
/// `[feat: add something (#123)](url)`) and multiline messages are handled
/// correctly.
pub(crate) fn categorize_commits(commits: &[&str]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = [
        "features",
        "bug_fixes",
        "infrastructure",
        "docs",
        "misc",
    ]
    .iter()
    .map(|&k| (k.to_string(), Vec::new()))
    .collect();

    for &commit in commits {
        let category = find_commit_category(commit);
        map.entry(category.to_string()).or_default().push(commit.to_string());
    }

    map
}

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

/// Finds the most recent tag reachable from (and an ancestor of) `branch_oid`.
pub(crate) fn find_most_recent_tag(repo: &git2::Repository, branch_oid: git2::Oid) -> Result<String, ShipItError> {
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
pub(crate) fn collect_commits_since_tag(
    repo: &git2::Repository,
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

/// Reads the annotation message from a local annotated tag.
///
/// Returns an error if the tag does not exist or is a lightweight tag (no annotation).
pub(crate) fn read_tag_annotation(repo: &git2::Repository, tag_name: &str) -> Result<String, ShipItError> {
    let tag_ref_name = format!("refs/tags/{}", tag_name);
    let obj = repo
        .revparse_single(&tag_ref_name)
        .map_err(|e| ShipItError::Git(e))?;
    let tag = obj.as_tag().ok_or_else(|| {
        ShipItError::Error(format!(
            "Tag '{}' is a lightweight tag with no annotation message",
            tag_name
        ))
    })?;
    Ok(tag.message().unwrap_or("").to_string())
}

/// Creates an annotated local tag pointing at `branch_oid`.
pub(crate) fn create_local_tag(
    repo: &git2::Repository,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ShipItError;

    // ── extract_repo_path ─────────────────────────────────────────────────────

    #[test]
    fn test_extract_repo_path_ssh_url() {
        assert_eq!(
            extract_repo_path("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_extract_repo_path_returns_none_for_empty_string() {
        assert_eq!(extract_repo_path(""), None);
    }

    #[test]
    fn test_extract_repo_path_returns_none_for_empty_ssh_path() {
        assert_eq!(extract_repo_path("git@github.com:"), None);
    }

    // ── open_merge_request ────────────────────────────────────────────────────

    struct MockPlatformSuccess;
    struct MockPlatformError;

    impl GitPlatform for MockPlatformSuccess {
        async fn open(&self, _: &str, _: &str, _: &str, _: &str) -> Result<String, ShipItError> {
            Ok("https://github.com/owner/repo/pull/1".to_string())
        }
    }

    impl GitPlatform for MockPlatformError {
        async fn open(&self, _: &str, _: &str, _: &str, _: &str) -> Result<String, ShipItError> {
            Err(ShipItError::Error("platform returned an error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_open_merge_request_success() {
        let result = open_merge_request(&MockPlatformSuccess, "feat", "main", "feat to main", "desc").await;
        assert_eq!(result.unwrap(), "https://github.com/owner/repo/pull/1");
    }

    #[tokio::test]
    async fn test_open_merge_request_propagates_platform_error() {
        let result = open_merge_request(&MockPlatformError, "feat", "main", "feat to main", "desc").await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    // ── find_commit_category ──────────────────────────────────────────────────

    #[test]
    fn test_find_commit_category_bug_fixes() {
        let _result = find_commit_category("See merge request endpoint/project_name!1 ec42387abc\n- Merge branch 'feat' into 'main'\n\nBUG: format the message\n\nmore info");
        assert!(matches!("bug_fixes", _result));
    }

    #[test]
    fn test_find_commit_category_infra() {
        let _result = find_commit_category("See merge request endpoint/project_name!1 ec42387abc\n- Merge branch 'feat' into 'main'\n\nINFRA: format the message\n\nmore info");
        assert!(matches!("infrastructure", _result));
    }

    // ── categorize_commits ────────────────────────────────────────────────────

    #[test]
    fn test_categorize_commits_empty_input() {
        let result = categorize_commits(&[]);
        assert!(result["features"].is_empty());
        assert!(result["bug_fixes"].is_empty());
        assert!(result["infrastructure"].is_empty());
        assert!(result["docs"].is_empty());
        assert!(result["misc"].is_empty());
    }

    #[test]
    fn test_categorize_commits_features() {
        let commits = vec!["feat: add login page", "feat(auth): add OAuth support"];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"], vec!["feat: add login page", "feat(auth): add OAuth support"]);
        assert!(result["bug_fixes"].is_empty());
        assert!(result["infrastructure"].is_empty());
        assert!(result["docs"].is_empty());
        assert!(result["misc"].is_empty());
    }

    #[test]
    fn test_categorize_commits_bug_fixes() {
        let commits = vec!["fix: resolve null pointer", "fix(ui): correct button alignment"];
        let result = categorize_commits(&commits);
        assert_eq!(result["bug_fixes"], vec!["fix: resolve null pointer", "fix(ui): correct button alignment"]);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_infrastructure() {
        let commits = vec![
            "ci: add github actions",
            "build: update dependencies",
            "chore: clean up temp files",
            "perf: cache expensive query",
            "refactor: extract helper",
            "style: fix trailing whitespace",
            "test: add unit tests",
        ];
        let result = categorize_commits(&commits);
        assert_eq!(result["infrastructure"].len(), 7);
        assert!(result["features"].is_empty());
        assert!(result["bug_fixes"].is_empty());
    }

    #[test]
    fn test_categorize_commits_docs() {
        let commits = vec!["docs: update README", "docs(api): add endpoint docs"];
        let result = categorize_commits(&commits);
        assert_eq!(result["docs"], vec!["docs: update README", "docs(api): add endpoint docs"]);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_misc() {
        let commits = vec!["wip: half done feature", "unknown: some commit"];
        let result = categorize_commits(&commits);
        assert_eq!(result["misc"], vec!["wip: half done feature", "unknown: some commit"]);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_mixed() {
        let commits = vec![
            "feat: add feature",
            "fix: fix bug",
            "docs: update docs",
            "ci: add workflow",
            "wip: in progress",
        ];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"], vec!["feat: add feature"]);
        assert_eq!(result["bug_fixes"], vec!["fix: fix bug"]);
        assert_eq!(result["docs"], vec!["docs: update docs"]);
        assert_eq!(result["infrastructure"], vec!["ci: add workflow"]);
        assert_eq!(result["misc"], vec!["wip: in progress"]);
    }

    // ── categorize_commits – enriched / nested conventional-commit headings ──

    #[test]
    fn test_categorize_commits_enriched_github_feat() {
        let commits = vec!["[feat: add login page (#42)](https://github.com/owner/repo/pull/42)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"].len(), 1);
        assert!(result["bug_fixes"].is_empty());
    }

    #[test]
    fn test_categorize_commits_enriched_github_fix() {
        let commits = vec!["[fix: resolve null pointer (#7)](https://github.com/owner/repo/pull/7)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["bug_fixes"].len(), 1);
        assert!(result["features"].is_empty());
    }

    #[test]
    fn test_categorize_commits_enriched_gitlab_feat() {
        let commits = vec!["[feat(api): expose new endpoint (!99)](https://gitlab.com/g/p/-/merge_requests/99)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["features"].len(), 1);
    }

    #[test]
    fn test_categorize_commits_enriched_ci_infrastructure() {
        let commits = vec!["[ci: add release workflow (#5)](https://github.com/owner/repo/pull/5)"];
        let result = categorize_commits(&commits);
        assert_eq!(result["infrastructure"].len(), 1);
    }

    #[test]
    fn test_categorize_commits_multiline_merge_commit_with_nested_type() {
        let msg = "Merge pull request #10 from owner/feature\n\nfeat: add something cool abc123";
        let result = categorize_commits(&[msg]);
        assert_eq!(result["features"].len(), 1);
        assert!(result["misc"].is_empty());
    }

    #[test]
    fn test_categorize_commits_multiline_fix_nested() {
        let msg = "Merge pull request #11 from owner/bugfix\n\nfix(ui): correct button alignment abc123";
        let result = categorize_commits(&[msg]);
        assert_eq!(result["bug_fixes"].len(), 1);
    }

    #[test]
    fn test_categorize_commits_no_conventional_type_falls_back_to_misc() {
        let msg = "Merge branch 'main' into develop abc123";
        let result = categorize_commits(&[msg]);
        assert_eq!(result["misc"].len(), 1);
        assert!(result["features"].is_empty());
    }

    // ── format_categorized_commits ────────────────────────────────────────────

    #[test]
    fn test_format_categorized_commits_empty_map() {
        let map: HashMap<String, Vec<String>> = HashMap::new();
        assert_eq!(format_categorized_commits(&map), "");
    }

    #[test]
    fn test_format_categorized_commits_skips_empty_categories() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec![]);
        map.insert("bug_fixes".to_string(), vec![]);
        assert_eq!(format_categorized_commits(&map), "");
    }

    #[test]
    fn test_format_categorized_commits_single_category_single_commit() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec!["feat: add login".to_string()]);
        assert_eq!(
            format_categorized_commits(&map),
            "## Features\n- feat: add login\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_single_category_multiple_commits() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert(
            "bug_fixes".to_string(),
            vec![
                "fix: resolve null pointer".to_string(),
                "fix(ui): correct alignment".to_string(),
            ],
        );
        assert_eq!(
            format_categorized_commits(&map),
            "## Bug Fixes\n- fix: resolve null pointer\n- fix(ui): correct alignment\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_single_category_three_commits() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert(
            "infrastructure".to_string(),
            vec![
                "ci: add github actions".to_string(),
                "build: update dependencies".to_string(),
                "chore: remove unused files".to_string(),
            ],
        );
        assert_eq!(
            format_categorized_commits(&map),
            "## Infrastructure\n- ci: add github actions\n- build: update dependencies\n- chore: remove unused files\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_multiple_categories_sorted() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec!["feat: add search".to_string()]);
        map.insert("bug_fixes".to_string(), vec!["fix: crash on startup".to_string()]);
        map.insert("docs".to_string(), vec!["docs: update README".to_string()]);
        assert_eq!(
            format_categorized_commits(&map),
            "## Bug Fixes\n- fix: crash on startup\n\n## Docs\n- docs: update README\n\n## Features\n- feat: add search\n"
        );
    }

    #[test]
    fn test_format_categorized_commits_mixed_empty_and_populated() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        map.insert("features".to_string(), vec!["feat: new flag".to_string()]);
        map.insert("misc".to_string(), vec![]);
        map.insert("docs".to_string(), vec!["docs: fix typo".to_string()]);
        assert_eq!(
            format_categorized_commits(&map),
            "## Docs\n- docs: fix typo\n\n## Features\n- feat: new flag\n"
        );
    }

    // ── next_version ──────────────────────────────────────────────────────────

    #[test]
    fn test_next_version_features_bumps_minor_and_resets_patch() {
        let map = categorize_commits(&["feat: add search"]);
        assert_eq!(next_version(&map, "1.3.7"), Some("1.4.0".to_string()));
    }

    #[test]
    fn test_next_version_bug_fixes_bumps_patch() {
        let map = categorize_commits(&["fix: null deref"]);
        assert_eq!(next_version(&map, "1.3.7"), Some("1.3.8".to_string()));
    }

    #[test]
    fn test_next_version_infrastructure_bumps_patch() {
        let map = categorize_commits(&["ci: add release workflow"]);
        assert_eq!(next_version(&map, "2.0.0"), Some("2.0.1".to_string()));
    }

    #[test]
    fn test_next_version_docs_bumps_patch() {
        let map = categorize_commits(&["docs: update README"]);
        assert_eq!(next_version(&map, "0.1.0"), Some("0.1.1".to_string()));
    }

    #[test]
    fn test_next_version_misc_bumps_patch() {
        let map = categorize_commits(&["wip: half done"]);
        assert_eq!(next_version(&map, "1.0.0"), Some("1.0.1".to_string()));
    }

    #[test]
    fn test_next_version_features_takes_priority_over_bug_fixes() {
        let map = categorize_commits(&["feat: new thing", "fix: broken thing"]);
        assert_eq!(next_version(&map, "1.3.7"), Some("1.4.0".to_string()));
    }

    #[test]
    fn test_next_version_no_commits_unchanged() {
        let map = categorize_commits(&[]);
        assert_eq!(next_version(&map, "1.3.7"), Some("1.3.7".to_string()));
    }

    #[test]
    fn test_next_version_invalid_current_returns_none() {
        let map = categorize_commits(&["feat: something"]);
        assert_eq!(next_version(&map, "not-a-version"), None);
    }

    #[test]
    fn test_next_version_partial_version_returns_none() {
        let map = categorize_commits(&["feat: something"]);
        assert_eq!(next_version(&map, "1.2"), None);
    }

    #[test]
    fn test_next_version_v_prefix() {
        let map = categorize_commits(&["feat: add search"]);
        assert_eq!(next_version(&map, "v1.3.7"), Some("1.4.0".to_string()));
    }

    #[test]
    fn test_next_version_version_prefix() {
        let map = categorize_commits(&["fix: crash"]);
        assert_eq!(next_version(&map, "version1.3.7"), Some("1.3.8".to_string()));
    }

    #[test]
    fn test_next_version_v_space_prefix() {
        let map = categorize_commits(&["fix: crash"]);
        assert_eq!(next_version(&map, "v 1.3.7"), Some("1.3.8".to_string()));
    }

    #[test]
    fn test_next_version_prefix_stripped_from_output() {
        let map = categorize_commits(&[]);
        assert_eq!(next_version(&map, "v2.4.1"), Some("2.4.1".to_string()));
    }
}
