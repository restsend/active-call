use crate::media::{
    AudioFrame, Samples,
    recorder::{Recorder, RecorderOption},
};
use anyhow::Result;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_recorder_rtp_g711() -> Result<()> {
    let temp_dir = tempdir()?;
    let file_path = temp_dir.path().join("test_g711.wav");
    let cancel_token = CancellationToken::new();
    let config = RecorderOption::default();

    let recorder = Arc::new(Recorder::new(
        cancel_token.clone(),
        "test_g711".to_string(),
        config,
    ));

    let (tx, rx) = mpsc::unbounded_channel();
    let recorder_clone = recorder.clone();
    let file_path_clone = file_path.clone();

    let handle =
        tokio::spawn(async move { recorder_clone.process_recording(&file_path_clone, rx).await });

    // Send G.711 PCMU (PT=0) frames
    let payload = vec![0u8; 160]; // 20ms
    let frame = AudioFrame {
        track_id: "track1".to_string(),
        samples: Samples::RTP {
            sequence_number: 1,
            payload_type: 0,
            payload: payload.clone(),
        },
        timestamp: 0,
        sample_rate: 8000,
    };
    tx.send(frame)?;

    drop(tx);
    handle.await??;

    // Verify file exists and size
    // Header (44 bytes) + 160 bytes data = 204 bytes
    let metadata = std::fs::metadata(&file_path)?;
    assert_eq!(metadata.len(), 44 + 160);

    Ok(())
}

#[tokio::test]
async fn test_recorder_rtp_g729() -> Result<()> {
    use crate::media::recorder::G729WavReader;

    let temp_dir = tempdir()?;
    // Note: Recorder will change extension to .g729
    let file_path = temp_dir.path().join("test_g729.wav");
    let expected_path = temp_dir.path().join("test_g729.g729");

    let cancel_token = CancellationToken::new();
    let config = RecorderOption::default();

    let recorder = Arc::new(Recorder::new(
        cancel_token.clone(),
        "test_g729".to_string(),
        config,
    ));

    let (tx, rx) = mpsc::unbounded_channel();
    let recorder_clone = recorder.clone();
    let file_path_clone = file_path.clone();

    let handle =
        tokio::spawn(async move { recorder_clone.process_recording(&file_path_clone, rx).await });

    // Send G.729 (PT=18) frames
    // 10 bytes per 10ms frame.
    // Note: G.729 decoder expects valid frames usually, but for test we check pipeline.
    // We use a dummy frame that hopefully doesn't crash the decoder.
    // A silence frame in G.729 is specific, but let's try zeros.
    let payload = vec![0u8; 10];
    let frame = AudioFrame {
        track_id: "track1".to_string(),
        samples: Samples::RTP {
            sequence_number: 1,
            payload_type: 18,
            payload: payload.clone(),
        },
        timestamp: 0,
        sample_rate: 8000,
    };
    tx.send(frame)?;

    drop(tx);
    handle.await??;

    // Verify file exists
    assert!(expected_path.exists());
    let metadata = std::fs::metadata(&expected_path)?;
    assert_eq!(metadata.len(), 10);

    // Use Reader
    let mut reader = G729WavReader::new(expected_path);
    let pcm = reader.read_all().await?;
    // 10 bytes -> 1 frame -> 80 samples (10ms at 8kHz)
    assert_eq!(pcm.len(), 80);

    Ok(())
}
