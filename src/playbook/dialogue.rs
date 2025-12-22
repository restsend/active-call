use crate::call::Command;
use anyhow::Result;
use async_trait::async_trait;
use voice_engine::event::SessionEvent;

#[async_trait]
pub trait DialogueHandler: Send + Sync {
    async fn on_start(&mut self) -> Result<Vec<Command>>;
    async fn on_event(&mut self, event: &SessionEvent) -> Result<Vec<Command>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use voice_engine::event::SessionEvent;

    struct RecordingHandler;

    #[async_trait]
    impl DialogueHandler for RecordingHandler {
        async fn on_start(&mut self) -> Result<Vec<Command>> {
            Ok(vec![Command::Hangup {
                reason: Some("start".to_string()),
                initiator: Some("tester".to_string()),
            }])
        }

        async fn on_event(&mut self, event: &SessionEvent) -> Result<Vec<Command>> {
            if let SessionEvent::Hangup { .. } = event {
                Ok(vec![Command::Hangup {
                    reason: Some("event".to_string()),
                    initiator: Some("tester".to_string()),
                }])
            } else {
                Ok(vec![])
            }
        }
    }

    #[tokio::test]
    async fn dialogue_handler_trait_works() -> Result<()> {
        let mut handler = RecordingHandler;

        let start_cmds = handler.on_start().await?;
        assert!(start_cmds.len() == 1);

        let event_cmds = handler
            .on_event(&SessionEvent::Hangup {
                track_id: "test".to_string(),
                timestamp: 0,
                reason: Some("done".to_string()),
                initiator: Some("tester".to_string()),
                start_time: "".to_string(),
                hangup_time: "".to_string(),
                answer_time: None,
                ringing_time: None,
                from: None,
                to: None,
                extra: None,
                refer: None,
            })
            .await?;
        assert!(event_cmds.len() == 1);
        Ok(())
    }
}
