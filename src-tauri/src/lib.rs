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
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::{fs, path::Path, thread};
use std::{path::PathBuf, process::Output, time::Duration};
use tauri::Manager;

mod ai_probes;
mod api_payloads;
mod app_init;
mod app_paths;
mod cli_adapter;
mod cloudflare;
mod cloudflare_commands;
mod command_path;
mod command_spawn;
mod config_persistence;
mod editorial_agent_runners;
mod editorial_helpers;
mod editorial_inputs;
mod editorial_io;
mod editorial_prompts;
mod human_logs;
mod link_audit;
mod logging;
mod provider_config;
mod provider_deepseek;
mod provider_grok;
mod provider_retry;
mod provider_routing;
mod provider_runners;
mod sanitize;
mod session_artifacts;
mod session_cancel;
mod session_commands;
mod session_controls;
mod session_evidence;
mod session_minutes;
mod session_orchestration;
mod session_persistence;
mod session_resume;
mod tauri_commands;

// Re-export the sanitization helpers so existing `use crate::sanitize_text`
// and similar imports across all 18+ sibling modules continue to resolve
// without per-file edits. Only the home of the implementation moved.
#[cfg(test)]
pub(crate) use crate::sanitize::{redact_secrets, should_redact_key};
pub(crate) use crate::sanitize::{
    sanitize_short, sanitize_text, sanitize_value, truncate_text_head_tail,
};

// Re-export the provider_config surface so existing `crate::sanitize_ai_provider_config`
// and similar unqualified call sites in lib.rs (and mod tests via `super::*`)
// continue to resolve without per-file edits. Only the home of the implementation
// moved. v0.3.40. Helpers consumed exclusively inside provider_config.rs
// (`normalize_provider_mode`, `provider_env_value`, `sanitize_optional_cost_rate`,
// `sanitize_optional_secret`) are not re-exported here.
#[cfg(test)]
use crate::config_persistence::persist_ai_provider_cloudflare_marker;
pub(crate) use crate::provider_config::sanitize_ai_provider_config;

// Re-export the config_persistence surface so existing
// `crate::persist_ai_provider_config` and similar unqualified call sites in
// lib.rs continue to resolve. v0.3.41. Helpers consumed only inside
// config_persistence.rs (`json_find_first_string`,
// `read_ai_provider_cloudflare_metadata`) are not re-exported here.

// Re-export the provider_routing surface so existing `crate::api_cli_for_agent`
// and similar unqualified call sites across sibling modules (ai_probes.rs,
// cloudflare.rs, editorial_agent_runners.rs, provider_config.rs,
// provider_deepseek.rs, provider_runners.rs) continue to resolve without
// per-file edits. v0.3.43. The Windows-only `windows_registry_env_value` is
// consumed only inside `provider_routing.rs` and is not re-exported.
pub(crate) use crate::provider_routing::{
    api_cli_for_agent, effective_provider_key, env_value_with_scope, first_env_value,
    provider_key_for_agent, provider_label_for_agent, provider_remote_present,
};

// Re-export the api_payloads surface so existing `crate::openai_api_input`
// and similar unqualified call sites in `provider_runners.rs` and
// `provider_deepseek.rs` continue to resolve without per-file edits.
// v0.3.44. Helpers consumed exclusively inside `api_payloads.rs`
// (`provider_supports_native_attachment`, the 4 per-provider
// `*_api_attachment_supported` helpers, `attachment_within_native_payload_cap`)
// are not re-exported here. `API_NATIVE_ATTACHMENT_MAX_FILE_BYTES` is
// `#[cfg(test)]`-gated because only `lib.rs::tests` exercises the cap
// directly; production code goes through `api_payloads.rs` internally.
#[cfg(test)]
use crate::api_payloads::API_NATIVE_ATTACHMENT_MAX_FILE_BYTES;
pub(crate) use crate::api_payloads::{
    anthropic_api_user_content, api_input_estimate_chars, gemini_api_user_parts, openai_api_input,
};

// Re-export the app_paths surface used throughout this file. The functions
// keep their pre-extraction call sites (`sessions_dir()`, `app_root()`, etc.)
// untouched; only the home of the implementation moved per
// `docs/code-split-plan.md`. See `app_paths.rs` for documentation.
#[cfg(test)]
use crate::app_paths::active_or_early_logs_dir;
#[cfg(test)]
use crate::app_paths::safe_run_id_from_entry;
#[cfg(test)]
pub(crate) use crate::app_paths::sessions_dir;
use crate::app_paths::{app_root, human_log_path_for, logs_dir};
pub(crate) use crate::app_paths::{checked_data_child_path, sanitize_path_segment};
#[cfg(test)]
use crate::cloudflare::ai_provider_secret_values;
use crate::command_path::{command_search_dirs, resolve_command};
#[cfg(test)]
use crate::editorial_helpers::{
    filter_existing_agents_to_active_set, finalize_running_agent_artifacts,
    resolve_effective_active_agents, review_complaint_fingerprint, FinalizeRunningArtifactsGuard,
};
#[cfg(test)]
use crate::editorial_inputs::{
    build_active_agents_resolved_log_context, resolve_time_budget_anchor,
};
use crate::editorial_prompts::{
    editorial_agent_specs, ordered_editorial_agent_specs, resolve_initial_agent_key,
};
use crate::logging::{create_log_session, write_log_record, LogEventInput, LogSession};
#[cfg(test)]
use crate::session_artifacts::{inspect_resumable_session_dir, load_resume_session_state};
#[cfg(test)]
use crate::session_persistence::{load_session_contract, write_session_contract};
#[cfg(test)]
use crate::session_resume::{
    extract_saved_initial_agent, extract_saved_prompt, extract_saved_session_name,
};
// Items below are only referenced from `mod tests` and cargo flags them as unused
// when compiled without the test cfg. Re-importing inside the test module avoids
// `#[cfg(test)]` annotations on the main use list.

#[cfg(test)]
use human_logs::should_collapse_human_log_event;
// `human_log_summary`, `severity_number_for`, `write_human_log_projection`
// are now consumed inside `logging.rs`; lib.rs no longer needs them.
#[cfg(test)]
use session_controls::ProviderCostRates;
use session_evidence::{AttachmentManifestEntry, PromptAttachmentRequest};

// `NATIVE_LOG_SEQUENCE` lives in `logging::NATIVE_LOG_SEQUENCE`.
// `APP_ROOT` lives in `app_paths::APP_ROOT`.

// `LogSession`, `LogEventInput`, `LogWriteResult`, `create_log_session`,
// and `write_log_record` were moved into `logging.rs` in v0.3.19. Imports
// land in the use list below alongside `app_paths::*`.

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct BootstrapConfig {
    pub(crate) schema_version: u8,
    pub(crate) credential_storage_mode: String,
    pub(crate) cloudflare_account_id: Option<String>,
    pub(crate) cloudflare_api_token_source: String,
    pub(crate) cloudflare_api_token_env_var: String,
    pub(crate) cloudflare_persistence_database: String,
    pub(crate) cloudflare_secret_store: String,
    pub(crate) windows_env_prefix: String,
    pub(crate) updated_at: String,
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
    pub(crate) grok_api_key: Option<String>,
    #[serde(default)]
    pub(crate) openai_api_key_remote: bool,
    #[serde(default)]
    pub(crate) anthropic_api_key_remote: bool,
    #[serde(default)]
    pub(crate) gemini_api_key_remote: bool,
    #[serde(default)]
    pub(crate) deepseek_api_key_remote: bool,
    #[serde(default)]
    pub(crate) grok_api_key_remote: bool,
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
    grok_input_usd_per_million: Option<f64>,
    #[serde(default)]
    grok_output_usd_per_million: Option<f64>,
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
pub(crate) struct AiProviderProbeRow {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) tone: String,
}

#[derive(Serialize)]
pub(crate) struct AiProviderProbeResult {
    pub(crate) rows: Vec<AiProviderProbeRow>,
    pub(crate) checked_at: String,
}

#[derive(Deserialize)]
pub(crate) struct LinkAuditRequest {
    pub(crate) text: String,
}

#[derive(Serialize)]
pub(crate) struct LinkAuditRow {
    pub(crate) url: String,
    pub(crate) status: String,
    pub(crate) tone: String,
}

#[derive(Serialize)]
pub(crate) struct LinkAuditResult {
    pub(crate) urls_found: usize,
    pub(crate) checked: usize,
    pub(crate) ok: usize,
    pub(crate) failed: usize,
    pub(crate) rows: Vec<LinkAuditRow>,
}

#[derive(Serialize)]
pub(crate) struct CloudflareEnvSnapshot {
    pub(crate) account_id: Option<String>,
    pub(crate) account_id_env_var: Option<String>,
    pub(crate) account_id_env_scope: Option<String>,
    pub(crate) api_token_present: bool,
    pub(crate) api_token_env_var: Option<String>,
    pub(crate) api_token_env_scope: Option<String>,
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
pub(crate) struct CliAdapterSmokeRequest {
    pub(crate) run_id: String,
    pub(crate) prompt_chars: usize,
    pub(crate) protocol_name: String,
    pub(crate) protocol_lines: usize,
    pub(crate) protocol_hash: String,
}

#[derive(Serialize)]
pub(crate) struct CliAdapterSmokeResult {
    pub(crate) run_id: String,
    pub(crate) agents: Vec<CliAdapterProbeResult>,
    pub(crate) all_ready: bool,
}

#[derive(Serialize)]
pub(crate) struct CliAdapterProbeResult {
    pub(crate) name: String,
    pub(crate) cli: String,
    pub(crate) tone: String,
    pub(crate) status: String,
    pub(crate) duration_ms: u128,
    pub(crate) exit_code: Option<i32>,
    pub(crate) marker_found: bool,
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
pub(crate) struct ResumeSessionRequest {
    pub(crate) run_id: String,
    pub(crate) protocol_name: Option<String>,
    pub(crate) protocol_text: Option<String>,
    pub(crate) protocol_hash: Option<String>,
    pub(crate) initial_agent: Option<String>,
    pub(crate) active_agents: Option<Vec<String>>,
    pub(crate) max_session_cost_usd: Option<f64>,
    pub(crate) max_session_minutes: Option<u64>,
    pub(crate) attachments: Option<Vec<PromptAttachmentRequest>>,
    pub(crate) links: Option<Vec<String>>,
}

#[derive(Serialize)]
pub(crate) struct EditorialSessionResult {
    pub(crate) run_id: String,
    pub(crate) session_dir: String,
    pub(crate) final_markdown_path: Option<String>,
    pub(crate) session_minutes_path: String,
    pub(crate) prompt_path: String,
    pub(crate) protocol_path: String,
    pub(crate) draft_path: Option<String>,
    pub(crate) agents: Vec<EditorialAgentResult>,
    pub(crate) consensus_ready: bool,
    pub(crate) status: String,
    pub(crate) active_agents: Vec<String>,
    pub(crate) max_session_cost_usd: Option<f64>,
    pub(crate) max_session_minutes: Option<u64>,
    pub(crate) observed_cost_usd: Option<f64>,
    pub(crate) links_path: Option<String>,
    pub(crate) attachments_manifest_path: Option<String>,
    pub(crate) human_log_path: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct ProviderCacheTelemetry {
    pub(crate) provider_mode: String,
    pub(crate) cache_key_hash: Option<String>,
    pub(crate) cache_control_status: Option<String>,
    pub(crate) cache_retention: Option<String>,
    pub(crate) cached_input_tokens: Option<u64>,
    pub(crate) cache_hit_tokens: Option<u64>,
    pub(crate) cache_miss_tokens: Option<u64>,
    pub(crate) cache_read_input_tokens: Option<u64>,
    pub(crate) cache_creation_input_tokens: Option<u64>,
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
    pub(crate) cache: Option<ProviderCacheTelemetry>,
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
pub(crate) struct ResumableSessionInfo {
    pub(crate) run_id: String,
    pub(crate) session_name: String,
    pub(crate) session_dir: String,
    pub(crate) prompt_path: String,
    pub(crate) protocol_path: String,
    pub(crate) draft_path: Option<String>,
    pub(crate) final_markdown_path: Option<String>,
    pub(crate) next_round: usize,
    pub(crate) last_activity_unix: u64,
    pub(crate) artifact_count: usize,
    pub(crate) protocol_lines: usize,
    pub(crate) status: String,
    /// `active_agents` from the saved session contract. Used by the frontend
    /// to pre-populate React state on cold app open so that clicking
    /// "Retomar" continues with the same peers selected when the session
    /// was last saved, instead of overwriting with the cold-open default
    /// of all 4 (B17 fix from v0.3.18).
    pub(crate) saved_active_agents: Vec<String>,
    /// `initial_agent` from the saved session contract.
    pub(crate) saved_initial_agent: Option<String>,
    /// Optional cost cap from the saved session contract.
    pub(crate) saved_max_session_cost_usd: Option<f64>,
    /// Optional time cap from the saved session contract.
    pub(crate) saved_max_session_minutes: Option<u64>,
}

pub(crate) struct ResumeSessionState {
    pub(crate) current_draft: String,
    pub(crate) current_draft_path: Option<PathBuf>,
    pub(crate) next_review_round: usize,
    pub(crate) existing_agents: Vec<EditorialAgentResult>,
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
    #[serde(default)]
    pub(crate) run_id: String,
    pub(crate) total_observed_cost_usd: f64,
    pub(crate) entries: Vec<CostLedgerEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CostLedgerEntry {
    #[serde(default)]
    pub(crate) run_id: String,
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
pub(crate) struct CliAdapterSpec {
    pub(crate) name: &'static str,
    pub(crate) command: &'static str,
    pub(crate) marker: &'static str,
    pub(crate) args: Vec<String>,
    pub(crate) timeout: Duration,
}

pub(crate) struct TimedCommandOutput {
    pub(crate) output: Output,
    pub(crate) duration_ms: u128,
    pub(crate) timed_out: bool,
    pub(crate) stdout_pipe_error: Option<String>,
    pub(crate) stderr_pipe_error: Option<String>,
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
            grok_api_key: None,
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            grok_api_key_remote: false,
            openai_input_usd_per_million: None,
            openai_output_usd_per_million: None,
            anthropic_input_usd_per_million: None,
            anthropic_output_usd_per_million: None,
            gemini_input_usd_per_million: None,
            gemini_output_usd_per_million: None,
            deepseek_input_usd_per_million: None,
            deepseek_output_usd_per_million: None,
            grok_input_usd_per_million: None,
            grok_output_usd_per_million: None,
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct RuntimeProfile {
    pub(crate) app_name: &'static str,
    pub(crate) storage_policy: &'static str,
    pub(crate) target_platform: &'static str,
    pub(crate) log_dir: String,
    pub(crate) log_file: String,
    pub(crate) human_log_file: String,
    pub(crate) log_session_id: String,
}

use crate::cloudflare_commands::{
    cloudflare_env_snapshot, dependency_preflight, verify_cloudflare_credentials,
};
use crate::session_commands::{
    list_resumable_sessions, resume_editorial_session, run_editorial_session,
    stop_editorial_session,
};
pub(crate) use crate::session_orchestration::{
    run_editorial_session_core, run_editorial_session_inner,
};
use crate::tauri_commands::{
    audit_links, diagnostics_snapshot, open_data_file, read_ai_provider_config,
    read_bootstrap_config, run_cli_adapter_smoke, runtime_profile, verify_ai_provider_credentials,
    write_ai_provider_config, write_bootstrap_config, write_log_event,
};

#[cfg(test)]
use crate::app_init::write_early_crash_record;
pub(crate) use crate::app_init::{hidden_command, initialize_app_root, install_process_panic_hook};

pub(crate) use crate::editorial_io::{
    api_error_message, command_working_dir_for_output, extract_maestro_status,
    extract_stdout_block, log_editorial_agent_finished, log_editorial_agent_running,
    log_editorial_agent_spawned, read_text_file, write_text_file,
};

#[derive(Clone)]
pub(crate) struct SessionArtifact {
    pub(crate) round: usize,
    pub(crate) agent: String,
    pub(crate) role: String,
    pub(crate) path: PathBuf,
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
            run_editorial_session,
            stop_editorial_session
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Maestro Editorial AI");
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
    use crate::command_spawn::{apply_editorial_agent_environment, classify_pipe_error};
    use crate::editorial_inputs::{effective_agent_input, prepare_agent_input};
    use crate::editorial_prompts::{claude_args, gemini_args};
    use crate::link_audit::{extract_public_urls, is_public_http_url};
    use crate::provider_retry::parse_retry_after_header;
    use crate::session_artifacts::{parse_agent_artifact_name, parse_agent_artifact_result};
    use crate::session_controls::{normalize_active_agents, provider_cost};
    use crate::session_minutes::build_blocked_minutes_decision;
    use crate::session_resume::{count_known_session_markdown_artifacts, protocol_backup_stats};

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
        let text = redact_secrets(
            &[
                ["cfat", "_", "secret123"].concat(),
                ["cfut", "_", "secret123"].concat(),
                ["cfk", "_", "secret123"].concat(),
            ]
            .join(" "),
        );
        assert_eq!(text, "<redacted> <redacted> <redacted>");
    }

    #[test]
    fn redacts_embedded_secret_values_without_whitespace_boundary() {
        let google_key = ["AI", "za", "12345678"].concat();
        let cloudflare_token = ["cfut", "_", "12345678"].concat();
        let anthropic_key = ["sk", "-ant-", "12345678"].concat();
        let text = redact_secrets(&format!(
            r#"url=https://example.test/?key={google_key} header=Authorization:Bearer {cloudflare_token} json={{"api_key":"{anthropic_key}"}}"#
        ));
        assert!(!text.contains(&google_key));
        assert!(!text.contains(&cloudflare_token));
        assert!(!text.contains(&anthropic_key));
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
        assert_eq!(
            claude_order,
            vec!["claude", "codex", "gemini", "deepseek", "grok"]
        );

        let codex_order = ordered_editorial_agent_specs("codex")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(
            codex_order,
            vec!["codex", "claude", "gemini", "deepseek", "grok"]
        );

        let gemini_order = ordered_editorial_agent_specs("gemini")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(
            gemini_order,
            vec!["gemini", "claude", "codex", "deepseek", "grok"]
        );

        let deepseek_order = ordered_editorial_agent_specs("deepseek")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(
            deepseek_order,
            vec!["deepseek", "claude", "codex", "gemini", "grok"]
        );

        let grok_order = ordered_editorial_agent_specs("grok")
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();
        assert_eq!(
            grok_order,
            vec!["grok", "claude", "codex", "gemini", "deepseek"]
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
    fn cloudflare_ai_provider_marker_does_not_store_secret_values_locally() {
        let config = sanitize_ai_provider_config(AiProviderConfig {
            schema_version: 1,
            provider_mode: "api".to_string(),
            credential_storage_mode: "cloudflare".to_string(),
            openai_api_key: Some("sk-test-value".to_string()),
            anthropic_api_key: Some("sk-ant-test-value".to_string()),
            gemini_api_key: Some("AIza-test-value".to_string()),
            deepseek_api_key: Some("ds-test-value".to_string()),
            grok_api_key: Some("xai-test-value".to_string()),
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            grok_api_key_remote: false,
            openai_input_usd_per_million: Some(2.50),
            openai_output_usd_per_million: Some(10.0),
            anthropic_input_usd_per_million: Some(3.0),
            anthropic_output_usd_per_million: Some(15.0),
            gemini_input_usd_per_million: Some(1.25),
            gemini_output_usd_per_million: Some(5.0),
            deepseek_input_usd_per_million: Some(0.55),
            deepseek_output_usd_per_million: Some(2.19),
            grok_input_usd_per_million: Some(3.0),
            grok_output_usd_per_million: Some(15.0),
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
        assert!(text.contains("\"grok_api_key_remote\": true"));
        assert!(!text.contains("sk-test-value"));
        assert!(!text.contains("sk-ant-test-value"));
        assert!(!text.contains("AIza-test-value"));
        assert!(!text.contains("ds-test-value"));
        assert!(!text.contains("xai-test-value"));
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
            grok_api_key: Some("xai-test-value".to_string()),
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            grok_api_key_remote: false,
            openai_input_usd_per_million: Some(2.50),
            openai_output_usd_per_million: Some(10.0),
            anthropic_input_usd_per_million: Some(3.0),
            anthropic_output_usd_per_million: Some(15.0),
            gemini_input_usd_per_million: Some(1.25),
            gemini_output_usd_per_million: Some(5.0),
            deepseek_input_usd_per_million: Some(0.55),
            deepseek_output_usd_per_million: Some(2.19),
            grok_input_usd_per_million: Some(3.0),
            grok_output_usd_per_million: Some(15.0),
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
            updated_at: "old".to_string(),
        });

        let values = ai_provider_secret_values(&config);

        assert_eq!(values.len(), 4);
        assert!(values.contains_key("MAESTRO_OPENAI_API_KEY"));
        assert!(values.contains_key("MAESTRO_GEMINI_API_KEY"));
        assert!(values.contains_key("MAESTRO_DEEPSEEK_API_KEY"));
        assert!(values.contains_key("MAESTRO_GROK_API_KEY"));
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
        write_text_file(&session_dir.join("session-contract.json"), legacy_payload).unwrap();
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
        let (effective, source) =
            resolve_effective_active_agents(None, None).expect("default fallback should resolve");
        assert_eq!(effective.len(), 5);
        assert_eq!(source, "default_all");
    }

    #[test]
    fn resolve_effective_active_agents_recovers_when_saved_contract_is_empty() {
        let saved: Vec<String> = Vec::new();
        let (effective, source) = resolve_effective_active_agents(None, Some(&saved))
            .expect("empty saved contract should fall through, not error");
        assert_eq!(effective.len(), 5);
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
                cache: None,
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
                cache: None,
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
                cache: None,
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
                cache: None,
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
                cache: None,
            },
        ];
        let active = vec!["claude".to_string(), "codex".to_string()];
        let filtered = filter_existing_agents_to_active_set(existing, &active);
        assert_eq!(
            filtered.len(),
            2,
            "alias names must normalize to active keys"
        );
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
                cache: None,
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
                cache: None,
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
            cache: None,
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
        write_text_file(&session_dir.join("protocolo.md"), &"Linha\n".repeat(120)).unwrap();
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
        write_text_file(&session_dir.join("protocolo.md"), &"Linha\n".repeat(120)).unwrap();
        fs::create_dir_all(session_dir.join("agent-runs")).unwrap();

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
                Some((key.to_str()?.to_string(), value?.to_str()?.to_string()))
            })
            .collect();
        let envs_map: std::collections::BTreeMap<_, _> = envs.into_iter().collect();
        assert_eq!(
            envs_map.get("PYTHONIOENCODING").map(String::as_str),
            Some("utf-8")
        );
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
                Some((key.to_str()?.to_string(), value?.to_str()?.to_string()))
            })
            .collect();
        let envs_map: std::collections::BTreeMap<_, _> = envs.into_iter().collect();
        assert_eq!(
            envs_map
                .get("GEMINI_CLI_TRUST_WORKSPACE")
                .map(String::as_str),
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
            cache: None,
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
                cache: None,
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
                cache: None,
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
