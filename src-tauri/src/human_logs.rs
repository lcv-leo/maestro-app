use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use crate::{checked_data_child_path, human_log_path_for, sanitize_text};

pub(crate) fn severity_number_for(level: &str) -> u8 {
    match level.to_ascii_lowercase().as_str() {
        "debug" => 5,
        "info" => 9,
        "warn" | "warning" => 13,
        "error" => 17,
        "fatal" => 21,
        _ => 1,
    }
}

pub(crate) fn human_log_summary(_category: &str, message: &str, context: &Value) -> String {
    let mut parts = Vec::new();
    for key in [
        "run_id",
        "round",
        "agent",
        "role",
        "status",
        "tone",
        "duration_ms",
        "elapsed_seconds",
        "cost_usd",
    ] {
        if let Some(value) = context.get(key) {
            if value.is_null() {
                continue;
            }
            let rendered = value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string());
            if !rendered.trim().is_empty() {
                parts.push(format!("{key}={}", sanitize_text(&rendered, 120)));
            }
        }
    }
    if parts.is_empty() {
        sanitize_text(message, 300)
    } else {
        format!("{} | {}", sanitize_text(message, 220), parts.join(" "))
    }
}

pub(crate) fn write_human_log_projection(
    raw_log_path: &Path,
    record: &Value,
) -> Result<(), String> {
    let category = record
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if should_collapse_human_log_event(category, record.get("context").unwrap_or(&Value::Null)) {
        return Ok(());
    }

    let path = checked_data_child_path(&human_log_path_for(raw_log_path))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create human log dir: {error}"))?;
    }
    let timestamp = record
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let level = record
        .get("severity_text")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            record
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("INFO")
        });
    let summary = record
        .get("human_summary")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            record
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
        });
    let line = format!(
        "{timestamp} {:<5} {:<42} {}\n",
        level,
        sanitize_text(category, 42),
        sanitize_text(summary, 900)
    );
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("failed to open human log file: {error}"))?;
    file.write_all(line.as_bytes())
        .map_err(|error| format!("failed to write human log line: {error}"))
}

pub(crate) fn should_collapse_human_log_event(category: &str, context: &Value) -> bool {
    match category {
        "session.editorial.heartbeat" | "session.agent.running" => {
            let elapsed = context
                .get("elapsed_seconds")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            elapsed == 0 || elapsed % 300 != 0
        }
        _ => false,
    }
}
