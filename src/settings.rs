use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[derive(Default)]
pub struct Settings {
    pub shipit: ShipitSettings,
    pub ollama: OllamaSettings,
    pub platform: PlatformSettings,
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ShipitSettings {
    pub agent: String,
    pub commits: String,
}

impl Default for ShipitSettings {
    fn default() -> Self {
        Self {
            agent: "shipit".to_string(),
            commits: "custom".to_string(),
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
#[derive(Default)]
pub struct PlatformSettings {
    pub domain: String,
    pub token: String,
}

