use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
    http::StatusCode,
};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use crate::app::AppState;

#[derive(Serialize)]
pub struct PlaybookInfo {
    name: String,
    updated: String,
}

#[derive(Serialize)]
pub struct RecordInfo {
    id: String,
    date: String,
    duration: String,
    status: String,
}

pub async fn list_playbooks() -> impl IntoResponse {
    let mut playbooks = Vec::new();
    let path = PathBuf::from("config/playbook");
    
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".md") {
                            let updated = metadata.modified()
                                .ok()
                                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                                .unwrap_or_default();
                            
                            playbooks.push(PlaybookInfo {
                                name: name.to_string(),
                                updated,
                            });
                        }
                    }
                }
            }
        }
    }
    
    Json(playbooks)
}

pub async fn get_playbook(Path(name): Path<String>) -> impl IntoResponse {
    let path = PathBuf::from("config/playbook").join(&name);
    
    // Security check: prevent directory traversal
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return (StatusCode::BAD_REQUEST, "Invalid filename").into_response();
    }

    match fs::read_to_string(path) {
        Ok(content) => content.into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Playbook not found").into_response(),
    }
}

pub async fn save_playbook(Path(name): Path<String>, body: String) -> impl IntoResponse {
    let path = PathBuf::from("config/playbook").join(&name);
    
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return (StatusCode::BAD_REQUEST, "Invalid filename").into_response();
    }

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    match fs::write(path, body) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn list_records(State(state): State<AppState>) -> impl IntoResponse {
    let mut records = Vec::new();
    let path = PathBuf::from(state.config.recorder_path());
    
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".jsonl") {
                            let updated = metadata.modified()
                                .ok()
                                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                                .unwrap_or_default();
                            
                            // In a real app, we'd parse the file to get duration/status
                            // For now, just return file info
                            records.push(RecordInfo {
                                id: name.replace(".events.jsonl", ""),
                                date: updated,
                                duration: "0s".to_string(), // Placeholder
                                status: "completed".to_string(), // Placeholder
                            });
                        }
                    }
                }
            }
        }
    }
    
    // Sort by date desc
    records.sort_by(|a, b| b.date.cmp(&a.date));
    
    Json(records)
}

pub async fn index() -> impl IntoResponse {
    match fs::read_to_string("static/index.html") {
        Ok(content) => (StatusCode::OK, [("content-type", "text/html")], content).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Index not found").into_response(),
    }
}
