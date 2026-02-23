pub(crate) mod agent;
pub(crate) mod git;

pub(crate) use agent::{OllamaAgent, summarize_with_agent};
pub(crate) use git::common::{
    categorize_commits, extract_repo_path, format_categorized_commits, open_merge_request,
};
pub(crate) use git::github::{
    fetch_github_pr_info, lookup_github_identifier, parse_github_pr_number, Github,
};
pub(crate) use git::gitlab::{
    fetch_gitlab_mr_info, lookup_gitlab_project_id, parse_gitlab_mr_iid, Gitlab,
};
