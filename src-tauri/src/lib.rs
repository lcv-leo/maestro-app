use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{self, Command, Output},
    time::Duration,
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
    account_id_env_scope: Option<String>,
    api_token_present: bool,
    api_token_env_var: Option<String>,
    api_token_env_scope: Option<String>,
}

#[derive(Deserialize)]
struct CloudflareProbeRequest {
    account_id: String,
    api_token: Option<String>,
    api_token_env_var: String,
    persistence_database: String,
    publication_database: String,
    secret_store: String,
}

#[derive(Serialize)]
struct CloudflareProbeRow {
    label: String,
    value: String,
    tone: String,
}

#[derive(Serialize)]
struct CloudflareProbeResult {
    rows: Vec<CloudflareProbeRow>,
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
            .map(|(_, _, value)| sanitize_text(value.trim(), 160))
            .filter(|value| !value.is_empty()),
        account_id_env_var: account_id.as_ref().map(|(name, _, _)| name.clone()),
        account_id_env_scope: account_id.map(|(_, scope, _)| scope),
        api_token_present: api_token.is_some(),
        api_token_env_var: api_token.as_ref().map(|(name, _, _)| name.clone()),
        api_token_env_scope: api_token.map(|(_, scope, _)| scope),
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

#[tauri::command]
fn verify_cloudflare_credentials(
    log_session: tauri::State<LogSession>,
    request: CloudflareProbeRequest,
) -> CloudflareProbeResult {
    let result = run_cloudflare_probe(&request);
    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: if result
                .rows
                .iter()
                .any(|row| row.tone == "error" || row.tone == "blocked")
            {
                "warn".to_string()
            } else {
                "info".to_string()
            },
            category: "settings.cloudflare.verify_completed".to_string(),
            message: "Cloudflare credential validation completed".to_string(),
            context: Some(json!({
                "account_id_present": !request.account_id.trim().is_empty(),
                "token_source": token_source_label(&request),
                "persistence_database": sanitize_short(&request.persistence_database, 80),
                "publication_database": sanitize_short(&request.publication_database, 80),
                "secret_store": sanitize_short(&request.secret_store, 80),
                "rows": result.rows.iter().map(|row| json!({
                    "label": row.label,
                    "tone": row.tone
                })).collect::<Vec<_>>()
            })),
        },
    );
    result
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

fn first_env_value(candidates: &[&str]) -> Option<(String, String, String)> {
    candidates.iter().find_map(|name| {
        env_value_with_scope(name).map(|(scope, value)| ((*name).to_string(), scope, value))
    })
}

fn env_value_with_scope(name: &str) -> Option<(String, String)> {
    if let Some(value) = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(("process".to_string(), value));
    }

    #[cfg(windows)]
    {
        if let Some(value) = windows_registry_env_value(r"HKCU\Environment", name) {
            return Some(("user".to_string(), value));
        }

        if let Some(value) = windows_registry_env_value(
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            name,
        ) {
            return Some(("machine".to_string(), value));
        }
    }

    None
}

#[cfg(windows)]
fn windows_registry_env_value(key: &str, name: &str) -> Option<String> {
    let output = Command::new("reg.exe")
        .args(["query", key, "/v", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with(name) {
            return None;
        }
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        let type_index = parts.iter().position(|part| part.starts_with("REG_"))?;
        let value = parts
            .iter()
            .skip(type_index + 1)
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn token_source_label(request: &CloudflareProbeRequest) -> String {
    if request
        .api_token
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        "ui_session_field".to_string()
    } else if !request.api_token_env_var.trim().is_empty() {
        format!("env:{}", sanitize_short(&request.api_token_env_var, 80))
    } else {
        "env:auto".to_string()
    }
}

fn token_from_probe_request(request: &CloudflareProbeRequest) -> Option<(String, String)> {
    if let Some(token) = request
        .api_token
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(("campo desta sessao".to_string(), token));
    }

    let requested_env = request.api_token_env_var.trim();
    if !requested_env.is_empty() {
        if let Some((scope, value)) = env_value_with_scope(requested_env) {
            return Some((format!("{requested_env} ({scope})"), value));
        }
    }

    first_env_value(&[
        "MAESTRO_CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_API_TOKEN",
        "CF_API_TOKEN",
    ])
    .map(|(name, _, value)| (name, value))
}

fn run_cloudflare_probe(request: &CloudflareProbeRequest) -> CloudflareProbeResult {
    let token = token_from_probe_request(request);
    let account_id = sanitize_short(request.account_id.trim(), 80);
    let persistence_database = sanitize_short(&request.persistence_database, 80);
    let publication_database = sanitize_short(&request.publication_database, 80);
    let secret_store = sanitize_short(&request.secret_store, 80);
    let mut rows = Vec::new();

    let Some((token_source, token_value)) = token else {
        return CloudflareProbeResult {
            rows: vec![
                probe_row(
                    "Token ativo",
                    "ausente: informe token no campo ou em env var",
                    "blocked",
                ),
                probe_row("Conta acessivel", "nao executado sem token", "blocked"),
                probe_row("D1 Read/Edit", "nao executado sem token", "blocked"),
                probe_row("Secrets Store", "nao executado sem token", "blocked"),
            ],
        };
    };

    if token_value.starts_with("cfat_") && account_id.is_empty() {
        return CloudflareProbeResult {
            rows: vec![
                probe_row(
                    "Token ativo",
                    "account token exige Account ID para verificacao",
                    "blocked",
                ),
                probe_row("Conta acessivel", "account id ausente", "blocked"),
                probe_row("D1 Read/Edit", "nao executado sem account id", "blocked"),
                probe_row("Secrets Store", "nao executado sem account id", "blocked"),
            ],
        };
    }

    let client = match Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(format!(
            "Maestro Editorial AI/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return CloudflareProbeResult {
                rows: vec![
                    probe_row(
                        "Token ativo",
                        format!("cliente HTTP falhou: {error}"),
                        "error",
                    ),
                    probe_row("Conta acessivel", "nao executado", "blocked"),
                    probe_row("D1 Read/Edit", "nao executado", "blocked"),
                    probe_row("Secrets Store", "nao executado", "blocked"),
                ],
            };
        }
    };

    let verify_path = cloudflare_verify_path(&token_value, &account_id);
    match cloudflare_get(&client, &token_value, &verify_path) {
        Ok(value) => {
            let status = value
                .get("result")
                .and_then(|result| result.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("status desconhecido");
            if status == "active" {
                rows.push(probe_row(
                    "Token ativo",
                    format!(
                        "active via {token_source}; {}",
                        cloudflare_token_kind(&token_value)
                    ),
                    "ok",
                ));
            } else {
                rows.push(probe_row(
                    "Token ativo",
                    format!("token retornou status {status}"),
                    "error",
                ));
                rows.push(probe_row("Conta acessivel", "nao executado", "blocked"));
                rows.push(probe_row("D1 Read/Edit", "nao executado", "blocked"));
                rows.push(probe_row("Secrets Store", "nao executado", "blocked"));
                return CloudflareProbeResult { rows };
            }
        }
        Err(error) => {
            rows.push(probe_row("Token ativo", error, "error"));
            rows.push(probe_row("Conta acessivel", "nao executado", "blocked"));
            rows.push(probe_row("D1 Read/Edit", "nao executado", "blocked"));
            rows.push(probe_row("Secrets Store", "nao executado", "blocked"));
            return CloudflareProbeResult { rows };
        }
    }

    if account_id.is_empty() {
        rows.push(probe_row(
            "Conta acessivel",
            "account id ausente",
            "blocked",
        ));
        rows.push(probe_row(
            "D1 Read/Edit",
            "nao executado sem account id",
            "blocked",
        ));
        rows.push(probe_row(
            "Secrets Store",
            "nao executado sem account id",
            "blocked",
        ));
        return CloudflareProbeResult { rows };
    }

    let account_path = format!("/accounts/{account_id}");
    match cloudflare_get(&client, &token_value, &account_path) {
        Ok(_) => rows.push(probe_row("Conta acessivel", "account id acessivel", "ok")),
        Err(error) => {
            rows.push(probe_row("Conta acessivel", error, "error"));
            rows.push(probe_row("D1 Read/Edit", "nao executado", "blocked"));
            rows.push(probe_row("Secrets Store", "nao executado", "blocked"));
            return CloudflareProbeResult { rows };
        }
    }

    let d1_path = format!("/accounts/{account_id}/d1/database");
    match cloudflare_get(&client, &token_value, &d1_path) {
        Ok(value) => {
            let names = cloudflare_result_names(&value);
            let mut missing = Vec::new();
            if !persistence_database.is_empty() && !names.contains(&persistence_database) {
                missing.push(persistence_database.clone());
            }
            if !publication_database.is_empty() && !names.contains(&publication_database) {
                missing.push(publication_database.clone());
            }

            if missing.is_empty() {
                rows.push(probe_row(
                    "D1 Read/Edit",
                    format!("{persistence_database} + {publication_database} acessiveis"),
                    "ok",
                ));
            } else {
                rows.push(probe_row(
                    "D1 Read/Edit",
                    format!("endpoint D1 acessivel; ausente: {}", missing.join(", ")),
                    "warn",
                ));
            }
        }
        Err(error) => rows.push(probe_row("D1 Read/Edit", error, "error")),
    }

    let stores_path = format!("/accounts/{account_id}/secrets_store/stores");
    match cloudflare_get(&client, &token_value, &stores_path) {
        Ok(value) => {
            let stores = cloudflare_result_names(&value);
            if secret_store.is_empty() {
                rows.push(probe_row("Secrets Store", "endpoint acessivel", "ok"));
            } else if stores.contains(&secret_store) {
                rows.push(probe_row(
                    "Secrets Store",
                    format!("store {secret_store} acessivel"),
                    "ok",
                ));
            } else {
                rows.push(probe_row(
                    "Secrets Store",
                    format!("endpoint acessivel; store {secret_store} ausente"),
                    "warn",
                ));
            }
        }
        Err(error) => rows.push(probe_row("Secrets Store", error, "error")),
    }

    CloudflareProbeResult { rows }
}

fn cloudflare_get(client: &Client, token: &str, path: &str) -> Result<Value, String> {
    let url = format!("https://api.cloudflare.com/client/v4{path}");
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .map_err(|error| format!("falha HTTP: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("falha ao ler resposta Cloudflare: {error}"))?;
    let value: Value = serde_json::from_str(&body).map_err(|error| {
        format!(
            "resposta Cloudflare invalida (HTTP {}): {}",
            status.as_u16(),
            sanitize_text(&error.to_string(), 120)
        )
    })?;
    let success = value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| status.is_success());

    if status.is_success() && success {
        Ok(value)
    } else {
        Err(cloudflare_error_summary(status.as_u16(), &value))
    }
}

fn cloudflare_verify_path(token: &str, account_id: &str) -> String {
    if token.starts_with("cfat_") && !account_id.is_empty() {
        format!("/accounts/{account_id}/tokens/verify")
    } else {
        "/user/tokens/verify".to_string()
    }
}

fn cloudflare_token_kind(token: &str) -> &'static str {
    if token.starts_with("cfat_") {
        "account token"
    } else if token.starts_with("cfut_") {
        "user token"
    } else if token.starts_with("cfk_") {
        "user api key"
    } else {
        "legacy token format"
    }
}

fn cloudflare_error_summary(status: u16, value: &Value) -> String {
    let errors = value
        .get("errors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("message")
                        .and_then(Value::as_str)
                        .map(|message| sanitize_text(message, 180))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if errors.is_empty() {
        format!("Cloudflare HTTP {status}")
    } else {
        format!("Cloudflare HTTP {status}: {}", errors.join("; "))
    }
}

fn cloudflare_result_names(value: &Value) -> BTreeSet<String> {
    value
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .flat_map(|item| {
            ["name", "id", "uuid"]
                .into_iter()
                .filter_map(|key| item.get(key).and_then(Value::as_str))
        })
        .map(|name| name.to_string())
        .collect()
}

fn probe_row(
    label: impl Into<String>,
    value: impl Into<String>,
    tone: impl Into<String>,
) -> CloudflareProbeRow {
    CloudflareProbeRow {
        label: sanitize_text(&label.into(), 80),
        value: sanitize_text(&value.into(), 240),
        tone: sanitize_short(&tone.into(), 16),
    }
}

fn command_check(label: &str, command: &str, args: &[&str]) -> Value {
    let resolved = resolve_command(command);
    let output = if let Some(path) = resolved.as_ref() {
        run_resolved_command(path, args)
    } else {
        Command::new(command).args(args).output()
    };

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("detectado")
                .trim();
            let resolved_note = resolved
                .as_ref()
                .map(|path| format!(" via {}", path.to_string_lossy()))
                .unwrap_or_default();
            json!({
                "label": label,
                "value": sanitize_text(&format!("{detail}{resolved_note}"), 220),
                "tone": "ok"
            })
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = stderr
                .lines()
                .chain(stdout.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("comando retornou falha")
                .trim();
            json!({
                "label": label,
                "value": sanitize_text(detail, 220),
                "tone": "warn"
            })
        }
        Err(error) => json!({
            "label": label,
            "value": sanitize_text(&format!("nao encontrado/executado: {error}"), 220),
            "tone": "blocked"
        }),
    }
}

fn resolve_command(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.is_absolute() || command.contains('\\') || command.contains('/') {
        return command_candidate_paths(command_path)
            .into_iter()
            .find(|path| path.is_file());
    }

    command_search_dirs()
        .into_iter()
        .flat_map(|dir| command_candidate_paths(&dir.join(command)))
        .find(|path| path.is_file())
}

fn command_candidate_paths(path: &Path) -> Vec<PathBuf> {
    if path.extension().is_some() {
        return vec![path.to_path_buf()];
    }

    #[cfg(windows)]
    {
        ["exe", "cmd", "bat", "ps1", ""]
            .into_iter()
            .map(|ext| {
                if ext.is_empty() {
                    path.to_path_buf()
                } else {
                    path.with_extension(ext)
                }
            })
            .collect()
    }

    #[cfg(not(windows))]
    {
        vec![path.to_path_buf()]
    }
}

fn command_search_dirs() -> Vec<PathBuf> {
    let mut dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    #[cfg(windows)]
    {
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let user_profile = PathBuf::from(user_profile);
            dirs.push(user_profile.join(".cargo").join("bin"));
        }
        if let Some(app_data) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(app_data).join("npm"));
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let local_app_data = PathBuf::from(local_app_data);
            dirs.push(local_app_data.join("Programs").join("nodejs"));
            dirs.push(
                local_app_data
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
            );
        }
        dirs.push(PathBuf::from(r"C:\Program Files\nodejs"));
        dirs.push(PathBuf::from(r"C:\nvm4w\nodejs"));
        dirs.push(PathBuf::from(r"C:\Program Files\GitHub CLI"));
    }

    let mut seen = BTreeSet::new();
    dirs.into_iter()
        .filter(|dir| seen.insert(dir.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

fn run_resolved_command(path: &Path, args: &[&str]) -> std::io::Result<Output> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    #[cfg(windows)]
    {
        if extension == "cmd" || extension == "bat" {
            return Command::new(
                std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string()),
            )
            .arg("/C")
            .arg(path)
            .args(args)
            .output();
        }

        if extension == "ps1" {
            return Command::new("powershell.exe")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(path)
                .args(args)
                .output();
        }
    }

    Command::new(path).args(args).output()
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
                || part.starts_with("cfat_")
                || part.starts_with("cfk_")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_account_owned_tokens_to_account_verify_endpoint() {
        assert_eq!(
            cloudflare_verify_path("cfat_example", "d65b76a0e64c3791e932edd9163b1c71"),
            "/accounts/d65b76a0e64c3791e932edd9163b1c71/tokens/verify"
        );
    }

    #[test]
    fn routes_user_tokens_to_user_verify_endpoint() {
        assert_eq!(
            cloudflare_verify_path("cfut_example", "d65b76a0e64c3791e932edd9163b1c71"),
            "/user/tokens/verify"
        );
    }

    #[test]
    fn redacts_cloudflare_token_prefixes() {
        let text = redact_secrets("cfat_secret cfut_secret cfk_secret");
        assert_eq!(text, "<redacted> <redacted> <redacted>");
    }
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
            dependency_preflight,
            verify_cloudflare_credentials
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Maestro Editorial AI");
}
