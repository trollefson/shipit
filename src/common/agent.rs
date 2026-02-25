use std::collections::HashMap;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde_json::json;

use crate::common::git::common::format_categorized_commits;
use crate::error::ShipItError;
use crate::settings::OllamaSettings;

pub(crate) trait Agent {
    async fn send_prompt(&self, text: &str) -> Result<String, ShipItError>;

    async fn generate_summary(
        &self,
        description: &str,
        _categorized: &HashMap<String, Vec<String>>,
    ) -> Result<String, ShipItError> {
        self.send_prompt(description).await
    }
}

pub(crate) struct OllamaAgent {
    settings: OllamaSettings,
    client: Client,
}

impl OllamaAgent {
    pub(crate) fn new(settings: OllamaSettings) -> Self {
        Self { settings, client: Client::new() }
    }
}

impl Agent for OllamaAgent {
    async fn generate_summary(
        &self,
        description: &str,
        _categorized: &HashMap<String, Vec<String>>,
    ) -> Result<String, ShipItError> {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        spinner.set_message(format!("Generating with {}...", self.settings.model));
        spinner.enable_steady_tick(Duration::from_millis(80));

        let result = self.send_prompt(description).await;
        spinner.finish_and_clear();
        result
    }

    async fn send_prompt(&self, text: &str) -> Result<String, ShipItError> {
        let client = &self.client;

        let prompt = format!("{}\n\n{}", self.settings.prompt, text);
        let url = format!(
            "http://{}:{}{}",
            self.settings.domain, self.settings.port, self.settings.endpoint
        );

        let response = client
            .post(&url)
            .json(&json!({
                "model": self.settings.model,
                "prompt": prompt,
                "stream": false,
                "options": self.settings.options,
            }))
            .send()
            .await
            .map_err(|e| ShipItError::Http(e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| ShipItError::Http(e))?;

        let summary = response["response"]
            .as_str()
            .ok_or_else(|| ShipItError::Error("Failed to parse Ollama response!".to_string()))?;

        Ok(summary.to_string())
    }
}

pub(crate) struct ShipitAgent;

impl Agent for ShipitAgent {
    async fn send_prompt(&self, _text: &str) -> Result<String, ShipItError> {
        Ok(String::new())
    }

    async fn generate_summary(
        &self,
        _description: &str,
        categorized: &HashMap<String, Vec<String>>,
    ) -> Result<String, ShipItError> {
        Ok(format_categorized_commits(categorized))
    }
}

#[cfg(test)]
pub(crate) async fn summarize_with_agent<A: Agent>(
    text: &str,
    agent: &A,
) -> Result<String, ShipItError> {
    agent.send_prompt(text).await
}

#[cfg(test)]
impl OllamaAgent {
    fn new_with_client(settings: OllamaSettings, client: Client) -> Self {
        Self { settings, client }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ShipItError;
    use crate::settings::{OllamaOptions, OllamaSettings};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ollama_settings(port: u16) -> OllamaSettings {
        OllamaSettings {
            model: "test-model".to_string(),
            domain: "127.0.0.1".to_string(),
            port,
            endpoint: "/api/generate".to_string(),
            prompt: "Summarize:".to_string(),
            options: OllamaOptions::default(),
        }
    }

    struct MockAgentSuccess;
    struct MockAgentError;

    impl Agent for MockAgentSuccess {
        async fn send_prompt(&self, _text: &str) -> Result<String, ShipItError> {
            Ok("mock summary".to_string())
        }
    }

    impl Agent for MockAgentError {
        async fn send_prompt(&self, _text: &str) -> Result<String, ShipItError> {
            Err(ShipItError::Error("agent error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_summarize_with_agent_delegates_to_agent() {
        let result = summarize_with_agent("some commits", &MockAgentSuccess).await;
        assert_eq!(result.unwrap(), "mock summary");
    }

    #[tokio::test]
    async fn test_summarize_with_agent_propagates_agent_error() {
        let result = summarize_with_agent("text", &MockAgentError).await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }

    #[tokio::test]
    async fn test_ollama_agent_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "response": "Generated summary" })),
            )
            .mount(&server)
            .await;

        let agent = OllamaAgent::new(ollama_settings(server.address().port()));
        let result = summarize_with_agent("some commits", &agent).await;
        assert_eq!(result.unwrap(), "Generated summary");
    }

    #[tokio::test]
    async fn test_ollama_agent_returns_http_error_on_connection_failure() {
        // Bind a listener that accepts TCP connections but never sends an HTTP
        // response. This avoids any race condition from dropping a port and
        // hoping nothing else claims it before the request fires.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();

        let agent = OllamaAgent::new_with_client(ollama_settings(port), client);
        let result = summarize_with_agent("text", &agent).await;
        assert!(matches!(result, Err(ShipItError::Http(_))));
    }

    #[tokio::test]
    async fn test_ollama_agent_returns_http_error_on_invalid_json_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&server)
            .await;

        let agent = OllamaAgent::new(ollama_settings(server.address().port()));
        let result = summarize_with_agent("text", &agent).await;
        assert!(matches!(result, Err(ShipItError::Http(_))));
    }

    #[tokio::test]
    async fn test_ollama_agent_returns_error_when_response_field_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "not_response": "oops" })),
            )
            .mount(&server)
            .await;

        let agent = OllamaAgent::new(ollama_settings(server.address().port()));
        let result = summarize_with_agent("text", &agent).await;
        assert!(matches!(result, Err(ShipItError::Error(_))));
    }
}
