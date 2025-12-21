use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use std::sync::Arc;
use tokio::signal;
use tracing::{Level, info, warn};
use tracing_subscriber::FmtSubscriber;
use voice_engine::media::engine::StreamEngine;

use active_call::app::AppStateBuilder;
use active_call::config::{Cli, Config};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let cli = Cli::parse();

    let (mut config, config_path) = if let Some(path) = cli.conf {
        let config = Config::load(&path).unwrap_or_else(|e| {
            warn!("Failed to load config from {}: {}, using defaults", path, e);
            Config::default()
        });
        (config, Some(path))
    } else {
        (Config::default(), None)
    };

    if let Some(http) = cli.http {
        config.http_addr = http;
    }

    if let Some(sip) = cli.sip {
        if let Ok(port) = sip.parse::<u16>() {
            config.sip_port = port;
        } else if let Ok(socket_addr) = sip.parse::<std::net::SocketAddr>() {
            config.sip_addr = socket_addr.ip().to_string();
            config.sip_port = socket_addr.port();
        } else {
            config.sip_addr = sip;
        }
    }

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("Starting active-call service...");

    let stream_engine = Arc::new(StreamEngine::new());

    let app_state = AppStateBuilder::new()
        .with_config(config.clone())
        .with_stream_engine(stream_engine)
        .with_config_metadata(config_path, chrono::Utc::now())
        .build()
        .await?;

    info!("AppState started");

    tokio::select! {
        res = app_state.serve() => {
            if let Err(e) = res {
                warn!("AppState server error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            info!("Shutdown signal received");
        }
    }
    info!("Shutting down...");
    Ok(())
}
