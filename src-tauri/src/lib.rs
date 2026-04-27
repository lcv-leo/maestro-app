use chrono::Utc;
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{self, Command, Output, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{path::BaseDirectory, Manager};

static NATIVE_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static APP_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone)]
struct LogSession {
    id: String,
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
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

#[derive(Clone, Deserialize)]
struct ResumeSessionRequest {
    run_id: String,
    protocol_name: Option<String>,
    protocol_text: Option<String>,
    protocol_hash: Option<String>,
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

#[derive(Serialize)]
struct ResumableSessionInfo {
    run_id: String,
    session_name: String,
    session_dir: String,
    prompt_path: String,
    protocol_path: String,
    draft_path: Option<String>,
    final_markdown_path: Option<String>,
    next_round: usize,
    last_activity_unix: u64,
    artifact_count: usize,
    protocol_lines: usize,
    status: String,
}

struct ResumeSessionState {
    current_draft: String,
    current_draft_path: Option<PathBuf>,
    next_review_round: usize,
    existing_agents: Vec<EditorialAgentResult>,
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
    let _guard = log_session
        .write_lock
        .lock()
        .map_err(|_| "failed to lock log writer".to_string())?;
    let dir = checked_data_child_path(&logs_dir())?;
    fs::create_dir_all(&dir).map_err(|error| format!("failed to create log dir: {error}"))?;
    let sequence = NATIVE_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    let log_path = checked_data_child_path(&log_session.path)?;

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
            "log_file": log_path.to_string_lossy().to_string()
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| format!("failed to open log file: {error}"))?;
    writeln!(file, "{record}").map_err(|error| format!("failed to write log record: {error}"))?;

    Ok(LogWriteResult {
        path: log_path.to_string_lossy().to_string(),
        session_id: log_session.id.clone(),
    })
}

#[tauri::command]
fn diagnostics_snapshot(log_session: tauri::State<LogSession>) -> Value {
    let dir = match checked_data_child_path(&logs_dir()) {
        Ok(dir) => dir,
        Err(error) => {
            return json!({
                "error": sanitize_text(&error, 240),
                "hint": "Maestro could not validate its diagnostic log directory."
            });
        }
    };
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
    let path = checked_data_child_path(&bootstrap_config_path())?;
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
async fn list_resumable_sessions(
    log_session: tauri::State<'_, LogSession>,
) -> Result<Vec<ResumableSessionInfo>, String> {
    let log_session = log_session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || list_resumable_sessions_blocking(&log_session))
        .await
        .map_err(|error| format!("resume session listing worker join failed: {error}"))?
}

#[tauri::command]
async fn resume_editorial_session(
    log_session: tauri::State<'_, LogSession>,
    request: ResumeSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let log_session = log_session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        resume_editorial_session_blocking(log_session, request)
    })
    .await
    .map_err(|error| format!("resume editorial worker join failed: {error}"))?
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

fn resume_editorial_session_blocking(
    log_session: LogSession,
    request: ResumeSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let run_id = sanitize_path_segment(&request.run_id, 120);
    if run_id.is_empty() {
        return Err("run_id vazio".to_string());
    }

    let session_dir = checked_data_child_path(&sessions_dir().join(&run_id))?;
    if !session_dir.is_dir() {
        return Err("sessao nao encontrada em data/sessions".to_string());
    }

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    let saved_prompt = read_text_file(&prompt_path)?;
    let saved_protocol = read_text_file(&protocol_path)?;
    let prompt = extract_saved_prompt(&saved_prompt)
        .unwrap_or_else(|| saved_prompt.trim().to_string())
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("prompt salvo da sessao esta vazio".to_string());
    }

    let session_name =
        extract_saved_session_name(&saved_prompt).unwrap_or_else(|| format!("Sessao {run_id}"));
    let override_protocol = request
        .protocol_text
        .as_deref()
        .map(str::trim)
        .filter(|value| value.len() >= 100)
        .map(str::to_string);
    let using_protocol_override = override_protocol.is_some();
    let protocol_text = override_protocol.unwrap_or_else(|| saved_protocol.trim().to_string());
    let protocol_name = if using_protocol_override {
        request
            .protocol_name
            .as_deref()
            .map(|value| sanitize_text(value, 200))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "protocolo-atualizado.md".to_string())
    } else {
        "protocolo.md".to_string()
    };
    let protocol_hash = request
        .protocol_hash
        .as_deref()
        .map(|value| sanitize_short(value, 80))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| stable_text_fingerprint(&protocol_text));

    let protocol_backup_path =
        if using_protocol_override && saved_protocol.trim() != protocol_text.trim() {
            let backup_path = session_dir.join(format!(
                "protocolo-anterior-{}.md",
                Utc::now().format("%Y%m%dT%H%M%SZ")
            ));
            write_text_file(&backup_path, &saved_protocol)?;
            Some(backup_path)
        } else {
            None
        };

    let agent_dir = checked_data_child_path(&session_dir.join("agent-runs"))?;
    fs::create_dir_all(&agent_dir)
        .map_err(|error| format!("failed to create agent run dir: {error}"))?;
    let resume_state = load_resume_session_state(&agent_dir)?;

    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.resume.started".to_string(),
            message: "operator requested editorial session resume".to_string(),
            context: Some(json!({
                "run_id": &run_id,
                "session_name": sanitize_text(&session_name, 200),
                "using_protocol_override": using_protocol_override,
                "protocol_name": sanitize_text(&protocol_name, 200),
                "protocol_chars": protocol_text.chars().count(),
                "protocol_lines": protocol_text.lines().count(),
                "protocol_hash_prefix": sanitize_short(&protocol_hash, 16),
                "resume_draft_path": resume_state
                    .current_draft_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                "protocol_backup_path": protocol_backup_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                "next_review_round": resume_state.next_review_round,
                "existing_agent_artifacts": resume_state.existing_agents.len()
            })),
        },
    );

    let request = EditorialSessionRequest {
        run_id,
        session_name,
        prompt,
        protocol_name,
        protocol_text,
        protocol_hash,
    };

    let result = match run_editorial_session_core(&request, &log_session, Some(resume_state)) {
        Ok(result) => result,
        Err(error) => {
            let _ = write_log_record(
                &log_session,
                LogEventInput {
                    level: "error".to_string(),
                    category: "session.resume.failed".to_string(),
                    message: "editorial session resume failed before structured result".to_string(),
                    context: Some(json!({
                        "run_id": sanitize_short(&request.run_id, 120),
                        "error": sanitize_text(&error, 500)
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
            category: "session.resume.completed".to_string(),
            message: "editorial session resume completed".to_string(),
            context: Some(json!({
                "run_id": result.run_id,
                "status": result.status,
                "consensus_ready": result.consensus_ready,
                "session_dir": result.session_dir,
                "final_markdown_path": result.final_markdown_path,
                "session_minutes_path": result.session_minutes_path
            })),
        },
    );

    Ok(result)
}

fn list_resumable_sessions_blocking(
    log_session: &LogSession,
) -> Result<Vec<ResumableSessionInfo>, String> {
    let root = checked_data_child_path(&sessions_dir())?;
    fs::create_dir_all(&root).map_err(|error| format!("failed to create sessions dir: {error}"))?;

    let mut sessions = Vec::new();
    for entry in
        fs::read_dir(&root).map_err(|error| format!("failed to read sessions dir: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to read session entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read session entry type: {error}"))?;
        if !file_type.is_dir() {
            continue;
        }
        let Some(run_id) = safe_run_id_from_entry(&entry) else {
            continue;
        };
        let path = root.join(run_id);
        if let Some(info) = inspect_resumable_session_dir(&path)? {
            sessions.push(info);
        }
    }

    sessions.sort_by(|left, right| {
        right
            .last_activity_unix
            .cmp(&left.last_activity_unix)
            .then_with(|| left.run_id.cmp(&right.run_id))
    });

    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.resume.listed".to_string(),
            message: "resumable sessions listed from data/sessions".to_string(),
            context: Some(json!({
                "count": sessions.len(),
                "run_ids": sessions.iter().take(30).map(|session| session.run_id.clone()).collect::<Vec<_>>()
            })),
        },
    );

    Ok(sessions)
}

fn app_root() -> PathBuf {
    if let Some(path) = APP_ROOT.get() {
        return path.clone();
    }

    #[cfg(test)]
    {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("maestro-editorial-ai-tests");
    }

    #[cfg(not(test))]
    {
        panic!("Maestro app root must be initialized by Tauri setup before use");
    }
}

fn initialize_app_root(app: &tauri::App) -> Result<(), String> {
    let root = app
        .path()
        .resolve("", BaseDirectory::Executable)
        .map_err(|error| format!("failed to resolve portable executable dir: {error}"))?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize portable executable dir: {error}"))?;
    let _ = APP_ROOT.set(root);
    Ok(())
}

fn data_dir() -> PathBuf {
    app_root().join("data")
}

fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

fn config_dir() -> PathBuf {
    data_dir().join("config")
}

fn bootstrap_config_path() -> PathBuf {
    config_dir().join("bootstrap.json")
}

fn sessions_dir() -> PathBuf {
    data_dir().join("sessions")
}

fn checked_data_child_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("internal data path must be absolute".to_string());
    }

    let data_root = data_dir();
    fs::create_dir_all(&data_root)
        .map_err(|error| format!("failed to create Maestro data root: {error}"))?;
    let relative = path
        .strip_prefix(&data_root)
        .map_err(|_| "internal data path escaped Maestro data directory".to_string())?;

    if !is_safe_relative_data_path(relative) {
        return Err("internal data path contains unsafe segments".to_string());
    }

    Ok(data_root.join(relative))
}

fn is_safe_relative_data_path(path: &Path) -> bool {
    path.components().all(|component| match component {
        Component::Normal(value) => value.to_str().map(is_safe_data_file_name).unwrap_or(false),
        _ => false,
    })
}

fn is_safe_data_file_name(value: &str) -> bool {
    // General data filenames may contain dots for extensions; run IDs stay stricter
    // through sanitize_path_segment because they become directory names.
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 255
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn safe_run_id_from_entry(entry: &fs::DirEntry) -> Option<String> {
    let name = entry.file_name();
    let name = name.to_str()?;
    let sanitized = sanitize_path_segment(name, 120);
    if sanitized == name {
        Some(sanitized)
    } else {
        None
    }
}

fn sanitize_path_segment(value: &str, max_len: usize) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(max_len)
        .collect::<String>()
        .trim_matches(['_', '-'])
        .to_string()
}

fn create_log_session() -> LogSession {
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let id = format!("{timestamp}-pid{}", process::id());
    LogSession {
        id: id.clone(),
        path: logs_dir().join(format!("maestro-{id}.ndjson")),
        write_lock: Arc::new(Mutex::new(())),
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
    let path = checked_data_child_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create config dir: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("failed to serialize bootstrap config: {error}"))?;
    fs::write(&path, bytes).map_err(|error| format!("failed to write bootstrap config: {error}"))
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
    run_editorial_session_core(request, log_session, None)
}

fn run_editorial_session_core(
    request: &EditorialSessionRequest,
    log_session: &LogSession,
    resume_state: Option<ResumeSessionState>,
) -> Result<EditorialSessionResult, String> {
    let run_id = sanitize_path_segment(&request.run_id, 120);
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

    let session_dir = checked_data_child_path(&sessions_dir().join(&run_id))?;
    let agent_dir = checked_data_child_path(&session_dir.join("agent-runs"))?;
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
    let mut round = 1usize;

    if let Some(state) = resume_state {
        agents = state.existing_agents;
        current_draft = state.current_draft;
        current_draft_path = state.current_draft_path;
        round = state.next_review_round.max(1);
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "info".to_string(),
                category: "session.resume.loaded".to_string(),
                message: "saved editorial session state loaded for continuation".to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "next_review_round": round,
                    "current_draft_chars": current_draft.chars().count(),
                    "current_draft_path": current_draft_path.as_ref().map(|path| path.to_string_lossy().to_string()),
                    "existing_agent_artifacts": agents.len()
                })),
            },
        );
    }

    if current_draft.trim().is_empty() {
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
            let draft_artifact = read_text_file(&output_path).unwrap_or_default();
            let draft_text =
                extract_stdout_block(&draft_artifact).unwrap_or(draft_artifact.as_str());
            if draft_run.tone != "error"
                && draft_run.tone != "blocked"
                && !draft_text.trim().is_empty()
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
            let artifact = read_text_file(&output_path).unwrap_or_default();
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
    let path = checked_data_child_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create artifact dir: {error}"))?;
    }
    fs::write(&path, text).map_err(|error| format!("failed to write artifact: {error}"))
}

fn read_text_file(path: &Path) -> Result<String, String> {
    let path = checked_data_child_path(path)?;
    fs::read_to_string(&path).map_err(|error| format!("failed to read artifact: {error}"))
}

fn inspect_resumable_session_dir(path: &Path) -> Result<Option<ResumableSessionInfo>, String> {
    let session_dir = checked_data_child_path(path)?;
    let run_id = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| sanitize_path_segment(value, 120))
        .unwrap_or_default();
    if run_id.is_empty() {
        return Ok(None);
    }

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    if !prompt_path.is_file() || !protocol_path.is_file() {
        return Ok(None);
    }

    let final_path = session_dir.join("texto-final.md");
    if final_path.is_file() {
        return Ok(None);
    }

    let prompt_text = read_text_file(&prompt_path)?;
    let protocol_text = read_text_file(&protocol_path)?;
    let agent_dir = checked_data_child_path(&session_dir.join("agent-runs"))?;
    let artifacts = read_agent_artifacts(&agent_dir)?;
    let latest_draft = find_latest_draft_artifact_from_artifacts(&artifacts)?;
    let next_round = latest_draft
        .as_ref()
        .map(|artifact| artifact.round.max(1))
        .unwrap_or(1);
    let artifact_count = count_known_session_markdown_artifacts(&session_dir, &artifacts)?;
    let last_activity_unix =
        known_session_activity_unix(&session_dir, &prompt_path, &protocol_path, &artifacts)
            .unwrap_or(0);
    let status = if latest_draft.is_some() {
        "pronta para continuar".to_string()
    } else {
        "aguardando primeiro rascunho".to_string()
    };

    Ok(Some(ResumableSessionInfo {
        run_id,
        session_name: extract_saved_session_name(&prompt_text)
            .unwrap_or_else(|| "Sessao editorial".to_string()),
        session_dir: session_dir.to_string_lossy().to_string(),
        prompt_path: prompt_path.to_string_lossy().to_string(),
        protocol_path: protocol_path.to_string_lossy().to_string(),
        draft_path: latest_draft
            .as_ref()
            .map(|artifact| artifact.path.to_string_lossy().to_string()),
        final_markdown_path: None,
        next_round,
        last_activity_unix,
        artifact_count,
        protocol_lines: protocol_text.lines().count(),
        status,
    }))
}

fn load_resume_session_state(agent_dir: &Path) -> Result<ResumeSessionState, String> {
    let agent_dir = checked_data_child_path(agent_dir)?;
    let latest_draft = find_latest_draft_artifact(&agent_dir)?;
    let existing_agents = load_agent_results_from_dir(&agent_dir)?;

    if let Some(artifact) = latest_draft {
        let text = read_text_file(&artifact.path)?;
        let draft = extract_stdout_block(&text)
            .unwrap_or(text.as_str())
            .trim()
            .to_string();
        if !draft.is_empty() {
            return Ok(ResumeSessionState {
                current_draft: draft,
                current_draft_path: Some(artifact.path),
                next_review_round: artifact.round.max(1),
                existing_agents,
            });
        }
    }

    Ok(ResumeSessionState {
        current_draft: String::new(),
        current_draft_path: None,
        next_review_round: 1,
        existing_agents,
    })
}

#[derive(Clone)]
struct SessionArtifact {
    round: usize,
    agent: String,
    role: String,
    path: PathBuf,
}

fn find_latest_draft_artifact(agent_dir: &Path) -> Result<Option<SessionArtifact>, String> {
    let artifacts = read_agent_artifacts(agent_dir)?;
    find_latest_draft_artifact_from_artifacts(&artifacts)
}

fn find_latest_draft_artifact_from_artifacts(
    artifacts: &[SessionArtifact],
) -> Result<Option<SessionArtifact>, String> {
    let mut artifacts = artifacts
        .iter()
        .filter(|artifact| artifact.role == "revision" || artifact.role == "draft")
        .cloned()
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| {
        artifact_resume_rank(right)
            .cmp(&artifact_resume_rank(left))
            .then_with(|| right.agent.cmp(&left.agent))
    });

    for artifact in artifacts {
        let text = read_text_file(&artifact.path).unwrap_or_default();
        let draft = extract_stdout_block(&text).unwrap_or(text.as_str());
        if !draft.trim().is_empty() {
            return Ok(Some(artifact));
        }
    }

    Ok(None)
}

fn artifact_resume_rank(artifact: &SessionArtifact) -> (usize, usize) {
    let role_rank = if artifact.role == "revision" { 1 } else { 0 };
    (artifact.round, role_rank)
}

fn load_agent_results_from_dir(agent_dir: &Path) -> Result<Vec<EditorialAgentResult>, String> {
    let mut artifacts = read_agent_artifacts(agent_dir)?;
    artifacts.sort_by(|left, right| {
        left.round
            .cmp(&right.round)
            .then_with(|| left.role.cmp(&right.role))
            .then_with(|| left.agent.cmp(&right.agent))
    });

    let mut agents = Vec::new();
    for artifact in artifacts {
        if let Some(result) = parse_agent_artifact_result(&artifact) {
            agents.push(result);
        }
    }
    Ok(agents)
}

fn read_agent_artifacts(agent_dir: &Path) -> Result<Vec<SessionArtifact>, String> {
    let agent_dir = checked_data_child_path(agent_dir)?;
    if !agent_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut artifacts = Vec::new();
    for entry in
        fs::read_dir(&agent_dir).map_err(|error| format!("failed to read agent dir: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to read agent artifact entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read agent artifact type: {error}"))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(artifact) = parse_agent_artifact_name(&agent_dir, name) {
            artifacts.push(artifact);
        }
    }
    Ok(artifacts)
}

fn parse_agent_artifact_name(agent_dir: &Path, name: &str) -> Option<SessionArtifact> {
    let rest = name.strip_prefix("round-")?;
    let (round_text, rest) = rest.split_once('-')?;
    let round = round_text.parse::<usize>().ok()?;
    let stem = rest.strip_suffix(".md")?;
    let (agent, role) = stem.rsplit_once('-')?;
    let agent = match agent {
        "claude" | "codex" | "gemini" => agent,
        _ => return None,
    };
    if !matches!(role, "draft" | "review" | "revision") {
        return None;
    }
    let canonical_name = format!("round-{round:03}-{agent}-{role}.md");
    if canonical_name != name {
        return None;
    }
    Some(SessionArtifact {
        round,
        agent: agent.to_string(),
        role: role.to_string(),
        path: agent_dir.join(canonical_name),
    })
}

fn parse_agent_artifact_result(artifact: &SessionArtifact) -> Option<EditorialAgentResult> {
    let text = read_text_file(&artifact.path).ok()?;
    let cli = extract_bullet_code_value(&text, "CLI").unwrap_or_else(|| artifact.agent.clone());
    let status = extract_bullet_code_value(&text, "Status").unwrap_or_else(|| {
        if artifact.role == "draft" || artifact.role == "revision" {
            "DRAFT_CREATED".to_string()
        } else {
            "NOT_READY".to_string()
        }
    });
    let duration_ms = extract_bullet_code_value(&text, "Duration ms")
        .and_then(|value| value.parse::<u128>().ok())
        .unwrap_or(0);
    let exit_code =
        extract_bullet_code_value(&text, "Exit code").and_then(|value| value.parse::<i32>().ok());
    let tone = if status == "READY" || status == "DRAFT_CREATED" {
        "ok"
    } else if status == "CLI_NOT_FOUND" {
        "blocked"
    } else if status.starts_with("EXEC_ERROR") {
        "error"
    } else {
        "warn"
    };

    Some(EditorialAgentResult {
        name: humanize_agent_name(&artifact.agent),
        role: artifact.role.clone(),
        cli,
        tone: tone.to_string(),
        status,
        duration_ms,
        exit_code,
        output_path: artifact.path.to_string_lossy().to_string(),
    })
}

fn extract_bullet_code_value(text: &str, label: &str) -> Option<String> {
    let prefix = format!("- {label}: `");
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?;
        let end = value.find('`')?;
        Some(value[..end].trim().to_string())
    })
}

fn humanize_agent_name(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        other => other
            .replace('_', " ")
            .split_whitespace()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn extract_saved_session_name(prompt_file: &str) -> Option<String> {
    prompt_file.lines().find_map(|line| {
        let value = line.strip_prefix("Sessao: ")?;
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn extract_saved_prompt(prompt_file: &str) -> Option<String> {
    let marker = "\n\n";
    let (_, prompt) = prompt_file.split_once(marker)?;
    let (_, prompt) = prompt.split_once(marker)?;
    let prompt = prompt.trim();
    if prompt.is_empty() {
        None
    } else {
        Some(prompt.to_string())
    }
}

fn stable_text_fingerprint(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64-{hash:016x}")
}

fn count_known_session_markdown_artifacts(
    session_dir: &Path,
    artifacts: &[SessionArtifact],
) -> Result<usize, String> {
    let session_dir = checked_data_child_path(session_dir)?;
    let backup_stats = protocol_backup_stats(&session_dir)?;
    let known_session_files = [
        "prompt.md",
        "protocolo.md",
        "ata-da-sessao.md",
        "texto-final.md",
    ];
    let session_file_count = known_session_files
        .iter()
        .filter(|file_name| session_dir.join(file_name).is_file())
        .count();
    Ok(session_file_count + backup_stats.count + artifacts.len())
}

fn known_session_activity_unix(
    session_dir: &Path,
    prompt_path: &Path,
    protocol_path: &Path,
    artifacts: &[SessionArtifact],
) -> Option<u64> {
    let backup_latest = protocol_backup_stats(session_dir)
        .ok()
        .and_then(|stats| stats.latest_activity_unix);
    [
        checked_data_child_path(session_dir).ok(),
        checked_data_child_path(prompt_path).ok(),
        checked_data_child_path(protocol_path).ok(),
        checked_data_child_path(&session_dir.join("ata-da-sessao.md")).ok(),
        checked_data_child_path(&session_dir.join("texto-final.md")).ok(),
    ]
    .into_iter()
    .flatten()
    .chain(artifacts.iter().map(|artifact| artifact.path.clone()))
    .filter_map(|path| {
        path.metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_to_unix)
    })
    .chain(backup_latest)
    .max()
}

struct ProtocolBackupStats {
    count: usize,
    latest_activity_unix: Option<u64>,
}

fn protocol_backup_stats(session_dir: &Path) -> Result<ProtocolBackupStats, String> {
    let session_dir = checked_data_child_path(session_dir)?;
    if !session_dir.is_dir() {
        return Ok(ProtocolBackupStats {
            count: 0,
            latest_activity_unix: None,
        });
    }

    let mut count = 0;
    let mut latest_activity_unix = None;
    for entry in fs::read_dir(&session_dir)
        .map_err(|error| format!("failed to read session backup dir: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to read session backup entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read session backup entry type: {error}"))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_protocol_backup_file_name(name) {
            continue;
        }
        count += 1;
        if let Ok(metadata) = entry.metadata() {
            if let Some(modified) = metadata.modified().ok().and_then(system_time_to_unix) {
                latest_activity_unix = Some(
                    latest_activity_unix
                        .map(|current: u64| current.max(modified))
                        .unwrap_or(modified),
                );
            }
        }
    }

    Ok(ProtocolBackupStats {
        count,
        latest_activity_unix,
    })
}

fn is_protocol_backup_file_name(name: &str) -> bool {
    is_safe_data_file_name(name) && name.starts_with("protocolo-anterior-") && name.ends_with(".md")
}

fn system_time_to_unix(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
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
        let artifact = read_text_file(Path::new(&agent.output_path)).unwrap_or_default();
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
    fn sanitizes_run_ids_for_path_segments() {
        assert_eq!(
            sanitize_path_segment("../run:2026/04/26", 120),
            "run20260426"
        );
        assert_eq!(
            sanitize_path_segment("run-2026_04_26", 120),
            "run-2026_04_26"
        );
        assert!(sanitize_path_segment("../../../", 120).is_empty());
    }

    #[test]
    fn rejects_paths_outside_data_dir() {
        let outside = app_root().join("outside.txt");
        assert!(checked_data_child_path(&outside).is_err());
        let traversal = sessions_dir().join("safe").join("..").join("escape.txt");
        assert!(checked_data_child_path(&traversal).is_err());
        let inside = sessions_dir().join("safe").join("artifact.md");
        assert!(checked_data_child_path(&inside).is_ok());
    }

    #[test]
    fn rejects_noncanonical_agent_artifact_names() {
        let agent_dir = sessions_dir()
            .join("run-artifact-name-test")
            .join("agent-runs");
        let valid = parse_agent_artifact_name(&agent_dir, "round-001-claude-draft.md")
            .expect("canonical artifact name must parse");
        assert_eq!(valid.round, 1);
        assert_eq!(valid.agent, "claude");
        assert_eq!(valid.role, "draft");
        assert!(valid.path.ends_with("round-001-claude-draft.md"));

        assert!(parse_agent_artifact_name(&agent_dir, "round-1-claude-draft.md").is_none());
        assert!(parse_agent_artifact_name(&agent_dir, "round-001-rogue-review.md").is_none());
        assert!(parse_agent_artifact_name(&agent_dir, "round-001-claude-other.md").is_none());
        assert!(parse_agent_artifact_name(&agent_dir, "round-001-claude-review.txt").is_none());
    }

    #[test]
    fn ignores_dotted_session_folder_names() {
        let root = sessions_dir();
        let bad_session_dir = root.join("run.bad");
        let _ = fs::remove_dir_all(&bad_session_dir);
        fs::create_dir_all(&bad_session_dir).unwrap();
        let entry = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_str() == Some("run.bad"))
            .expect("dotted test folder should be visible");

        assert!(safe_run_id_from_entry(&entry).is_none());
        let _ = fs::remove_dir_all(&bad_session_dir);
    }

    #[test]
    fn counts_protocol_backup_artifacts_without_recursive_scan() {
        let session_dir = sessions_dir().join("run-protocol-backup-count-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();
        write_text_file(&session_dir.join("prompt.md"), "prompt").unwrap();
        write_text_file(&session_dir.join("protocolo.md"), "protocol").unwrap();
        write_text_file(
            &session_dir.join("protocolo-anterior-20260426T000000Z.md"),
            "old protocol",
        )
        .unwrap();
        write_text_file(
            &session_dir.join("protocolo-anterior-unsafe.txt"),
            "ignored",
        )
        .unwrap();

        let stats = protocol_backup_stats(&session_dir).unwrap();
        assert_eq!(stats.count, 1);
        assert!(stats.latest_activity_unix.is_some());
        assert_eq!(
            count_known_session_markdown_artifacts(&session_dir, &[]).unwrap(),
            3
        );

        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn keeps_concurrent_log_writes_as_valid_ndjson() {
        let session = create_log_session();
        let log_path = checked_data_child_path(&session.path).unwrap();
        let _ = fs::remove_file(&log_path);
        let handles = (0..18)
            .map(|index| {
                let session = session.clone();
                thread::spawn(move || {
                    write_log_record(
                        &session,
                        LogEventInput {
                            level: "info".to_string(),
                            category: "test.concurrent_log".to_string(),
                            message: format!("concurrent log event {index}"),
                            context: Some(json!({ "index": index })),
                        },
                    )
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle
                .join()
                .expect("log writer thread must not panic")
                .unwrap();
        }

        let text = fs::read_to_string(log_path).unwrap();
        let lines = text.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 18);
        for line in lines {
            serde_json::from_str::<Value>(line).expect("each log line must be valid JSON");
        }
    }

    #[test]
    fn extracts_saved_prompt_for_session_resume() {
        let prompt_file =
            "# Prompt da Sessao\n\nSessao: Teste Editorial\nRun: `run-resume`\n\nEscreva o artigo.";
        assert_eq!(
            extract_saved_session_name(prompt_file).as_deref(),
            Some("Teste Editorial")
        );
        assert_eq!(
            extract_saved_prompt(prompt_file).as_deref(),
            Some("Escreva o artigo.")
        );
    }

    #[test]
    fn detects_latest_revision_for_session_resume() {
        let session_dir = sessions_dir().join("run-resume-detection-test");
        let _ = fs::remove_dir_all(&session_dir);
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        write_text_file(
            &session_dir.join("prompt.md"),
            "# Prompt da Sessao\n\nSessao: Retomada\nRun: `run-resume-detection-test`\n\nPrompt salvo.",
        )
        .unwrap();
        write_text_file(
            &session_dir.join("protocolo.md"),
            &"protocolo\n".repeat(120),
        )
        .unwrap();
        write_text_file(
            &agent_dir.join("round-001-claude-draft.md"),
            "# Claude - draft\n\n- CLI: `claude`\n- Status: `DRAFT_CREATED`\n- Exit code: `0`\n- Duration ms: `10`\n\n## Stdout\n\n```text\nrascunho antigo\n```\n\n## Stderr\n\n```text\n\n```\n",
        )
        .unwrap();
        write_text_file(
            &agent_dir.join("round-007-gemini-revision.md"),
            "# Gemini - revision\n\n- CLI: `gemini`\n- Status: `DRAFT_CREATED`\n- Exit code: `0`\n- Duration ms: `20`\n\n## Stdout\n\n```text\nrascunho revisado\n```\n\n## Stderr\n\n```text\n\n```\n",
        )
        .unwrap();

        let info = inspect_resumable_session_dir(&session_dir)
            .unwrap()
            .expect("session should be resumable");
        assert_eq!(info.run_id, "run-resume-detection-test");
        assert_eq!(info.next_round, 7);
        assert!(info
            .draft_path
            .as_deref()
            .unwrap()
            .ends_with("round-007-gemini-revision.md"));

        let state = load_resume_session_state(&agent_dir).unwrap();
        assert_eq!(state.current_draft, "rascunho revisado");
        assert_eq!(state.next_review_round, 7);
        assert_eq!(state.existing_agents.len(), 2);

        let _ = fs::remove_dir_all(&session_dir);
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
        .setup(|app| {
            initialize_app_root(app)?;
            app.manage(create_log_session());
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
            list_resumable_sessions,
            resume_editorial_session,
            run_editorial_session
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Maestro Editorial AI");
}
