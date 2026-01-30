use super::*;
use crate::event::SessionEvent;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Mutex;

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

    async fn call_stream(
        &self,
        _config: &LlmConfig,
        _history: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        let response = self.call(_config, _history).await?;
        let s = async_stream::stream! {
            yield Ok(LlmStreamEvent::Content(response));
        };
        Ok(Box::pin(s))
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

#[test]
fn test_build_system_prompt_with_features() {
    let config = LlmConfig {
        prompt: Some("Base prompt".to_string()),
        language: Some("zh".to_string()),
        features: Some(vec!["intent_clarification".to_string()]),
        ..Default::default()
    };

    let prompt = LlmHandler::build_system_prompt(&config, None);
    assert!(prompt.contains("Base prompt"));
    assert!(prompt.contains("### Enhanced Capabilities:"));
    // This is the content of intent_clarification.zh.md
    assert!(prompt.contains("如果用户意图模糊"));
    assert!(prompt.contains("<hangup/>"));
}

#[test]
fn test_build_system_prompt_missing_feature() {
    let config = LlmConfig {
        prompt: Some("Base prompt".to_string()),
        language: Some("zh".to_string()),
        features: Some(vec!["non_existent_feature".to_string()]),
        ..Default::default()
    };

    // Should not crash, just warn and omit the feature
    let prompt = LlmHandler::build_system_prompt(&config, None);
    assert!(prompt.contains("Base prompt"));
    assert!(!prompt.contains("Enhanced Capabilities"));
}

#[test]
fn test_build_system_prompt_en() {
    let config = LlmConfig {
        prompt: Some("Base prompt".to_string()),
        language: Some("en".to_string()),
        features: Some(vec!["intent_clarification".to_string()]),
        ..Default::default()
    };

    let prompt = LlmHandler::build_system_prompt(&config, None);
    assert!(prompt.contains("If the user's intent is unclear"));
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

    let provider = Arc::new(TestProvider::new(vec![response.to_string()]));
    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    let event = SessionEvent::AsrFinal {
        track_id: "track-1".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "hello".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };

    let commands = handler.on_event(&event).await?;
    assert!(matches!(
        commands.get(0),
        Some(Command::Tts {
            text,
            wait_input_timeout: Some(15000),
            auto_hangup: Some(true),
            ..
        }) if text == "Goodbye"
    ));
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
    let provider = Arc::new(TestProvider::new(vec![
        rag_instruction.to_string(),
        "Final answer".to_string(),
    ]));
    let rag = Arc::new(RecordingRag::new());
    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        rag.clone(),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    let event = SessionEvent::AsrFinal {
        track_id: "track-2".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "reep".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
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

#[tokio::test]
async fn test_full_dialogue_flow() -> Result<()> {
    let responses = vec![
        "Hello! How can I help you today?".to_string(),
        r#"{"text": "I can help with that. Anything else?", "waitInputTimeout": 5000}"#.to_string(),
        r#"{"text": "Goodbye!", "tools": [{"name": "hangup", "reason": "completed"}]}"#.to_string(),
    ];

    let provider = Arc::new(TestProvider::new(responses));
    let config = LlmConfig {
        greeting: Some("Welcome to the voice assistant.".to_string()),
        ..Default::default()
    };

    let mut handler = LlmHandler::with_provider(
        config,
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    // 1. Start the dialogue
    let commands = handler.on_start().await?;
    assert_eq!(commands.len(), 1);
    if let Command::Tts { text, .. } = &commands[0] {
        assert_eq!(text, "Welcome to the voice assistant.");
    } else {
        panic!("Expected Tts command");
    }

    // 2. User says something
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "I need help".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    // "Hello! How can I help you today?" -> split into two + EOS
    assert_eq!(commands.len(), 3);
    if let Command::Tts { text, .. } = &commands[0] {
        assert!(text.contains("Hello"));
    } else {
        panic!("Expected Tts command");
    }

    // 3. User says something else
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 1,
        start_time: None,
        end_time: None,
        text: "Tell me a joke".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    assert_eq!(commands.len(), 1);
    if let Command::Tts {
        text,
        wait_input_timeout,
        ..
    } = &commands[0]
    {
        assert_eq!(text, "I can help with that. Anything else?");
        assert_eq!(*wait_input_timeout, Some(5000));
    } else {
        panic!("Expected Tts command");
    }

    // 4. User says goodbye
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 2,
        start_time: None,
        end_time: None,
        text: "That's all, thanks".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    // Should have Tts with auto_hangup
    assert_eq!(commands.len(), 1);

    let has_tts_hangup = commands.iter().any(|c| {
        matches!(
            c,
            Command::Tts {
                text,
                auto_hangup: Some(true),
                ..
            } if text == "Goodbye!"
        )
    });

    assert!(has_tts_hangup);

    Ok(())
}

#[tokio::test]
async fn test_xml_tools_and_sentence_splitting() -> Result<()> {
    let responses = vec!["Hello! <refer to=\"sip:123\"/> How are you? <hangup/>".to_string()];
    let provider = Arc::new(TestProvider::new(responses));
    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "hi".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };

    let commands = handler.on_event(&event).await?;

    // Expected commands:
    // 1. TTS "Hello! "
    // 2. Refer "sip:123"
    // 3. TTS " How are you? "
    // 4. Hangup
    assert_eq!(commands.len(), 4);

    if let Command::Tts {
        text,
        play_id: pid1,
        ..
    } = &commands[0]
    {
        assert!(text.contains("Hello"));
        assert!(pid1.is_some());

        if let Command::Refer { callee, .. } = &commands[1] {
            assert_eq!(callee, "sip:123");
        } else {
            panic!("Expected Refer");
        }

        if let Command::Tts {
            text,
            play_id: pid2,
            ..
        } = &commands[2]
        {
            assert!(text.contains("How are you"));
            assert_eq!(*pid1, *pid2); // Same play_id
        } else {
            panic!("Expected Tts");
        }

        if let Command::Tts {
            auto_hangup: Some(true),
            ..
        } = &commands[3]
        {
            // Ok
        } else {
            panic!("Expected Tts with auto_hangup");
        }
    } else {
        panic!("Expected Tts");
    }

    Ok(())
}

#[tokio::test]
async fn test_interruption_logic() -> Result<()> {
    let provider = Arc::new(TestProvider::new(vec!["Some long response".to_string()]));
    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    // 1. Trigger a response
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "hello".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    handler.on_event(&event).await?;
    assert!(handler.is_speaking);

    // Sleep to bypass the 800ms interruption guard
    tokio::time::sleep(std::time::Duration::from_millis(850)).await;

    // 2. Simulate user starting to speak (AsrDelta)
    let event = SessionEvent::AsrDelta {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "I...".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::Interrupt { .. }));
    assert!(!handler.is_speaking);

    Ok(())
}

#[tokio::test]
async fn test_rag_iteration_limit() -> Result<()> {
    // Provider that always returns a RAG tool call
    let rag_instruction = r#"{"tools": [{"name": "rag", "query": "endless"}]}"#;
    let provider = Arc::new(TestProvider::new(vec![
        rag_instruction.to_string(),
        rag_instruction.to_string(),
        rag_instruction.to_string(),
        rag_instruction.to_string(),
        "Should not reach here".to_string(),
    ]));

    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(RecordingRag::new()),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "loop".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };

    let commands = handler.on_event(&event).await?;
    // After 3 attempts (MAX_RAG_ATTEMPTS), it should stop and return the last raw response
    assert_eq!(commands.len(), 1);
    if let Command::Tts { text, .. } = &commands[0] {
        assert_eq!(text, rag_instruction);
    }

    Ok(())
}

#[tokio::test]
async fn test_follow_up_logic() -> Result<()> {
    use std::time::Duration;

    // 1. Setup handler with follow-up config
    let follow_up_config = super::super::FollowUpConfig {
        timeout: 100, // 100ms for testing
        max_count: 2,
    };

    // Provider that returns responses for follow-ups
    let provider = Arc::new(TestProvider::new(vec![
        "Follow up 1".to_string(),
        "Follow up 2".to_string(),
        "Response to user".to_string(),
    ]));

    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        Some(follow_up_config),
        HashMap::new(),
        None,
        None,
    );

    // 2. Simulate initial interaction end
    handler.last_interaction_at = std::time::Instant::now();
    handler.is_speaking = false;

    // 3. Silence < timeout
    let event = SessionEvent::Silence {
        track_id: "t1".to_string(),
        timestamp: 0,
        start_time: 0,
        duration: 50,
        samples: None,
    };
    let commands = handler.on_event(&event).await?;
    assert!(commands.is_empty(), "Should not trigger if < timeout");

    // 4. Silence >= timeout
    tokio::time::sleep(Duration::from_millis(110)).await;
    // We need to act as if time passed. logic uses last_interaction_at.elapsed()
    // Wait, handler.last_interaction_at was set to now(). Sleep ensures elapsed() > 100ms.

    let event = SessionEvent::Silence {
        track_id: "t1".to_string(),
        timestamp: 0,
        start_time: 0,
        duration: 100,
        samples: None,
    };
    let commands = handler.on_event(&event).await?;
    assert_eq!(commands.len(), 1, "Should trigger follow-up 1");
    if let Command::Tts { text, .. } = &commands[0] {
        assert_eq!(text, "Follow up 1");
    }
    assert_eq!(handler.consecutive_follow_ups, 1);

    // 4b. Simulate bot finishing speaking Follow up 1
    // generate_response sets is_speaking = true. We need to clear it.
    let event = SessionEvent::TrackEnd {
        track_id: "t1".to_string(),
        timestamp: 0,
        play_id: None,
        duration: 100,
        ssrc: 0,
    };
    handler.on_event(&event).await?;
    assert!(
        !handler.is_speaking,
        "Bot should not be speaking after TrackEnd"
    );

    // Wait again for timeout
    tokio::time::sleep(Duration::from_millis(110)).await;
    let event = SessionEvent::Silence {
        track_id: "t1".to_string(),
        timestamp: 0,
        start_time: 0,
        duration: 100,
        samples: None,
    };
    let commands = handler.on_event(&event).await?;
    assert_eq!(commands.len(), 1, "Should trigger follow-up 2");
    if let Command::Tts { text, .. } = &commands[0] {
        assert_eq!(text, "Follow up 2");
    }
    assert_eq!(handler.consecutive_follow_ups, 2);

    // 5b. Simulate bot finishing speaking Follow up 2
    let event = SessionEvent::TrackEnd {
        track_id: "t1".to_string(),
        timestamp: 0,
        play_id: None,
        duration: 100,
        ssrc: 0,
    };
    handler.on_event(&event).await?;

    // 6. Max count reached
    tokio::time::sleep(Duration::from_millis(110)).await;
    let event = SessionEvent::Silence {
        track_id: "t1".to_string(),
        timestamp: 0,
        start_time: 0,
        duration: 100,
        samples: None,
    };
    let commands = handler.on_event(&event).await?;
    assert_eq!(commands.len(), 1, "Should hangup after max count");
    assert!(matches!(commands[0], Command::Hangup { .. }));

    handler.consecutive_follow_ups = 2; // Artificially set high
    let event = SessionEvent::AsrFinal {
        track_id: "t1".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "User speaks".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };

    let _ = handler.on_event(&event).await?;
    assert_eq!(
        handler.consecutive_follow_ups, 0,
        "Should reset count on AsrFinal"
    );

    Ok(())
}

#[tokio::test]
async fn test_interruption_protection_period() -> Result<()> {
    let provider = Arc::new(TestProvider::new(vec!["Some long response".to_string()]));
    let mut config = crate::playbook::InterruptionConfig::default();
    config.ignore_first_ms = Some(800);

    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        config,
        None,
        HashMap::new(),
        None,
        None,
    );

    // 1. Trigger a response
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "hello".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    handler.on_event(&event).await?;
    assert!(handler.is_speaking);

    // 2. Simulate user starting to speak immediately (AsrDelta)
    let event = SessionEvent::AsrDelta {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "I...".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    // Should be ignored due to protection period
    assert_eq!(commands.len(), 0);
    assert!(handler.is_speaking);

    Ok(())
}

#[tokio::test]
async fn test_interruption_filler_word() -> Result<()> {
    let provider = Arc::new(TestProvider::new(vec!["Some long response".to_string()]));
    let mut config = crate::playbook::InterruptionConfig::default();
    config.filler_word_filter = Some(true);
    config.ignore_first_ms = Some(0); // Disable protection period for this test

    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        config,
        None,
        HashMap::new(),
        None,
        None,
    );

    // 1. Trigger a response
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "hello".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };
    handler.on_event(&event).await?;
    assert!(handler.is_speaking);

    // Sleep to bypass the 500ms stale event guard
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // 2. Simulate user saying "uh" (filler)
    let event = SessionEvent::AsrDelta {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "uh".to_string(),
        is_filler: Some(true),
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    // Should be ignored
    assert_eq!(commands.len(), 0);
    assert!(handler.is_speaking);

    // 3. Simulate user saying "Wait" (not filler)
    let event = SessionEvent::AsrDelta {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "Wait".to_string(),
        is_filler: Some(false),
        confidence: None,
        task_id: None,
    };
    let commands = handler.on_event(&event).await?;
    // Should trigger interruption
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::Interrupt { .. }));

    Ok(())
}

#[tokio::test]
async fn test_eou_early_response() -> Result<()> {
    let provider = Arc::new(TestProvider::new(vec![
        "End of Utterance response".to_string(),
    ]));
    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    // 1. Receive EOU
    let event = SessionEvent::Eou {
        track_id: "test".to_string(),
        timestamp: 0,
        completed: true,
        interrupt_point: None,
    };
    let commands = handler.on_event(&event).await?;
    assert_eq!(commands.len(), 1);
    if let Command::Tts { text, .. } = &commands[0] {
        assert_eq!(text, "End of Utterance response");
    } else {
        panic!("Expected Tts");
    }

    Ok(())
}

#[tokio::test]
async fn test_summary_and_history() -> Result<()> {
    let provider = Arc::new(TestProvider::new(vec!["Test summary".to_string()]));
    let mut handler = LlmHandler::with_provider(
        LlmConfig::default(),
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    // Add some history
    handler.history.push(ChatMessage {
        role: "user".to_string(),
        content: "Hello".to_string(),
    });
    handler.history.push(ChatMessage {
        role: "assistant".to_string(),
        content: "Hi there".to_string(),
    });

    let history = handler.get_history().await;
    assert_eq!(history.len(), 3);

    let summary = handler.summarize("Summarize this").await?;
    assert_eq!(summary, "Test summary");

    Ok(())
}

#[tokio::test]
async fn test_rolling_summary() -> Result<()> {
    let responses = vec![
        "This is the summary of previous conversation.".to_string(),
        "Response to user input after summary.".to_string(),
    ];
    let provider = Arc::new(TestProvider::new(responses));

    let mut config = LlmConfig::default();
    config.features = Some(vec!["rolling_summary".to_string()]);
    config.summary_limit = Some(4);

    let mut handler = LlmHandler::with_provider(
        config,
        provider,
        Arc::new(NoopRagRetriever),
        crate::playbook::InterruptionConfig::default(),
        None,
        HashMap::new(),
        None,
        None,
    );

    for i in 1..=12 {
        let role = if i % 2 == 1 { "user" } else { "assistant" };
        handler.history.push(ChatMessage {
            role: role.to_string(),
            content: format!("Message {}", i),
        });
    }
    let event = SessionEvent::AsrFinal {
        track_id: "test".to_string(),
        timestamp: 0,
        index: 0,
        start_time: None,
        end_time: None,
        text: "Trigger summary".to_string(),
        is_filler: None,
        confidence: None,
        task_id: None,
    };

    let commands = handler.on_event(&event).await?;

    if let Command::Tts { text, .. } = &commands[0] {
        assert_eq!(text, "Response to user input after summary.");
    } else {
        panic!("Expected Tts");
    }

    assert_eq!(handler.history.len(), 8);

    // Verify system prompt contains summary
    let system_msg = &handler.history[0];
    assert_eq!(system_msg.role, "system");
    assert!(
        system_msg
            .content
            .contains("[Previous Context Summary]: This is the summary of previous conversation.")
    );

    assert_eq!(
        handler.history.last().unwrap().content,
        "Response to user input after summary."
    );
    assert_eq!(
        handler.history[handler.history.len() - 2].content,
        "Trigger summary"
    );
    assert_eq!(
        handler.history[handler.history.len() - 3].content,
        "Message 12"
    );

    Ok(())
}
