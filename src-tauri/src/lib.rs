// Editorial spawn-funnel guard: every editorial CLI launch must reach
// `hidden_command` -> `resolved_command_builder` -> `apply_editorial_agent_environment`.
// `clippy.toml` forbids direct `std::process::Command::new` calls; only
// `hidden_command` itself and unit-test fixtures may bypass via
// `#[allow(clippy::disallowed_methods)]`. (Codex NB-5 from v0.3.15.)
//
// Hardened to `deny` per cross-review-v2 R1 of session d4d7a9c1: `warn` was
// casca vazia because `cargo clippy` already accumulates 17 unrelated warnings
// that pass without failing CI, so a "warn" signal here would never block.
// `deny` makes the build fail if a future commit introduces a direct
// `Command::new` call outside the funnel.
#![deny(clippy::disallowed_methods)]

use chrono::Utc;
use regex::Regex;
use reqwest::{blocking::Client, redirect::Policy, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::{Path, PathBuf},
    process::{self, Command, Output, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

mod app_paths;
mod cloudflare;
mod editorial_helpers;
mod editorial_inputs;
mod editorial_prompts;
mod human_logs;
mod logging;
mod provider_deepseek;
mod provider_retry;
mod provider_runners;
mod session_controls;
mod session_evidence;
mod session_persistence;
mod session_resume;

// Re-export the app_paths surface used throughout this file. The functions
// keep their pre-extraction call sites (`sessions_dir()`, `app_root()`, etc.)
// untouched; only the home of the implementation moved per
// `docs/code-split-plan.md`. See `app_paths.rs` for documentation.
use crate::app_paths::{
    active_or_early_logs_dir, ai_provider_config_path, app_root, app_root_if_initialized,
    bootstrap_config_path, checked_data_child_path, data_dir, human_log_path_for, logs_dir,
    resolve_portable_app_root, safe_run_id_from_entry, sanitize_path_segment, sessions_dir,
    try_set_app_root,
};
use crate::cloudflare::{
    ai_provider_secret_values, cloudflare_client, cloudflare_get, cloudflare_post_json,
    cloudflare_result_id_for_name, cloudflare_token_from_provider_request,
    ensure_cloudflare_d1_database, ensure_cloudflare_secret_store, run_cloudflare_probe,
    token_source_label, upsert_ai_provider_secrets, write_ai_provider_metadata_to_cloudflare,
};
use crate::editorial_helpers::{
    filter_existing_agents_to_active_set, finalize_running_agent_artifacts,
    resolve_effective_active_agents, review_complaint_fingerprint, write_editorial_agent_error_artifact,
    write_editorial_agent_running_artifact, FinalizeRunningArtifactsGuard,
};
use crate::editorial_inputs::{
    build_active_agents_resolved_log_context, effective_agent_input, prepare_agent_input,
    resolve_time_budget_anchor,
};
use crate::editorial_prompts::{
    build_draft_prompt, build_review_prompt, build_revision_prompt, editorial_agent_specs,
    ordered_editorial_agent_specs, resolve_initial_agent_key,
};
use crate::logging::{create_log_session, write_log_record, LogEventInput, LogSession, LogWriteResult};
use crate::provider_deepseek::run_deepseek_api_agent;
use crate::provider_runners::{
    run_anthropic_api_agent, run_gemini_api_agent, run_openai_api_agent, write_provider_error_result,
};
use crate::session_persistence::{
    append_agent_cost_to_ledger, load_cost_ledger, load_session_contract, write_session_contract,
};
use crate::session_resume::{
    count_known_session_markdown_artifacts, extract_bullet_code_value, extract_saved_initial_agent,
    extract_saved_prompt, extract_saved_session_name, humanize_agent_name,
    known_session_activity_unix, parse_created_at, remaining_session_duration,
    session_time_exhausted, stable_text_fingerprint,
};
// Items below are only referenced from `mod tests` and cargo flags them as unused
// when compiled without the test cfg. Re-importing inside the test module avoids
// `#[cfg(test)]` annotations on the main use list.

#[cfg(test)]
use human_logs::should_collapse_human_log_event;
// `human_log_summary`, `severity_number_for`, `write_human_log_projection`
// are now consumed inside `logging.rs`; lib.rs no longer needs them.
use session_controls::{
    all_agent_keys, effective_draft_lead, provider_cost_guard_for, sanitize_optional_positive_f64,
    sanitize_optional_positive_u64, selected_editorial_agent_specs, ProviderCostGuard,
    ProviderCostRates,
};
use session_evidence::{
    attachment_base64, attachment_data_url, attachment_payload_base64_chars, is_audio_attachment,
    is_image_attachment, is_known_document_attachment, is_pdf_attachment, is_text_like_attachment,
    is_video_attachment, normalized_attachment_media_type, process_session_evidence,
    AttachmentManifestEntry, PromptAttachmentRequest,
};

// `NATIVE_LOG_SEQUENCE` lives in `logging::NATIVE_LOG_SEQUENCE`.
// `APP_ROOT` lives in `app_paths::APP_ROOT`.
const API_NATIVE_ATTACHMENT_MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

// `LogSession`, `LogEventInput`, `LogWriteResult`, `create_log_session`,
// and `write_log_record` were moved into `logging.rs` in v0.3.19. Imports
// land in the use list below alongside `app_paths::*`.

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

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AiProviderConfig {
    schema_version: u8,
    provider_mode: String,
    credential_storage_mode: String,
    #[serde(default)]
    pub(crate) openai_api_key: Option<String>,
    #[serde(default)]
    pub(crate) anthropic_api_key: Option<String>,
    #[serde(default)]
    pub(crate) gemini_api_key: Option<String>,
    #[serde(default)]
    pub(crate) deepseek_api_key: Option<String>,
    #[serde(default)]
    pub(crate) openai_api_key_remote: bool,
    #[serde(default)]
    pub(crate) anthropic_api_key_remote: bool,
    #[serde(default)]
    pub(crate) gemini_api_key_remote: bool,
    #[serde(default)]
    pub(crate) deepseek_api_key_remote: bool,
    #[serde(default)]
    openai_input_usd_per_million: Option<f64>,
    #[serde(default)]
    openai_output_usd_per_million: Option<f64>,
    #[serde(default)]
    anthropic_input_usd_per_million: Option<f64>,
    #[serde(default)]
    anthropic_output_usd_per_million: Option<f64>,
    #[serde(default)]
    gemini_input_usd_per_million: Option<f64>,
    #[serde(default)]
    gemini_output_usd_per_million: Option<f64>,
    #[serde(default)]
    deepseek_input_usd_per_million: Option<f64>,
    #[serde(default)]
    deepseek_output_usd_per_million: Option<f64>,
    #[serde(default)]
    cloudflare_secret_store_id: Option<String>,
    #[serde(default)]
    cloudflare_secret_store_name: Option<String>,
    updated_at: String,
}

#[derive(Clone, Deserialize)]
pub(crate) struct CloudflareProviderStorageRequest {
    pub(crate) account_id: String,
    pub(crate) api_token: Option<String>,
    pub(crate) api_token_env_var: String,
    pub(crate) persistence_database: String,
    pub(crate) secret_store: String,
}

#[derive(Serialize)]
struct AiProviderProbeRow {
    label: String,
    value: String,
    tone: String,
}

#[derive(Serialize)]
struct AiProviderProbeResult {
    rows: Vec<AiProviderProbeRow>,
    checked_at: String,
}

#[derive(Deserialize)]
struct LinkAuditRequest {
    text: String,
}

#[derive(Serialize)]
struct LinkAuditRow {
    url: String,
    status: String,
    tone: String,
}

#[derive(Serialize)]
struct LinkAuditResult {
    urls_found: usize,
    checked: usize,
    ok: usize,
    failed: usize,
    rows: Vec<LinkAuditRow>,
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
pub(crate) struct CloudflareProbeRequest {
    pub(crate) account_id: String,
    pub(crate) api_token: Option<String>,
    pub(crate) api_token_env_var: String,
    pub(crate) persistence_database: String,
    pub(crate) publication_database: String,
    pub(crate) secret_store: String,
}

#[derive(Serialize)]
pub(crate) struct CloudflareProbeRow {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) tone: String,
}

#[derive(Serialize)]
pub(crate) struct CloudflareProbeResult {
    pub(crate) rows: Vec<CloudflareProbeRow>,
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
pub(crate) struct EditorialSessionRequest {
    pub(crate) run_id: String,
    pub(crate) session_name: String,
    pub(crate) prompt: String,
    pub(crate) protocol_name: String,
    pub(crate) protocol_text: String,
    pub(crate) protocol_hash: String,
    pub(crate) initial_agent: Option<String>,
    pub(crate) active_agents: Option<Vec<String>>,
    pub(crate) max_session_cost_usd: Option<f64>,
    pub(crate) max_session_minutes: Option<u64>,
    pub(crate) attachments: Option<Vec<PromptAttachmentRequest>>,
    pub(crate) links: Option<Vec<String>>,
}

#[derive(Clone, Deserialize)]
struct ResumeSessionRequest {
    run_id: String,
    protocol_name: Option<String>,
    protocol_text: Option<String>,
    protocol_hash: Option<String>,
    initial_agent: Option<String>,
    active_agents: Option<Vec<String>>,
    max_session_cost_usd: Option<f64>,
    max_session_minutes: Option<u64>,
    attachments: Option<Vec<PromptAttachmentRequest>>,
    links: Option<Vec<String>>,
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
    active_agents: Vec<String>,
    max_session_cost_usd: Option<f64>,
    max_session_minutes: Option<u64>,
    observed_cost_usd: Option<f64>,
    links_path: Option<String>,
    attachments_manifest_path: Option<String>,
    human_log_path: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct EditorialAgentResult {
    pub(crate) name: String,
    pub(crate) role: String,
    pub(crate) cli: String,
    pub(crate) tone: String,
    pub(crate) status: String,
    pub(crate) duration_ms: u128,
    pub(crate) exit_code: Option<i32>,
    pub(crate) output_path: String,
    pub(crate) usage_input_tokens: Option<u64>,
    pub(crate) usage_output_tokens: Option<u64>,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) cost_estimated: Option<bool>,
}

#[derive(Clone)]
pub(crate) struct PreparedAgentInput {
    pub(crate) stdin_text: String,
    pub(crate) original_chars: usize,
    pub(crate) input_path: Option<PathBuf>,
}

pub(crate) struct EffectiveAgentInput {
    pub(crate) args: Vec<String>,
    pub(crate) stdin_text: Option<String>,
    pub(crate) stdin_chars: usize,
    pub(crate) delivery: &'static str,
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
    /// `active_agents` from the saved session contract. Used by the frontend
    /// to pre-populate React state on cold app open so that clicking
    /// "Retomar" continues with the same peers selected when the session
    /// was last saved, instead of overwriting with the cold-open default
    /// of all 4 (B17 fix from v0.3.18).
    saved_active_agents: Vec<String>,
    /// `initial_agent` from the saved session contract.
    saved_initial_agent: Option<String>,
    /// Optional cost cap from the saved session contract.
    saved_max_session_cost_usd: Option<f64>,
    /// Optional time cap from the saved session contract.
    saved_max_session_minutes: Option<u64>,
}

struct ResumeSessionState {
    current_draft: String,
    current_draft_path: Option<PathBuf>,
    next_review_round: usize,
    existing_agents: Vec<EditorialAgentResult>,
}

#[derive(Clone, Copy)]
pub(crate) struct EditorialAgentSpec {
    pub(crate) key: &'static str,
    pub(crate) name: &'static str,
    pub(crate) command: &'static str,
    pub(crate) args: fn() -> Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SessionContract {
    #[serde(default = "default_session_contract_schema_version")]
    schema_version: u8,
    run_id: String,
    session_name: String,
    created_at: String,
    #[serde(default)]
    active_agents: Vec<String>,
    #[serde(default)]
    initial_agent: String,
    #[serde(default)]
    max_session_cost_usd: Option<f64>,
    #[serde(default)]
    max_session_minutes: Option<u64>,
    #[serde(default)]
    pub(crate) links: Vec<String>,
    #[serde(default)]
    pub(crate) attachments: Vec<AttachmentManifestEntry>,
}

fn default_session_contract_schema_version() -> u8 {
    1
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CostLedger {
    pub(crate) schema_version: u8,
    pub(crate) run_id: String,
    pub(crate) total_observed_cost_usd: f64,
    pub(crate) entries: Vec<CostLedgerEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CostLedgerEntry {
    pub(crate) at: String,
    pub(crate) provider: String,
    pub(crate) agent: String,
    pub(crate) role: String,
    pub(crate) model: String,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cost_usd: f64,
    pub(crate) estimated: bool,
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
    stdout_pipe_error: Option<String>,
    stderr_pipe_error: Option<String>,
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

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            provider_mode: "hybrid".to_string(),
            credential_storage_mode: "local_json".to_string(),
            openai_api_key: None,
            anthropic_api_key: None,
            gemini_api_key: None,
            deepseek_api_key: None,
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            openai_input_usd_per_million: None,
            openai_output_usd_per_million: None,
            anthropic_input_usd_per_million: None,
            anthropic_output_usd_per_million: None,
            gemini_input_usd_per_million: None,
            gemini_output_usd_per_million: None,
            deepseek_input_usd_per_million: None,
            deepseek_output_usd_per_million: None,
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
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
    human_log_file: String,
    log_session_id: String,
}

#[tauri::command]
fn runtime_profile(log_session: tauri::State<LogSession>) -> RuntimeProfile {
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
fn write_log_event(
    log_session: tauri::State<LogSession>,
    event: LogEventInput,
) -> Result<LogWriteResult, String> {
    write_log_record(&log_session, event)
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
        "active_human_log_file": human_log_path_for(&log_session.path).to_string_lossy(),
        "log_session_id": log_session.id.clone(),
        "files": files,
        "hint": "Attach the newest data/logs/maestro-*.ndjson file for machine diagnosis; use data/logs/human/*.log for quick human reading."
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
fn read_ai_provider_config() -> Result<AiProviderConfig, String> {
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
fn write_ai_provider_config(
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
fn verify_ai_provider_credentials(config: AiProviderConfig) -> AiProviderProbeResult {
    run_ai_provider_probe(&sanitize_ai_provider_config(config))
}

#[tauri::command]
fn audit_links(request: LinkAuditRequest) -> LinkAuditResult {
    run_link_audit(&request.text)
}

#[tauri::command]
fn open_data_file(path: String) -> Result<String, String> {
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
                "initial_agent": resolve_initial_agent_key(request.initial_agent.as_deref()).0,
                "active_agents_requested": request.active_agents.clone(),
                "max_session_cost_usd": request.max_session_cost_usd,
                "max_session_minutes": request.max_session_minutes,
                "attachment_count": request.attachments.as_ref().map(|items| items.len()).unwrap_or_default(),
                "link_count": request.links.as_ref().map(|items| items.len()).unwrap_or_default(),
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
                "active_agents": result.active_agents.clone(),
                "observed_cost_usd": result.observed_cost_usd,
                "human_log_path": result.human_log_path.clone(),
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
    let saved_contract = load_session_contract(&session_dir);
    let saved_initial_agent = extract_saved_initial_agent(&saved_prompt);
    let requested_initial_agent = request.initial_agent.clone();
    let effective_initial_agent = saved_initial_agent
        .clone()
        .or_else(|| requested_initial_agent.clone());
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
                "saved_initial_agent": saved_initial_agent.clone(),
                "requested_initial_agent": requested_initial_agent.clone(),
                "effective_initial_agent": effective_initial_agent.clone(),
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
        initial_agent: effective_initial_agent,
        active_agents: request.active_agents.clone().or_else(|| {
            saved_contract
                .as_ref()
                .map(|contract| contract.active_agents.clone())
        }),
        max_session_cost_usd: request.max_session_cost_usd.or_else(|| {
            saved_contract
                .as_ref()
                .and_then(|contract| contract.max_session_cost_usd)
        }),
        max_session_minutes: request.max_session_minutes.or_else(|| {
            saved_contract
                .as_ref()
                .and_then(|contract| contract.max_session_minutes)
        }),
        attachments: request.attachments.clone(),
        links: request.links.clone().or_else(|| {
            saved_contract
                .as_ref()
                .map(|contract| contract.links.clone())
        }),
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
                "active_agents": result.active_agents.clone(),
                "observed_cost_usd": result.observed_cost_usd,
                "human_log_path": result.human_log_path.clone(),
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

fn initialize_app_root(app: &tauri::App) -> Result<(), String> {
    let _ = app;
    let root = resolve_portable_app_root()?;
    try_set_app_root(root);
    Ok(())
}

fn install_process_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("unknown panic payload");
        let location = panic_info.location().map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        });
        let _ = write_early_crash_record(payload, location.as_deref());
    }));
}

fn write_early_crash_record(payload: &str, location: Option<&str>) -> Result<(), String> {
    let dir = active_or_early_logs_dir();
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create early crash log dir: {error}"))?;
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let path = dir.join(format!(
        "maestro-crash-{timestamp}-pid{}.json",
        process::id()
    ));
    let record = json!({
        "schema_version": 1,
        "timestamp": Utc::now().to_rfc3339(),
        "level": "fatal",
        "category": "native.panic",
        "message": "native panic captured before normal diagnostic logger completed startup",
        "panic": {
            "payload": sanitize_text(payload, 1000),
            "location": location.map(|value| sanitize_text(value, 500))
        },
        "app": {
            "name": "Maestro Editorial AI",
            "version": env!("CARGO_PKG_VERSION"),
            "target": std::env::consts::OS,
            "arch": std::env::consts::ARCH
        },
        "process": {
            "pid": process::id(),
            "cwd": std::env::current_dir().ok().map(|path| path.to_string_lossy().to_string()),
            "current_exe": std::env::current_exe().ok().map(|path| path.to_string_lossy().to_string()),
            "app_root": app_root_if_initialized().map(|path| path.to_string_lossy().to_string())
        }
    });
    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|error| format!("failed to serialize early crash log: {error}"))?;
    fs::write(&path, bytes).map_err(|error| format!("failed to write early crash log: {error}"))
}

fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    // SAFE-FUNNEL: this is the single allowed Command::new call site for editorial spawns.
    // See `clippy.toml` and the `#![warn(clippy::disallowed_methods)]` at the top of this file.
    #[allow(clippy::disallowed_methods)]
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

fn persist_ai_provider_config(path: &PathBuf, config: &AiProviderConfig) -> Result<(), String> {
    let path = checked_data_child_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create config dir: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("failed to serialize AI provider config: {error}"))?;
    fs::write(&path, bytes).map_err(|error| format!("failed to write AI provider config: {error}"))
}

fn persist_ai_provider_cloudflare_marker(
    path: &PathBuf,
    config: &AiProviderConfig,
) -> Result<(), String> {
    let marker = AiProviderConfig {
        schema_version: config.schema_version,
        provider_mode: config.provider_mode.clone(),
        credential_storage_mode: "cloudflare".to_string(),
        openai_api_key: None,
        anthropic_api_key: None,
        gemini_api_key: None,
        deepseek_api_key: None,
        openai_api_key_remote: config.openai_api_key.is_some() || config.openai_api_key_remote,
        anthropic_api_key_remote: config.anthropic_api_key.is_some()
            || config.anthropic_api_key_remote,
        gemini_api_key_remote: config.gemini_api_key.is_some() || config.gemini_api_key_remote,
        deepseek_api_key_remote: config.deepseek_api_key.is_some()
            || config.deepseek_api_key_remote,
        openai_input_usd_per_million: config.openai_input_usd_per_million,
        openai_output_usd_per_million: config.openai_output_usd_per_million,
        anthropic_input_usd_per_million: config.anthropic_input_usd_per_million,
        anthropic_output_usd_per_million: config.anthropic_output_usd_per_million,
        gemini_input_usd_per_million: config.gemini_input_usd_per_million,
        gemini_output_usd_per_million: config.gemini_output_usd_per_million,
        deepseek_input_usd_per_million: config.deepseek_input_usd_per_million,
        deepseek_output_usd_per_million: config.deepseek_output_usd_per_million,
        cloudflare_secret_store_id: config.cloudflare_secret_store_id.clone(),
        cloudflare_secret_store_name: config.cloudflare_secret_store_name.clone(),
        updated_at: config.updated_at.clone(),
    };
    persist_ai_provider_config(path, &marker)
}

fn persist_ai_provider_config_to_cloudflare(
    config: &AiProviderConfig,
    request: &CloudflareProviderStorageRequest,
) -> Result<(), String> {
    let token = cloudflare_token_from_provider_request(request)?;
    let account_id = sanitize_short(request.account_id.trim(), 80);
    if account_id.is_empty() {
        return Err("Account ID da Cloudflare ausente".to_string());
    }

    let persistence_database = sanitize_short(&request.persistence_database, 80);
    if persistence_database.is_empty() {
        return Err("nome do banco D1 de persistencia ausente".to_string());
    }

    let requested_store_name = sanitize_short(&request.secret_store, 80);
    if requested_store_name.is_empty() {
        return Err("nome do Secrets Store ausente".to_string());
    }

    let client = cloudflare_client()?;
    let database_id =
        ensure_cloudflare_d1_database(&client, &token, &account_id, &persistence_database)?;
    let store = ensure_cloudflare_secret_store(
        &client,
        &token,
        &account_id,
        &requested_store_name,
        Some(&database_id),
    )?;

    let secrets = ai_provider_secret_values(config);
    let secret_records =
        upsert_ai_provider_secrets(&client, &token, &account_id, &store, &secrets)?;

    write_ai_provider_metadata_to_cloudflare(
        &client,
        &token,
        &account_id,
        &database_id,
        config,
        &requested_store_name,
        &store,
        &secret_records,
    )
}

fn normalize_storage_mode(value: &str) -> &'static str {
    match value {
        "windows_env" => "windows_env",
        "cloudflare" => "cloudflare",
        _ => "local_json",
    }
}

fn normalize_provider_mode(value: &str) -> &'static str {
    match value {
        "cli" => "cli",
        "api" => "api",
        _ => "hybrid",
    }
}

fn normalize_cloudflare_token_source(value: &str) -> &'static str {
    match value {
        "windows_env" => "windows_env",
        "local_encrypted" => "local_encrypted",
        _ => "prompt_each_launch",
    }
}

fn sanitize_ai_provider_config(config: AiProviderConfig) -> AiProviderConfig {
    AiProviderConfig {
        schema_version: 1,
        provider_mode: normalize_provider_mode(&config.provider_mode).to_string(),
        credential_storage_mode: normalize_storage_mode(&config.credential_storage_mode)
            .to_string(),
        openai_api_key: sanitize_optional_secret(config.openai_api_key),
        anthropic_api_key: sanitize_optional_secret(config.anthropic_api_key),
        gemini_api_key: sanitize_optional_secret(config.gemini_api_key),
        deepseek_api_key: sanitize_optional_secret(config.deepseek_api_key),
        openai_api_key_remote: config.openai_api_key_remote,
        anthropic_api_key_remote: config.anthropic_api_key_remote,
        gemini_api_key_remote: config.gemini_api_key_remote,
        deepseek_api_key_remote: config.deepseek_api_key_remote,
        openai_input_usd_per_million: sanitize_optional_cost_rate(
            config.openai_input_usd_per_million,
        ),
        openai_output_usd_per_million: sanitize_optional_cost_rate(
            config.openai_output_usd_per_million,
        ),
        anthropic_input_usd_per_million: sanitize_optional_cost_rate(
            config.anthropic_input_usd_per_million,
        ),
        anthropic_output_usd_per_million: sanitize_optional_cost_rate(
            config.anthropic_output_usd_per_million,
        ),
        gemini_input_usd_per_million: sanitize_optional_cost_rate(
            config.gemini_input_usd_per_million,
        ),
        gemini_output_usd_per_million: sanitize_optional_cost_rate(
            config.gemini_output_usd_per_million,
        ),
        deepseek_input_usd_per_million: sanitize_optional_cost_rate(
            config.deepseek_input_usd_per_million,
        ),
        deepseek_output_usd_per_million: sanitize_optional_cost_rate(
            config.deepseek_output_usd_per_million,
        ),
        cloudflare_secret_store_id: config
            .cloudflare_secret_store_id
            .map(|value| sanitize_short(&value, 80))
            .filter(|value| !value.is_empty()),
        cloudflare_secret_store_name: config
            .cloudflare_secret_store_name
            .map(|value| sanitize_short(&value, 80))
            .filter(|value| !value.is_empty()),
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn sanitize_optional_cost_rate(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0 && *value <= 10_000.0)
}

fn provider_cost_rates_from_config(
    agent_key: &str,
    config: &AiProviderConfig,
) -> Result<ProviderCostRates, String> {
    let (label, input, output) = match agent_key {
        "claude" => (
            "Anthropic / Claude",
            config.anthropic_input_usd_per_million,
            config.anthropic_output_usd_per_million,
        ),
        "codex" => (
            "OpenAI / Codex",
            config.openai_input_usd_per_million,
            config.openai_output_usd_per_million,
        ),
        "gemini" => (
            "Google / Gemini",
            config.gemini_input_usd_per_million,
            config.gemini_output_usd_per_million,
        ),
        "deepseek" => (
            "DeepSeek",
            config.deepseek_input_usd_per_million,
            config.deepseek_output_usd_per_million,
        ),
        _ => {
            return Err(format!(
                "Peer editorial sem provedor de tarifa conhecido: {}.",
                sanitize_text(agent_key, 80)
            ))
        }
    };
    let input = input.ok_or_else(|| {
        format!(
            "Configure a tarifa de entrada do provedor {label} em Configuracoes > Agentes via API > Tabela de tarifas."
        )
    })?;
    let output = output.ok_or_else(|| {
        format!(
            "Configure a tarifa de saida do provedor {label} em Configuracoes > Agentes via API > Tabela de tarifas."
        )
    })?;
    Ok(ProviderCostRates {
        input_usd_per_million: input,
        output_usd_per_million: output,
    })
}

fn api_provider_for_agent(agent_key: &str) -> Option<&'static str> {
    match agent_key {
        "claude" => Some("anthropic"),
        "codex" => Some("openai"),
        "gemini" => Some("gemini"),
        "deepseek" => Some("deepseek"),
        _ => None,
    }
}

fn api_cli_for_agent(agent_key: &str) -> &'static str {
    match agent_key {
        "claude" => "anthropic-api",
        "codex" => "openai-api",
        "gemini" => "gemini-api",
        "deepseek" => "deepseek-api",
        _ => "provider-api",
    }
}

pub(crate) fn provider_label_for_agent(agent_key: &str) -> &'static str {
    match agent_key {
        "claude" => "Anthropic / Claude",
        "codex" => "OpenAI / Codex",
        "gemini" => "Google / Gemini",
        "deepseek" => "DeepSeek",
        _ => "Provedor API",
    }
}

pub(crate) fn provider_remote_present(config: &AiProviderConfig, agent_key: &str) -> bool {
    match agent_key {
        "claude" => config.anthropic_api_key_remote,
        "codex" => config.openai_api_key_remote,
        "gemini" => config.gemini_api_key_remote,
        "deepseek" => config.deepseek_api_key_remote,
        _ => false,
    }
}

pub(crate) fn provider_key_for_agent(config: &AiProviderConfig, agent_key: &str) -> Option<(String, String)> {
    match agent_key {
        "claude" => effective_provider_key(
            config.anthropic_api_key.as_deref(),
            &["MAESTRO_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"],
        ),
        "codex" => effective_provider_key(
            config.openai_api_key.as_deref(),
            &["MAESTRO_OPENAI_API_KEY", "OPENAI_API_KEY"],
        ),
        "gemini" => effective_provider_key(
            config.gemini_api_key.as_deref(),
            &["MAESTRO_GEMINI_API_KEY", "GEMINI_API_KEY"],
        ),
        "deepseek" => effective_provider_key(
            config.deepseek_api_key.as_deref(),
            &["MAESTRO_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"],
        ),
        _ => None,
    }
}

fn should_run_agent_via_api(agent_key: &str, config: &AiProviderConfig) -> bool {
    if api_provider_for_agent(agent_key).is_none() {
        return false;
    }
    let mode = normalize_provider_mode(&config.provider_mode);
    if agent_key == "deepseek" {
        return true;
    }
    match mode {
        "api" => true,
        "cli" => false,
        _ => provider_key_for_agent(config, agent_key).is_some(),
    }
}

fn sanitize_optional_secret(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().chars().take(4096).collect::<String>())
        .filter(|text| !text.is_empty())
}

fn merge_ai_provider_env_values(mut config: AiProviderConfig) -> AiProviderConfig {
    if config.openai_api_key.is_none() {
        config.openai_api_key = provider_env_value(&["MAESTRO_OPENAI_API_KEY", "OPENAI_API_KEY"]);
    }
    if config.anthropic_api_key.is_none() {
        config.anthropic_api_key =
            provider_env_value(&["MAESTRO_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"]);
    }
    if config.gemini_api_key.is_none() {
        config.gemini_api_key = provider_env_value(&["MAESTRO_GEMINI_API_KEY", "GEMINI_API_KEY"]);
    }
    if config.deepseek_api_key.is_none() {
        config.deepseek_api_key =
            provider_env_value(&["MAESTRO_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"]);
    }
    sanitize_ai_provider_config(config)
}

fn provider_env_value(candidates: &[&str]) -> Option<String> {
    first_env_value(candidates).map(|(_, _, value)| value)
}

fn enrich_ai_provider_config_from_cloudflare(
    mut config: AiProviderConfig,
    bootstrap: &BootstrapConfig,
) -> AiProviderConfig {
    if let Ok(remote) = read_ai_provider_cloudflare_metadata(bootstrap) {
        config.provider_mode = remote.provider_mode;
        config.openai_api_key_remote |= remote.openai_api_key_remote;
        config.anthropic_api_key_remote |= remote.anthropic_api_key_remote;
        config.gemini_api_key_remote |= remote.gemini_api_key_remote;
        config.deepseek_api_key_remote |= remote.deepseek_api_key_remote;
        config.cloudflare_secret_store_id = remote
            .cloudflare_secret_store_id
            .or(config.cloudflare_secret_store_id);
        config.cloudflare_secret_store_name = remote
            .cloudflare_secret_store_name
            .or(config.cloudflare_secret_store_name);
    }
    config
}

fn read_ai_provider_cloudflare_metadata(
    bootstrap: &BootstrapConfig,
) -> Result<AiProviderConfig, String> {
    let account_id = bootstrap
        .cloudflare_account_id
        .as_deref()
        .map(|value| sanitize_short(value, 80))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Cloudflare account id ausente no bootstrap".to_string())?;
    let token_request = CloudflareProviderStorageRequest {
        account_id: account_id.clone(),
        api_token: None,
        api_token_env_var: bootstrap.cloudflare_api_token_env_var.clone(),
        persistence_database: bootstrap.cloudflare_persistence_database.clone(),
        secret_store: bootstrap.cloudflare_secret_store.clone(),
    };
    let token = cloudflare_token_from_provider_request(&token_request)?;
    let client = cloudflare_client()?;
    let d1_path = format!("/accounts/{account_id}/d1/database");
    let listed = cloudflare_get(&client, &token, &d1_path)?;
    let Some(database_id) =
        cloudflare_result_id_for_name(&listed, &bootstrap.cloudflare_persistence_database)
    else {
        return Err("maestro_db nao encontrado para recuperar metadados".to_string());
    };

    let raw_path = format!("/accounts/{account_id}/d1/database/{database_id}/raw");
    let value = cloudflare_post_json(
        &client,
        &token,
        &raw_path,
        json!({
            "sql": "SELECT value_json FROM maestro_settings WHERE key = ?",
            "params": ["ai.providers"]
        }),
    )?;
    let Some(value_json) = json_find_first_string(&value, "value_json") else {
        return Err("metadados ai.providers nao encontrados em maestro_db".to_string());
    };
    let metadata: Value = serde_json::from_str(&value_json)
        .map_err(|error| format!("metadados ai.providers invalidos: {error}"))?;
    let mut config = AiProviderConfig {
        credential_storage_mode: "cloudflare".to_string(),
        provider_mode: metadata
            .get("provider_mode")
            .and_then(Value::as_str)
            .unwrap_or("hybrid")
            .to_string(),
        updated_at: metadata
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        cloudflare_secret_store_id: metadata
            .get("effective_store_id")
            .and_then(Value::as_str)
            .map(|value| value.to_string()),
        cloudflare_secret_store_name: metadata
            .get("effective_store_name")
            .and_then(Value::as_str)
            .map(|value| value.to_string()),
        openai_input_usd_per_million: metadata
            .get("openai_input_usd_per_million")
            .and_then(Value::as_f64),
        openai_output_usd_per_million: metadata
            .get("openai_output_usd_per_million")
            .and_then(Value::as_f64),
        anthropic_input_usd_per_million: metadata
            .get("anthropic_input_usd_per_million")
            .and_then(Value::as_f64),
        anthropic_output_usd_per_million: metadata
            .get("anthropic_output_usd_per_million")
            .and_then(Value::as_f64),
        gemini_input_usd_per_million: metadata
            .get("gemini_input_usd_per_million")
            .and_then(Value::as_f64),
        gemini_output_usd_per_million: metadata
            .get("gemini_output_usd_per_million")
            .and_then(Value::as_f64),
        deepseek_input_usd_per_million: metadata
            .get("deepseek_input_usd_per_million")
            .and_then(Value::as_f64),
        deepseek_output_usd_per_million: metadata
            .get("deepseek_output_usd_per_million")
            .and_then(Value::as_f64),
        ..AiProviderConfig::default()
    };

    if let Some(items) = metadata.get("secrets").and_then(Value::as_array) {
        for item in items {
            match item.get("name").and_then(Value::as_str).unwrap_or_default() {
                "MAESTRO_OPENAI_API_KEY" => config.openai_api_key_remote = true,
                "MAESTRO_ANTHROPIC_API_KEY" => config.anthropic_api_key_remote = true,
                "MAESTRO_GEMINI_API_KEY" => config.gemini_api_key_remote = true,
                "MAESTRO_DEEPSEEK_API_KEY" => config.deepseek_api_key_remote = true,
                _ => {}
            }
        }
    }

    Ok(sanitize_ai_provider_config(config))
}

fn json_find_first_string(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .map(|value| value.to_string())
            .or_else(|| {
                map.values()
                    .find_map(|item| json_find_first_string(item, key))
            }),
        Value::Array(items) => items
            .iter()
            .find_map(|item| json_find_first_string(item, key)),
        _ => None,
    }
}

pub(crate) fn first_env_value(candidates: &[&str]) -> Option<(String, String, String)> {
    candidates.iter().find_map(|name| {
        env_value_with_scope(name).map(|(scope, value)| ((*name).to_string(), scope, value))
    })
}

pub(crate) fn env_value_with_scope(name: &str) -> Option<(String, String)> {
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
    let _finalize_guard = FinalizeRunningArtifactsGuard::new(agent_dir.clone());

    let saved_contract = load_session_contract(&session_dir);
    let (active_agent_keys, active_agents_source) = resolve_effective_active_agents(
        request.active_agents.as_ref(),
        saved_contract.as_ref().map(|contract| &contract.active_agents),
    )?;
    let (draft_lead_key, invalid_initial_agent) =
        effective_draft_lead(request.initial_agent.as_deref(), &active_agent_keys);
    let draft_lead_name = selected_editorial_agent_specs(draft_lead_key, &active_agent_keys)
        .first()
        .map(|spec| spec.name)
        .unwrap_or("Claude");
    let log_context = build_active_agents_resolved_log_context(
        &run_id,
        request.active_agents.as_ref(),
        saved_contract.as_ref(),
        &active_agent_keys,
        active_agents_source,
        draft_lead_key,
        invalid_initial_agent.as_deref(),
        request.max_session_cost_usd,
        request.max_session_minutes,
    );
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.editorial.active_agents_resolved".to_string(),
            message: "effective active_agents and caps resolved before spawning".to_string(),
            context: Some(log_context),
        },
    );
    let max_session_cost_usd =
        sanitize_optional_positive_f64(request.max_session_cost_usd.or_else(|| {
            saved_contract
                .as_ref()
                .and_then(|contract| contract.max_session_cost_usd)
        }));
    let max_session_minutes =
        sanitize_optional_positive_u64(request.max_session_minutes.or_else(|| {
            saved_contract
                .as_ref()
                .and_then(|contract| contract.max_session_minutes)
        }));
    let created_at = saved_contract
        .as_ref()
        .map(|contract| parse_created_at(&contract.created_at))
        .unwrap_or_else(Utc::now);
    let time_budget_anchor =
        resolve_time_budget_anchor(created_at, resume_state.is_some(), Utc::now());
    let ai_provider_config =
        read_ai_provider_config().unwrap_or_else(|_| AiProviderConfig::default());
    let mut cost_ledger = load_cost_ledger(&session_dir, &run_id);
    let api_agent_keys = active_agent_keys
        .iter()
        .filter(|key| should_run_agent_via_api(key, &ai_provider_config))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut provider_cost_rates = BTreeMap::new();
    for agent_key in &api_agent_keys {
        match provider_cost_rates_from_config(agent_key, &ai_provider_config) {
            Ok(rates) => {
                provider_cost_rates.insert(agent_key.clone(), rates);
            }
            Err(error) => {
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "error".to_string(),
                        category: "session.cost.config_missing".to_string(),
                        message: "API usage requires UI provider tariff configuration".to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "error": sanitize_text(&error, 500),
                            "agent": agent_key,
                            "provider": api_provider_for_agent(agent_key).unwrap_or("unknown"),
                            "active_agents": active_agent_keys.clone()
                        })),
                    },
                );
                let prompt_path = session_dir.join("prompt.md");
                let protocol_path = session_dir.join("protocolo.md");
                let _ = write_text_file(
                        &prompt_path,
                        &format!(
                            "# Prompt da Sessao\n\nSessao: {}\nRun: `{}`\nAgente redator inicial: `{}`\n\n{}",
                            sanitize_text(&request.session_name, 200),
                            run_id,
                            draft_lead_key,
                            prompt
                        ),
                    );
                let _ = write_text_file(&protocol_path, &request.protocol_text);
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &[], false, None),
                )?;
                return Ok(EditorialSessionResult {
                    run_id,
                    session_dir: session_dir.to_string_lossy().to_string(),
                    final_markdown_path: None,
                    session_minutes_path: minutes_path.to_string_lossy().to_string(),
                    prompt_path: session_dir.join("prompt.md").to_string_lossy().to_string(),
                    protocol_path: session_dir
                        .join("protocolo.md")
                        .to_string_lossy()
                        .to_string(),
                    draft_path: None,
                    agents: Vec::new(),
                    consensus_ready: false,
                    status: "PAUSED_COST_RATES_MISSING".to_string(),
                    active_agents: active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: Some(cost_ledger.total_observed_cost_usd),
                    links_path: None,
                    attachments_manifest_path: None,
                    human_log_path: Some(
                        human_log_path_for(&log_session.path)
                            .to_string_lossy()
                            .to_string(),
                    ),
                });
            }
        }
    }
    let evidence = process_session_evidence(
        &session_dir,
        request.links.as_ref(),
        request.attachments.as_ref(),
        saved_contract.as_ref(),
    )?;
    let contract = SessionContract {
        schema_version: 1,
        run_id: run_id.clone(),
        session_name: sanitize_text(&request.session_name, 200),
        created_at: created_at.to_rfc3339(),
        active_agents: active_agent_keys.clone(),
        initial_agent: draft_lead_key.to_string(),
        max_session_cost_usd,
        max_session_minutes,
        links: evidence.links.clone(),
        attachments: evidence.attachments.clone(),
    };
    write_session_contract(&session_dir, &contract)?;

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    write_text_file(
        &prompt_path,
        &format!(
            "# Prompt da Sessao\n\nSessao: {}\nRun: `{}`\nAgente redator inicial: `{}`\n\n{}",
            sanitize_text(&request.session_name, 200),
            run_id,
            draft_lead_key,
            prompt
        ),
    )?;
    write_text_file(&protocol_path, &request.protocol_text)?;
    let human_log_path = human_log_path_for(&log_session.path);

    let mut agents = Vec::new();
    let mut current_draft = String::new();
    let mut current_draft_path: Option<PathBuf> = None;
    let mut round = 1usize;
    let mut agent_review_fingerprints: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut consecutive_all_error_rounds: u32 = 0;
    const ALL_ERROR_ESCALATION_THRESHOLD: u32 = 3;

    if let Some(invalid_initial_agent) = invalid_initial_agent {
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "warn".to_string(),
                category: "session.draft_lead.invalid".to_string(),
                message: "unknown initial editorial agent requested; falling back to Claude"
                    .to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "requested_initial_agent": invalid_initial_agent,
                    "fallback_initial_agent": draft_lead_key
                })),
            },
        );
    }

    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.draft_lead.selected".to_string(),
            message: "editorial draft lead selected for initial draft and revision fallback order"
                .to_string(),
            context: Some(json!({
                "run_id": &run_id,
                "initial_agent": draft_lead_key,
                "initial_agent_name": draft_lead_name,
                "active_agents": active_agent_keys.clone(),
                "agent_order": selected_editorial_agent_specs(draft_lead_key, &active_agent_keys)
                    .iter()
                    .map(|spec| spec.key)
                    .collect::<Vec<_>>()
            })),
        },
    );

    if let Some(state) = resume_state {
        agents = filter_existing_agents_to_active_set(state.existing_agents, &active_agent_keys);
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
        let draft_specs = selected_editorial_agent_specs(draft_lead_key, &active_agent_keys);

        for spec in draft_specs {
            if session_time_exhausted(time_budget_anchor, max_session_minutes) {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "TIME_LIMIT_REACHED",
                ));
            }
            let output_path = agent_dir.join(format!("round-001-{}-draft.md", spec.key));
            let timeout = remaining_session_duration(time_budget_anchor, max_session_minutes);
            let use_api_agent = api_agent_keys.contains(spec.key);
            let cost_guard = if use_api_agent {
                provider_cost_guard_for(
                    max_session_cost_usd,
                    provider_cost_rates.get(spec.key).copied(),
                    &cost_ledger,
                )
            } else {
                None
            };
            let draft_run = run_editorial_agent_for_spec(
                log_session,
                &run_id,
                spec,
                "draft",
                build_draft_prompt(request, &run_id, &evidence.block),
                &evidence.attachments,
                &output_path,
                timeout,
                &ai_provider_config,
                cost_guard,
                use_api_agent,
            );
            agents.push(draft_run.clone());
            append_agent_cost_to_ledger(&session_dir, &mut cost_ledger, &draft_run)?;
            if draft_run.status == "COST_LIMIT_REACHED" {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "COST_LIMIT_REACHED",
                ));
            }
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
                        "agent": spec.name,
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

        let context = SessionResultContext {
            run_id: &run_id,
            session_dir: &session_dir,
            prompt_path: &prompt_path,
            protocol_path: &protocol_path,
            active_agents: &active_agent_keys,
            max_session_cost_usd,
            max_session_minutes,
            observed_cost_usd: cost_ledger.total_observed_cost_usd,
            links_path: evidence.links_path.as_ref(),
            attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
            human_log_path: &human_log_path,
        };
        return Ok(editorial_session_result(
            &context,
            None,
            &minutes_path,
            current_draft_path,
            agents,
            false,
            "PAUSED_DRAFT_UNAVAILABLE",
        ));
    }

    let final_path: PathBuf;
    loop {
        if session_time_exhausted(time_budget_anchor, max_session_minutes) {
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "TIME_LIMIT_REACHED",
            ));
        }
        let review_specs = selected_editorial_agent_specs(draft_lead_key, &active_agent_keys);
        let enforce_sequential_budget = max_session_cost_usd.is_some()
            && review_specs
                .iter()
                .any(|spec| api_agent_keys.contains(spec.key));
        let mut round_results = Vec::new();
        if enforce_sequential_budget {
            for spec in review_specs {
                let prompt = build_review_prompt(request, &run_id, &current_draft, &evidence.block);
                let output_path =
                    agent_dir.join(format!("round-{round:03}-{}-review.md", spec.key));
                let timeout = remaining_session_duration(time_budget_anchor, max_session_minutes);
                let use_api_agent = api_agent_keys.contains(spec.key);
                let cost_guard = if use_api_agent {
                    provider_cost_guard_for(
                        max_session_cost_usd,
                        provider_cost_rates.get(spec.key).copied(),
                        &cost_ledger,
                    )
                } else {
                    None
                };
                let result = run_editorial_agent_for_spec(
                    log_session,
                    &run_id,
                    spec,
                    "review",
                    prompt,
                    &evidence.attachments,
                    &output_path,
                    timeout,
                    &ai_provider_config,
                    cost_guard,
                    use_api_agent,
                );
                let cost_limit_reached = result.status == "COST_LIMIT_REACHED";
                round_results.push(result.clone());
                append_agent_cost_to_ledger(&session_dir, &mut cost_ledger, &result)?;
                agents.push(result);
                if cost_limit_reached {
                    break;
                }
            }
        } else {
            let review_handles = review_specs
                .into_iter()
                .map(|spec| {
                    let prompt =
                        build_review_prompt(request, &run_id, &current_draft, &evidence.block);
                    let attachments = evidence.attachments.clone();
                    let output_path =
                        agent_dir.join(format!("round-{round:03}-{}-review.md", spec.key));
                    let run_id = run_id.clone();
                    let log_session = log_session.clone();
                    let ai_provider_config = ai_provider_config.clone();
                    let timeout = remaining_session_duration(time_budget_anchor, max_session_minutes);
                    let use_api_agent = api_agent_keys.contains(spec.key);
                    let cost_guard = if use_api_agent {
                        provider_cost_guard_for(
                            max_session_cost_usd,
                            provider_cost_rates.get(spec.key).copied(),
                            &cost_ledger,
                        )
                    } else {
                        None
                    };
                    thread::spawn(move || {
                        run_editorial_agent_for_spec(
                            &log_session,
                            &run_id,
                            spec,
                            "review",
                            prompt,
                            &attachments,
                            &output_path,
                            timeout,
                            &ai_provider_config,
                            cost_guard,
                            use_api_agent,
                        )
                    })
                })
                .collect::<Vec<_>>();

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
                    usage_input_tokens: None,
                    usage_output_tokens: None,
                    cost_usd: None,
                    cost_estimated: None,
                });
                round_results.push(result.clone());
                append_agent_cost_to_ledger(&session_dir, &mut cost_ledger, &result)?;
                agents.push(result);
            }
        }

        if round_results
            .iter()
            .any(|agent| agent.status == "COST_LIMIT_REACHED")
        {
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "COST_LIMIT_REACHED",
            ));
        }

        let consensus_ready = round_results
            .iter()
            .all(|agent| agent.tone == "ok" && agent.status == "READY");
        if consensus_ready {
            let path = session_dir.join("texto-final.md");
            write_text_file(&path, &current_draft)?;
            final_path = path;
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
                        "next_state": "continue_with_revision_and_retry_reviews"
                    })),
                },
            );
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

        for agent in &round_results {
            if agent.status == "READY" {
                continue;
            }
            let fingerprint =
                review_complaint_fingerprint(&read_text_file(Path::new(&agent.output_path)).unwrap_or_default());
            let history = agent_review_fingerprints
                .entry(agent.name.clone())
                .or_default();
            history.push(fingerprint);
            if history.len() > 3 {
                history.remove(0);
            }
            if history.len() == 3 && history.windows(2).all(|pair| pair[0] == pair[1]) {
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "warn".to_string(),
                        category: "session.divergence.persistent".to_string(),
                        message: "agent has repeated the same NOT_READY rebuttal across 3 consecutive rounds".to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "round": round,
                            "agent": agent.name.clone(),
                            "status": agent.status.clone(),
                            "fingerprint": history.last().copied(),
                            "policy": "partial_v0_3_15_surface_only_no_auto_resolve"
                        })),
                    },
                );
            }
        }

        let all_review_peers_in_error = !round_results.is_empty()
            && round_results
                .iter()
                .all(|agent| agent.tone == "error" || agent.tone == "blocked");
        if all_review_peers_in_error {
            consecutive_all_error_rounds += 1;
        } else {
            consecutive_all_error_rounds = 0;
        }
        if consecutive_all_error_rounds >= ALL_ERROR_ESCALATION_THRESHOLD {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "error".to_string(),
                    category: "session.escalation.all_peers_failing".to_string(),
                    message: "all review peers have been in error tone across N consecutive rounds; pausing session for operator review".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "consecutive_all_error_rounds": consecutive_all_error_rounds,
                        "threshold": ALL_ERROR_ESCALATION_THRESHOLD,
                        "peer_statuses": round_results.iter()
                            .map(|agent| json!({
                                "agent": agent.name,
                                "status": agent.status,
                                "tone": agent.tone,
                            }))
                            .collect::<Vec<_>>(),
                        "policy": "pause_for_operator_review_to_avoid_burning_quota_and_time"
                    })),
                },
            );
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "ALL_PEERS_FAILING",
            ));
        }

        round += 1;
        let revision_prompt = build_revision_prompt(
            request,
            &run_id,
            round,
            &current_draft,
            &round_results,
            &evidence.block,
        );
        let revision_specs = selected_editorial_agent_specs(draft_lead_key, &active_agent_keys);
        let mut revised = false;
        for spec in revision_specs {
            if session_time_exhausted(time_budget_anchor, max_session_minutes) {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "TIME_LIMIT_REACHED",
                ));
            }
            let output_path = agent_dir.join(format!("round-{round:03}-{}-revision.md", spec.key));
            let timeout = remaining_session_duration(time_budget_anchor, max_session_minutes);
            let use_api_agent = api_agent_keys.contains(spec.key);
            let cost_guard = if use_api_agent {
                provider_cost_guard_for(
                    max_session_cost_usd,
                    provider_cost_rates.get(spec.key).copied(),
                    &cost_ledger,
                )
            } else {
                None
            };
            let revision_run = run_editorial_agent_for_spec(
                log_session,
                &run_id,
                spec,
                "revision",
                revision_prompt.clone(),
                &evidence.attachments,
                &output_path,
                timeout,
                &ai_provider_config,
                cost_guard,
                use_api_agent,
            );
            agents.push(revision_run.clone());
            append_agent_cost_to_ledger(&session_dir, &mut cost_ledger, &revision_run)?;
            if revision_run.status == "COST_LIMIT_REACHED" {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "COST_LIMIT_REACHED",
                ));
            }
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
                        "no revision agent produced usable text; keeping current draft and retrying review"
                            .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "policy": "continue_until_unanimous_ready"
                    })),
                },
            );
        }
    }

    let minutes_path = session_dir.join("ata-da-sessao.md");
    write_text_file(
        &minutes_path,
        &build_session_minutes(request, &run_id, &agents, true, Some(&final_path)),
    )?;

    let context = SessionResultContext {
        run_id: &run_id,
        session_dir: &session_dir,
        prompt_path: &prompt_path,
        protocol_path: &protocol_path,
        active_agents: &active_agent_keys,
        max_session_cost_usd,
        max_session_minutes,
        observed_cost_usd: cost_ledger.total_observed_cost_usd,
        links_path: evidence.links_path.as_ref(),
        attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
        human_log_path: &human_log_path,
    };
    Ok(editorial_session_result(
        &context,
        Some(&final_path),
        &minutes_path,
        current_draft_path,
        agents,
        true,
        "READY_UNANIMOUS",
    ))
}

pub(crate) fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    let path = checked_data_child_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create artifact dir: {error}"))?;
    }
    fs::write(&path, text).map_err(|error| format!("failed to write artifact: {error}"))
}

pub(crate) fn read_text_file(path: &Path) -> Result<String, String> {
    let path = checked_data_child_path(path)?;
    fs::read_to_string(&path).map_err(|error| format!("failed to read artifact: {error}"))
}

struct SessionResultContext<'a> {
    run_id: &'a str,
    session_dir: &'a Path,
    prompt_path: &'a Path,
    protocol_path: &'a Path,
    active_agents: &'a [String],
    max_session_cost_usd: Option<f64>,
    max_session_minutes: Option<u64>,
    observed_cost_usd: f64,
    links_path: Option<&'a PathBuf>,
    attachments_manifest_path: Option<&'a PathBuf>,
    human_log_path: &'a Path,
}

fn editorial_session_result(
    context: &SessionResultContext<'_>,
    final_path: Option<&PathBuf>,
    minutes_path: &Path,
    draft_path: Option<PathBuf>,
    agents: Vec<EditorialAgentResult>,
    consensus_ready: bool,
    status: &str,
) -> EditorialSessionResult {
    finalize_running_agent_artifacts(&context.session_dir.join("agent-runs"));
    EditorialSessionResult {
        run_id: context.run_id.to_string(),
        session_dir: context.session_dir.to_string_lossy().to_string(),
        final_markdown_path: final_path.map(|path| path.to_string_lossy().to_string()),
        session_minutes_path: minutes_path.to_string_lossy().to_string(),
        prompt_path: context.prompt_path.to_string_lossy().to_string(),
        protocol_path: context.protocol_path.to_string_lossy().to_string(),
        draft_path: draft_path.map(|path| path.to_string_lossy().to_string()),
        agents,
        consensus_ready,
        status: status.to_string(),
        active_agents: context.active_agents.to_vec(),
        max_session_cost_usd: context.max_session_cost_usd,
        max_session_minutes: context.max_session_minutes,
        observed_cost_usd: Some(context.observed_cost_usd),
        links_path: context
            .links_path
            .map(|path| path.to_string_lossy().to_string()),
        attachments_manifest_path: context
            .attachments_manifest_path
            .map(|path| path.to_string_lossy().to_string()),
        human_log_path: Some(context.human_log_path.to_string_lossy().to_string()),
    }
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

    let saved_contract = load_session_contract(&session_dir);
    let saved_active_agents = saved_contract
        .as_ref()
        .map(|contract| contract.active_agents.clone())
        .unwrap_or_default();
    let saved_initial_agent = saved_contract
        .as_ref()
        .map(|contract| contract.initial_agent.clone())
        .filter(|value| !value.trim().is_empty());
    let saved_max_session_cost_usd = saved_contract
        .as_ref()
        .and_then(|contract| contract.max_session_cost_usd);
    let saved_max_session_minutes = saved_contract
        .as_ref()
        .and_then(|contract| contract.max_session_minutes);

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
        saved_active_agents,
        saved_initial_agent,
        saved_max_session_cost_usd,
        saved_max_session_minutes,
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
pub(crate) struct SessionArtifact {
    pub(crate) round: usize,
    pub(crate) agent: String,
    pub(crate) role: String,
    pub(crate) path: PathBuf,
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
        "claude" | "codex" | "gemini" | "deepseek" => agent,
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
    let usage_input_tokens = extract_bullet_code_value(&text, "Usage input tokens")
        .and_then(|value| value.parse::<u64>().ok());
    let usage_output_tokens = extract_bullet_code_value(&text, "Usage output tokens")
        .and_then(|value| value.parse::<u64>().ok());
    let cost_usd =
        extract_bullet_code_value(&text, "Cost USD").and_then(|value| value.parse::<f64>().ok());
    let tone = if status == "READY" || status == "DRAFT_CREATED" {
        "ok"
    } else if status == "CLI_NOT_FOUND"
        || status == "API_KEY_NOT_AVAILABLE"
        || status == "REMOTE_SECRET_NOT_READABLE"
    {
        "blocked"
    } else if status.starts_with("EXEC_ERROR")
        || status == "AGENT_FAILED_NO_OUTPUT"
        || status == "AGENT_FAILED_EMPTY"
        || status == "EMPTY_DRAFT"
        || status == "RUNNING"
    {
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
        usage_input_tokens,
        usage_output_tokens,
        cost_usd,
        cost_estimated: cost_usd.map(|_| true),
    })
}



fn run_editorial_agent_for_spec(
    log_session: &LogSession,
    run_id: &str,
    spec: EditorialAgentSpec,
    role: &str,
    stdin_text: String,
    attachments: &[AttachmentManifestEntry],
    output_path: &Path,
    timeout: Option<Duration>,
    config: &AiProviderConfig,
    cost_guard: Option<ProviderCostGuard>,
    use_api_agent: bool,
) -> EditorialAgentResult {
    if use_api_agent {
        return run_provider_api_agent(
            log_session,
            run_id,
            spec,
            role,
            stdin_text,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        );
    }

    run_editorial_agent(
        log_session,
        run_id,
        spec.name,
        role,
        spec.command,
        (spec.args)(),
        stdin_text,
        output_path,
        timeout,
    )
}

fn run_provider_api_agent(
    log_session: &LogSession,
    run_id: &str,
    spec: EditorialAgentSpec,
    role: &str,
    prompt: String,
    attachments: &[AttachmentManifestEntry],
    output_path: &Path,
    timeout: Option<Duration>,
    config: &AiProviderConfig,
    cost_guard: Option<ProviderCostGuard>,
) -> EditorialAgentResult {
    match spec.key {
        "claude" => run_anthropic_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        "codex" => run_openai_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        "gemini" => run_gemini_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        "deepseek" => run_deepseek_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        _ => write_provider_error_result(
            log_session,
            run_id,
            spec.name,
            api_cli_for_agent(spec.key),
            "unknown",
            role,
            output_path,
            "unknown",
            "API_PROVIDER_NOT_SUPPORTED",
            0,
        ),
    }
}

pub(crate) fn api_input_estimate_chars(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
    provider: &str,
) -> usize {
    let attachment_chars = attachments
        .iter()
        .filter(|entry| provider_supports_native_attachment(provider, entry))
        .map(|entry| {
            attachment_payload_base64_chars(entry)
                + entry.file_name.chars().count()
                + normalized_attachment_media_type(entry).chars().count()
                + 96
        })
        .sum::<usize>();
    prompt.chars().count().saturating_add(attachment_chars)
}

fn provider_supports_native_attachment(provider: &str, entry: &AttachmentManifestEntry) -> bool {
    match provider {
        "openai" => openai_api_attachment_supported(entry),
        "anthropic" => anthropic_api_attachment_supported(entry),
        "gemini" => gemini_api_attachment_supported(entry),
        _ => false,
    }
}

fn openai_api_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    if !attachment_within_native_payload_cap(entry) {
        return false;
    }
    is_image_attachment(entry) || openai_api_file_attachment_supported(entry)
}

fn openai_api_file_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    is_known_document_attachment(entry)
}

fn anthropic_api_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    if !attachment_within_native_payload_cap(entry) {
        return false;
    }
    is_image_attachment(entry) || is_pdf_attachment(entry)
}

fn gemini_api_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    if !attachment_within_native_payload_cap(entry) {
        return false;
    }
    is_image_attachment(entry)
        || is_audio_attachment(entry)
        || is_video_attachment(entry)
        || is_pdf_attachment(entry)
        || is_text_like_attachment(entry)
        || is_known_document_attachment(entry)
}

fn attachment_within_native_payload_cap(entry: &AttachmentManifestEntry) -> bool {
    entry.size_bytes <= API_NATIVE_ATTACHMENT_MAX_FILE_BYTES
}

pub(crate) fn openai_api_input(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
) -> Result<Value, String> {
    let mut content = vec![json!({ "type": "input_text", "text": prompt })];
    for entry in attachments {
        if !attachment_within_native_payload_cap(entry) {
            continue;
        }
        if is_image_attachment(entry) {
            content.push(json!({
                "type": "input_image",
                "image_url": attachment_data_url(entry)?
            }));
        } else if openai_api_file_attachment_supported(entry) {
            content.push(json!({
                "type": "input_file",
                "filename": entry.file_name.as_str(),
                "file_data": attachment_data_url(entry)?
            }));
        }
    }
    Ok(json!([
        {
            "role": "user",
            "content": content
        }
    ]))
}

pub(crate) fn anthropic_api_user_content(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
) -> Result<Value, String> {
    let mut content = vec![json!({ "type": "text", "text": prompt })];
    for entry in attachments {
        if !attachment_within_native_payload_cap(entry) {
            continue;
        }
        if is_image_attachment(entry) {
            content.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": normalized_attachment_media_type(entry),
                    "data": attachment_base64(entry)?
                }
            }));
        } else if is_pdf_attachment(entry) {
            content.push(json!({
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": attachment_base64(entry)?
                },
                "title": entry.file_name.as_str()
            }));
        }
    }
    Ok(Value::Array(content))
}

pub(crate) fn gemini_api_user_parts(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
) -> Result<Vec<Value>, String> {
    let mut parts = vec![json!({ "text": prompt })];
    for entry in attachments {
        if gemini_api_attachment_supported(entry) {
            parts.push(json!({
                "inline_data": {
                    "mime_type": normalized_attachment_media_type(entry),
                    "data": attachment_base64(entry)?
                }
            }));
        }
    }
    Ok(parts)
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
    let working_dir = command_working_dir_for_output(output_path);
    let prepared_input = prepare_agent_input(name, role, &stdin_text, output_path);
    let effective_input = effective_agent_input(command, args, &prepared_input);
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
                "stdin_chars": effective_input.stdin_chars,
                "original_prompt_chars": prepared_input.original_chars,
                "input_path": prepared_input
                    .input_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                "input_delivery": effective_input.delivery,
                "timeout_seconds": timeout.map(|value| value.as_secs()),
                "timeout_policy": if timeout.is_some() { "diagnostic_or_limited" } else { "none_editorial_session" },
                "working_dir": working_dir.to_string_lossy().to_string(),
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
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
        };
        log_editorial_agent_finished(log_session, run_id, &result, None, None, None, false);
        return result;
    };

    let _ = write_editorial_agent_running_artifact(
        output_path,
        name,
        role,
        command,
        &path,
        &effective_input.args,
        effective_input.stdin_chars,
        prepared_input.original_chars,
        prepared_input.input_path.as_deref(),
    );

    let progress = CommandProgressContext {
        log_session,
        run_id,
        agent: name,
        role,
        cli: command,
        output_path,
    };
    let command_result = run_resolved_command_observed(
        &path,
        &effective_input.args,
        timeout,
        effective_input.stdin_text.as_deref(),
        Some(progress),
    );

    match command_result {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.output.stderr).to_string();
            let exit_code = result.output.status.code();
            let status = if role == "review" {
                if stdout.trim().is_empty() {
                    if result.output.status.success() {
                        "AGENT_FAILED_EMPTY"
                    } else {
                        "AGENT_FAILED_NO_OUTPUT"
                    }
                } else {
                    extract_maestro_status(&stdout).unwrap_or("NOT_READY")
                }
            } else if stdout.trim().is_empty() {
                "EMPTY_DRAFT"
            } else {
                "DRAFT_CREATED"
            };
            let tone = if result.timed_out
                || status == "AGENT_FAILED_EMPTY"
                || status == "AGENT_FAILED_NO_OUTPUT"
            {
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
            let note = if status == "AGENT_FAILED_NO_OUTPUT" {
                "\n> O agente encerrou sem entregar avaliacao editorial em stdout. Este arquivo e diagnostico operacional, nao parecer de revisao.\n"
            } else if status == "AGENT_FAILED_EMPTY" {
                "\n> O agente encerrou com exit code 0 mas devolveu stdout vazio. Tratado como falha operacional (NAO READY), nao como parecer editorial.\n"
            } else {
                ""
            };
            let input_line = prepared_input
                .input_path
                .as_ref()
                .map(|input_path| format!("- Input file: `{}`\n", input_path.to_string_lossy()))
                .unwrap_or_else(|| "- Input file: `inline stdin`\n".to_string());
            let pipe_diagnostic_line = if result.stdout_pipe_error.is_some()
                || result.stderr_pipe_error.is_some()
            {
                format!(
                    "- Stdout pipe error: `{}`\n- Stderr pipe error: `{}`\n",
                    result
                        .stdout_pipe_error
                        .as_deref()
                        .unwrap_or("none"),
                    result
                        .stderr_pipe_error
                        .as_deref()
                        .unwrap_or("none"),
                )
            } else {
                String::new()
            };
            let artifact = format!(
                "# {name} - {role}\n\n- CLI: `{command}`\n- Resolved path: `{}`\n- Args: `{}`\n- Status: `{status}`\n- Exit code: `{}`\n- Duration ms: `{}`\n- Timed out: `{}`\n- Stdin chars: `{}`\n- Original prompt chars: `{}`\n{input_line}- Stdout chars: `{}`\n- Stderr chars: `{}`\n{pipe_diagnostic_line}{note}\n## Stdout\n\n```text\n{}\n```\n\n## Stderr\n\n```text\n{}\n```\n",
                path.to_string_lossy(),
                sanitize_text(&effective_input.args.join(" "), 1000),
                exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                result.duration_ms,
                result.timed_out,
                effective_input.stdin_chars,
                prepared_input.original_chars,
                stdout.chars().count(),
                stderr.chars().count(),
                stdout,
                truncate_text_head_tail(&stderr, 1024, 60 * 1024)
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
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
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
            let _ = write_editorial_agent_error_artifact(
                output_path,
                name,
                role,
                command,
                &path,
                &effective_input.args,
                &status,
                started.elapsed().as_millis(),
                effective_input.stdin_chars,
                prepared_input.original_chars,
                prepared_input.input_path.as_deref(),
            );
            let agent_result = EditorialAgentResult {
                name: name.to_string(),
                role: role.to_string(),
                cli: command.to_string(),
                tone: "error".to_string(),
                status,
                duration_ms: started.elapsed().as_millis(),
                exit_code: None,
                output_path: output_path.to_string_lossy().to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
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


/// Filter `existing_agents` recovered from the agent-runs/ directory on resume
/// down to only those whose names normalize to a key in `active_agent_keys`.
///
/// **B19 fix (v0.3.18)**: without this filter, the "Ultimo estado" UI summary
/// shows peers that ran in earlier sessions but were narrowed out for this
/// run, misleading the operator into thinking the current run touched them.
/// Older artifacts stay on disk under `agent-runs/` (operator can still
/// inspect them); only the in-memory snapshot used for status reporting is
/// filtered. Name normalization mirrors `normalize_active_agents` so
/// "Claude"/"anthropic"/"claude" all map to the same key.

pub(crate) fn log_editorial_agent_finished(
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
                "usage_input_tokens": result.usage_input_tokens,
                "usage_output_tokens": result.usage_output_tokens,
                "cost_usd": result.cost_usd,
                "cost_estimated": result.cost_estimated,
                "output_path": result.output_path
            })),
        },
    );
}

fn command_working_dir_for_output(output_path: &Path) -> PathBuf {
    output_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(app_root)
}

fn log_editorial_agent_spawned(
    progress: &CommandProgressContext<'_>,
    child_id: u32,
    path: &Path,
    working_dir: &Path,
) {
    let _ = write_log_record(
        progress.log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.spawned".to_string(),
            message: "editorial agent child process spawned".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(progress.run_id, 120),
                "agent": progress.agent,
                "role": progress.role,
                "cli": progress.cli,
                "child_pid": child_id,
                "resolved_path": path.to_string_lossy().to_string(),
                "working_dir": working_dir.to_string_lossy().to_string(),
                "output_path": progress.output_path.to_string_lossy().to_string()
            })),
        },
    );
}

fn log_editorial_agent_running(
    progress: &CommandProgressContext<'_>,
    child_id: u32,
    elapsed: Duration,
    stdout_bytes: u64,
    stderr_bytes: u64,
) {
    let _ = write_log_record(
        progress.log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.running".to_string(),
            message: "editorial agent still running".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(progress.run_id, 120),
                "agent": progress.agent,
                "role": progress.role,
                "cli": progress.cli,
                "child_pid": child_id,
                "elapsed_seconds": elapsed.as_secs(),
                "stdout_bytes_so_far": stdout_bytes,
                "stderr_bytes_so_far": stderr_bytes,
                "working_dir": command_working_dir_for_output(progress.output_path).to_string_lossy().to_string(),
                "output_path": progress.output_path.to_string_lossy().to_string()
            })),
        },
    );
}

pub(crate) fn extract_maestro_status(output: &str) -> Option<&'static str> {
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

pub(crate) fn extract_stdout_block(artifact: &str) -> Option<&str> {
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
        "# Ata da Sessao Maestro\n\n- Run: `{run_id}`\n- Sessao: {}\n- Protocolo: `{}`\n- Hash do protocolo: `{}`\n- Consenso unanime: `{}`\n- Texto final: `{}`\n- Peers ativos: `{}`\n- Limite de custo: `{}`\n- Limite de tempo: `{}`\n\n## Solicitacao\n\n{}\n\n## Rodada 001\n\n",
        sanitize_text(&request.session_name, 200),
        sanitize_text(&request.protocol_name, 200),
        sanitize_short(&request.protocol_hash, 80),
        consensus_ready,
        final_path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "bloqueado".to_string()),
        request
            .active_agents
            .clone()
            .unwrap_or_else(all_agent_keys)
            .join(", "),
        request
            .max_session_cost_usd
            .map(|value| format!("US$ {value:.4}"))
            .unwrap_or_else(|| "ignorado".to_string()),
        request
            .max_session_minutes
            .map(|value| format!("{value} min"))
            .unwrap_or_else(|| "ignorado".to_string()),
        request.prompt
    );

    for agent in agents {
        text.push_str(&format!(
            "- **{} / {}**: `{}` (`{}`), {} ms, artifact: `{}`\n",
            agent.name, agent.role, agent.status, agent.tone, agent.duration_ms, agent.output_path
        ));
    }

    if !consensus_ready {
        text.push_str("\n## Decisao\n\n");
        text.push_str(&build_blocked_minutes_decision(agents));
    } else {
        text.push_str("\n## Decisao\n\nTexto final liberado por unanimidade dos agentes.\n");
    }

    text
}

fn build_blocked_minutes_decision(agents: &[EditorialAgentResult]) -> String {
    let review_agents = agents
        .iter()
        .filter(|agent| agent.role == "review")
        .collect::<Vec<_>>();
    let ready_reviews = review_agents
        .iter()
        .filter(|agent| agent.status == "READY")
        .count();
    let operational_failures = agents
        .iter()
        .filter(|agent| {
            agent.tone == "error"
                || agent.tone == "blocked"
                || agent.status == "RUNNING"
                || agent.status == "AGENT_FAILED_NO_OUTPUT"
                || agent.status == "AGENT_FAILED_EMPTY"
                || agent.status == "EMPTY_DRAFT"
                || agent.status.starts_with("EXEC_ERROR")
        })
        .collect::<Vec<_>>();
    let editorial_divergences = review_agents
        .iter()
        .filter(|agent| agent.status != "READY" && agent.tone != "error" && agent.tone != "blocked")
        .collect::<Vec<_>>();

    let mut text = format!(
        "Texto final indisponivel nesta chamada.\n\n- Revisoes READY registradas: {ready_reviews}/{}.\n- Falhas operacionais detectadas: {}.\n- Divergencias editoriais ainda abertas: {}.\n",
        review_agents.len(),
        operational_failures.len(),
        editorial_divergences.len()
    );

    if !operational_failures.is_empty() {
        text.push_str("\n### Falhas operacionais\n\n");
        for agent in operational_failures.iter().rev().take(8) {
            text.push_str(&format!(
                "- **{} / {}**: `{}` (`{}`), exit code `{}`, artifact: `{}`\n",
                agent.name,
                agent.role,
                agent.status,
                agent.tone,
                agent
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                agent.output_path
            ));
        }
    }

    if !editorial_divergences.is_empty() {
        text.push_str("\n### Divergencias editoriais\n\n");
        for agent in editorial_divergences.iter().rev().take(8) {
            text.push_str(&format!(
                "- **{} / {}**: `{}` (`{}`), artifact: `{}`\n",
                agent.name, agent.role, agent.status, agent.tone, agent.output_path
            ));
        }
    }

    text.push_str(
        "\nA regra permanece: divergencia editorial exige novas rodadas ate unanimidade; falha operacional exige retry ou intervencao do operador antes de qualquer entrega final.\n",
    );
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

fn run_ai_provider_probe(config: &AiProviderConfig) -> AiProviderProbeResult {
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
            return AiProviderProbeResult {
                rows: vec![ai_probe_row(
                    "APIs",
                    format!("cliente HTTP falhou: {error}"),
                    "error",
                )],
                checked_at: Utc::now().to_rfc3339(),
            };
        }
    };

    AiProviderProbeResult {
        rows: vec![
            probe_openai_api(&client, config),
            probe_anthropic_api(&client, config),
            probe_gemini_api(&client, config),
            probe_deepseek_api(&client, config),
        ],
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn probe_openai_api(client: &Client, config: &AiProviderConfig) -> AiProviderProbeRow {
    let Some((key, _source)) = effective_provider_key(
        config.openai_api_key.as_deref(),
        &["MAESTRO_OPENAI_API_KEY", "OPENAI_API_KEY"],
    ) else {
        return missing_provider_key_row("OpenAI / Codex", config.openai_api_key_remote);
    };

    let response = client
        .get("https://api.openai.com/v1/models")
        .bearer_auth(&key)
        .send();
    summarize_ai_probe_response("OpenAI / Codex", response)
}

fn probe_anthropic_api(client: &Client, config: &AiProviderConfig) -> AiProviderProbeRow {
    let Some((key, _source)) = effective_provider_key(
        config.anthropic_api_key.as_deref(),
        &["MAESTRO_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"],
    ) else {
        return missing_provider_key_row("Anthropic / Claude", config.anthropic_api_key_remote);
    };

    let response = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", &key)
        .header("anthropic-version", "2023-06-01")
        .send();
    summarize_ai_probe_response("Anthropic / Claude", response)
}

fn probe_gemini_api(client: &Client, config: &AiProviderConfig) -> AiProviderProbeRow {
    let Some((key, _source)) = effective_provider_key(
        config.gemini_api_key.as_deref(),
        &["MAESTRO_GEMINI_API_KEY", "GEMINI_API_KEY"],
    ) else {
        return missing_provider_key_row("Google / Gemini", config.gemini_api_key_remote);
    };

    let response = client
        .get("https://generativelanguage.googleapis.com/v1beta/models")
        .query(&[("key", &key)])
        .send();
    summarize_ai_probe_response("Google / Gemini", response)
}

fn probe_deepseek_api(client: &Client, config: &AiProviderConfig) -> AiProviderProbeRow {
    let Some((key, _source)) = effective_provider_key(
        config.deepseek_api_key.as_deref(),
        &["MAESTRO_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"],
    ) else {
        return missing_provider_key_row("DeepSeek", config.deepseek_api_key_remote);
    };

    let response = client
        .get("https://api.deepseek.com/models")
        .bearer_auth(&key)
        .send();
    summarize_ai_probe_response("DeepSeek", response)
}

pub(crate) fn effective_provider_key(
    config_value: Option<&str>,
    env_candidates: &[&str],
) -> Option<(String, String)> {
    if let Some(value) = config_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
    {
        return Some((value, "config".to_string()));
    }
    first_env_value(env_candidates).map(|(name, scope, value)| (value, format!("{name}:{scope}")))
}

fn missing_provider_key_row(label: &str, remote_present: bool) -> AiProviderProbeRow {
    if remote_present {
        ai_probe_row(
            label,
            "segredo no Cloudflare; valor nao pode ser lido de volta neste app local",
            "warn",
        )
    } else {
        ai_probe_row(label, "API key nao informada", "warn")
    }
}

fn summarize_ai_probe_response(
    label: &str,
    response: Result<reqwest::blocking::Response, reqwest::Error>,
) -> AiProviderProbeRow {
    match response {
        Ok(response) => {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            if status.is_success() {
                ai_probe_row(label, "API respondeu; credencial aceita", "ok")
            } else if status.as_u16() == 401 || status.as_u16() == 403 {
                ai_probe_row(
                    label,
                    format!(
                        "credencial recusada (HTTP {}): {}",
                        status.as_u16(),
                        api_error_message(&body)
                    ),
                    "error",
                )
            } else if status.as_u16() == 429 {
                ai_probe_row(
                    label,
                    format!(
                        "credencial aceita, mas limite ativo (HTTP {}): {}",
                        status.as_u16(),
                        api_error_message(&body)
                    ),
                    "warn",
                )
            } else {
                ai_probe_row(
                    label,
                    format!(
                        "resposta inesperada (HTTP {}): {}",
                        status.as_u16(),
                        api_error_message(&body)
                    ),
                    "warn",
                )
            }
        }
        Err(error) => {
            let safe_error = error.without_url();
            ai_probe_row(label, format!("falha de rede: {safe_error}"), "error")
        }
    }
}

pub(crate) fn api_error_message(body: &str) -> String {
    if body.trim().is_empty() {
        return "sem detalhe na resposta".to_string();
    }

    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(message) = value
            .pointer("/error/message")
            .or_else(|| value.pointer("/error/status"))
            .or_else(|| value.pointer("/error/code"))
            .and_then(Value::as_str)
        {
            return sanitize_text(message, 180);
        }

        if let Some(message) = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
        {
            return sanitize_text(message, 180);
        }
    }

    sanitize_text(body, 180)
}

fn ai_probe_row(
    label: impl Into<String>,
    value: impl Into<String>,
    tone: impl Into<String>,
) -> AiProviderProbeRow {
    AiProviderProbeRow {
        label: sanitize_text(&label.into(), 80),
        value: sanitize_text(&value.into(), 240),
        tone: sanitize_short(&tone.into(), 16),
    }
}

fn run_link_audit(text: &str) -> LinkAuditResult {
    let urls = extract_public_urls(text);
    let client = match Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(Policy::limited(5))
        .user_agent(format!(
            "Maestro Editorial AI/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return LinkAuditResult {
                urls_found: urls.len(),
                checked: 0,
                ok: 0,
                failed: urls.len(),
                rows: vec![link_audit_row(
                    "http-client",
                    format!("cliente HTTP falhou: {error}"),
                    "error",
                )],
            };
        }
    };

    let rows = urls
        .iter()
        .map(|url| probe_public_url(&client, url))
        .collect::<Vec<_>>();
    let ok = rows.iter().filter(|row| row.tone == "ok").count();
    let failed = rows
        .iter()
        .filter(|row| row.tone == "error" || row.tone == "blocked")
        .count();

    LinkAuditResult {
        urls_found: urls.len(),
        checked: rows.len(),
        ok,
        failed,
        rows,
    }
}

fn extract_public_urls(text: &str) -> Vec<String> {
    let Some(regex) = Regex::new(r#"https?://[^\s<>"')\]]+"#).ok() else {
        return Vec::new();
    };

    let mut urls = BTreeSet::new();
    for matched in regex.find_iter(text).take(80) {
        let cleaned = matched
            .as_str()
            .trim_end_matches(['.', ',', ';', ':'])
            .to_string();
        if is_public_http_url(&cleaned) {
            urls.insert(cleaned);
        }
        if urls.len() >= 30 {
            break;
        }
    }
    urls.into_iter().collect()
}

pub(crate) fn is_public_http_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str().map(|host| host.to_ascii_lowercase()) else {
        return false;
    };

    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return false;
    }

    let host_for_ip = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host_for_ip.parse::<IpAddr>() {
        return !is_blocked_link_audit_ip(ip);
    }

    true
}

fn is_blocked_link_audit_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_blocked_link_audit_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_blocked_link_audit_ipv6(ipv6),
    }
}

fn is_blocked_link_audit_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 0
        || octets[0] == 10
        || octets[0] == 127
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 169 && octets[1] == 254)
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 224
}

fn is_blocked_link_audit_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }

    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_blocked_link_audit_ipv4(mapped);
    }

    let segments = ip.segments();
    if segments[0..5].iter().all(|segment| *segment == 0)
        && (segments[5] == 0 || segments[5] == 0xffff)
    {
        let [a, b] = segments[6].to_be_bytes();
        let [c, d] = segments[7].to_be_bytes();
        return is_blocked_link_audit_ipv4(Ipv4Addr::new(a, b, c, d));
    }

    let first_segment = segments[0];
    (first_segment & 0xfe00) == 0xfc00
        || (first_segment & 0xffc0) == 0xfe80
        || (first_segment & 0xff00) == 0xff00
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
}

fn probe_public_url(client: &Client, url: &str) -> LinkAuditRow {
    let head = client.head(url).send();
    match head {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            link_audit_row(url, format!("HTTP {}", response.status().as_u16()), "ok")
        }
        Ok(response) if response.status().as_u16() == 405 || response.status().as_u16() == 403 => {
            probe_public_url_with_get(client, url)
        }
        Ok(response) => link_audit_row(
            url,
            format!("HTTP {}", response.status().as_u16()),
            if response.status().is_client_error() || response.status().is_server_error() {
                "error"
            } else {
                "warn"
            },
        ),
        Err(_) => probe_public_url_with_get(client, url),
    }
}

fn probe_public_url_with_get(client: &Client, url: &str) -> LinkAuditRow {
    match client.get(url).send() {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            link_audit_row(url, format!("HTTP {}", response.status().as_u16()), "ok")
        }
        Ok(response) => link_audit_row(
            url,
            format!("HTTP {}", response.status().as_u16()),
            if response.status().is_client_error() || response.status().is_server_error() {
                "error"
            } else {
                "warn"
            },
        ),
        Err(error) => link_audit_row(url, format!("falha HTTP: {error}"), "error"),
    }
}

fn link_audit_row(
    url: impl Into<String>,
    status: impl Into<String>,
    tone: impl Into<String>,
) -> LinkAuditRow {
    LinkAuditRow {
        url: sanitize_text(&url.into(), 240),
        status: sanitize_text(&status.into(), 160),
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

struct CommandProgressContext<'a> {
    log_session: &'a LogSession,
    run_id: &'a str,
    agent: &'a str,
    role: &'a str,
    cli: &'a str,
    output_path: &'a Path,
}

fn run_resolved_command_with_timeout(
    path: &Path,
    args: &[String],
    timeout: Duration,
    stdin_text: Option<&str>,
) -> std::io::Result<TimedCommandOutput> {
    run_resolved_command_observed(path, args, Some(timeout), stdin_text, None)
}

fn run_resolved_command_observed(
    path: &Path,
    args: &[String],
    timeout: Option<Duration>,
    stdin_text: Option<&str>,
    progress: Option<CommandProgressContext<'_>>,
) -> std::io::Result<TimedCommandOutput> {
    let started = Instant::now();
    let mut command = resolved_command_builder(path, args);
    let working_dir = progress
        .as_ref()
        .map(|progress| command_working_dir_for_output(progress.output_path))
        .unwrap_or_else(app_root);
    command
        .current_dir(&working_dir)
        .stdin(if stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let child_id = child.id();
    if let Some(progress) = progress.as_ref() {
        log_editorial_agent_spawned(progress, child_id, path, &working_dir);
    }
    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(text.as_bytes()) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_bytes = Arc::new(AtomicU64::new(0));
    let stderr_bytes = Arc::new(AtomicU64::new(0));
    let stdout_counter = Arc::clone(&stdout_bytes);
    let stderr_counter = Arc::clone(&stderr_bytes);
    let stdout_handle =
        thread::spawn(move || read_pipe_to_end_counting_classified(stdout, stdout_counter));
    let stderr_handle =
        thread::spawn(move || read_pipe_to_end_counting_classified(stderr, stderr_counter));
    let mut last_progress = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            let (stdout, stdout_pipe_error) = stdout_handle
                .join()
                .unwrap_or_else(|_| (Vec::new(), Some("stdout_thread_panic".to_string())));
            let (stderr, stderr_pipe_error) = stderr_handle
                .join()
                .unwrap_or_else(|_| (Vec::new(), Some("stderr_thread_panic".to_string())));
            return Ok(TimedCommandOutput {
                output: Output {
                    status,
                    stdout,
                    stderr,
                },
                duration_ms: started.elapsed().as_millis(),
                timed_out: false,
                stdout_pipe_error,
                stderr_pipe_error,
            });
        }

        if let Some(timeout) = timeout {
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let status = child.wait()?;
                let (stdout, stdout_pipe_error) = stdout_handle
                    .join()
                    .unwrap_or_else(|_| (Vec::new(), Some("stdout_thread_panic".to_string())));
                let (stderr, stderr_pipe_error) = stderr_handle
                    .join()
                    .unwrap_or_else(|_| (Vec::new(), Some("stderr_thread_panic".to_string())));
                return Ok(TimedCommandOutput {
                    output: Output {
                        status,
                        stdout,
                        stderr,
                    },
                    duration_ms: started.elapsed().as_millis(),
                    timed_out: true,
                    stdout_pipe_error,
                    stderr_pipe_error,
                });
            }
        }

        if last_progress.elapsed() >= Duration::from_secs(30) {
            if let Some(progress) = progress.as_ref() {
                log_editorial_agent_running(
                    progress,
                    child_id,
                    started.elapsed(),
                    stdout_bytes.load(Ordering::Relaxed),
                    stderr_bytes.load(Ordering::Relaxed),
                );
            }
            last_progress = Instant::now();
        }

        thread::sleep(Duration::from_millis(250));
    }
}

fn read_pipe_to_end_counting_classified(
    pipe: Option<impl Read>,
    byte_counter: Arc<AtomicU64>,
) -> (Vec<u8>, Option<String>) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut pipe_error: Option<String> = None;
    if let Some(mut pipe) = pipe {
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    byte_counter.fetch_add(count as u64, Ordering::Relaxed);
                    buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) => {
                    pipe_error = Some(classify_pipe_error(&error));
                    break;
                }
            }
        }
    }
    (buffer, pipe_error)
}

fn classify_pipe_error(error: &std::io::Error) -> String {
    let raw = error.raw_os_error();
    let kind = error.kind();
    let label = match (raw, kind) {
        (Some(109), _) => "windows_error_109_broken_pipe",
        (Some(232), _) => "windows_error_232_pipe_closing",
        (Some(233), _) => "windows_error_233_pipe_no_listener",
        (_, std::io::ErrorKind::BrokenPipe) => "broken_pipe",
        (_, std::io::ErrorKind::UnexpectedEof) => "unexpected_eof",
        (_, std::io::ErrorKind::Interrupted) => "interrupted",
        (_, std::io::ErrorKind::TimedOut) => "timed_out",
        _ => "other",
    };
    let raw_label = raw
        .map(|code| code.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!("{label} (kind={kind:?}, raw_os_error={raw_label})")
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
            apply_editorial_agent_environment(&mut command, path);
            return command;
        }

        if extension == "ps1" {
            let mut command = hidden_command("powershell.exe");
            command
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(path)
                .args(args);
            apply_editorial_agent_environment(&mut command, path);
            return command;
        }
    }

    let mut command = hidden_command(path);
    command.args(args);
    apply_editorial_agent_environment(&mut command, path);
    command
}

fn apply_editorial_agent_environment(command: &mut Command, path: &Path) {
    command
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env("LC_ALL", "C.UTF-8")
        .env("LANG", "C.UTF-8");

    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if stem == "gemini" {
        command.env("GEMINI_CLI_TRUST_WORKSPACE", "true");
    }
}

pub(crate) fn sanitize_short(value: &str, max_len: usize) -> String {
    sanitize_text(value, max_len)
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
        })
        .collect::<String>()
}

pub(crate) fn sanitize_text(value: &str, max_len: usize) -> String {
    let redacted = redact_secrets(value);
    redacted.chars().take(max_len).collect()
}

/// Truncates large stderr/stdout text preserving head and tail with a marker in the middle.
/// Useful for CLIs (Codex, others) that emit large preambles followed by the actual error tail.
pub(crate) fn truncate_text_head_tail(
    value: &str,
    head_chars: usize,
    tail_chars: usize,
) -> String {
    let redacted = redact_secrets(value);
    let total = redacted.chars().count();
    let cap = head_chars + tail_chars;
    if total <= cap {
        return redacted;
    }
    let head: String = redacted.chars().take(head_chars).collect();
    let tail: String = redacted
        .chars()
        .skip(total - tail_chars)
        .collect();
    let dropped = total - cap;
    format!(
        "{head}\n\n[... {dropped} chars truncated (head {head_chars} / tail {tail_chars}) ...]\n\n{tail}"
    )
}

pub(crate) fn sanitize_value(value: Value, depth: usize) -> Value {
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
    // Test-only path helpers (not used in non-test builds; re-imported here to
    // avoid `#[cfg(test)]` clutter on the main use list above).
    use crate::app_paths::{config_dir, portable_root_from_exe_path};
    use crate::cloudflare::{
        cloudflare_page_path, cloudflare_store_for_target_or_existing, cloudflare_verify_path,
    };
    use crate::editorial_prompts::{claude_args, gemini_args};
    use crate::provider_retry::parse_retry_after_header;
    use crate::session_controls::{normalize_active_agents, provider_cost};
    use crate::session_resume::protocol_backup_stats;

    fn test_manifest_attachment(
        session_dir: &Path,
        file_name: &str,
        media_type: &str,
        data: &[u8],
    ) -> AttachmentManifestEntry {
        let path = session_dir.join(file_name);
        fs::write(&path, data).unwrap();
        AttachmentManifestEntry {
            original_name: file_name.to_string(),
            file_name: file_name.to_string(),
            media_type: media_type.to_string(),
            size_bytes: data.len() as u64,
            sha256: "test".to_string(),
            path: path.to_string_lossy().to_string(),
            inline_preview_chars: 0,
            inline_preview_truncated: false,
        }
    }

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
    fn resolves_requested_initial_agent_aliases() {
        assert_eq!(resolve_initial_agent_key(Some("Codex")).0, "codex");
        assert_eq!(resolve_initial_agent_key(Some("chatgpt")).0, "codex");
        assert_eq!(resolve_initial_agent_key(Some("Google")).0, "gemini");
        assert_eq!(resolve_initial_agent_key(Some("Anthropic")).0, "claude");
        assert_eq!(resolve_initial_agent_key(Some("DeepSeek")).0, "deepseek");
        let (fallback, invalid) = resolve_initial_agent_key(Some("unknown-peer"));
        assert_eq!(fallback, "claude");
        assert_eq!(invalid.as_deref(), Some("unknown-peer"));
    }

    #[test]
    fn normalizes_active_agents_and_rejects_unknown_peer() {
        let selected = normalize_active_agents(Some(&vec![
            "Codex".to_string(),
            "openai".to_string(),
            "DeepSeek".to_string(),
        ]))
        .unwrap();
        assert_eq!(selected, vec!["codex".to_string(), "deepseek".to_string()]);
        assert!(normalize_active_agents(Some(&vec!["unknown".to_string()])).is_err());
    }

    #[test]
    fn provider_cost_uses_configured_rates() {
        let rates = ProviderCostRates {
            input_usd_per_million: 1.0,
            output_usd_per_million: 2.0,
        };
        let cost = provider_cost(1_000, 2_000, rates);
        assert!((cost - 0.005).abs() < 1e-12);
    }

    #[test]
    fn api_payloads_embed_provider_supported_attachments() {
        let session_dir = sessions_dir().join("run-api-attachment-payload-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();

        let image = test_manifest_attachment(&session_dir, "image.png", "image/png", b"png");
        let pdf = test_manifest_attachment(&session_dir, "brief.pdf", "application/pdf", b"%PDF");
        let unknown = test_manifest_attachment(
            &session_dir,
            "archive.bin",
            "application/octet-stream",
            b"bin",
        );
        let mut oversized_pdf =
            test_manifest_attachment(&session_dir, "huge.pdf", "application/pdf", b"%PDF");
        oversized_pdf.size_bytes = API_NATIVE_ATTACHMENT_MAX_FILE_BYTES + 1;

        let openai = openai_api_input(
            "prompt",
            &[
                image.clone(),
                pdf.clone(),
                unknown.clone(),
                oversized_pdf.clone(),
            ],
        )
        .expect("openai payload should build");
        let openai_text = serde_json::to_string(&openai).unwrap();
        assert!(openai_text.contains("\"input_image\""));
        assert!(openai_text.contains("\"input_file\""));
        assert!(openai_text.contains("data:image/png;base64,"));
        assert!(openai_text.contains("data:application/pdf;base64,"));
        assert!(!openai_text.contains("archive.bin"));
        assert!(!openai_text.contains("huge.pdf"));

        let anthropic = anthropic_api_user_content("prompt", &[image.clone(), pdf.clone()])
            .expect("anthropic payload should build");
        let anthropic_text = serde_json::to_string(&anthropic).unwrap();
        assert!(anthropic_text.contains("\"image\""));
        assert!(anthropic_text.contains("\"document\""));

        let gemini = gemini_api_user_parts(
            "prompt",
            &[image.clone(), pdf.clone(), unknown, oversized_pdf],
        )
        .expect("gemini payload should build");
        let gemini_text = serde_json::to_string(&gemini).unwrap();
        assert!(gemini_text.contains("\"inline_data\""));
        assert!(gemini_text.contains("\"mime_type\":\"image/png\""));
        assert!(gemini_text.contains("\"mime_type\":\"application/pdf\""));
        assert!(!gemini_text.contains("archive.bin"));
        assert!(!gemini_text.contains("huge.pdf"));

        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn api_cost_projection_counts_native_attachment_payloads() {
        let session_dir = sessions_dir().join("run-api-attachment-cost-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();

        let pdf = test_manifest_attachment(&session_dir, "brief.pdf", "application/pdf", b"%PDF");
        let unknown = test_manifest_attachment(
            &session_dir,
            "archive.bin",
            "application/octet-stream",
            b"bin",
        );

        let prompt_chars = "abcd".chars().count();
        assert_eq!(
            api_input_estimate_chars("abcd", &[pdf.clone(), unknown.clone()], "deepseek"),
            prompt_chars
        );
        assert!(
            api_input_estimate_chars("abcd", &[pdf.clone(), unknown.clone()], "openai")
                > prompt_chars
        );
        assert!(api_input_estimate_chars("abcd", &[pdf, unknown], "gemini") > prompt_chars);

        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn human_log_projection_collapses_heartbeat_spam() {
        assert!(should_collapse_human_log_event(
            "session.editorial.heartbeat",
            &json!({ "elapsed_seconds": 60 })
        ));
        assert!(!should_collapse_human_log_event(
            "session.editorial.heartbeat",
            &json!({ "elapsed_seconds": 300 })
        ));
        assert!(!should_collapse_human_log_event(
            "session.agent.finished",
            &json!({})
        ));
    }

    #[test]
    fn orders_draft_lead_before_fallback_agents() {
        let claude_order = ordered_editorial_agent_specs("claude")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(claude_order, vec!["claude", "codex", "gemini", "deepseek"]);

        let codex_order = ordered_editorial_agent_specs("codex")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(codex_order, vec!["codex", "claude", "gemini", "deepseek"]);

        let gemini_order = ordered_editorial_agent_specs("gemini")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(gemini_order, vec!["gemini", "claude", "codex", "deepseek"]);

        let deepseek_order = ordered_editorial_agent_specs("deepseek")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(
            deepseek_order,
            vec!["deepseek", "claude", "codex", "gemini"]
        );
    }

    #[test]
    fn preserves_whitespace_when_redacting() {
        let text = redact_secrets("line1\nline2\tcfat_12345678");
        assert_eq!(text, "line1\nline2\t<redacted>");
    }

    #[test]
    fn link_audit_blocks_local_and_private_targets() {
        assert!(!is_public_http_url("http://localhost:8787/test"));
        assert!(!is_public_http_url("http://127.0.0.1/test"));
        assert!(!is_public_http_url("http://0.0.0.0/test"));
        assert!(!is_public_http_url("http://10.0.0.1/test"));
        assert!(!is_public_http_url("http://192.168.1.10/test"));
        assert!(!is_public_http_url("http://172.16.0.1/test"));
        assert!(!is_public_http_url("http://100.64.0.1/test"));
        assert!(!is_public_http_url("http://169.254.169.254/latest"));
        assert!(!is_public_http_url("http://192.0.2.1/test"));
        assert!(!is_public_http_url("http://198.51.100.1/test"));
        assert!(!is_public_http_url("http://203.0.113.1/test"));
        assert!(!is_public_http_url("http://224.0.0.1/test"));
        assert!(!is_public_http_url("http://255.255.255.255/test"));
        assert!(!is_public_http_url("http://[::1]/test"));
        assert!(!is_public_http_url("http://[fc00::1]/test"));
        assert!(!is_public_http_url("http://[fe80::1]/test"));
        assert!(!is_public_http_url("http://[ff02::1]/test"));
        assert!(!is_public_http_url("http://[2001:db8::1]/test"));
        assert!(!is_public_http_url("http://[::127.0.0.1]/test"));
        assert!(!is_public_http_url("http://[::ffff:127.0.0.1]/test"));
        assert!(is_public_http_url("https://example.com/source"));
        assert!(is_public_http_url("https://10.0.0.1.example.com/source"));
    }

    #[test]
    fn link_audit_extracts_public_urls_only() {
        let urls = extract_public_urls(
            "Veja https://example.com/a, http://localhost:8787/x e https://example.org/b.",
        );
        assert_eq!(urls, vec!["https://example.com/a", "https://example.org/b"]);
    }

    #[test]
    fn ai_provider_config_trims_empty_secret_fields() {
        let config = sanitize_ai_provider_config(AiProviderConfig {
            schema_version: 99,
            provider_mode: "api".to_string(),
            credential_storage_mode: "windows_env".to_string(),
            openai_api_key: Some("  sk-test-value  ".to_string()),
            anthropic_api_key: Some("   ".to_string()),
            gemini_api_key: None,
            deepseek_api_key: Some("  ds-test-value  ".to_string()),
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            openai_input_usd_per_million: Some(2.50),
            openai_output_usd_per_million: Some(10.0),
            anthropic_input_usd_per_million: Some(3.0),
            anthropic_output_usd_per_million: Some(15.0),
            gemini_input_usd_per_million: Some(1.25),
            gemini_output_usd_per_million: Some(5.0),
            deepseek_input_usd_per_million: Some(0.55),
            deepseek_output_usd_per_million: Some(2.19),
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
            updated_at: "old".to_string(),
        });

        assert_eq!(config.schema_version, 1);
        assert_eq!(config.provider_mode, "api");
        assert_eq!(config.credential_storage_mode, "windows_env");
        assert_eq!(config.openai_api_key.as_deref(), Some("sk-test-value"));
        assert!(config.anthropic_api_key.is_none());
        assert!(config.gemini_api_key.is_none());
        assert_eq!(config.deepseek_api_key.as_deref(), Some("ds-test-value"));
    }

    #[test]
    fn cloudflare_ai_provider_marker_does_not_store_secret_values_locally() {
        let config = sanitize_ai_provider_config(AiProviderConfig {
            schema_version: 1,
            provider_mode: "api".to_string(),
            credential_storage_mode: "cloudflare".to_string(),
            openai_api_key: Some("sk-test-value".to_string()),
            anthropic_api_key: Some("sk-ant-test-value".to_string()),
            gemini_api_key: Some("AIza-test-value".to_string()),
            deepseek_api_key: Some("ds-test-value".to_string()),
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            openai_input_usd_per_million: Some(2.50),
            openai_output_usd_per_million: Some(10.0),
            anthropic_input_usd_per_million: Some(3.0),
            anthropic_output_usd_per_million: Some(15.0),
            gemini_input_usd_per_million: Some(1.25),
            gemini_output_usd_per_million: Some(5.0),
            deepseek_input_usd_per_million: Some(0.55),
            deepseek_output_usd_per_million: Some(2.19),
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
            updated_at: "old".to_string(),
        });
        let path = config_dir().join("ai-provider-cloudflare-marker-test.json");
        let _ = fs::remove_file(&path);

        persist_ai_provider_cloudflare_marker(&path, &config).unwrap();

        let text = fs::read_to_string(checked_data_child_path(&path).unwrap()).unwrap();
        assert!(text.contains("\"credential_storage_mode\": \"cloudflare\""));
        assert!(text.contains("\"openai_api_key_remote\": true"));
        assert!(text.contains("\"anthropic_api_key_remote\": true"));
        assert!(text.contains("\"gemini_api_key_remote\": true"));
        assert!(text.contains("\"deepseek_api_key_remote\": true"));
        assert!(!text.contains("sk-test-value"));
        assert!(!text.contains("sk-ant-test-value"));
        assert!(!text.contains("AIza-test-value"));
        assert!(!text.contains("ds-test-value"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn ai_provider_secret_values_use_cloudflare_safe_names() {
        let config = sanitize_ai_provider_config(AiProviderConfig {
            schema_version: 1,
            provider_mode: "api".to_string(),
            credential_storage_mode: "cloudflare".to_string(),
            openai_api_key: Some("sk-test-value".to_string()),
            anthropic_api_key: None,
            gemini_api_key: Some("AIza-test-value".to_string()),
            deepseek_api_key: Some("ds-test-value".to_string()),
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            openai_input_usd_per_million: Some(2.50),
            openai_output_usd_per_million: Some(10.0),
            anthropic_input_usd_per_million: Some(3.0),
            anthropic_output_usd_per_million: Some(15.0),
            gemini_input_usd_per_million: Some(1.25),
            gemini_output_usd_per_million: Some(5.0),
            deepseek_input_usd_per_million: Some(0.55),
            deepseek_output_usd_per_million: Some(2.19),
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
            updated_at: "old".to_string(),
        });

        let values = ai_provider_secret_values(&config);

        assert_eq!(values.len(), 3);
        assert!(values.contains_key("MAESTRO_OPENAI_API_KEY"));
        assert!(values.contains_key("MAESTRO_GEMINI_API_KEY"));
        assert!(values.contains_key("MAESTRO_DEEPSEEK_API_KEY"));
        assert!(!values.contains_key("MAESTRO_ANTHROPIC_API_KEY"));
    }

    #[test]
    fn nonzero_empty_review_output_is_operational_failure_not_editorial_not_ready() {
        let session_dir = sessions_dir().join("run-empty-review-artifact-test");
        let _ = fs::remove_dir_all(&session_dir);
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        let path = agent_dir.join("round-001-claude-review.md");
        write_text_file(
            &path,
            "# Claude - review\n\n- CLI: `claude`\n- Status: `AGENT_FAILED_NO_OUTPUT`\n- Exit code: `1`\n- Duration ms: `42`\n\n## Stdout\n\n```text\n\n```\n",
        )
        .unwrap();

        let artifact = parse_agent_artifact_name(&agent_dir, "round-001-claude-review.md")
            .expect("artifact should parse");
        let parsed = parse_agent_artifact_result(&artifact).expect("artifact result should parse");

        assert_eq!(parsed.status, "AGENT_FAILED_NO_OUTPUT");
        assert_eq!(parsed.tone, "error");
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn truncate_text_head_tail_preserves_both_ends() {
        let body = "A".repeat(100) + &"X".repeat(2_000) + &"Z".repeat(100);
        let truncated = truncate_text_head_tail(&body, 50, 50);
        assert!(truncated.starts_with(&"A".repeat(50)));
        assert!(truncated.ends_with(&"Z".repeat(50)));
        assert!(truncated.contains("chars truncated"));
    }

    #[test]
    fn truncate_text_head_tail_passthrough_when_under_cap() {
        let body = "short body";
        let truncated = truncate_text_head_tail(body, 1024, 60 * 1024);
        assert_eq!(truncated, "short body");
    }

    #[test]
    fn session_contract_loads_legacy_payload_without_links_attachments() {
        let session_dir = sessions_dir().join("run-legacy-contract-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();
        let legacy_payload = r#"{
            "run_id": "run-legacy",
            "session_name": "Legacy",
            "created_at": "2026-04-01T12:00:00.000000+00:00",
            "active_agents": ["claude"],
            "initial_agent": "claude"
        }"#;
        write_text_file(
            &session_dir.join("session-contract.json"),
            legacy_payload,
        )
        .unwrap();
        let loaded = load_session_contract(&session_dir).expect("legacy contract should parse");
        assert_eq!(loaded.created_at, "2026-04-01T12:00:00.000000+00:00");
        assert_eq!(loaded.active_agents, vec!["claude".to_string()]);
        assert_eq!(loaded.links.len(), 0);
        assert_eq!(loaded.attachments.len(), 0);
        assert_eq!(
            loaded.schema_version, 1,
            "schema_version must default to 1 when missing from legacy payload"
        );
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn resolve_effective_active_agents_request_overrides_saved() {
        let request = vec!["claude".to_string()];
        let saved = vec!["codex".to_string(), "gemini".to_string()];
        let (effective, source) = resolve_effective_active_agents(Some(&request), Some(&saved))
            .expect("request override should resolve");
        assert_eq!(effective, vec!["claude".to_string()]);
        assert_eq!(source, "request");
    }

    #[test]
    fn resolve_effective_active_agents_falls_back_to_saved_contract_when_request_omits() {
        let saved = vec!["codex".to_string(), "gemini".to_string()];
        let (effective, source) = resolve_effective_active_agents(None, Some(&saved))
            .expect("saved fallback should resolve");
        assert_eq!(effective, vec!["codex".to_string(), "gemini".to_string()]);
        assert_eq!(source, "saved_contract");
    }

    #[test]
    fn resolve_effective_active_agents_falls_back_to_default_when_both_missing() {
        let (effective, source) = resolve_effective_active_agents(None, None)
            .expect("default fallback should resolve");
        assert_eq!(effective.len(), 4);
        assert_eq!(source, "default_all");
    }

    #[test]
    fn resolve_effective_active_agents_recovers_when_saved_contract_is_empty() {
        let saved: Vec<String> = Vec::new();
        let (effective, source) = resolve_effective_active_agents(None, Some(&saved))
            .expect("empty saved contract should fall through, not error");
        assert_eq!(effective.len(), 4);
        assert_eq!(source, "default_all");
    }

    #[test]
    fn resolve_effective_active_agents_rejects_empty_request_array() {
        let request: Vec<String> = Vec::new();
        let result = resolve_effective_active_agents(Some(&request), None);
        assert!(
            result.is_err(),
            "empty request array must be a hard error, not silent override"
        );
    }

    #[test]
    fn resolve_effective_active_agents_rejects_empty_request_even_when_saved_contract_present() {
        let request: Vec<String> = Vec::new();
        let saved = vec!["codex".to_string()];
        let result = resolve_effective_active_agents(Some(&request), Some(&saved));
        assert!(
            result.is_err(),
            "empty request array must be a hard error even when a saved contract exists; explicit Some([]) does not silently fall back to saved"
        );
    }

    #[test]
    fn resolve_time_budget_anchor_returns_now_when_resuming() {
        use chrono::TimeZone;
        let original = Utc.with_ymd_and_hms(2026, 4, 26, 19, 28, 26).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 1, 19, 0, 0).unwrap();
        let anchor = resolve_time_budget_anchor(original, true, now);
        assert_eq!(
            anchor, now,
            "resume should anchor the time budget at NOW, not the original created_at"
        );
    }

    #[test]
    fn resolve_time_budget_anchor_returns_created_at_when_fresh_start() {
        use chrono::TimeZone;
        let original = Utc.with_ymd_and_hms(2026, 5, 1, 19, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 1, 19, 0, 30).unwrap();
        let anchor = resolve_time_budget_anchor(original, false, now);
        assert_eq!(
            anchor, original,
            "fresh start should anchor at created_at so the cap matches operator intent"
        );
    }

    #[test]
    fn filter_existing_agents_keeps_only_agents_in_active_set() {
        let existing = vec![
            EditorialAgentResult {
                name: "Claude".to_string(),
                role: "review".to_string(),
                cli: "claude".to_string(),
                tone: "warn".to_string(),
                status: "NOT_READY".to_string(),
                duration_ms: 100,
                exit_code: Some(0),
                output_path: "agent-runs/round-001-claude-review.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
            EditorialAgentResult {
                name: "Codex".to_string(),
                role: "review".to_string(),
                cli: "openai-api".to_string(),
                tone: "error".to_string(),
                status: "PROVIDER_NETWORK_ERROR".to_string(),
                duration_ms: 30000,
                exit_code: None,
                output_path: "agent-runs/round-001-codex-review.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
            EditorialAgentResult {
                name: "DeepSeek".to_string(),
                role: "review".to_string(),
                cli: "deepseek-api".to_string(),
                tone: "error".to_string(),
                status: "AGENT_FAILED_EMPTY".to_string(),
                duration_ms: 32000,
                exit_code: Some(0),
                output_path: "agent-runs/round-001-deepseek-review.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
        ];
        let active = vec!["deepseek".to_string()];
        let filtered = filter_existing_agents_to_active_set(existing, &active);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "DeepSeek");
    }

    #[test]
    fn filter_existing_agents_normalizes_agent_name_aliases() {
        let existing = vec![
            EditorialAgentResult {
                name: "Anthropic".to_string(),
                role: "review".to_string(),
                cli: "anthropic-api".to_string(),
                tone: "ok".to_string(),
                status: "READY".to_string(),
                duration_ms: 100,
                exit_code: Some(0),
                output_path: "irrelevant.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
            EditorialAgentResult {
                name: "OpenAI".to_string(),
                role: "review".to_string(),
                cli: "openai-api".to_string(),
                tone: "ok".to_string(),
                status: "READY".to_string(),
                duration_ms: 100,
                exit_code: Some(0),
                output_path: "irrelevant.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
        ];
        let active = vec!["claude".to_string(), "codex".to_string()];
        let filtered = filter_existing_agents_to_active_set(existing, &active);
        assert_eq!(filtered.len(), 2, "alias names must normalize to active keys");
    }

    #[test]
    fn filter_existing_agents_trims_whitespace_to_match_normalize_active_agents() {
        let existing = vec![
            EditorialAgentResult {
                name: " Claude\n".to_string(),
                role: "review".to_string(),
                cli: "claude".to_string(),
                tone: "ok".to_string(),
                status: "READY".to_string(),
                duration_ms: 100,
                exit_code: Some(0),
                output_path: "irrelevant.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
            EditorialAgentResult {
                name: "\tdeepseek-api\t".to_string(),
                role: "review".to_string(),
                cli: "deepseek-api".to_string(),
                tone: "error".to_string(),
                status: "AGENT_FAILED_EMPTY".to_string(),
                duration_ms: 100,
                exit_code: Some(0),
                output_path: "irrelevant.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
        ];
        let active = vec!["claude".to_string(), "deepseek".to_string()];
        let filtered = filter_existing_agents_to_active_set(existing, &active);
        assert_eq!(
            filtered.len(),
            2,
            "whitespace-padded names must trim to canonical keys, mirroring normalize_active_agents"
        );
    }

    #[test]
    fn filter_existing_agents_returns_empty_when_active_set_is_empty() {
        let existing = vec![EditorialAgentResult {
            name: "Claude".to_string(),
            role: "review".to_string(),
            cli: "claude".to_string(),
            tone: "ok".to_string(),
            status: "READY".to_string(),
            duration_ms: 100,
            exit_code: Some(0),
            output_path: "irrelevant.md".to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
        }];
        let active: Vec<String> = Vec::new();
        let filtered = filter_existing_agents_to_active_set(existing, &active);
        assert!(filtered.is_empty());
    }

    #[test]
    fn inspect_resumable_session_dir_reports_saved_active_agents_for_picker() {
        let session_dir = sessions_dir().join("run-resumable-info-saved-contract-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();
        write_text_file(&session_dir.join("prompt.md"), "Sessao: Teste\n\nbody").unwrap();
        write_text_file(
            &session_dir.join("protocolo.md"),
            &"Linha\n".repeat(120),
        )
        .unwrap();
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        let contract = SessionContract {
            schema_version: 1,
            run_id: "run-resumable-info-saved-contract-test".to_string(),
            session_name: "Teste".to_string(),
            created_at: "2026-04-26T19:28:26.000000+00:00".to_string(),
            active_agents: vec!["deepseek".to_string()],
            initial_agent: "deepseek".to_string(),
            max_session_cost_usd: Some(7.5),
            max_session_minutes: Some(45),
            links: Vec::new(),
            attachments: Vec::new(),
        };
        write_session_contract(&session_dir, &contract).unwrap();

        let info = inspect_resumable_session_dir(&session_dir)
            .unwrap()
            .expect("session dir should be reported as resumable");
        assert_eq!(info.saved_active_agents, vec!["deepseek".to_string()]);
        assert_eq!(info.saved_initial_agent, Some("deepseek".to_string()));
        assert_eq!(info.saved_max_session_cost_usd, Some(7.5));
        assert_eq!(info.saved_max_session_minutes, Some(45));
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn inspect_resumable_session_dir_returns_empty_saved_when_contract_missing() {
        let session_dir = sessions_dir().join("run-resumable-info-no-contract-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();
        write_text_file(&session_dir.join("prompt.md"), "Sessao: Teste\n\nbody").unwrap();
        write_text_file(
            &session_dir.join("protocolo.md"),
            &"Linha\n".repeat(120),
        )
        .unwrap();
        fs::create_dir_all(&session_dir.join("agent-runs")).unwrap();

        let info = inspect_resumable_session_dir(&session_dir)
            .unwrap()
            .expect("session dir should be reported even without saved contract");
        assert!(info.saved_active_agents.is_empty());
        assert_eq!(info.saved_initial_agent, None);
        assert_eq!(info.saved_max_session_cost_usd, None);
        assert_eq!(info.saved_max_session_minutes, None);
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn active_agents_resolved_log_context_pins_field_shape_and_sources() {
        let saved = SessionContract {
            schema_version: 1,
            run_id: "run-saved".to_string(),
            session_name: "Saved".to_string(),
            created_at: "2026-04-01T00:00:00.000Z".to_string(),
            active_agents: vec!["codex".to_string(), "gemini".to_string()],
            initial_agent: "codex".to_string(),
            max_session_cost_usd: Some(7.5),
            max_session_minutes: None,
            links: Vec::new(),
            attachments: Vec::new(),
        };
        let effective = vec!["codex".to_string(), "gemini".to_string()];
        let context = build_active_agents_resolved_log_context(
            "run-test",
            None,
            Some(&saved),
            &effective,
            "saved_contract",
            "codex",
            None,
            None,
            Some(45),
        );
        let object = context
            .as_object()
            .expect("log context must be a JSON object");
        for required_key in [
            "run_id",
            "active_agents_requested",
            "active_agents_saved_contract",
            "active_agents_effective",
            "active_agents_source",
            "draft_lead_key",
            "invalid_initial_agent",
            "max_session_cost_usd_requested",
            "max_session_cost_usd_saved",
            "max_session_cost_usd_source",
            "max_session_minutes_requested",
            "max_session_minutes_saved",
            "max_session_minutes_source",
        ] {
            assert!(
                object.contains_key(required_key),
                "log context must contain key {required_key}"
            );
        }
        assert_eq!(object["active_agents_source"], "saved_contract");
        assert_eq!(
            object["max_session_cost_usd_source"], "saved_contract",
            "cost source should fall back to saved contract when request omits it"
        );
        assert_eq!(
            object["max_session_minutes_source"], "request",
            "minutes source should be request when request supplies the value"
        );
        assert_eq!(object["max_session_cost_usd_saved"], 7.5);
        assert_eq!(object["max_session_minutes_requested"], 45);
        assert!(object["active_agents_requested"].is_null());
    }

    #[test]
    fn active_agents_resolved_log_context_marks_caps_unset_when_neither_request_nor_saved_supply() {
        let effective = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
        ];
        let context = build_active_agents_resolved_log_context(
            "run-test",
            None,
            None,
            &effective,
            "default_all",
            "claude",
            None,
            None,
            None,
        );
        assert_eq!(context["max_session_cost_usd_source"], "unset");
        assert_eq!(context["max_session_minutes_source"], "unset");
        assert_eq!(context["active_agents_source"], "default_all");
        assert!(context["active_agents_saved_contract"].is_null());
    }

    #[test]
    fn finalize_running_agent_artifacts_rewrites_running_to_failed_no_output() {
        let session_dir = sessions_dir().join("run-finalize-running-test");
        let _ = fs::remove_dir_all(&session_dir);
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        let stuck_path = agent_dir.join("round-007-codex-review.md");
        let stuck_artifact = "# Codex - review\n\n- CLI: `codex`\n- Status: `RUNNING`\n- Exit code: `unknown`\n\n## Stdout\n\n```text\n\n```\n";
        write_text_file(&stuck_path, stuck_artifact).unwrap();
        let normal_path = agent_dir.join("round-007-claude-review.md");
        let normal_artifact = "# Claude - review\n\n- CLI: `claude`\n- Status: `READY`\n- Exit code: `0`\n\n## Stdout\n\n```text\nMAESTRO_STATUS=READY\n```\n";
        write_text_file(&normal_path, normal_artifact).unwrap();

        finalize_running_agent_artifacts(&agent_dir);

        let stuck_after = read_text_file(&stuck_path).unwrap();
        assert!(stuck_after.contains("- Status: `AGENT_FAILED_NO_OUTPUT`"));
        assert!(!stuck_after.contains("- Status: `RUNNING`"));
        assert!(stuck_after.contains("Reclassificado para AGENT_FAILED_NO_OUTPUT"));
        let normal_after = read_text_file(&normal_path).unwrap();
        assert_eq!(normal_after, normal_artifact);
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn finalize_running_artifacts_drop_guard_runs_on_panic() {
        use std::panic::AssertUnwindSafe;
        let session_dir = sessions_dir().join("run-finalize-drop-guard-test");
        let _ = fs::remove_dir_all(&session_dir);
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        let stuck_path = agent_dir.join("round-099-codex-review.md");
        write_text_file(
            &stuck_path,
            "# Codex - review\n\n- CLI: `codex`\n- Status: `RUNNING`\n- Exit code: `unknown`\n\n## Stdout\n\n```text\n\n```\n",
        )
        .unwrap();

        let agent_dir_clone = agent_dir.clone();
        let panic_caught = std::panic::catch_unwind(AssertUnwindSafe(move || {
            let _guard = FinalizeRunningArtifactsGuard::new(agent_dir_clone);
            panic!("simulating mid-session panic");
        }));
        assert!(panic_caught.is_err(), "panic must propagate");

        let after = read_text_file(&stuck_path).unwrap();
        assert!(
            after.contains("- Status: `AGENT_FAILED_NO_OUTPUT`"),
            "Drop guard must rewrite RUNNING placeholder even on panic; got: {after}"
        );
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn parse_retry_after_header_reads_delta_seconds() {
        use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("42"));
        assert_eq!(parse_retry_after_header(&headers), Some(42));
    }

    #[test]
    fn parse_retry_after_header_reads_http_date() {
        use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
        let future = chrono::Utc::now() + chrono::Duration::seconds(45);
        let value = future.to_rfc2822();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_str(&value).unwrap());
        let parsed = parse_retry_after_header(&headers).expect("date should parse");
        assert!(
            (43..=47).contains(&parsed),
            "expected ~45s until reset, got {parsed}"
        );
    }

    #[test]
    fn parse_retry_after_header_returns_none_when_absent() {
        use reqwest::header::HeaderMap;
        let headers = HeaderMap::new();
        assert_eq!(parse_retry_after_header(&headers), None);
    }

    #[test]
    fn parse_retry_after_header_returns_none_for_garbage() {
        use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("never"));
        assert_eq!(parse_retry_after_header(&headers), None);
    }

    #[test]
    fn classify_pipe_error_recognises_windows_109_as_broken_pipe() {
        let error = std::io::Error::from_raw_os_error(109);
        let classification = classify_pipe_error(&error);
        assert!(classification.contains("windows_error_109_broken_pipe"));
        assert!(classification.contains("raw_os_error=109"));
    }

    #[test]
    fn classify_pipe_error_recognises_kind_broken_pipe() {
        let error = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "child closed pipe");
        let classification = classify_pipe_error(&error);
        assert!(classification.contains("broken_pipe"));
    }

    #[test]
    fn editorial_agent_environment_sets_utf8_for_all_clis() {
        #[allow(clippy::disallowed_methods)]
        let mut command = std::process::Command::new("printf");
        let path = Path::new("printf");
        apply_editorial_agent_environment(&mut command, path);
        let envs: Vec<(String, String)> = command
            .get_envs()
            .filter_map(|(key, value)| {
                Some((
                    key.to_str()?.to_string(),
                    value?.to_str()?.to_string(),
                ))
            })
            .collect();
        let envs_map: std::collections::BTreeMap<_, _> = envs.into_iter().collect();
        assert_eq!(envs_map.get("PYTHONIOENCODING").map(String::as_str), Some("utf-8"));
        assert_eq!(envs_map.get("PYTHONUTF8").map(String::as_str), Some("1"));
        assert_eq!(envs_map.get("LC_ALL").map(String::as_str), Some("C.UTF-8"));
        assert_eq!(envs_map.get("LANG").map(String::as_str), Some("C.UTF-8"));
        assert_eq!(envs_map.get("GEMINI_CLI_TRUST_WORKSPACE"), None);
    }

    #[test]
    fn editorial_agent_environment_sets_gemini_trust_only_for_gemini_cli() {
        #[allow(clippy::disallowed_methods)]
        let mut command = std::process::Command::new("gemini");
        let path = Path::new("gemini");
        apply_editorial_agent_environment(&mut command, path);
        let envs: Vec<(String, String)> = command
            .get_envs()
            .filter_map(|(key, value)| {
                Some((
                    key.to_str()?.to_string(),
                    value?.to_str()?.to_string(),
                ))
            })
            .collect();
        let envs_map: std::collections::BTreeMap<_, _> = envs.into_iter().collect();
        assert_eq!(
            envs_map.get("GEMINI_CLI_TRUST_WORKSPACE").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn review_complaint_fingerprint_stable_across_whitespace_normalization() {
        let artifact_a = "# C - review\n\n- Status: NOT_READY\n\n## Stdout\n\n```text\nLink quebrado\n  na referencia 12.\n```\n";
        let artifact_b = "# C - review\n\n- Status: NOT_READY\n\n## Stdout\n\n```text\nLink   quebrado na referencia 12.\n```\n";
        assert_eq!(
            review_complaint_fingerprint(artifact_a),
            review_complaint_fingerprint(artifact_b)
        );
    }

    #[test]
    fn review_complaint_fingerprint_differs_on_distinct_complaints() {
        let artifact_a = "# C - review\n\n## Stdout\n\n```text\nLink A quebrado.\n```\n";
        let artifact_b = "# C - review\n\n## Stdout\n\n```text\nLink B quebrado.\n```\n";
        assert_ne!(
            review_complaint_fingerprint(artifact_a),
            review_complaint_fingerprint(artifact_b)
        );
    }

    #[test]
    fn nonzero_empty_review_with_success_exit_classifies_as_agent_failed_empty() {
        let session_dir = sessions_dir().join("run-success-empty-review-test");
        let _ = fs::remove_dir_all(&session_dir);
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        let path = agent_dir.join("round-001-deepseek-review.md");
        write_text_file(
            &path,
            "# DeepSeek - review\n\n- CLI: `deepseek-api`\n- Status: `AGENT_FAILED_EMPTY`\n- Exit code: `0`\n- Duration ms: `120`\n\n## Stdout\n\n```text\n\n```\n",
        )
        .unwrap();
        let artifact = parse_agent_artifact_name(&agent_dir, "round-001-deepseek-review.md")
            .expect("artifact should parse");
        let parsed = parse_agent_artifact_result(&artifact).expect("artifact result should parse");
        assert_eq!(parsed.status, "AGENT_FAILED_EMPTY");
        assert_eq!(parsed.tone, "error");
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn blocked_minutes_decision_includes_agent_failed_empty_in_operational_failures() {
        let agents = vec![EditorialAgentResult {
            name: "DeepSeek".to_string(),
            role: "review".to_string(),
            cli: "deepseek-api".to_string(),
            tone: "error".to_string(),
            status: "AGENT_FAILED_EMPTY".to_string(),
            duration_ms: 120,
            exit_code: Some(0),
            output_path: "agent-runs/round-001-deepseek-review.md".to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
        }];
        let decision = build_blocked_minutes_decision(&agents);
        assert!(decision.contains("Falhas operacionais"));
        assert!(decision.contains("AGENT_FAILED_EMPTY"));
    }

    #[test]
    fn blocked_minutes_decision_names_operational_failure() {
        let agents = vec![
            EditorialAgentResult {
                name: "Claude".to_string(),
                role: "review".to_string(),
                cli: "claude".to_string(),
                tone: "error".to_string(),
                status: "AGENT_FAILED_NO_OUTPUT".to_string(),
                duration_ms: 42,
                exit_code: Some(1),
                output_path: "agent-runs/round-001-claude-review.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
            EditorialAgentResult {
                name: "Gemini".to_string(),
                role: "review".to_string(),
                cli: "gemini".to_string(),
                tone: "warn".to_string(),
                status: "NOT_READY".to_string(),
                duration_ms: 84,
                exit_code: Some(0),
                output_path: "agent-runs/round-001-gemini-review.md".to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            },
        ];

        let decision = build_blocked_minutes_decision(&agents);

        assert!(decision.contains("Falhas operacionais"));
        assert!(decision.contains("AGENT_FAILED_NO_OUTPUT"));
        assert!(decision.contains("Divergencias editoriais"));
        assert!(decision.contains("NOT_READY"));
    }

    #[test]
    fn long_agent_input_is_materialized_as_sidecar_file() {
        let session_dir = sessions_dir().join("run-agent-input-sidecar-test");
        let _ = fs::remove_dir_all(&session_dir);
        let agent_dir = session_dir.join("agent-runs");
        fs::create_dir_all(&agent_dir).unwrap();
        let output_path = agent_dir.join("round-001-claude-review.md");
        let long_input = "protocolo e rascunho\n".repeat(3_000);

        let prepared = prepare_agent_input("Claude", "review", &long_input, &output_path);

        assert_eq!(prepared.original_chars, long_input.chars().count());
        assert!(prepared.stdin_text.chars().count() < prepared.original_chars);
        let input_path = prepared
            .input_path
            .expect("sidecar input file should be created");
        assert!(input_path.ends_with("round-001-claude-review-input.md"));
        assert_eq!(fs::read_to_string(&input_path).unwrap(), long_input);
        assert!(prepared
            .stdin_text
            .contains("round-001-claude-review-input.md"));
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn gemini_sidecar_input_is_delivered_through_prompt_arg() {
        let prepared = PreparedAgentInput {
            stdin_text: "Leia integralmente o arquivo local `large-input.md`.".to_string(),
            original_chars: 60_000,
            input_path: Some(PathBuf::from("large-input.md")),
        };

        let effective = effective_agent_input("gemini", gemini_args(), &prepared);
        let prompt_index = effective
            .args
            .iter()
            .position(|arg| arg == "--prompt")
            .expect("gemini args should include --prompt");

        assert_eq!(
            effective.args.get(prompt_index + 1),
            Some(&prepared.stdin_text)
        );
        assert!(effective.stdin_text.is_none());
        assert_eq!(effective.stdin_chars, 0);
        assert_eq!(effective.delivery, "prompt_arg_sidecar");
    }

    #[test]
    fn non_gemini_sidecar_input_stays_on_stdin() {
        let prepared = PreparedAgentInput {
            stdin_text: "Leia integralmente o arquivo local `large-input.md`.".to_string(),
            original_chars: 60_000,
            input_path: Some(PathBuf::from("large-input.md")),
        };

        let effective = effective_agent_input("claude", claude_args(), &prepared);

        assert_eq!(
            effective.stdin_text.as_deref(),
            Some(prepared.stdin_text.as_str())
        );
        assert_eq!(effective.stdin_chars, prepared.stdin_text.chars().count());
        assert_eq!(effective.delivery, "stdin_sidecar");
    }

    #[test]
    fn cloudflare_page_path_preserves_existing_query_parameters() {
        assert_eq!(
            cloudflare_page_path("/accounts/abc/secrets_store/stores/xyz/secrets", 2, 50),
            "/accounts/abc/secrets_store/stores/xyz/secrets?page=2&per_page=50"
        );
        assert_eq!(
            cloudflare_page_path("/accounts/abc/d1/database?name=maestro", 3, 100),
            "/accounts/abc/d1/database?name=maestro&page=3&per_page=100"
        );
    }

    #[test]
    fn secrets_store_selection_prefers_target_without_renaming() {
        let value = json!({
            "result": [
                { "id": "store-1", "name": "existing-store" },
                { "id": "store-2", "name": "maestro" }
            ]
        });

        let selected = cloudflare_store_for_target_or_existing(&value, "maestro")
            .expect("target store should be selected");

        assert_eq!(selected.name, "maestro");
        assert_eq!(selected.id, "store-2");
    }

    #[test]
    fn secrets_store_selection_uses_existing_store_when_target_absent() {
        let value = json!({
            "result": [
                { "id": "only-store-id", "name": "account-store" }
            ]
        });

        let selected = cloudflare_store_for_target_or_existing(&value, "maestro")
            .expect("existing account store should be reused");

        assert_eq!(selected.name, "account-store");
        assert_eq!(selected.id, "only-store-id");
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
    fn resolves_portable_root_from_current_exe_parent() {
        let exe_path = std::env::current_exe().unwrap();
        let expected_root = exe_path.parent().unwrap().canonicalize().unwrap();

        let resolved = portable_root_from_exe_path(&exe_path).unwrap();

        assert_eq!(resolved, expected_root);
    }

    #[test]
    fn writes_early_crash_record_before_normal_logger() {
        let marker = format!(
            "startup panic marker {}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        write_early_crash_record(&marker, Some("startup.rs:10:20")).unwrap();

        let log_dir = active_or_early_logs_dir();
        let found = fs::read_dir(&log_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| name.starts_with("maestro-crash-") && name.ends_with(".json"))
                    .unwrap_or(false)
            })
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .any(|text| text.contains(&marker) && text.contains("startup.rs:10:20"));

        assert!(found);
    }

    #[test]
    fn extracts_saved_prompt_for_session_resume() {
        let prompt_file =
            "# Prompt da Sessao\n\nSessao: Teste Editorial\nRun: `run-resume`\nAgente redator inicial: `codex`\n\nEscreva o artigo.";
        assert_eq!(
            extract_saved_session_name(prompt_file).as_deref(),
            Some("Teste Editorial")
        );
        assert_eq!(
            extract_saved_initial_agent(prompt_file).as_deref(),
            Some("codex")
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
        write_text_file(
            &agent_dir.join("round-008-claude-revision.md"),
            "# Claude - revision\n\n- CLI: `claude`\n- Status: `RUNNING`\n- Exit code: `unknown`\n- Duration ms: `0`\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n\n```\n",
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
        assert_eq!(state.existing_agents.len(), 3);

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
    install_process_panic_hook();
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
            read_ai_provider_config,
            write_ai_provider_config,
            verify_ai_provider_credentials,
            audit_links,
            open_data_file,
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
