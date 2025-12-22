use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tokio::fs;
use voice_engine::EouOption;
use voice_engine::media::recorder::RecorderOption;
use voice_engine::media::vad::VADOption;
use voice_engine::synthesis::SynthesisOption;
use voice_engine::transcription::TranscriptionOption;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlaybookConfig {
    pub asr: Option<TranscriptionOption>,
    pub tts: Option<SynthesisOption>,
    pub llm: Option<LlmConfig>,
    pub vad: Option<VADOption>,
    pub denoise: Option<bool>,
    pub recorder: Option<RecorderOption>,
    pub extra: Option<HashMap<String, String>>,
    pub eou: Option<EouOption>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub prompt: Option<String>,
    pub greeting: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Playbook {
    pub config: PlaybookConfig,
}

impl Playbook {
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path).await?;
        if !content.starts_with("---") {
            return Err(anyhow!("Missing front matter"));
        }

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(anyhow!("Invalid front matter format"));
        }

        let yaml_str = parts[1];
        let prompt = parts[2].trim().to_string();

        let mut config: PlaybookConfig = serde_yaml::from_str(yaml_str)?;
        if let Some(llm) = config.llm.as_mut() {
            llm.prompt = Some(prompt);
        }

        Ok(Self { config })
    }
}

pub mod dialogue;
pub mod handler;
pub mod runner;

pub use dialogue::DialogueHandler;
pub use handler::{LlmHandler, RagRetriever};
pub use runner::PlaybookRunner;
