use gitlab::api::{projects, AsyncQuery};
use gitlab::Gitlab;
use octocrab::OctocrabBuilder;
use reqwest::Client;
use serde_json::json;

use crate::error::ShipItError;
use crate::settings::OllamaSettings;


pub(crate) async fn open_github_pr(
    source: &str, target: &str, domain: &str, token: &str,
    owner: &str, repo: &str, description: &str,
) -> Result<String, ShipItError> {
    let mut builder = OctocrabBuilder::new().personal_token(token.to_string());

    if domain != "github.com" {
        let base_uri = format!("https://{}/api/v3/", domain);
        builder = builder.base_uri(base_uri)
            .map_err(|e| ShipItError::Error(format!("Invalid GitHub domain: {}", e)))?;
    }

    let octo = builder.build().map_err(|e| ShipItError::Github(e))?;

    let pr = octo
        .pulls(owner, repo)
        .create(format!("{} to {}", source, target), source, target)
        .body(description)
        .send()
        .await
        .map_err(|e| ShipItError::Github(e))?;

    let url = pr.html_url
        .ok_or_else(|| ShipItError::Error("Failed to get PR URL from GitHub response".to_string()))?;

    Ok(url.to_string())
}


pub(crate) async fn open_gitlab_mr(
    source: &str, target: &str, domain: &str, token: &str,
    project_id: &u64, description: &str
) -> Result<serde_json::Value, ShipItError> {
    let client = Gitlab::builder(domain, token).build_async().await.map_err(|e| ShipItError::Gitlab(e))?;

    let create_mr = projects::merge_requests::CreateMergeRequest::builder()
        .project(*project_id)
        .source_branch(source)
        .target_branch(target)
        .title(format!("{} to {}", source, target))
        .description(description)
        .remove_source_branch(true)
        .build().map_err(|_e| ShipItError::Error("Failed to build a Gitlab MR!".to_string()))?;

    let merge_request: serde_json::Value = create_mr.query_async(&client).await.map_err(|_e| ShipItError::Error("Failed to create a Gitlab merge request!".to_string()))?;

    Ok(merge_request)
}


pub(crate) async fn summarize_with_ollama(text: &str, ollama: &OllamaSettings) -> Result<String, ShipItError> {
    let client = Client::new();

    let prompt = format!(
        "You are a technical writer tasked with creating organized and concise release notes. Categorize the following comma separated list of commit titles followed by their commit ids into markdown formatted subheadings.  The heading cateogries are new features, bug fixes, infrastructure, and docs.  If a category has no content, exclude it from the output. Do not format or alter the commit messages in any other way. Do not wrap the body of your result in markdown syntax highlighting ticks.\n\n{}",
         text
    );

    let url = format!("http://{}:{}{}", ollama.domain, ollama.port, ollama.endpoint);

    let response = client.post(&url)
        .json(&json!({
            "model": ollama.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": ollama.options.temperature,
                "top_p": ollama.options.top_p,
                "seed": ollama.options.seed
            }
        }))
        .send()
        .await.map_err(|e| ShipItError::Http(e))?
        .json::<serde_json::Value>()
        .await.map_err(|e| ShipItError::Http(e))?;

    let summary = response["response"]
        .as_str()
        .ok_or_else(|| ShipItError::Error("Failed to parse Ollama response!".to_string()))?;

    Ok(summary.to_string())
}
