use crate::call::ActiveCallRef;
use anyhow::{Result, anyhow};
use tracing::{error, info};
use voice_engine::CallOption;

use super::{Playbook, PlaybookConfig, dialogue::DialogueHandler, handler::LlmHandler};

pub struct PlaybookRunner {
    handler: Box<dyn DialogueHandler>,
    call: ActiveCallRef,
}

impl PlaybookRunner {
    pub fn new(playbook: Playbook, call: ActiveCallRef) -> Result<Self> {
        if let Ok(mut state) = call.call_state.write() {
            if let Some(option) = state.option.as_mut() {
                apply_playbook_config(option, &playbook.config);
            }
        }

        let handler: Box<dyn DialogueHandler> = if let Some(llm_config) = playbook.config.llm {
            Box::new(LlmHandler::new(llm_config))
        } else {
            return Err(anyhow!(
                "No valid dialogue handler configuration found (e.g. missing 'llm')"
            ));
        };

        Ok(Self { handler, call })
    }

    pub async fn run(mut self) {
        info!(
            "PlaybookRunner started for session {}",
            self.call.session_id
        );

        if let Ok(commands) = self.handler.on_start().await {
            for cmd in commands {
                if let Err(e) = self.call.enqueue_command(cmd).await {
                    error!("Failed to enqueue start command: {}", e);
                }
            }
        }

        let mut event_receiver = self.call.event_sender.subscribe();

        while let Ok(event) = event_receiver.recv().await {
            match &event {
                voice_engine::event::SessionEvent::AsrFinal { text, .. } => {
                    info!("User said: {}", text);
                }
                voice_engine::event::SessionEvent::Hangup { .. } => {
                    info!("Call hung up, stopping playbook");
                    break;
                }
                _ => {}
            }

            if let Ok(commands) = self.handler.on_event(&event).await {
                for cmd in commands {
                    if let Err(e) = self.call.enqueue_command(cmd).await {
                        error!("Failed to enqueue command: {}", e);
                    }
                }
            }
        }
    }
}

pub fn apply_playbook_config(option: &mut CallOption, config: &PlaybookConfig) {
    if let Some(asr) = config.asr.clone() {
        option.asr = Some(asr);
    }
    if let Some(tts) = config.tts.clone() {
        option.tts = Some(tts);
    }
    if let Some(vad) = config.vad.clone() {
        option.vad = Some(vad);
    }
    if let Some(denoise) = config.denoise {
        option.denoise = Some(denoise);
    }
    if let Some(recorder) = config.recorder.clone() {
        option.recorder = Some(recorder);
    }
    if let Some(extra) = config.extra.clone() {
        option.extra = Some(extra);
    }
    if let Some(eou) = config.eou.clone() {
        option.eou = Some(eou);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use voice_engine::{
        EouOption, media::recorder::RecorderOption, media::vad::VADOption,
        synthesis::SynthesisOption, transcription::TranscriptionOption,
    };

    #[test]
    fn apply_playbook_config_sets_fields() {
        let mut option = CallOption::default();
        let mut extra = HashMap::new();
        extra.insert("k".to_string(), "v".to_string());

        let config = PlaybookConfig {
            asr: Some(TranscriptionOption::default()),
            tts: Some(SynthesisOption::default()),
            vad: Some(VADOption::default()),
            denoise: Some(true),
            recorder: Some(RecorderOption::default()),
            extra: Some(extra.clone()),
            eou: Some(EouOption {
                r#type: Some("test".to_string()),
                endpoint: None,
                secret_key: Some("key".to_string()),
                secret_id: Some("id".to_string()),
                timeout: Some(123),
                extra: None,
            }),
            ..Default::default()
        };

        apply_playbook_config(&mut option, &config);

        assert!(option.asr.is_some());
        assert!(option.tts.is_some());
        assert!(option.vad.is_some());
        assert_eq!(option.denoise, Some(true));
        assert!(option.recorder.is_some());
        assert_eq!(option.extra, Some(extra));
        assert!(option.eou.is_some());
    }
}
