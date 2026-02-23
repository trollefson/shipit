pub(crate) mod agent;
pub(crate) mod git;
pub(crate) mod ops;

pub(crate) use agent::{OllamaAgent, summarize_with_agent};
pub(crate) use git::common::{
    categorize_commits, extract_repo_path, format_categorized_commits, open_merge_request,
};
pub(crate) use git::github::{
    create_github_release, fetch_github_pr_info, lookup_github_identifier, parse_github_pr_number,
    Github,
};
pub(crate) use git::gitlab::{
    create_gitlab_tag, fetch_gitlab_mr_info, lookup_gitlab_project_id, parse_gitlab_mr_iid, Gitlab,
};
pub(crate) use ops::{
    collect_messages, enrich_messages, generate_summary, open_repo, resolve_project_id,
    resolve_remote_url,
};
