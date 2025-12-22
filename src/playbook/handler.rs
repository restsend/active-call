use crate::call::Command;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};
use voice_engine::ReferOption;
use voice_engine::event::SessionEvent;

use super::LlmConfig;
use super::dialogue::DialogueHandler;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

const MAX_RAG_ATTEMPTS: usize = 3;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn call(&self, config: &LlmConfig, history: &[ChatMessage]) -> Result<String>;
}

struct DefaultLlmProvider {
    client: Client,
}

impl DefaultLlmProvider {
    fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for DefaultLlmProvider {
    async fn call(&self, config: &LlmConfig, history: &[ChatMessage]) -> Result<String> {
        let mut url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "gpt-3.5-turbo".to_string());
        let api_key = config.api_key.clone().unwrap_or_default();

        if !url.ends_with("/chat/completions") {
            url = format!("{}/chat/completions", url.trim_end_matches('/'));
        }

        let body = json!({
            "model": model,
            "messages": history,
        });

        let res = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(anyhow!("LLM request failed: {}", res.status()));
        }

        let json: serde_json::Value = res.json().await?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid LLM response"))?
            .to_string();

        Ok(content)
    }
}

#[async_trait]
pub trait RagRetriever: Send + Sync {
    async fn retrieve(&self, query: &str) -> Result<String>;
}

struct NoopRagRetriever;

#[async_trait]
impl RagRetriever for NoopRagRetriever {
    async fn retrieve(&self, _query: &str) -> Result<String> {
        Ok(String::new())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructuredResponse {
    text: Option<String>,
    wait_input_timeout: Option<u32>,
    tools: Option<Vec<ToolInvocation>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "name", rename_all = "lowercase")]
enum ToolInvocation {
    #[serde(rename_all = "camelCase")]
    Hangup {
        reason: Option<String>,
        initiator: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Refer {
        caller: String,
        callee: String,
        options: Option<ReferOption>,
    },
    #[serde(rename_all = "camelCase")]
    Rag {
        query: String,
        source: Option<String>,
    },
}

pub struct LlmHandler {
    config: LlmConfig,
    history: Vec<ChatMessage>,
    provider: Box<dyn LlmProvider>,
    rag_retriever: Arc<dyn RagRetriever>,
    is_speaking: bool,
}

impl LlmHandler {
    pub fn new(config: LlmConfig) -> Self {
        Self::with_provider(
            config,
            Box::new(DefaultLlmProvider::new()),
            Arc::new(NoopRagRetriever),
        )
    }

    pub fn with_provider(
        config: LlmConfig,
        provider: Box<dyn LlmProvider>,
        rag_retriever: Arc<dyn RagRetriever>,
    ) -> Self {
        let mut history = Vec::new();
        if let Some(prompt) = &config.prompt {
            history.push(ChatMessage {
                role: "system".to_string(),
                content: prompt.clone(),
            });
        }

        Self {
            config,
            history,
            provider,
            rag_retriever,
            is_speaking: false,
        }
    }

    async fn call_llm(&self) -> Result<String> {
        self.provider.call(&self.config, &self.history).await
    }

    fn create_tts_command(&self, text: String, wait_input_timeout: Option<u32>) -> Command {
        let timeout = wait_input_timeout.unwrap_or(10000);
        Command::Tts {
            text,
            speaker: None,
            play_id: None,
            auto_hangup: None,
            streaming: None,
            end_of_stream: None,
            option: None,
            wait_input_timeout: Some(timeout),
            base64: None,
        }
    }

    async fn generate_response(&mut self) -> Result<Vec<Command>> {
        let initial = self.call_llm().await?;
        self.interpret_response(initial).await
    }

    async fn interpret_response(&mut self, initial: String) -> Result<Vec<Command>> {
        let mut tool_commands = Vec::new();
        let mut wait_input_timeout = None;
        let mut attempts = 0;
        let final_text: Option<String>;
        let mut raw = initial;

        loop {
            attempts += 1;
            let mut rerun_for_rag = false;

            if let Some(structured) = parse_structured_response(&raw) {
                if wait_input_timeout.is_none() {
                    wait_input_timeout = structured.wait_input_timeout;
                }

                if let Some(tools) = structured.tools {
                    for tool in tools {
                        match tool {
                            ToolInvocation::Hangup { reason, initiator } => {
                                tool_commands.push(Command::Hangup { reason, initiator });
                            }
                            ToolInvocation::Refer {
                                caller,
                                callee,
                                options,
                            } => {
                                tool_commands.push(Command::Refer {
                                    caller,
                                    callee,
                                    options,
                                });
                            }
                            ToolInvocation::Rag { query, source } => {
                                let rag_result = self.rag_retriever.retrieve(&query).await?;
                                let summary = if let Some(source) = source {
                                    format!("[{}] {}", source, rag_result)
                                } else {
                                    rag_result
                                };
                                self.history.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("RAG result for {}: {}", query, summary),
                                });
                                rerun_for_rag = true;
                            }
                        }
                    }
                }

                if rerun_for_rag {
                    if attempts >= MAX_RAG_ATTEMPTS {
                        warn!("Reached RAG iteration limit, using last response");
                        final_text = structured.text.or_else(|| Some(raw.clone()));
                        break;
                    }
                    raw = self.call_llm().await?;
                    continue;
                }

                final_text = Some(structured.text.unwrap_or_else(|| raw.clone()));
                break;
            }

            final_text = Some(raw.clone());
            break;
        }

        let mut commands = Vec::new();
        if let Some(text) = final_text {
            if !text.trim().is_empty() {
                self.history.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: text.clone(),
                });
                self.is_speaking = true;
                commands.push(self.create_tts_command(text, wait_input_timeout));
            }
        }

        commands.extend(tool_commands);

        Ok(commands)
    }
}

fn parse_structured_response(raw: &str) -> Option<StructuredResponse> {
    let payload = extract_json_block(raw)?;
    serde_json::from_str(payload).ok()
}

fn extract_json_block(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.starts_with('`') {
        if let Some(end) = trimmed.rfind("```") {
            if end <= 3 {
                return None;
            }
            let mut inner = &trimmed[3..end];
            inner = inner.trim();
            if inner.to_lowercase().starts_with("json") {
                if let Some(newline) = inner.find('\n') {
                    inner = inner[newline + 1..].trim();
                } else if inner.len() > 4 {
                    inner = inner[4..].trim();
                } else {
                    inner = inner.trim();
                }
            }
            return Some(inner);
        }
    } else if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(trimmed);
    }
    None
}

#[async_trait]
impl DialogueHandler for LlmHandler {
    async fn on_start(&mut self) -> Result<Vec<Command>> {
        if let Some(greeting) = &self.config.greeting {
            self.is_speaking = true;
            return Ok(vec![self.create_tts_command(greeting.clone(), None)]);
        }

        self.generate_response().await
    }

    async fn on_event(&mut self, event: &SessionEvent) -> Result<Vec<Command>> {
        match event {
            SessionEvent::AsrFinal { text, .. } => {
                if text.trim().is_empty() {
                    return Ok(vec![]);
                }

                self.history.push(ChatMessage {
                    role: "user".to_string(),
                    content: text.clone(),
                });

                self.generate_response().await
            }

            SessionEvent::AsrDelta { .. } | SessionEvent::Speaking { .. } => {
                if self.is_speaking {
                    info!("Interruption detected, stopping playback");
                    self.is_speaking = false;
                    return Ok(vec![Command::Interrupt {
                        graceful: Some(true),
                    }]);
                }
                Ok(vec![])
            }

            SessionEvent::Silence { .. } => {
                info!("Silence timeout detected, triggering follow-up");
                self.generate_response().await
            }

            SessionEvent::TrackEnd { .. } => {
                self.is_speaking = false;
                Ok(vec![])
            }

            _ => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, anyhow};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use voice_engine::event::SessionEvent;

    struct TestProvider {
        responses: Mutex<VecDeque<String>>,
    }

    impl TestProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for TestProvider {
        async fn call(&self, _config: &LlmConfig, _history: &[ChatMessage]) -> Result<String> {
            let mut guard = self.responses.lock().unwrap();
            guard
                .pop_front()
                .ok_or_else(|| anyhow!("Test provider ran out of responses"))
        }
    }

    struct RecordingRag {
        queries: Mutex<Vec<String>>,
    }

    impl RecordingRag {
        fn new() -> Self {
            Self {
                queries: Mutex::new(Vec::new()),
            }
        }

        fn recorded_queries(&self) -> Vec<String> {
            self.queries.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl RagRetriever for RecordingRag {
        async fn retrieve(&self, query: &str) -> Result<String> {
            self.queries.lock().unwrap().push(query.to_string());
            Ok(format!("retrieved {}", query))
        }
    }

    #[tokio::test]
    async fn handler_applies_tool_instructions() -> Result<()> {
        let response = r#"{
            "text": "Goodbye",
            "waitInputTimeout": 15000,
            "tools": [
                {"name": "hangup", "reason": "done", "initiator": "agent"},
                {"name": "refer", "caller": "sip:bot", "callee": "sip:lead"}
            ]
        }"#;

        let provider = Box::new(TestProvider::new(vec![response.to_string()]));
        let mut handler =
            LlmHandler::with_provider(LlmConfig::default(), provider, Arc::new(NoopRagRetriever));

        let event = SessionEvent::AsrFinal {
            track_id: "track-1".to_string(),
            timestamp: 0,
            index: 0,
            start_time: None,
            end_time: None,
            text: "hello".to_string(),
        };

        let commands = handler.on_event(&event).await?;
        assert!(matches!(
            commands.get(0),
            Some(Command::Tts {
                text,
                wait_input_timeout: Some(15000),
                ..
            }) if text == "Goodbye"
        ));
        assert!(commands.iter().any(|cmd| matches!(
            cmd,
            Command::Hangup {
                reason: Some(reason),
                initiator: Some(origin),
            } if reason == "done" && origin == "agent"
        )));
        assert!(commands.iter().any(|cmd| matches!(
            cmd,
            Command::Refer {
                caller,
                callee,
                ..
            } if caller == "sip:bot" && callee == "sip:lead"
        )));

        Ok(())
    }

    #[tokio::test]
    async fn handler_requeries_after_rag() -> Result<()> {
        let rag_instruction = r#"{"tools": [{"name": "rag", "query": "policy"}]}"#;
        let provider = Box::new(TestProvider::new(vec![
            rag_instruction.to_string(),
            "Final answer".to_string(),
        ]));
        let rag = Arc::new(RecordingRag::new());
        let mut handler = LlmHandler::with_provider(LlmConfig::default(), provider, rag.clone());

        let event = SessionEvent::AsrFinal {
            track_id: "track-2".to_string(),
            timestamp: 0,
            index: 0,
            start_time: None,
            end_time: None,
            text: "reep".to_string(),
        };

        let commands = handler.on_event(&event).await?;
        assert!(matches!(
            commands.get(0),
            Some(Command::Tts {
                text,
                wait_input_timeout: Some(timeout),
                ..
            }) if text == "Final answer" && *timeout == 10000
        ));
        assert_eq!(rag.recorded_queries(), vec!["policy".to_string()]);

        Ok(())
    }
}
