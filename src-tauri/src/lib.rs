use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[derive(Serialize)]
struct RuntimeProfile {
    app_name: &'static str,
    storage_policy: &'static str,
    target_platform: &'static str,
    log_dir: String,
}

#[derive(Deserialize)]
struct LogEventInput {
    level: String,
    category: String,
    message: String,
    context: Option<Value>,
}

#[derive(Serialize)]
struct LogWriteResult {
    path: String,
}

#[tauri::command]
fn runtime_profile(app: tauri::AppHandle<tauri::Wry>) -> RuntimeProfile {
    RuntimeProfile {
        app_name: "Maestro Editorial AI",
        storage_policy: "app-folder-json-only",
        target_platform: "Windows 11+",
        log_dir: logs_dir(&app).to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn write_log_event(
    app: tauri::AppHandle<tauri::Wry>,
    event: LogEventInput,
) -> Result<LogWriteResult, String> {
    let dir = logs_dir(&app);
    fs::create_dir_all(&dir).map_err(|error| format!("failed to create log dir: {error}"))?;

    let path = dir.join(format!("maestro-{}.ndjson", Utc::now().format("%Y-%m-%d")));
    let record = json!({
        "schema_version": 1,
        "timestamp": Utc::now().to_rfc3339(),
        "level": sanitize_short(&event.level, 16),
        "category": sanitize_short(&event.category, 80),
        "message": sanitize_text(&event.message, 500),
        "context": sanitize_value(event.context.unwrap_or(Value::Null), 8),
        "app": {
            "name": "Maestro Editorial AI",
            "version": env!("CARGO_PKG_VERSION"),
            "target": std::env::consts::OS,
            "arch": std::env::consts::ARCH
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("failed to open log file: {error}"))?;
    writeln!(file, "{record}").map_err(|error| format!("failed to write log record: {error}"))?;

    Ok(LogWriteResult {
        path: path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn diagnostics_snapshot(app: tauri::AppHandle<tauri::Wry>) -> Value {
    let dir = logs_dir(&app);
    let files = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            Some(json!({
                "name": entry.file_name().to_string_lossy(),
                "path": entry.path().to_string_lossy(),
                "bytes": metadata.len(),
                "modified": metadata.modified().ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs())
            }))
        })
        .collect::<Vec<_>>();

    json!({
        "log_dir": dir.to_string_lossy(),
        "files": files,
        "hint": "Attach the latest data/logs/*.ndjson file when asking Codex to diagnose a Maestro issue."
    })
}

fn app_root(app: &tauri::AppHandle<tauri::Wry>) -> PathBuf {
    let _ = app;

    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn logs_dir(app: &tauri::AppHandle<tauri::Wry>) -> PathBuf {
    app_root(app).join("data").join("logs")
}

fn sanitize_short(value: &str, max_len: usize) -> String {
    sanitize_text(value, max_len)
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':'))
        .collect::<String>()
}

fn sanitize_text(value: &str, max_len: usize) -> String {
    let redacted = redact_secrets(value);
    redacted.chars().take(max_len).collect()
}

fn sanitize_value(value: Value, depth: usize) -> Value {
    if depth == 0 {
        return Value::String("<max_depth_reached>".to_string());
    }

    match value {
        Value::String(text) => Value::String(sanitize_text(&text, 1200)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .take(80)
                .map(|item| sanitize_value(item, depth - 1))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .take(120)
                .map(|(key, value)| {
                    let lowered = key.to_ascii_lowercase();
                    if lowered.contains("secret")
                        || lowered.contains("token")
                        || lowered.contains("password")
                        || lowered.contains("credential")
                        || lowered.contains("api_key")
                    {
                        (key, Value::String("<redacted>".to_string()))
                    } else {
                        (sanitize_text(&key, 80), sanitize_value(value, depth - 1))
                    }
                })
                .collect(),
        ),
        primitive => primitive,
    }
}

fn redact_secrets(value: &str) -> String {
    let private_block_marker = format!("{}BEGIN", "-".repeat(5));

    value
        .split_whitespace()
        .map(|part| {
            if part.starts_with("sk-")
                || part.starts_with("sk-ant-")
                || part.starts_with("sk_live_")
                || part.starts_with("cfut_")
                || part.starts_with("xoxb-")
                || part.starts_with("xoxa-")
                || part.starts_with("xoxp-")
                || part.starts_with("xoxr-")
                || part.starts_with("xoxs-")
                || part.starts_with("ghp_")
                || part.starts_with("gho_")
                || part.starts_with("ghu_")
                || part.starts_with("ghs_")
                || part.starts_with("ghr_")
                || part.starts_with("AIza")
                || looks_like_resend_key(part)
                || looks_like_aws_access_key(part)
                || part.contains(&private_block_marker)
            {
                "<redacted>"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_resend_key(part: &str) -> bool {
    part.starts_with("re_") && part.len() >= 23
}

fn looks_like_aws_access_key(part: &str) -> bool {
    part.len() >= 20
        && part.starts_with("AKIA")
        && part
            .chars()
            .take(20)
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let _ = write_log_event(
                handle,
                LogEventInput {
                    level: "info".to_string(),
                    category: "app.lifecycle".to_string(),
                    message: "native runtime started".to_string(),
                    context: Some(json!({
                        "app_root": app_root(app.handle()).to_string_lossy(),
                        "log_dir": logs_dir(app.handle()).to_string_lossy()
                    })),
                },
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_profile,
            write_log_event,
            diagnostics_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Maestro Editorial AI");
}
