use reqwest::Client;
use serde_json::json;

use crate::error::ShipItError;
use crate::settings::OllamaSettings;

pub(crate) trait Agent {
    async fn send_prompt(&self, text: &str) -> Result<String, ShipItError>;
}

pub(crate) struct OllamaAgent {
    settings: OllamaSettings,
}

impl OllamaAgent {
    pub(crate) fn new(settings: OllamaSettings) -> Self {
        Self { settings }
    }
}

impl Agent for OllamaAgent {
    async fn send_prompt(&self, text: &str) -> Result<String, ShipItError> {
        let client = Client::new();

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

pub(crate) async fn summarize_with_agent<A: Agent>(
    text: &str,
    agent: &A,
) -> Result<String, ShipItError> {
    agent.send_prompt(text).await
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
        let closed_port = {
            let server = MockServer::start().await;
            server.address().port()
        };
        let agent = OllamaAgent::new(ollama_settings(closed_port));
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
