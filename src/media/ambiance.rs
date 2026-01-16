use super::processor::Processor;
use crate::media::{AudioFrame, INTERNAL_SAMPLERATE, Samples};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AmbianceOption {
    pub path: Option<String>,
    pub duck_level: Option<f32>,
    pub normal_level: Option<f32>,
    pub transition_speed: Option<f32>,
    pub enabled: Option<bool>,
}

impl AmbianceOption {
    pub fn merge(&mut self, other: &AmbianceOption) {
        if self.path.is_none() {
            self.path = other.path.clone();
        }
        if self.duck_level.is_none() {
            self.duck_level = other.duck_level;
        }
        if self.normal_level.is_none() {
            self.normal_level = other.normal_level;
        }
        if self.transition_speed.is_none() {
            self.transition_speed = other.transition_speed;
        }
        if self.enabled.is_none() {
            self.enabled = other.enabled;
        }
    }
}

pub struct AmbianceProcessor {
    samples: Vec<i16>,
    cursor: usize,
    duck_level: f32,
    normal_level: f32,
    enabled: bool,
    current_level: f32,
    transition_speed: f32,
    resample_phase: u32,
    resample_step: u32,
}

impl AmbianceProcessor {
    pub async fn new(option: AmbianceOption) -> Result<Self> {
        let path = option
            .path
            .ok_or_else(|| anyhow::anyhow!("Ambiance path required"))?;

        let samples =
            crate::media::loader::load_audio_as_pcm(&path, INTERNAL_SAMPLERATE, true).await?;

        info!("Loading ambiance {}: samples={}", path, samples.len());

        let normal_level = option.normal_level.unwrap_or(0.3);
        Ok(Self {
            samples,
            cursor: 0,
            duck_level: option.duck_level.unwrap_or(0.1),
            normal_level,
            enabled: option.enabled.unwrap_or(true),
            current_level: normal_level,
            transition_speed: option.transition_speed.unwrap_or(0.01),
            resample_phase: 0,
            resample_step: 1 << 16,
        })
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_levels(&mut self, normal: f32, duck: f32) {
        self.normal_level = normal;
        self.duck_level = duck;
    }

    #[inline]
    fn get_ambient_sample_with_rate(&mut self, target_sample_rate: u32) -> i16 {
        if self.samples.is_empty() {
            return 0;
        }

        self.resample_step =
            (((INTERNAL_SAMPLERATE as u64) << 16) / target_sample_rate as u64) as u32;
        let sample = self.samples[self.cursor];

        self.resample_phase += self.resample_step;
        while self.resample_phase >= (1 << 16) {
            self.resample_phase -= 1 << 16;
            self.cursor = (self.cursor + 1) % self.samples.len();
        }

        sample
    }

    #[inline]
    fn soft_mix(signal: i16, ambient: i16, level: f32) -> i16 {
        let ambient_scaled = (ambient as i32 * (level * 256.0) as i32) >> 8;
        let signal_i32 = signal as i32;
        let mixed = signal_i32 + ambient_scaled;

        if mixed > 32767 {
            let over = mixed - 32767;
            (32767 - (over >> 2)) as i16
        } else if mixed < -32768 {
            let under = -32768 - mixed;
            (-32768 + (under >> 2)) as i16
        } else {
            mixed as i16
        }
    }
}

impl Processor for AmbianceProcessor {
    fn process_frame(&mut self, frame: &mut AudioFrame) -> Result<()> {
        if !self.enabled || self.samples.is_empty() {
            return Ok(());
        }

        let is_server_side_speaking = match &frame.samples {
            Samples::PCM { samples } => !samples.is_empty(),
            Samples::RTP { .. } => true,
            Samples::Empty => false,
        };

        let target_level = if is_server_side_speaking {
            self.duck_level
        } else {
            self.normal_level
        };

        if (self.current_level - target_level).abs() > 0.001 {
            if self.current_level < target_level {
                self.current_level = (self.current_level + self.transition_speed).min(target_level);
            } else {
                self.current_level = (self.current_level - self.transition_speed).max(target_level);
            }
        }

        let sample_rate = if frame.sample_rate > 0 {
            frame.sample_rate
        } else {
            INTERNAL_SAMPLERATE
        };
        let channels = frame.channels.max(1) as usize;

        match &mut frame.samples {
            Samples::PCM { samples } => {
                let frame_sample_count = samples.len() / channels;
                for i in 0..frame_sample_count {
                    let ambient = self.get_ambient_sample_with_rate(sample_rate);
                    for c in 0..channels {
                        let idx = i * channels + c;
                        if idx < samples.len() {
                            samples[idx] =
                                Self::soft_mix(samples[idx], ambient, self.current_level);
                        }
                    }
                }
            }
            Samples::Empty => {
                let frame_size = (sample_rate as usize * 20) / 1000;
                let mut ambient_samples = Vec::with_capacity(frame_size * channels);
                for _ in 0..frame_size {
                    let ambient = self.get_ambient_sample_with_rate(sample_rate);
                    let ambient_scaled =
                        ((ambient as i32 * (self.current_level * 256.0) as i32) >> 8) as i16;
                    for _ in 0..channels {
                        ambient_samples.push(ambient_scaled);
                    }
                }
                frame.samples = Samples::PCM {
                    samples: ambient_samples,
                };
                frame.sample_rate = sample_rate;
                frame.channels = channels as u16;
            }
            _ => {}
        }

        Ok(())
    }
}
