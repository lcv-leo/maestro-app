use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{self, Command},
};
use tauri::Manager;

#[derive(Clone)]
struct LogSession {
    id: String,
    path: PathBuf,
}

#[derive(Clone, Deserialize, Serialize)]
struct BootstrapConfig {
    schema_version: u8,
    credential_storage_mode: String,
    cloudflare_account_id: Option<String>,
    cloudflare_api_token_source: String,
    cloudflare_api_token_env_var: String,
    cloudflare_persistence_database: String,
    cloudflare_secret_store: String,
    windows_env_prefix: String,
    updated_at: String,
}

#[derive(Serialize)]
struct CloudflareEnvSnapshot {
    account_id: Option<String>,
    account_id_env_var: Option<String>,
    api_token_present: bool,
    api_token_env_var: Option<String>,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            credential_storage_mode: "local_json".to_string(),
            cloudflare_account_id: None,
            cloudflare_api_token_source: "prompt_each_launch".to_string(),
            cloudflare_api_token_env_var: "MAESTRO_CLOUDFLARE_API_TOKEN".to_string(),
            cloudflare_persistence_database: "maestro_db".to_string(),
            cloudflare_secret_store: "maestro".to_string(),
            windows_env_prefix: "MAESTRO_".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
struct RuntimeProfile {
    app_name: &'static str,
    storage_policy: &'static str,
    target_platform: &'static str,
    log_dir: String,
    log_file: String,
    log_session_id: String,
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
    session_id: String,
}

#[tauri::command]
fn runtime_profile(log_session: tauri::State<LogSession>) -> RuntimeProfile {
    RuntimeProfile {
        app_name: "Maestro Editorial AI",
        storage_policy: "app-folder-json-only",
        target_platform: "Windows 11+",
        log_dir: logs_dir().to_string_lossy().to_string(),
        log_file: log_session.path.to_string_lossy().to_string(),
        log_session_id: log_session.id.clone(),
    }
}

#[tauri::command]
fn write_log_event(
    log_session: tauri::State<LogSession>,
    event: LogEventInput,
) -> Result<LogWriteResult, String> {
    write_log_record(&log_session, event)
}

fn write_log_record(
    log_session: &LogSession,
    event: LogEventInput,
) -> Result<LogWriteResult, String> {
    let dir = logs_dir();
    fs::create_dir_all(&dir).map_err(|error| format!("failed to create log dir: {error}"))?;

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
        },
        "session": {
            "id": log_session.id.clone(),
            "log_file": log_session.path.to_string_lossy().to_string()
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_session.path)
        .map_err(|error| format!("failed to open log file: {error}"))?;
    writeln!(file, "{record}").map_err(|error| format!("failed to write log record: {error}"))?;

    Ok(LogWriteResult {
        path: log_session.path.to_string_lossy().to_string(),
        session_id: log_session.id.clone(),
    })
}

#[tauri::command]
fn diagnostics_snapshot(log_session: tauri::State<LogSession>) -> Value {
    let dir = logs_dir();
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
        "active_log_file": log_session.path.to_string_lossy(),
        "log_session_id": log_session.id.clone(),
        "files": files,
        "hint": "Attach the newest per-run data/logs/maestro-*.ndjson file when asking Codex to diagnose a Maestro issue."
    })
}

#[tauri::command]
fn read_bootstrap_config() -> Result<BootstrapConfig, String> {
    let path = bootstrap_config_path();
    if !path.exists() {
        let config = BootstrapConfig::default();
        persist_bootstrap_config(&path, &config)?;
        return Ok(config);
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read bootstrap config: {error}"))?;
    let mut config: BootstrapConfig = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse bootstrap config: {error}"))?;
    config.credential_storage_mode =
        normalize_storage_mode(&config.credential_storage_mode).to_string();
    Ok(config)
}

#[tauri::command]
fn write_bootstrap_config(config: BootstrapConfig) -> Result<BootstrapConfig, String> {
    let path = bootstrap_config_path();
    let account_id = config
        .cloudflare_account_id
        .map(|value| sanitize_text(value.trim(), 160))
        .filter(|value| !value.is_empty());
    let sanitized = BootstrapConfig {
        schema_version: 1,
        credential_storage_mode: normalize_storage_mode(&config.credential_storage_mode)
            .to_string(),
        cloudflare_account_id: account_id,
        cloudflare_api_token_source: normalize_cloudflare_token_source(
            &config.cloudflare_api_token_source,
        )
        .to_string(),
        cloudflare_api_token_env_var: sanitize_short(&config.cloudflare_api_token_env_var, 80),
        cloudflare_persistence_database: sanitize_short(
            &config.cloudflare_persistence_database,
            80,
        ),
        cloudflare_secret_store: sanitize_short(&config.cloudflare_secret_store, 80),
        windows_env_prefix: sanitize_short(&config.windows_env_prefix, 80),
        updated_at: Utc::now().to_rfc3339(),
    };

    persist_bootstrap_config(&path, &sanitized)?;
    Ok(sanitized)
}

#[tauri::command]
fn cloudflare_env_snapshot() -> CloudflareEnvSnapshot {
    let account_id = first_env_value(&[
        "MAESTRO_CLOUDFLARE_ACCOUNT_ID",
        "CLOUDFLARE_ACCOUNT_ID",
        "CF_ACCOUNT_ID",
    ]);
    let api_token = first_env_value(&[
        "MAESTRO_CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_API_TOKEN",
        "CF_API_TOKEN",
    ]);

    CloudflareEnvSnapshot {
        account_id: account_id
            .as_ref()
            .map(|(_, value)| sanitize_text(value.trim(), 160))
            .filter(|value| !value.is_empty()),
        account_id_env_var: account_id.map(|(name, _)| name),
        api_token_present: api_token.is_some(),
        api_token_env_var: api_token.map(|(name, _)| name),
    }
}

#[tauri::command]
fn dependency_preflight() -> Value {
    let cloudflare = cloudflare_env_snapshot();
    let cloudflare_value = match (cloudflare.account_id.as_ref(), cloudflare.api_token_present) {
        (Some(_), true) => "account id + token detectados",
        (Some(_), false) => "account id detectado; token ausente",
        (None, true) => "token detectado; account id ausente",
        (None, false) => "env vars nao detectadas",
    };
    let cloudflare_tone = if cloudflare.account_id.is_some() && cloudflare.api_token_present {
        "ok"
    } else {
        "warn"
    };

    json!({
        "checks": [
            {
                "label": "WebView2",
                "value": "ativo pelo runtime Tauri",
                "tone": "ok"
            },
            command_check("Claude CLI", "claude", &["--version"]),
            command_check("Codex CLI", "codex", &["--version"]),
            command_check("Gemini CLI", "gemini", &["--version"]),
            command_check("Node.js", "node", &["--version"]),
            command_check("npm", "npm", &["--version"]),
            command_check("Rust cargo", "cargo", &["--version"]),
            command_check("GitHub CLI", "gh", &["--version"]),
            {
                "label": "Cloudflare env",
                "value": cloudflare_value,
                "tone": cloudflare_tone
            },
            {
                "label": "Wrangler",
                "value": "usar npx --yes wrangler@latest quando autorizado",
                "tone": "pending"
            }
        ]
    })
}

fn app_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn logs_dir() -> PathBuf {
    app_root().join("data").join("logs")
}

fn config_dir() -> PathBuf {
    app_root().join("data").join("config")
}

fn bootstrap_config_path() -> PathBuf {
    config_dir().join("bootstrap.json")
}

fn create_log_session() -> LogSession {
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let id = format!("{timestamp}-pid{}", process::id());
    LogSession {
        id: id.clone(),
        path: logs_dir().join(format!("maestro-{id}.ndjson")),
    }
}

fn persist_bootstrap_config(path: &PathBuf, config: &BootstrapConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create config dir: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("failed to serialize bootstrap config: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("failed to write bootstrap config: {error}"))
}

fn normalize_storage_mode(value: &str) -> &'static str {
    match value {
        "windows_env" => "windows_env",
        "cloudflare" => "cloudflare",
        _ => "local_json",
    }
}

fn normalize_cloudflare_token_source(value: &str) -> &'static str {
    match value {
        "windows_env" => "windows_env",
        "local_encrypted" => "local_encrypted",
        _ => "prompt_each_launch",
    }
}

fn first_env_value(candidates: &[&str]) -> Option<(String, String)> {
    candidates.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| ((*name).to_string(), value))
    })
}

fn command_check(label: &str, command: &str, args: &[&str]) -> Value {
    match Command::new(command).args(args).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("detectado")
                .trim();
            json!({
                "label": label,
                "value": sanitize_text(detail, 160),
                "tone": "ok"
            })
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            json!({
                "label": label,
                "value": sanitize_text(stderr.trim(), 160),
                "tone": "warn"
            })
        }
        Err(_) => json!({
            "label": label,
            "value": "nao encontrado no PATH",
            "tone": "blocked"
        }),
    }
}

fn sanitize_short(value: &str, max_len: usize) -> String {
    sanitize_text(value, max_len)
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
        })
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
        .manage(create_log_session())
        .setup(|app| {
            let log_session = app.state::<LogSession>();
            let _ = write_log_record(
                &log_session,
                LogEventInput {
                    level: "info".to_string(),
                    category: "app.lifecycle".to_string(),
                    message: "native runtime started".to_string(),
                    context: Some(json!({
                        "app_root": app_root().to_string_lossy(),
                        "log_dir": logs_dir().to_string_lossy(),
                        "log_file": log_session.path.to_string_lossy(),
                        "log_session_id": log_session.id.clone()
                    })),
                },
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_profile,
            write_log_event,
            diagnostics_snapshot,
            read_bootstrap_config,
            write_bootstrap_config,
            cloudflare_env_snapshot,
            dependency_preflight
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Maestro Editorial AI");
}
