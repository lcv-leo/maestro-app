use chrono::Utc;
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{self, Command, Output, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

static NATIVE_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

#[derive(Clone, Deserialize)]
struct CliAdapterSmokeRequest {
    run_id: String,
    prompt_chars: usize,
    protocol_name: String,
    protocol_lines: usize,
    protocol_hash: String,
}

#[derive(Serialize)]
struct CliAdapterSmokeResult {
    run_id: String,
    agents: Vec<CliAdapterProbeResult>,
    all_ready: bool,
}

#[derive(Serialize)]
struct CliAdapterProbeResult {
    name: String,
    cli: String,
    tone: String,
    status: String,
    duration_ms: u128,
    exit_code: Option<i32>,
    marker_found: bool,
}

#[derive(Clone, Deserialize)]
struct EditorialSessionRequest {
    run_id: String,
    session_name: String,
    prompt: String,
    protocol_name: String,
    protocol_text: String,
    protocol_hash: String,
}

#[derive(Serialize)]
struct EditorialSessionResult {
    run_id: String,
    session_dir: String,
    final_markdown_path: Option<String>,
    session_minutes_path: String,
    prompt_path: String,
    protocol_path: String,
    draft_path: Option<String>,
    agents: Vec<EditorialAgentResult>,
    consensus_ready: bool,
    status: String,
}

#[derive(Clone, Serialize)]
struct EditorialAgentResult {
    name: String,
    role: String,
    cli: String,
    tone: String,
    status: String,
    duration_ms: u128,
    exit_code: Option<i32>,
    output_path: String,
}

#[derive(Clone)]
struct CliAdapterSpec {
    name: &'static str,
    command: &'static str,
    marker: &'static str,
    args: Vec<String>,
    timeout: Duration,
}

struct TimedCommandOutput {
    output: Output,
    duration_ms: u128,
    timed_out: bool,
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
    let sequence = NATIVE_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;

    let record = json!({
        "schema_version": 1,
        "timestamp": Utc::now().to_rfc3339(),
        "native_log_sequence": sequence,
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
        "process": {
            "pid": process::id(),
            "cwd": std::env::current_dir().ok().map(|path| path.to_string_lossy().to_string()),
            "app_root": app_root().to_string_lossy().to_string()
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
async fn dependency_preflight() -> Value {
    tauri::async_runtime::spawn_blocking(dependency_preflight_inner)
        .await
        .unwrap_or_else(|error| {
            json!({
                "checks": [
                    {
                        "label": "Preflight",
                        "value": sanitize_text(&format!("falha no worker de diagnostico: {error}"), 220),
                        "tone": "error"
                    }
                ]
            })
        })
}

fn dependency_preflight_inner() -> Value {
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

#[tauri::command]
fn run_cli_adapter_smoke(
    log_session: tauri::State<LogSession>,
    request: CliAdapterSmokeRequest,
) -> CliAdapterSmokeResult {
    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.cli_adapters.smoke_started".to_string(),
            message: "CLI adapter smoke started".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(&request.run_id, 120),
                "prompt_chars": request.prompt_chars,
                "protocol_name": sanitize_text(&request.protocol_name, 160),
                "protocol_lines": request.protocol_lines,
                "protocol_hash_prefix": sanitize_short(&request.protocol_hash, 16),
                "agents": ["claude", "codex", "gemini"]
            })),
        },
    );

    let handles = cli_adapter_specs(&request)
        .into_iter()
        .map(|spec| thread::spawn(move || run_cli_adapter_probe(spec)))
        .collect::<Vec<_>>();
    let agents = handles
        .into_iter()
        .map(|handle| {
            handle.join().unwrap_or_else(|_| CliAdapterProbeResult {
                name: "Unknown".to_string(),
                cli: "unknown".to_string(),
                tone: "error".to_string(),
                status: "thread do adaptador falhou".to_string(),
                duration_ms: 0,
                exit_code: None,
                marker_found: false,
            })
        })
        .collect::<Vec<_>>();
    let all_ready = agents.iter().all(|agent| agent.tone == "ok");
    let result = CliAdapterSmokeResult {
        run_id: sanitize_short(&request.run_id, 120),
        all_ready,
        agents,
    };

    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: if all_ready { "info" } else { "warn" }.to_string(),
            category: "session.cli_adapters.smoke_completed".to_string(),
            message: "CLI adapter smoke completed".to_string(),
            context: Some(json!({
                "run_id": result.run_id,
                "all_ready": result.all_ready,
                "agents": result.agents.iter().map(|agent| json!({
                    "name": agent.name,
                    "cli": agent.cli,
                    "tone": agent.tone,
                    "duration_ms": agent.duration_ms,
                    "exit_code": agent.exit_code,
                    "marker_found": agent.marker_found
                })).collect::<Vec<_>>()
            })),
        },
    );

    result
}

#[tauri::command]
async fn run_editorial_session(
    log_session: tauri::State<'_, LogSession>,
    request: EditorialSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let log_session = log_session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        run_editorial_session_blocking(log_session, request)
    })
    .await
    .map_err(|error| format!("editorial worker join failed: {error}"))?
}

fn run_editorial_session_blocking(
    log_session: LogSession,
    request: EditorialSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.editorial.started".to_string(),
            message: "real editorial session command received".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(&request.run_id, 120),
                "session_name": sanitize_text(&request.session_name, 200),
                "prompt_chars": request.prompt.chars().count(),
                "protocol_name": sanitize_text(&request.protocol_name, 200),
                "protocol_chars": request.protocol_text.chars().count(),
                "protocol_lines": request.protocol_text.lines().count(),
                "protocol_hash_prefix": sanitize_short(&request.protocol_hash, 16),
                "agents": ["claude", "codex", "gemini"],
                "artifact_policy": "raw agent outputs are written under data/sessions, not embedded in NDJSON"
            })),
        },
    );

    let result = match run_editorial_session_inner(&request, &log_session) {
        Ok(result) => result,
        Err(error) => {
            let _ = write_log_record(
                &log_session,
                LogEventInput {
                    level: "error".to_string(),
                    category: "session.editorial.failed".to_string(),
                    message: "real editorial session failed before structured result".to_string(),
                    context: Some(json!({
                        "run_id": sanitize_short(&request.run_id, 120),
                        "error": sanitize_text(&error, 500),
                        "session_name": sanitize_text(&request.session_name, 200),
                        "prompt_chars": request.prompt.chars().count(),
                        "protocol_chars": request.protocol_text.chars().count(),
                        "protocol_hash_prefix": sanitize_short(&request.protocol_hash, 16)
                    })),
                },
            );
            return Err(error);
        }
    };
    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: if result.consensus_ready {
                "info"
            } else {
                "warn"
            }
            .to_string(),
            category: "session.editorial.completed".to_string(),
            message: "real editorial session completed".to_string(),
            context: Some(json!({
                "run_id": result.run_id,
                "status": result.status,
                "consensus_ready": result.consensus_ready,
                "session_dir": result.session_dir,
                "final_markdown_path": result.final_markdown_path,
                "session_minutes_path": result.session_minutes_path,
                "agents": result.agents.iter().map(|agent| json!({
                    "name": agent.name,
                    "role": agent.role,
                    "tone": agent.tone,
                    "duration_ms": agent.duration_ms,
                    "exit_code": agent.exit_code,
                    "output_path": agent.output_path
                })).collect::<Vec<_>>()
            })),
        },
    );
    Ok(result)
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

fn sessions_dir() -> PathBuf {
    app_root().join("data").join("sessions")
}

fn create_log_session() -> LogSession {
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let id = format!("{timestamp}-pid{}", process::id());
    LogSession {
        id: id.clone(),
        path: logs_dir().join(format!("maestro-{id}.ndjson")),
    }
}

fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    apply_hidden_window_policy(&mut command);
    command
}

#[cfg(windows)]
fn apply_hidden_window_policy(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn apply_hidden_window_policy(_command: &mut Command) {}

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
    let output = hidden_command("reg.exe")
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

fn run_editorial_session_inner(
    request: &EditorialSessionRequest,
    log_session: &LogSession,
) -> Result<EditorialSessionResult, String> {
    let run_id = sanitize_short(&request.run_id, 120);
    if run_id.is_empty() {
        return Err("run_id vazio".to_string());
    }

    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err("prompt editorial vazio".to_string());
    }
    if request.protocol_text.trim().len() < 100 {
        return Err("protocolo editorial integral nao foi carregado".to_string());
    }

    let session_dir = sessions_dir().join(&run_id);
    let agent_dir = session_dir.join("agent-runs");
    fs::create_dir_all(&agent_dir)
        .map_err(|error| format!("failed to create session dir: {error}"))?;

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    write_text_file(
        &prompt_path,
        &format!(
            "# Prompt da Sessao\n\nSessao: {}\nRun: `{}`\n\n{}",
            sanitize_text(&request.session_name, 200),
            run_id,
            prompt
        ),
    )?;
    write_text_file(&protocol_path, &request.protocol_text)?;

    let mut agents = Vec::new();
    let mut current_draft = String::new();
    let mut current_draft_path: Option<PathBuf> = None;
    let draft_specs = vec![
        ("Claude", "claude", claude_args()),
        ("Codex", "codex", codex_args()),
        ("Gemini", "gemini", gemini_args()),
    ];

    for (name, command, args) in draft_specs {
        let output_path =
            agent_dir.join(format!("round-001-{}-draft.md", name.to_ascii_lowercase()));
        let draft_run = run_editorial_agent(
            log_session,
            &run_id,
            name,
            "draft",
            command,
            args,
            build_draft_prompt(request, &run_id),
            &output_path,
            None,
        );
        agents.push(draft_run.clone());
        let draft_artifact = fs::read_to_string(&output_path).unwrap_or_default();
        let draft_text = extract_stdout_block(&draft_artifact).unwrap_or(draft_artifact.as_str());
        if draft_run.tone != "error" && draft_run.tone != "blocked" && !draft_text.trim().is_empty()
        {
            current_draft = draft_text.trim().to_string();
            current_draft_path = Some(output_path);
            break;
        }

        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "warn".to_string(),
                category: "session.draft.retry".to_string(),
                message: "draft agent did not produce usable text; trying next available agent"
                    .to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "agent": name,
                    "status": draft_run.status,
                    "tone": draft_run.tone,
                    "next_policy": "continue_with_next_agent_without_final_delivery"
                })),
            },
        );
    }

    if current_draft.trim().is_empty() {
        let minutes_path = session_dir.join("ata-da-sessao.md");
        write_text_file(
            &minutes_path,
            &build_session_minutes(request, &run_id, &agents, false, None),
        )?;

        return Ok(EditorialSessionResult {
            run_id,
            session_dir: session_dir.to_string_lossy().to_string(),
            final_markdown_path: None,
            session_minutes_path: minutes_path.to_string_lossy().to_string(),
            prompt_path: prompt_path.to_string_lossy().to_string(),
            protocol_path: protocol_path.to_string_lossy().to_string(),
            draft_path: current_draft_path.map(|path| path.to_string_lossy().to_string()),
            agents,
            consensus_ready: false,
            status: "PAUSED_DRAFT_UNAVAILABLE".to_string(),
        });
    }

    let mut final_path: Option<PathBuf> = None;
    let mut round = 1usize;
    loop {
        let review_specs = vec![
            (
                "Claude",
                "review",
                "claude",
                claude_args(),
                agent_dir.join(format!("round-{round:03}-claude-review.md")),
            ),
            (
                "Codex",
                "review",
                "codex",
                codex_args(),
                agent_dir.join(format!("round-{round:03}-codex-review.md")),
            ),
            (
                "Gemini",
                "review",
                "gemini",
                gemini_args(),
                agent_dir.join(format!("round-{round:03}-gemini-review.md")),
            ),
        ];
        let review_handles = review_specs
            .into_iter()
            .map(|(name, role, command, args, output_path)| {
                let prompt = build_review_prompt(request, &run_id, &current_draft);
                let run_id = run_id.clone();
                let log_session = log_session.clone();
                thread::spawn(move || {
                    run_editorial_agent(
                        &log_session,
                        &run_id,
                        name,
                        role,
                        command,
                        args,
                        prompt,
                        &output_path,
                        None,
                    )
                })
            })
            .collect::<Vec<_>>();

        let mut round_results = Vec::new();
        for handle in review_handles {
            let result = handle.join().unwrap_or_else(|_| EditorialAgentResult {
                name: "Unknown".to_string(),
                role: "review".to_string(),
                cli: "unknown".to_string(),
                tone: "error".to_string(),
                status: "thread de revisao falhou".to_string(),
                duration_ms: 0,
                exit_code: None,
                output_path: String::new(),
            });
            round_results.push(result.clone());
            agents.push(result);
        }

        let consensus_ready = round_results
            .iter()
            .all(|agent| agent.tone == "ok" && agent.status == "READY");
        if consensus_ready {
            let path = session_dir.join("texto-final.md");
            write_text_file(&path, &current_draft)?;
            final_path = Some(path);
            break;
        }

        let operational_failure = round_results
            .iter()
            .any(|agent| agent.tone == "error" || agent.tone == "blocked");
        if operational_failure {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.review.operational_failure".to_string(),
                    message: "review round has an operational agent failure; final delivery remains unavailable".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "policy": "no_final_delivery_without_unanimity",
                        "next_state": "paused_for_operator_or_retry"
                    })),
                },
            );
            break;
        }

        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "info".to_string(),
                category: "session.review.not_ready".to_string(),
                message: "review round did not reach unanimity; continuing with a revision round"
                    .to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "round": round,
                    "policy": "continue_until_unanimous_ready",
                    "not_ready_agents": round_results.iter()
                        .filter(|agent| agent.status != "READY")
                        .map(|agent| agent.name.clone())
                        .collect::<Vec<_>>()
                })),
            },
        );

        round += 1;
        let revision_prompt =
            build_revision_prompt(request, &run_id, round, &current_draft, &round_results);
        let revision_specs = vec![
            ("Claude", "claude", claude_args()),
            ("Codex", "codex", codex_args()),
            ("Gemini", "gemini", gemini_args()),
        ];
        let mut revised = false;
        for (name, command, args) in revision_specs {
            let output_path = agent_dir.join(format!(
                "round-{round:03}-{}-revision.md",
                name.to_ascii_lowercase()
            ));
            let revision_run = run_editorial_agent(
                log_session,
                &run_id,
                name,
                "revision",
                command,
                args,
                revision_prompt.clone(),
                &output_path,
                None,
            );
            agents.push(revision_run.clone());
            let artifact = fs::read_to_string(&output_path).unwrap_or_default();
            let revised_text = extract_stdout_block(&artifact).unwrap_or(artifact.as_str());
            if revision_run.tone != "error"
                && revision_run.tone != "blocked"
                && !revised_text.trim().is_empty()
            {
                current_draft = revised_text.trim().to_string();
                current_draft_path = Some(output_path);
                revised = true;
                break;
            }
        }

        if !revised {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.revision.unavailable".to_string(),
                    message:
                        "no revision agent produced usable text; final delivery remains unavailable"
                            .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "policy": "no_final_delivery_without_unanimity"
                    })),
                },
            );
            break;
        }
    }

    let consensus_ready = final_path.is_some();
    let minutes_path = session_dir.join("ata-da-sessao.md");
    write_text_file(
        &minutes_path,
        &build_session_minutes(
            request,
            &run_id,
            &agents,
            consensus_ready,
            final_path.as_ref(),
        ),
    )?;

    Ok(EditorialSessionResult {
        run_id,
        session_dir: session_dir.to_string_lossy().to_string(),
        final_markdown_path: final_path.map(|path| path.to_string_lossy().to_string()),
        session_minutes_path: minutes_path.to_string_lossy().to_string(),
        prompt_path: prompt_path.to_string_lossy().to_string(),
        protocol_path: protocol_path.to_string_lossy().to_string(),
        draft_path: current_draft_path.map(|path| path.to_string_lossy().to_string()),
        agents,
        consensus_ready,
        status: if consensus_ready {
            "READY_UNANIMOUS".to_string()
        } else {
            "PAUSED_WITH_REAL_AGENT_OUTPUTS".to_string()
        },
    })
}

fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create artifact dir: {error}"))?;
    }
    fs::write(path, text).map_err(|error| format!("failed to write artifact: {error}"))
}

fn claude_args() -> Vec<String> {
    vec![
        "--print".to_string(),
        "--input-format".to_string(),
        "text".to_string(),
        "--output-format".to_string(),
        "text".to_string(),
        "--permission-mode".to_string(),
        "dontAsk".to_string(),
    ]
}

fn codex_args() -> Vec<String> {
    vec![
        "--ask-for-approval".to_string(),
        "never".to_string(),
        "exec".to_string(),
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        "read-only".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "Leia integralmente o bloco <stdin> fornecido pelo Maestro e responda conforme as instrucoes.".to_string(),
    ]
}

fn gemini_args() -> Vec<String> {
    vec![
        "--prompt".to_string(),
        "Leia o stdin integralmente e responda conforme as instrucoes do Maestro.".to_string(),
        "--output-format".to_string(),
        "text".to_string(),
        "--approval-mode".to_string(),
        "yolo".to_string(),
        "--skip-trust".to_string(),
    ]
}

fn build_draft_prompt(request: &EditorialSessionRequest, run_id: &str) -> String {
    format!(
        r#"# Maestro Editorial AI - Geracao Real

Run: `{run_id}`
Sessao: {}

Voce e o primeiro peer editorial. Leia integralmente o protocolo abaixo antes de escrever.
Gere um rascunho em Markdown puro para a solicitacao do operador.
Nao invente links. Se faltar evidencia, marque explicitamente `[EVIDENCIA_PENDENTE]`.

## Solicitacao do operador

{}

## Protocolo editorial integral

```markdown
{}
```
"#,
        sanitize_text(&request.session_name, 200),
        request.prompt,
        request.protocol_text
    )
}

fn build_review_prompt(request: &EditorialSessionRequest, run_id: &str, draft: &str) -> String {
    format!(
        r#"# Maestro Editorial AI - Revisao Real

Run: `{run_id}`
Sessao: {}

Leia integralmente o protocolo editorial e revise o rascunho abaixo.
Responda em Markdown.

Obrigatorio:
- A primeira linha deve ser exatamente `MAESTRO_STATUS: READY` ou `MAESTRO_STATUS: NOT_READY`.
- Use READY somente se o rascunho pode ser entregue como texto final conforme o protocolo.
- Use NOT_READY se houver falhas, links a verificar, violacao ABNT, falta de evidencia, confabulacao, ou problema editorial.
- Liste correcoes concretas.

## Solicitacao do operador

{}

## Protocolo editorial integral

```markdown
{}
```

## Rascunho a revisar

```markdown
{}
```
"#,
        sanitize_text(&request.session_name, 200),
        request.prompt,
        request.protocol_text,
        draft
    )
}

fn build_revision_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    round: usize,
    draft: &str,
    review_agents: &[EditorialAgentResult],
) -> String {
    let mut review_notes = String::new();
    for agent in review_agents {
        let artifact = fs::read_to_string(&agent.output_path).unwrap_or_default();
        let artifact_excerpt = artifact.chars().take(40_000).collect::<String>();
        review_notes.push_str(&format!(
            "\n### {} / {}\n\nStatus: `{}` (`{}`)\nArtifact: `{}`\n\n```markdown\n{}\n```\n",
            agent.name, agent.role, agent.status, agent.tone, agent.output_path, artifact_excerpt
        ));
    }

    format!(
        r#"# Maestro Editorial AI - Revisao de Rascunho

Run: `{run_id}`
Rodada de revisao: `{round}`
Sessao: {}

Leia integralmente o protocolo editorial, o rascunho atual e as manifestacoes dos peers.
Sua tarefa e produzir uma nova versao completa do texto em Markdown puro, incorporando todas as correcoes concretas.
Nao entregue comentarios sobre o processo. Entregue apenas o texto revisado.
Nao invente links. Se faltar evidencia, preserve marcador `[EVIDENCIA_PENDENTE]`.

## Solicitacao do operador

{}

## Protocolo editorial integral

```markdown
{}
```

## Rascunho atual

```markdown
{}
```

## Manifestacoes dos peers

{}
"#,
        sanitize_text(&request.session_name, 200),
        request.prompt,
        request.protocol_text,
        draft,
        review_notes
    )
}

fn run_editorial_agent(
    log_session: &LogSession,
    run_id: &str,
    name: &str,
    role: &str,
    command: &str,
    args: Vec<String>,
    stdin_text: String,
    output_path: &Path,
    timeout: Option<Duration>,
) -> EditorialAgentResult {
    let started = Instant::now();
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.started".to_string(),
            message: "editorial agent process starting".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(run_id, 120),
                "agent": name,
                "role": role,
                "cli": command,
                "stdin_chars": stdin_text.chars().count(),
                "timeout_seconds": timeout.map(|value| value.as_secs()),
                "timeout_policy": if timeout.is_some() { "diagnostic_or_limited" } else { "none_editorial_session" },
                "output_path": output_path.to_string_lossy().to_string()
            })),
        },
    );
    let Some(path) = resolve_command(command) else {
        let _ = write_text_file(
            output_path,
            &format!(
                "# {name} - {role}\n\n- CLI: `{command}`\n- Status: `CLI_NOT_FOUND`\n- PATH dirs checked: `{}`\n\nCLI nao encontrada no PATH efetivo.\n",
                command_search_dirs().len()
            ),
        );
        let result = EditorialAgentResult {
            name: name.to_string(),
            role: role.to_string(),
            cli: command.to_string(),
            tone: "blocked".to_string(),
            status: "CLI_NOT_FOUND".to_string(),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            output_path: output_path.to_string_lossy().to_string(),
        };
        log_editorial_agent_finished(log_session, run_id, &result, None, None, None, false);
        return result;
    };

    let command_result = if let Some(timeout) = timeout {
        run_resolved_command_with_timeout(&path, &args, timeout, Some(&stdin_text))
    } else {
        run_resolved_command_unbounded(&path, &args, Some(&stdin_text))
    };

    match command_result {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.output.stderr).to_string();
            let exit_code = result.output.status.code();
            let status = if role == "review" {
                extract_maestro_status(&stdout).unwrap_or("NOT_READY")
            } else if stdout.trim().is_empty() {
                "EMPTY_DRAFT"
            } else {
                "DRAFT_CREATED"
            };
            let tone = if result.timed_out {
                "error"
            } else if result.output.status.success()
                && (status == "READY" || status == "DRAFT_CREATED")
            {
                "ok"
            } else if result.output.status.success() {
                "warn"
            } else {
                "error"
            };
            let artifact = format!(
                "# {name} - {role}\n\n- CLI: `{command}`\n- Resolved path: `{}`\n- Args: `{}`\n- Status: `{status}`\n- Exit code: `{}`\n- Duration ms: `{}`\n- Timed out: `{}`\n- Stdin chars: `{}`\n- Stdout chars: `{}`\n- Stderr chars: `{}`\n\n## Stdout\n\n```text\n{}\n```\n\n## Stderr\n\n```text\n{}\n```\n",
                path.to_string_lossy(),
                sanitize_text(&args.join(" "), 1000),
                exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                result.duration_ms,
                result.timed_out,
                stdin_text.chars().count(),
                stdout.chars().count(),
                stderr.chars().count(),
                stdout,
                sanitize_text(&stderr, 8000)
            );
            let _ = write_text_file(output_path, &artifact);

            let agent_result = EditorialAgentResult {
                name: name.to_string(),
                role: role.to_string(),
                cli: command.to_string(),
                tone: tone.to_string(),
                status: status.to_string(),
                duration_ms: result.duration_ms,
                exit_code,
                output_path: output_path.to_string_lossy().to_string(),
            };
            log_editorial_agent_finished(
                log_session,
                run_id,
                &agent_result,
                Some(stdout.chars().count()),
                Some(stderr.chars().count()),
                Some(path.to_string_lossy().to_string()),
                result.timed_out,
            );
            agent_result
        }
        Err(error) => {
            let status = sanitize_text(&format!("EXEC_ERROR: {error}"), 240);
            let _ = write_text_file(output_path, &status);
            let agent_result = EditorialAgentResult {
                name: name.to_string(),
                role: role.to_string(),
                cli: command.to_string(),
                tone: "error".to_string(),
                status,
                duration_ms: started.elapsed().as_millis(),
                exit_code: None,
                output_path: output_path.to_string_lossy().to_string(),
            };
            log_editorial_agent_finished(
                log_session,
                run_id,
                &agent_result,
                None,
                None,
                Some(path.to_string_lossy().to_string()),
                false,
            );
            agent_result
        }
    }
}

fn log_editorial_agent_finished(
    log_session: &LogSession,
    run_id: &str,
    result: &EditorialAgentResult,
    stdout_chars: Option<usize>,
    stderr_chars: Option<usize>,
    resolved_path: Option<String>,
    timed_out: bool,
) {
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: if result.tone == "ok" {
                "info".to_string()
            } else if result.tone == "warn" || result.tone == "blocked" {
                "warn".to_string()
            } else {
                "error".to_string()
            },
            category: "session.agent.finished".to_string(),
            message: "editorial agent process finished".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(run_id, 120),
                "agent": result.name,
                "role": result.role,
                "cli": result.cli,
                "tone": result.tone,
                "status": result.status,
                "duration_ms": result.duration_ms,
                "exit_code": result.exit_code,
                "timed_out": timed_out,
                "resolved_path": resolved_path,
                "stdout_chars": stdout_chars,
                "stderr_chars": stderr_chars,
                "output_path": result.output_path
            })),
        },
    );
}

fn extract_maestro_status(output: &str) -> Option<&'static str> {
    output.lines().find_map(|line| {
        let normalized = line.trim().to_ascii_uppercase();
        if normalized == "MAESTRO_STATUS: READY" {
            Some("READY")
        } else if normalized == "MAESTRO_STATUS: NOT_READY" {
            Some("NOT_READY")
        } else {
            None
        }
    })
}

fn extract_stdout_block(artifact: &str) -> Option<&str> {
    let marker = "## Stdout\n\n```text\n";
    let start = artifact.find(marker)? + marker.len();
    let rest = &artifact[start..];
    let end = rest.find("\n```\n\n## Stderr")?;
    Some(rest[..end].trim())
}

fn build_session_minutes(
    request: &EditorialSessionRequest,
    run_id: &str,
    agents: &[EditorialAgentResult],
    consensus_ready: bool,
    final_path: Option<&PathBuf>,
) -> String {
    let mut text = format!(
        "# Ata da Sessao Maestro\n\n- Run: `{run_id}`\n- Sessao: {}\n- Protocolo: `{}`\n- Hash do protocolo: `{}`\n- Consenso unanime: `{}`\n- Texto final: `{}`\n\n## Solicitacao\n\n{}\n\n## Rodada 001\n\n",
        sanitize_text(&request.session_name, 200),
        sanitize_text(&request.protocol_name, 200),
        sanitize_short(&request.protocol_hash, 80),
        consensus_ready,
        final_path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "bloqueado".to_string()),
        request.prompt
    );

    for agent in agents {
        text.push_str(&format!(
            "- **{} / {}**: `{}` (`{}`), {} ms, artifact: `{}`\n",
            agent.name, agent.role, agent.status, agent.tone, agent.duration_ms, agent.output_path
        ));
    }

    if !consensus_ready {
        text.push_str(
            "\n## Decisao\n\nTexto final indisponivel nesta chamada. A regra permanece: divergencia editorial exige novas rodadas ate unanimidade; falha operacional exige retry ou intervencao do operador antes de qualquer entrega final.\n",
        );
    } else {
        text.push_str("\n## Decisao\n\nTexto final liberado por unanimidade trilateral.\n");
    }

    text
}

fn cli_adapter_specs(request: &CliAdapterSmokeRequest) -> Vec<CliAdapterSpec> {
    let run_id = sanitize_short(&request.run_id, 120);
    let protocol_name = sanitize_text(&request.protocol_name, 160);
    let protocol_hash_prefix = sanitize_short(&request.protocol_hash, 16);
    let prompt_base = format!(
        "Maestro Editorial AI adapter smoke. Run {run_id}. Prompt chars: {}. Protocol: {protocol_name}; lines: {}; hash prefix: {protocol_hash_prefix}. Do not use tools. Reply only with the exact marker requested.",
        request.prompt_chars, request.protocol_lines
    );

    vec![
        CliAdapterSpec {
            name: "Claude",
            command: "claude",
            marker: "MAESTRO_CLI_SMOKE_CLAUDE_READY",
            args: vec![
                "--print".to_string(),
                "--output-format".to_string(),
                "text".to_string(),
                "--permission-mode".to_string(),
                "dontAsk".to_string(),
                format!("{prompt_base} Marker: MAESTRO_CLI_SMOKE_CLAUDE_READY"),
            ],
            timeout: Duration::from_secs(90),
        },
        CliAdapterSpec {
            name: "Codex",
            command: "codex",
            marker: "MAESTRO_CLI_SMOKE_CODEX_READY",
            args: vec![
                "--ask-for-approval".to_string(),
                "never".to_string(),
                "exec".to_string(),
                "--skip-git-repo-check".to_string(),
                "--sandbox".to_string(),
                "read-only".to_string(),
                "--color".to_string(),
                "never".to_string(),
                format!("{prompt_base} Marker: MAESTRO_CLI_SMOKE_CODEX_READY"),
            ],
            timeout: Duration::from_secs(90),
        },
        CliAdapterSpec {
            name: "Gemini",
            command: "gemini",
            marker: "MAESTRO_CLI_SMOKE_GEMINI_READY",
            args: vec![
                "--prompt".to_string(),
                format!("{prompt_base} Marker: MAESTRO_CLI_SMOKE_GEMINI_READY"),
                "--output-format".to_string(),
                "text".to_string(),
                "--approval-mode".to_string(),
                "yolo".to_string(),
                "--skip-trust".to_string(),
            ],
            timeout: Duration::from_secs(90),
        },
    ]
}

fn run_cli_adapter_probe(spec: CliAdapterSpec) -> CliAdapterProbeResult {
    let started = Instant::now();
    let Some(path) = resolve_command(spec.command) else {
        return CliAdapterProbeResult {
            name: spec.name.to_string(),
            cli: spec.command.to_string(),
            tone: "blocked".to_string(),
            status: "CLI nao encontrada no PATH efetivo".to_string(),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            marker_found: false,
        };
    };

    match run_resolved_command_with_timeout(&path, &spec.args, spec.timeout, None) {
        Ok(result) => {
            let exit_code = result.output.status.code();
            let stdout = String::from_utf8_lossy(&result.output.stdout);
            let stderr = String::from_utf8_lossy(&result.output.stderr);
            let marker_found = stdout.contains(spec.marker) || stderr.contains(spec.marker);

            let (tone, status) = if result.timed_out {
                ("error", "timeout aguardando resposta da CLI")
            } else if result.output.status.success() && marker_found {
                ("ok", "CLI executada e marcador recebido")
            } else if result.output.status.success() {
                ("warn", "CLI executada, mas marcador esperado nao apareceu")
            } else {
                ("error", "CLI retornou codigo de saida diferente de zero")
            };

            CliAdapterProbeResult {
                name: spec.name.to_string(),
                cli: spec.command.to_string(),
                tone: tone.to_string(),
                status: status.to_string(),
                duration_ms: result.duration_ms,
                exit_code,
                marker_found,
            }
        }
        Err(error) => CliAdapterProbeResult {
            name: spec.name.to_string(),
            cli: spec.command.to_string(),
            tone: "error".to_string(),
            status: sanitize_text(&format!("falha ao executar CLI: {error}"), 240),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            marker_found: false,
        },
    }
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
    let Some(path) = resolve_command(command) else {
        return json!({
            "label": label,
            "value": "nao encontrado no PATH efetivo",
            "tone": "blocked"
        });
    };
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = run_resolved_command_with_timeout(&path, &args, Duration::from_secs(12), None);

    match output {
        Ok(result) if result.timed_out => json!({
            "label": label,
            "value": sanitize_text("diagnostico excedeu 12s; CLI pode exigir login ou inicializacao lenta", 220),
            "tone": "warn"
        }),
        Ok(result) if result.output.status.success() => {
            let stdout = String::from_utf8_lossy(&result.output.stdout);
            let stderr = String::from_utf8_lossy(&result.output.stderr);
            let detail = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("detectado")
                .trim();
            let resolved_note = format!(" via {}", path.to_string_lossy());
            json!({
                "label": label,
                "value": sanitize_text(&format!("{detail}{resolved_note}"), 220),
                "tone": "ok"
            })
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.output.stderr);
            let stdout = String::from_utf8_lossy(&result.output.stdout);
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

fn run_resolved_command_with_timeout(
    path: &Path,
    args: &[String],
    timeout: Duration,
    stdin_text: Option<&str>,
) -> std::io::Result<TimedCommandOutput> {
    let started = Instant::now();
    let mut command = resolved_command_builder(path, args);
    command
        .current_dir(app_root())
        .stdin(if stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = thread::spawn(move || read_pipe_to_end(stdout));
    let stderr_handle = thread::spawn(move || read_pipe_to_end(stderr));

    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = stdout_handle.join().unwrap_or_default();
            let stderr = stderr_handle.join().unwrap_or_default();
            let output = Output {
                status,
                stdout,
                stderr,
            };
            return Ok(TimedCommandOutput {
                output,
                duration_ms: started.elapsed().as_millis(),
                timed_out: false,
            });
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let status = child.wait()?;
            let stdout = stdout_handle.join().unwrap_or_default();
            let stderr = stderr_handle.join().unwrap_or_default();
            let output = Output {
                status,
                stdout,
                stderr,
            };
            return Ok(TimedCommandOutput {
                output,
                duration_ms: started.elapsed().as_millis(),
                timed_out: true,
            });
        }

        thread::sleep(Duration::from_millis(250));
    }
}

fn run_resolved_command_unbounded(
    path: &Path,
    args: &[String],
    stdin_text: Option<&str>,
) -> std::io::Result<TimedCommandOutput> {
    let started = Instant::now();
    let mut command = resolved_command_builder(path, args);
    command
        .current_dir(app_root())
        .stdin(if stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = thread::spawn(move || read_pipe_to_end(stdout));
    let stderr_handle = thread::spawn(move || read_pipe_to_end(stderr));
    let status = child.wait()?;
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    Ok(TimedCommandOutput {
        output: Output {
            status,
            stdout,
            stderr,
        },
        duration_ms: started.elapsed().as_millis(),
        timed_out: false,
    })
}

fn read_pipe_to_end(pipe: Option<impl Read>) -> Vec<u8> {
    let mut buffer = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut buffer);
    }
    buffer
}

fn resolved_command_builder(path: &Path, args: &[String]) -> Command {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    #[cfg(windows)]
    {
        if extension == "cmd" || extension == "bat" {
            let mut command =
                hidden_command(std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string()));
            command.arg("/C").arg(path).args(args);
            return command;
        }

        if extension == "ps1" {
            let mut command = hidden_command("powershell.exe");
            command
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(path)
                .args(args);
            return command;
        }
    }

    let mut command = hidden_command(path);
    command.args(args);
    command
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
                    if should_redact_key(&key) {
                        (
                            sanitize_text(&key, 80),
                            Value::String("<redacted>".to_string()),
                        )
                    } else {
                        (sanitize_text(&key, 80), sanitize_value(value, depth - 1))
                    }
                })
                .collect(),
        ),
        primitive => primitive,
    }
}

fn should_redact_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "credential_storage_mode"
            | "cloudflare_api_token_source"
            | "cloudflare_api_token_env_var"
            | "cloudflare_api_token_env_scope"
            | "cloudflare_api_token_present"
            | "token_source"
            | "token_env_var"
            | "token_present"
            | "secret_store"
    ) {
        return false;
    }

    let safe_suffixes = [
        "_present",
        "_source",
        "_scope",
        "_env_var",
        "_env_scope",
        "_mode",
        "_label",
        "_name",
        "_status",
        "_tone",
        "_kind",
        "_prefix",
    ];
    if safe_suffixes.iter().any(|suffix| lowered.ends_with(suffix)) {
        return false;
    }

    lowered.contains("secret")
        || lowered.contains("token")
        || lowered.contains("password")
        || lowered.contains("credential")
        || lowered.contains("api_key")
        || lowered.contains("api-key")
        || lowered.contains("authorization")
        || lowered.contains("cookie")
        || lowered.contains("private")
}

fn redact_secrets(value: &str) -> String {
    secret_value_regex()
        .replace_all(value, "<redacted>")
        .to_string()
}

fn secret_value_regex() -> &'static Regex {
    static SECRET_VALUE_REGEX: OnceLock<Regex> = OnceLock::new();
    SECRET_VALUE_REGEX.get_or_init(|| {
        Regex::new(
            r"(?m)(sk-ant-[A-Za-z0-9_-]{8,}|sk_live_[A-Za-z0-9_-]{8,}|sk-[A-Za-z0-9_-]{8,}|cfut_[A-Za-z0-9_-]{8,}|cfat_[A-Za-z0-9_-]{8,}|cfk_[A-Za-z0-9_-]{8,}|xox[baprs]-[A-Za-z0-9-]{8,}|gh[pousr]_[A-Za-z0-9_]{8,}|AIza[0-9A-Za-z_-]{8,}|re_[A-Za-z0-9_-]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN[^\r\n]*(?:\r?\n[^\r\n]*){0,80})",
        )
        .expect("valid secret redaction regex")
    })
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
        let text = redact_secrets("cfat_secret123 cfut_secret123 cfk_secret123");
        assert_eq!(text, "<redacted> <redacted> <redacted>");
    }

    #[test]
    fn redacts_embedded_secret_values_without_whitespace_boundary() {
        let text = redact_secrets(
            r#"url=https://example.test/?key=AIza12345678 header=Authorization:Bearer cfut_12345678 json={"api_key":"sk-ant-12345678"}"#,
        );
        assert!(!text.contains("AIza12345678"));
        assert!(!text.contains("cfut_12345678"));
        assert!(!text.contains("sk-ant-12345678"));
        assert!(text.contains("<redacted>"));
    }

    #[test]
    fn preserves_whitespace_when_redacting() {
        let text = redact_secrets("line1\nline2\tcfat_12345678");
        assert_eq!(text, "line1\nline2\t<redacted>");
    }

    #[test]
    fn keeps_safe_diagnostic_token_metadata_visible() {
        assert!(!should_redact_key("cloudflare_api_token_present"));
        assert!(!should_redact_key("cloudflare_api_token_env_var"));
        assert!(!should_redact_key("token_source"));
        assert!(!should_redact_key("credential_storage_mode"));
    }

    #[test]
    fn redacts_raw_secret_like_keys() {
        assert!(should_redact_key("api_token"));
        assert!(should_redact_key("authorization"));
        assert!(should_redact_key("private_key"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(create_log_session())
        .setup(|app| {
            let log_session = app.state::<LogSession>();
            let panic_log_session = log_session.inner().clone();
            std::panic::set_hook(Box::new(move |panic_info| {
                let payload = panic_info
                    .payload()
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic_info.payload().downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("unknown panic payload");
                let location = panic_info.location().map(|location| {
                    format!(
                        "{}:{}:{}",
                        location.file(),
                        location.line(),
                        location.column()
                    )
                });
                let _ = write_log_record(
                    &panic_log_session,
                    LogEventInput {
                        level: "fatal".to_string(),
                        category: "native.panic".to_string(),
                        message: "native panic captured".to_string(),
                        context: Some(json!({
                            "payload": sanitize_text(payload, 1000),
                            "location": location
                        })),
                    },
                );
            }));
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
                        "log_session_id": log_session.id.clone(),
                        "current_exe": std::env::current_exe().ok().map(|path| path.to_string_lossy().to_string()),
                        "args_count": std::env::args().count(),
                        "path_entries": command_search_dirs().len(),
                        "resolved_commands": {
                            "claude": resolve_command("claude").map(|path| path.to_string_lossy().to_string()),
                            "codex": resolve_command("codex").map(|path| path.to_string_lossy().to_string()),
                            "gemini": resolve_command("gemini").map(|path| path.to_string_lossy().to_string()),
                            "node": resolve_command("node").map(|path| path.to_string_lossy().to_string()),
                            "npm": resolve_command("npm").map(|path| path.to_string_lossy().to_string()),
                            "cargo": resolve_command("cargo").map(|path| path.to_string_lossy().to_string()),
                            "gh": resolve_command("gh").map(|path| path.to_string_lossy().to_string())
                        }
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
            verify_cloudflare_credentials,
            run_cli_adapter_smoke,
            run_editorial_session
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Maestro Editorial AI");
}
