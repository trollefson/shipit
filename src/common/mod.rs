pub(crate) mod agent;
pub(crate) mod git;
pub(crate) mod ops;

pub(crate) use agent::{OllamaAgent, ShipitAgent};
pub(crate) use git::common::{
    categorize_commits, collect_commits_since_tag, create_local_tag, extract_repo_path,
    find_most_recent_tag, next_version, open_merge_request,
};
pub(crate) use git::github::{
    create_github_release, fetch_github_pr_info, fetch_latest_github_tag,
    lookup_github_identifier, parse_github_pr_number, Github,
};
pub(crate) use git::gitlab::{
    create_gitlab_tag, fetch_gitlab_mr_info, fetch_latest_gitlab_tag, lookup_gitlab_project_id,
    parse_gitlab_mr_iid, Gitlab,
};
pub(crate) use ops::{
    check_needs_push, collect_commits, collect_messages, create_platform_tag, enrich_messages,
    generate_summary, open_platform_mr, open_repo, resolve_project_id, resolve_remote_url,
    suggest_title,
};
