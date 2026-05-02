// Modulo: src-tauri/src/tauri_commands.rs
// Descricao: Remaining Tauri command suite extracted from lib.rs in v0.5.6
// per `docs/code-split-plan.md`. Holds config CRUD, diagnostics, and
// editorial utility commands. Session orchestration commands
// (run_editorial_session, resume_editorial_session, list_resumable_sessions,
// stop_editorial_session) stay in lib.rs because they are tightly coupled
// with the *_blocking helpers and run_editorial_session_core.
//
// What's here (11 items):
//
//   Config CRUD (4):
//   - `read_bootstrap_config` / `write_bootstrap_config` — atomic JSON
//     persistence for the per-launch bootstrap (Cloudflare account id /
//     token source / persistence DB / secret store / Windows env prefix).
//   - `read_ai_provider_config` / `write_ai_provider_config` — AI provider
//     credentials + tariffs; Cloudflare mode goes through Secrets Store
//     via `persist_ai_provider_config_to_cloudflare`.
//
//   Diagnostics + observability (3):
//   - `runtime_profile` — read-only Tauri State exposing app metadata +
//     active log paths.
//   - `write_log_event` — frontend-driven NDJSON log emission via the
//     `LogSession` Tauri State.
//   - `diagnostics_snapshot` — file inventory of `data/logs/` for the
//     diagnostic surface.
//
//   Editorial utilities (4):
//   - `verify_ai_provider_credentials` — runs `ai_probes::run_ai_provider_probe`
//     (HTTP HEAD/GET probes for the 4 provider /models endpoints).
//   - `audit_links` — wraps `link_audit::run_link_audit`.
//   - `open_data_file` — opens an artifact path under `data/` via Windows
//     `explorer.exe` or `xdg-open` on other platforms.
//   - `run_cli_adapter_smoke` — runs the 3-spec CLI adapter probe table
//     across Claude/Codex/Gemini in parallel threads.
//
// Pure move from lib.rs v0.5.5 (commit 0744639): every Tauri command
// attribute, NDJSON shape, log category, normalize_*/sanitize_* call,
// thread spawn pattern, and explorer/xdg-open branch preserved verbatim.

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::thread;

use chrono::Utc;

use crate::ai_probes::run_ai_provider_probe;
use crate::app_init::hidden_command;
use crate::app_paths::{
    ai_provider_config_path, bootstrap_config_path, checked_data_child_path, data_dir,
    human_log_path_for, logs_dir,
};
use crate::cli_adapter::{cli_adapter_specs, run_cli_adapter_probe};
use crate::config_persistence::{
    enrich_ai_provider_config_from_cloudflare, persist_ai_provider_cloudflare_marker,
    persist_ai_provider_config, persist_ai_provider_config_to_cloudflare, persist_bootstrap_config,
};
use crate::link_audit::run_link_audit;
use crate::logging::{write_log_record, LogEventInput, LogSession, LogWriteResult};
use crate::provider_config::{
    merge_ai_provider_env_values, normalize_cloudflare_token_source, normalize_storage_mode,
    sanitize_ai_provider_config,
};
use crate::sanitize::{sanitize_short, sanitize_text};
use crate::{
    AiProviderConfig, AiProviderProbeResult, BootstrapConfig, CliAdapterProbeResult,
    CliAdapterSmokeRequest, CliAdapterSmokeResult, CloudflareProviderStorageRequest,
    LinkAuditRequest, LinkAuditResult, RuntimeProfile,
};

#[tauri::command]
pub(crate) fn runtime_profile(log_session: tauri::State<LogSession>) -> RuntimeProfile {
    RuntimeProfile {
        app_name: "Maestro Editorial AI",
        storage_policy: "app-folder-json-only",
        target_platform: "Windows 11+",
        log_dir: logs_dir().to_string_lossy().to_string(),
        log_file: log_session.path.to_string_lossy().to_string(),
        human_log_file: human_log_path_for(&log_session.path)
            .to_string_lossy()
            .to_string(),
        log_session_id: log_session.id.clone(),
    }
}

#[tauri::command]
pub(crate) fn write_log_event(
    log_session: tauri::State<LogSession>,
    event: LogEventInput,
) -> Result<LogWriteResult, String> {
    write_log_record(&log_session, event)
}

#[tauri::command]
pub(crate) fn diagnostics_snapshot(log_session: tauri::State<LogSession>) -> Value {
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
        "active_human_log_file": human_log_path_for(&log_session.path).to_string_lossy(),
        "log_session_id": log_session.id.clone(),
        "files": files,
        "hint": "Attach the newest data/logs/maestro-*.ndjson file for machine diagnosis; use data/logs/human/*.log for quick human reading."
    })
}

#[tauri::command]
pub(crate) fn read_bootstrap_config() -> Result<BootstrapConfig, String> {
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
pub(crate) fn write_bootstrap_config(config: BootstrapConfig) -> Result<BootstrapConfig, String> {
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
pub(crate) fn read_ai_provider_config() -> Result<AiProviderConfig, String> {
    let path = checked_data_child_path(&ai_provider_config_path())?;
    if !path.exists() {
        let config = AiProviderConfig {
            credential_storage_mode: read_bootstrap_config()
                .map(|config| config.credential_storage_mode)
                .unwrap_or_else(|_| "local_json".to_string()),
            ..AiProviderConfig::default()
        };
        persist_ai_provider_config(&path, &config)?;
        return Ok(merge_ai_provider_env_values(config));
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read AI provider config: {error}"))?;
    let mut config: AiProviderConfig = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse AI provider config: {error}"))?;
    if let Ok(bootstrap) = read_bootstrap_config() {
        config.credential_storage_mode =
            normalize_storage_mode(&bootstrap.credential_storage_mode).to_string();
        if config.credential_storage_mode == "cloudflare" {
            config = enrich_ai_provider_config_from_cloudflare(config, &bootstrap);
        }
    }
    Ok(merge_ai_provider_env_values(sanitize_ai_provider_config(
        config,
    )))
}

#[tauri::command]
pub(crate) fn write_ai_provider_config(
    config: AiProviderConfig,
    cloudflare: Option<CloudflareProviderStorageRequest>,
) -> Result<AiProviderConfig, String> {
    let path = ai_provider_config_path();
    let sanitized = sanitize_ai_provider_config(config);
    if sanitized.credential_storage_mode == "cloudflare" {
        let cloudflare = cloudflare.ok_or_else(|| {
            "configuracao Cloudflare ausente para salvar APIs no Secrets Store".to_string()
        })?;
        persist_ai_provider_config_to_cloudflare(&sanitized, &cloudflare)?;
        persist_ai_provider_cloudflare_marker(&path, &sanitized)?;
    } else {
        persist_ai_provider_config(&path, &sanitized)?;
    }
    Ok(sanitized)
}

#[tauri::command]
pub(crate) fn verify_ai_provider_credentials(config: AiProviderConfig) -> AiProviderProbeResult {
    run_ai_provider_probe(&sanitize_ai_provider_config(config))
}

#[tauri::command]
pub(crate) fn audit_links(request: LinkAuditRequest) -> LinkAuditResult {
    run_link_audit(&request.text)
}

#[tauri::command]
pub(crate) fn open_data_file(path: String) -> Result<String, String> {
    let requested = PathBuf::from(path.trim());
    let absolute = if requested.is_absolute() {
        requested
    } else {
        data_dir().join(requested)
    };
    let checked = checked_data_child_path(&absolute)?;
    if !checked.exists() {
        return Err("arquivo nao encontrado na pasta de dados do Maestro".to_string());
    }

    #[cfg(windows)]
    {
        let mut command = hidden_command("explorer.exe");
        command.arg(&checked);
        command
            .spawn()
            .map_err(|error| format!("falha ao abrir arquivo: {error}"))?;
    }

    #[cfg(not(windows))]
    {
        let mut command = hidden_command("xdg-open");
        command.arg(&checked);
        command
            .spawn()
            .map_err(|error| format!("failed to open file: {error}"))?;
    }

    Ok(checked.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn run_cli_adapter_smoke(
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
                "agents": ["claude", "codex", "gemini", "deepseek"]
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
