use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub shipit: ShipitSettings,
    pub ollama: OllamaSettings,
    pub gitlab: GitlabSettings,
    pub github: GithubSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shipit: ShipitSettings::default(),
            ollama: OllamaSettings::default(),
            gitlab: GitlabSettings::default(),
            github: GithubSettings::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShipitSettings {
    pub agent: String,
    pub ai: bool,
    pub commits: String,
    pub dryrun: bool,
}

impl Default for ShipitSettings {
    fn default() -> Self {
        Self {
            agent: "ollama".to_string(),
            ai: false,
            commits: "custom".to_string(),
            dryrun: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaSettings {
    pub model: String,
    pub domain: String,
    pub port: u16,
    pub endpoint: String,
    pub prompt: String,
    pub options: OllamaOptions,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            model: "qwen2.5-coder:7b".to_string(),
            domain: "localhost".to_string(),
            port: 11434,
            endpoint: "/api/generate".to_string(),
            prompt: "You are a technical writer tasked with creating organized and concise release notes. Categorize the following comma separated list of commit titles followed by their commit ids into markdown formatted subheadings.  The heading cateogries are new features, bug fixes, infrastructure, and docs.  If a category has no content, exclude it from the output. Do not format or alter the commit messages in any other way. Do not wrap the body of your result in markdown syntax highlighting ticks.".to_string(),
            options: OllamaOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaOptions {
    pub temperature: f64,
    pub top_p: f64,
    pub seed: u64,
}

impl Default for OllamaOptions {
    fn default() -> Self {
        Self {
            temperature: 0.1,
            top_p: 0.4,
            seed: 43,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabSettings {
    pub domain: String,
    pub token: Option<String>,
}

impl Default for GitlabSettings {
    fn default() -> Self {
        Self {
            domain: "gitlab.com".to_string(),
            token: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GithubSettings {
    pub domain: String,
    pub token: Option<String>,
}

impl Default for GithubSettings {
    fn default() -> Self {
        Self {
            domain: "github.com".to_string(),
            token: None,
        }
    }
}
