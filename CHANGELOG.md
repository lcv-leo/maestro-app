# Changelog

All notable changes to Maestro Editorial AI will be documented in this file.

## [Unreleased]

## [v0.3.27] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting session contract + cost ledger persistence helpers into a dedicated module.

### Changed (extracted to `src-tauri/src/session_persistence.rs`, ~128 lines with doc header)
- `pub(crate) fn session_contract_path`, `cost_ledger_path` — canonical paths inside a session directory.
- `pub(crate) fn load_session_contract`, `write_session_contract` — JSON persistence with parse-failure logging.
- `pub(crate) fn load_cost_ledger`, `write_cost_ledger` — JSON persistence for the cumulative per-session cost ledger.
- `pub(crate) fn append_agent_cost_to_ledger` — appends one entry and recomputes the total.
- `pub(crate) fn api_provider_from_cli` — peer CLI name → provider id mapping.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct CostLedger` field upgrades (`schema_version`, `run_id`, `entries` had been private; v0.3.27 makes them `pub(crate)` along with the existing `total_observed_cost_usd`).
- `pub(crate) struct CostLedgerEntry` + 9 fields.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6693 → 6605 lines (−88 deleted; +5 mod+use lines = net −83).
- ZERO-line diff vs v0.3.26 baseline (commit aaaacff).

## [v0.3.26] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting agent input preparation, the active-agents log context builder, and the time-budget anchor helper into a dedicated module.

### Changed (extracted to `src-tauri/src/editorial_inputs.rs`, ~190 lines with doc header)
- `pub(crate) fn effective_agent_input` — gemini-aware adapter that places the prepared prompt into argv when a sidecar input file is written.
- `pub(crate) fn prepare_agent_input` — write large prompts (> 48k chars) to a `<output>-input.md` sidecar.
- `pub(crate) fn build_active_agents_resolved_log_context` — JSON payload builder for `session.editorial.active_agents_resolved` NDJSON entry.
- `pub(crate) fn resolve_time_budget_anchor` — clock anchor for `max_session_minutes` cap (B18 fix from v0.3.18).

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct PreparedAgentInput` + 3 fields.
- `pub(crate) struct EffectiveAgentInput` + 4 fields.

### Validation
- `cargo test`: 74 passed, 0 failed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6840 → 6688 lines (−152; 154 deleted, 4 mod+use added, 2 struct prefix net).

## [v0.3.25] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the agent specs (CLI args + spec table) and prompt builders (draft/review/revision) into a dedicated module. The heavy `run_editorial_session_inner` block stays in lib.rs for v0.3.26.

### Changed (extracted to `src-tauri/src/editorial_prompts.rs`, ~290 lines with doc header)
- `pub(crate) fn claude_args`, `codex_args`, `gemini_args`, `deepseek_args` — argv templates per peer CLI.
- `pub(crate) fn editorial_agent_specs` — 4-entry vector keyed by peer name.
- `pub(crate) fn resolve_initial_agent_key` — normalize operator's free-form initial-agent string.
- `pub(crate) fn ordered_editorial_agent_specs` — places chosen first key at the head of the spec list.
- `pub(crate) fn build_draft_prompt`, `build_review_prompt`, `build_revision_prompt` — markdown prompt templates.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed by `editorial_prompts.rs`)
- `pub(crate) struct EditorialSessionRequest` + 12 fields (run_id, session_name, prompt, protocol_*, initial_agent, active_agents, max_session_*, attachments, links).
- `pub(crate) struct EditorialAgentSpec` `name`/`command`/`args` fields (the struct itself was already pub(crate); only the `key` field had been upgraded prior).

### Validation
- `cargo test`: 74 passed, 0 failed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 7081 → 6840 lines (−241; net delta 6836+4 mod/use lines).

## [v0.3.24] - 2026-05-02

Pure refactor — no behavior change. Begins migration step 5 ("editorial orchestration and artifacts") by extracting the small/standalone helpers cluster (active-agent filtering/resolution, review-complaint fingerprinting, RUNNING-artifact finalization, per-attempt running/error artifact writers) into a dedicated module. The heavy `run_editorial_session_inner` block stays in lib.rs for a follow-up batch.

### Changed (extracted to `src-tauri/src/editorial_helpers.rs`, ~250 lines with doc header)
- `pub(crate) fn filter_existing_agents_to_active_set` — resume-side filter mirroring `normalize_active_agents` alias normalization.
- `pub(crate) fn resolve_effective_active_agents` — request/saved/default decision tree with audit-log source label.
- `pub(crate) fn review_complaint_fingerprint` — stable u64 hash for persistent-divergence detection.
- `pub(crate) struct FinalizeRunningArtifactsGuard` (+ impl + Drop) — RAII guard for RUNNING-placeholder cleanup on every exit path (Codex NB-2 from v0.3.15).
- `pub(crate) fn finalize_running_agent_artifacts` — idempotent final-pass safety net.
- `pub(crate) fn write_editorial_agent_running_artifact` — initial RUNNING placeholder before child spawn.
- `pub(crate) fn write_editorial_agent_error_artifact` — error envelope for failed commands without structured output.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed by `editorial_helpers.rs`)
- `pub(crate) fn read_text_file` (consumed by `finalize_running_agent_artifacts`).
- `pub(crate) fn extract_stdout_block` (consumed by `review_complaint_fingerprint`).

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression).
- `npm run typecheck`: clean.
- `npm run build`: clean (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 7270 → 7075 lines (−195). `editorial_helpers.rs`: ~250 lines new.

### Operational notes
- Followed the now-stable "delete first, edit second" sequence: built `editorial_helpers.rs`, captured fresh line numbers, single `sed -i 4304,4498d` (195 lines), then added `mod` + `use` + `pub(crate)` upgrades.
- Defensive `#[allow(clippy::too_many_arguments)]` added on `write_editorial_agent_running_artifact` (9 args) and `write_editorial_agent_error_artifact` (11 args), following the v0.3.22 sibling-helper convention.

## [v0.3.23] - 2026-05-02

Pure refactor — no behavior change. Begins `docs/code-split-plan.md` migration step 4 ("Cloudflare D1 and Secrets Store operations") by extracting the Cloudflare client + probe + D1 + Secrets Store surface into a dedicated module.

### Changed (extracted to `src-tauri/src/cloudflare.rs`, ~960 lines with doc header)
- HTTP layer: `cloudflare_client`, `cloudflare_get`, `cloudflare_post_json`, `cloudflare_patch_json`, `cloudflare_get_paginated_results`, `cloudflare_page_path`, `cloudflare_verify_path`, `cloudflare_token_kind`, `cloudflare_error_summary`.
- Token resolution: `token_from_probe_request`, `token_source_label`, `cloudflare_token_from_provider_request`.
- JSON helpers: `cloudflare_result_names`, `cloudflare_result_id_for_name`, `cloudflare_store_records`, `cloudflare_store_for_target_or_existing`, `cloudflare_secret_ids_by_name`, `cloudflare_secret_id_from_response`, `cloudflare_created_result_id`, `CloudflareStoreRecord` struct.
- D1 + Secrets Store ensure logic: `ensure_cloudflare_d1_database`, `ensure_cloudflare_secret_store`, `provision_maestro_d1_schema`, `link_secret_store_reference`.
- AI provider bridge: `ai_provider_secret_values`, `upsert_ai_provider_secrets`, `write_ai_provider_metadata_to_cloudflare`.
- Probe entry point: `run_cloudflare_probe`, `probe_row`.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed by `cloudflare.rs`)
- `pub(crate) fn env_value_with_scope` (used by token resolvers in cloudflare.rs).
- `pub(crate) struct CloudflareProbeRequest` + 6 fields.
- `pub(crate) struct CloudflareProbeRow` + 3 fields.
- `pub(crate) struct CloudflareProbeResult` + 1 field.
- `pub(crate) struct CloudflareProviderStorageRequest` + 5 fields.

### Stayed in `lib.rs`
- `cloudflare_env_snapshot`, `verify_cloudflare_credentials` (Tauri commands — must stay because of `tauri::generate_handler!` registration).
- `persist_ai_provider_cloudflare_marker`, `persist_ai_provider_config_to_cloudflare`, `enrich_ai_provider_config_from_cloudflare`, `read_ai_provider_cloudflare_metadata` (AI provider <-> Cloudflare bridges).
- `tests::secrets_store_selection_*`, `tests::routes_*_tokens_to_*_verify_endpoint`, `tests::ai_provider_secret_values_use_cloudflare_safe_names` (kept in `lib.rs::tests`; functions re-imported via `use crate::cloudflare::{cloudflare_page_path, cloudflare_store_for_target_or_existing, cloudflare_verify_path}`).

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression).
- `npm run typecheck`: clean.
- `npm run build`: clean, 1.17s, 2434 modules transformed (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 8236 → 7260 lines (−976). `cloudflare.rs`: ~960 lines new.

### Operational notes
- Followed advisor's "delete first, edit second" sequence: built `cloudflare.rs` before any `lib.rs` mutation, then captured fresh line numbers immediately before a single `sed -i 5250,6225d` deletion (976 lines), then added `mod` + `use` + `pub(crate)` upgrades. Same workflow that made v0.3.22 succeed in 2 cross-review rounds vs v0.3.21's 5.

## [v0.3.22] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 3 by bundling the 3 isomorphic provider runners (OpenAI / Anthropic / Gemini) and their shared helper family into a single new module. v0.3.21 already extracted DeepSeek (the structural outlier). With v0.3.22, all 4 per-provider runners are out of `lib.rs`.

### Changed (extracted to `src-tauri/src/provider_runners.rs`, ~1100 lines with doc header)
- **Runners** (3 isomorphic): `pub(crate) fn run_openai_api_agent`, `pub(crate) fn run_anthropic_api_agent`, `pub(crate) fn run_gemini_api_agent`. Each preserves its provider-specific request body (responses-API for OpenAI; messages-API for Anthropic; generateContent for Gemini), response parser, and endpoint string byte-for-byte.
- **Unified provider helper family**: `pub(crate) fn editorial_api_system_prompt`, `pub(crate) fn api_cost_preflight_result`, `pub(crate) fn write_provider_missing_key_result`, `pub(crate) fn write_provider_error_result`, `pub(crate) fn write_provider_failure_result`, `pub(crate) fn write_provider_success_result`, `pub(crate) fn log_provider_api_started`.
- **Per-provider model resolvers**: `pub(crate) fn resolve_openai_model`, `pub(crate) fn resolve_anthropic_model`, `pub(crate) fn resolve_gemini_model`. Plus the private helpers they share (`choose_preferred_model`, `api_model_ids`, `gemini_model_ids`).
- **Per-provider response parsers**: `pub(crate) fn openai_response_text`, plus the private `anthropic_response_text`, `gemini_response_text`, `gemini_usage_tokens`.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed from `provider_runners.rs`)
- `fn provider_label_for_agent`, `fn provider_remote_present`, `fn provider_key_for_agent` — the last has an external caller in `should_run_agent_via_api`.
- `fn api_input_estimate_chars` — also has unit tests in `lib.rs::tests`.
- `fn openai_api_input`, `fn anthropic_api_user_content`, `fn gemini_api_user_parts` — all 3 have unit tests in `lib.rs::tests` covering the JSON envelope shape per provider.

### Stayed in `lib.rs` (not moved this batch)
- The 4 attachment-shape predicates (`provider_supports_native_attachment`, `openai_api_attachment_supported`, `openai_api_file_attachment_supported`, `anthropic_api_attachment_supported`, `gemini_api_attachment_supported`, `attachment_within_native_payload_cap`) — they sit beside the `*_api_input` builders that own the tests. Moving them later if the test ownership migrates first.
- The 3 attachment-payload builders (`openai_api_input`, `anthropic_api_user_content`, `gemini_api_user_parts`) — same reason.
- `provider_label_for_agent`/`provider_remote_present`/`provider_key_for_agent` — shared by other lib.rs code paths beyond the runners.

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression).
- `npm run typecheck`: clean.
- `npm run build`: clean, 1.48s, 2434 modules transformed (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 9351 → 8233 lines (−1118). `provider_runners.rs`: ~1100 lines new.

### Operational notes
- Followed the advisor's "delete first, edit second" sequence: built `provider_runners.rs` before any `lib.rs` mutation, then captured fresh line numbers immediately before a single `sed -i` deletion (range `3886,5003d`), then added `mod` + `use` + `pub(crate)` upgrades. This eliminated the line-shift class that bit v0.3.21.
- `FUNDING.yml` URL update (operator-requested) is bundled into the same commit since it is a small documentation-only follow-up that was deferred from v0.3.21.

## [v0.3.21] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration order step 3 ("AI provider credentials/probes, including DeepSeek"). v0.3.20 extracted the shared `provider_retry` primitives; v0.3.21 begins migrating the per-provider runners themselves, starting with DeepSeek because it is the structural outlier (custom error helper, predates the unified `write_provider_failure_result` family used by openai/anthropic/gemini). The remaining 3 isomorphic runners + their shared helper family are scheduled for v0.3.22.

### Changed (extracted to `src-tauri/src/provider_deepseek.rs`, 467 lines with doc header)
- `pub(crate) fn run_deepseek_api_agent(...) -> EditorialAgentResult` — the chat-completions runner with retry, cost pre-flight, output sanitization and NDJSON instrumentation. Byte-for-byte preserves every status string, log line, format string and tone classification from the v0.3.20 lib.rs source.
- `pub(crate) fn write_deepseek_error_result(...)` — DeepSeek-specific error artifact + result envelope (custom helper, not the unified `write_provider_failure_result`).
- `pub(crate) fn deepseek_model() -> String` — env override (`MAESTRO_DEEPSEEK_MODEL` / `CROSS_REVIEW_DEEPSEEK_MODEL`) with `deepseek-v4-pro` fallback.
- `pub(crate) fn resolve_deepseek_model(client, api_key) -> String` — env override first, then `/models` listing with the candidate-preference list, with `deepseek-reasoner` as ultimate fallback.
- `pub(crate) fn deepseek_model_ids(value) -> Vec<String>` — JSON ID extractor.
- `#[cfg(test)] fn deepseek_model_ids_extract_current_api_shape` test moved with the function it covers.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed from `provider_deepseek.rs`)
- `struct AiProviderConfig` + the 4 fields the runner reads (`deepseek_api_key`, `deepseek_api_key_remote`, plus the parallel openai/anthropic/gemini fields upgraded for consistency since v0.3.22 will need them).
- `struct EditorialAgentResult` + all 12 fields (since the runner constructs the struct).
- `fn first_env_value` (consumed by `deepseek_model` and `resolve_deepseek_model`).
- `fn effective_provider_key` (consumed by the runner).
- `fn log_editorial_agent_finished` (consumed by the runner and the error helper).
- `fn extract_maestro_status` (consumed by the runner).
- `fn api_error_message` (consumed by the runner).

### Stayed in `lib.rs` (move in v0.3.22)
- `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent` (the 3 isomorphic runners that share the unified provider helper family).
- `editorial_api_system_prompt`, `api_cost_preflight_result`, `write_provider_missing_key_result`, `write_provider_error_result`, `write_provider_failure_result`, `api_input_estimate_chars`, `provider_key_for_agent`, `provider_remote_present`, `openai_response_text`, `log_provider_api_started`, `write_provider_success_result` — the unified provider helpers.

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression). The `deepseek_model_ids_extract_current_api_shape` test now runs as `provider_deepseek::tests::deepseek_model_ids_extract_current_api_shape`.
- `npm run typecheck`: clean.
- `npm run build`: clean, 1.11s, 2434 modules transformed (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 9776 → 9351 lines (−425). `provider_deepseek.rs`: 467 lines new (includes 17-line doc header + use block + the test module).

## [v0.3.20] - 2026-05-02

Pure refactor — no behavior change. Extracts the shared provider HTTP networking primitives into `src-tauri/src/provider_retry.rs` (137 lines with doc comments). Per `docs/code-split-plan.md` migration step 3 ("AI provider credentials/probes"). The 4 provider runner functions (DeepSeek/OpenAI/Anthropic/Gemini) stay in `lib.rs` and will move in v0.3.21+ along with their request body shapes and response parsers.

### Changed (extracted to `src-tauri/src/provider_retry.rs`)
- `pub(crate) fn build_api_client(timeout: Option<Duration>) -> Result<Client, reqwest::Error>` — `reqwest::blocking::Client` factory with the Maestro user-agent.
- `pub(crate) const PROVIDER_RETRY_MAX_ATTEMPTS: u32 = 2` — at most one retry.
- `pub(crate) const PROVIDER_RETRY_NETWORK_BACKOFF_MS: u64 = 1500` — backoff between attempts on transient network errors.
- `pub(crate) const PROVIDER_RETRY_429_DEFAULT_SECS: u64 = 30` — default sleep on 429 with no Retry-After header.
- `pub(crate) const PROVIDER_RETRY_429_CAP_SECS: u64 = 120` — hard cap on Retry-After to prevent provider-driven hangs.
- `pub(crate) fn parse_retry_after_header` — RFC 7231 delta-seconds OR RFC 2822 HTTP-date.
- `pub(crate) fn send_with_retry<F>` — bounded retry policy on transient network errors and HTTP 429 with Retry-After respect; logs `session.provider.retry_network` and `session.provider.retry_after_429` warn entries via the v0.3.19 `logging::write_log_record`.

### Stayed in `lib.rs`
- `run_deepseek_api_agent`, `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent` (the 4 runners) — each with provider-specific request body shapes and response parsers; planned for v0.3.21+ batches.
- `editorial_api_system_prompt`, `api_cost_preflight_result`, `write_provider_missing_key_result`, `write_provider_error_result`, `write_deepseek_error_result`, `api_input_estimate_chars`, `provider_key_for_agent`, `provider_remote_present`, `openai_response_text` — provider-orchestration helpers; planned for the same v0.3.21+ batches.

### Validation
- `cargo test`: 74 passed (zero regression). Tests for `parse_retry_after_header_*` (4) re-imported via `mod tests` `use crate::provider_retry::parse_retry_after_header;`.
- `npm run typecheck` + `npm run build`: clean.
- `cargo clippy --no-deps --all-targets`: 0 errors.
- `lib.rs`: 9873 → 9776 lines (−97). `provider_retry.rs`: 137 lines new.

## [v0.3.19] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration order step 2 ("logging and path safety"), which was partially completed in v0.3.17. This batch finishes the logger surface extraction.

### Changed (extracted to `src-tauri/src/logging.rs`, 157 lines with doc comments)
- `NATIVE_LOG_SEQUENCE: AtomicU64` static (process-scoped sequence stamp).
- `LogSession` struct (`#[derive(Clone)]`, holds `id`, `path`, `Arc<Mutex<()>> write_lock`).
- `LogEventInput` struct (`#[derive(Deserialize)]`).
- `LogWriteResult` struct (`#[derive(Serialize)]`).
- `create_log_session()` factory.
- `write_log_record(&LogSession, LogEventInput) -> Result<LogWriteResult, String>`.

### Stayed in `lib.rs`
- `runtime_profile` and `write_log_event` Tauri commands (the IPC boundary uses `tauri::State<LogSession>` and stays in the `#[tauri::command]` registry).
- `install_process_panic_hook()` and `write_early_crash_record()` (compose a separate JSON crash schema and integrate with process-level panic state).
- `log_editorial_agent_finished/spawned/running` helpers (depend on `EditorialAgentResult` and move with the editorial orchestration batch v0.3.22).
- Sanitization helpers (`sanitize_text`, `sanitize_short`, `sanitize_value`, `redact_secrets`, `truncate_text_head_tail`, `stable_text_fingerprint`) — planned for a separate `text_utils.rs` extraction.

### Visibility upgrades (required for cross-module access)
- `sanitize_short` and `sanitize_value` upgraded from `fn` to `pub(crate) fn` so `logging.rs` can import them. `sanitize_text` was already `pub(crate)`. No body changes.

### Validation
- `cargo test`: 74 passed (no test count change; same surface).
- `npm run typecheck` clean.
- `npm run build` clean.
- `cargo clippy --no-deps --all-targets`: 0 errors. `#![deny(clippy::disallowed_methods)]` from v0.3.16 preserved.
- `lib.rs`: 9930 → 9873 lines (−57). `logging.rs`: 157 lines new.

### Next batches per `docs/code-split-plan.md`
- v0.3.20: extract editorial provider runners (DeepSeek/OpenAI/Anthropic/Gemini API + retry helper) into `providers/` module.
- v0.3.21: extract Cloudflare D1 + Secrets Store operations.
- v0.3.22: extract editorial orchestration (`run_editorial_session_inner`, agent helpers, build_session_minutes, etc.).

## [v0.3.18] - 2026-05-02

Closes the 3 production bugs found in `data/logs/maestro-2026-05-01T18-56-07Z` and `19-01-53Z` running v0.3.16/v0.3.17. These were deferred from v0.3.17 per `docs/code-split-plan.md` rule "Do not combine code split with behavior changes."

### Fixed
- **B17 — saved_contract pre-select on cold-open.** v0.3.16 fix made the frontend resume invoke always send the React state. But the React state defaults to all-4 peers on cold app open, so an operator who paused with `["deepseek"]` and reopened the app saw all 4 peers re-spawn. Backend NDJSON 19:02:45 confirmed `saved_contract: ["deepseek"]`, `requested: [all 4]`, `source: "request"`. Fix: extended `ResumableSessionInfo` to expose `saved_active_agents`, `saved_initial_agent`, `saved_max_session_cost_usd`, `saved_max_session_minutes` from the inspected contract; `startResumeSession` in `App.tsx` now applies those to React state (`setActiveAgents`, `setInitialAgent`, `setMaxSessionMinutes`, `setMaxSessionCostUsd`) AND snapshots them into the runOptions passed to `runRealEditorialSession`, so the resume request reflects what the operator paused with — not the cold-open default. New NDJSON `session.resume.contract_applied` log entry surfaces the applied snapshot for audit.
- **B18 — TIME_LIMIT_REACHED instant-fire on resume.** Operator set `max_session_minutes=5` intending to limit the resume to 5 minutes; backend computed elapsed = (now − 2026-04-26T19:28:26) ≈ 5 days ≫ 5 min and exhausted in 130ms without spawning anything. New helper `resolve_time_budget_anchor(created_at, is_resume, now) -> DateTime<Utc>` returns `now` when resuming and `created_at` otherwise. The 7 call sites of `session_time_exhausted` and `remaining_session_duration` in `run_editorial_session_inner` swap from `created_at` to `time_budget_anchor`. `created_at` remains the single source of truth for cumulative metrics and stays persisted in the session contract; only the time-budget gate switches to the per-call anchor.
- **B19 — stale "Ultimo estado" mixing artifacts from prior runs.** When a resumed session loaded `existing_agents` from `agent-runs/`, the in-memory list included peers that ran in EARLIER sessions but were narrowed out for THIS run. Frontend "Ultimo estado" summary then showed all 4 peers as if they had run, misleading the operator. New helper `filter_existing_agents_to_active_set(existing, active_agent_keys)` filters the recovered list to only include peers in this run's effective active set, normalizing aliases (`"Anthropic"`/`"Claude"` → `"claude"`, `"OpenAI"`/`"Codex"` → `"codex"`, etc.) to mirror `normalize_active_agents`. Older artifacts stay on disk under `agent-runs/`; only the in-memory snapshot used for status reporting is filtered.

### Added (testable helpers from B18/B19)
- `resolve_time_budget_anchor(created_at, is_resume, now)` — pure function; takes `now` as a parameter for deterministic testing.
- `filter_existing_agents_to_active_set(existing, active_agent_keys)` — pure function; alias normalization mirrors `normalize_active_agents`.

### Backend `ResumableSessionInfo` extensions (B17)
4 new fields populated by `inspect_resumable_session_dir` from the saved session contract:
- `saved_active_agents: Vec<String>`
- `saved_initial_agent: Option<String>` (filtered to non-empty)
- `saved_max_session_cost_usd: Option<f64>`
- `saved_max_session_minutes: Option<u64>`

### Frontend (B17)
- `ResumableSessionInfo` TypeScript type extended with the 4 fields.
- `startResumeSession`:
  - Validates `saved_active_agents` against `initialAgentOptions` keys; falls back to current React state if invalid.
  - Calls `setActiveAgents`, `setInitialAgent`, `setMaxSessionMinutes`, `setMaxSessionCostUsd` to keep UI in sync with the saved contract.
  - Builds `resumeRunOptions` synchronously from the validated saved contract values, bypassing the default React state.
  - Emits `session.resume.contract_applied` info NDJSON log entry with the applied snapshot.

### Validation
- `cargo test`: 73 passed (7 new + 66 existing). New: `resolve_time_budget_anchor_returns_now_when_resuming`, `resolve_time_budget_anchor_returns_created_at_when_fresh_start`, `filter_existing_agents_keeps_only_agents_in_active_set`, `filter_existing_agents_normalizes_agent_name_aliases`, `filter_existing_agents_returns_empty_when_active_set_is_empty`, `inspect_resumable_session_dir_reports_saved_active_agents_for_picker`, `inspect_resumable_session_dir_returns_empty_saved_when_contract_missing`.
- `npm run typecheck` clean.
- `npm run build` clean (~1s, 2434 modules).
- `cargo clippy --no-deps --all-targets`: 0 errors. v0.3.16's `#![deny(clippy::disallowed_methods)]` preserved.

### Notes
- `created_at` continues to anchor cumulative metrics (artifact age, cost ledger entries) and stays in the session contract.
- The 4 new `saved_*` fields in `ResumableSessionInfo` are additive; no existing field renamed or removed.
- `session.resume.contract_applied` is a new info-level NDJSON event; existing `session.editorial.active_agents_resolved` already logs the post-normalize effective values, so the new event is for "what the picker applied", complementing the existing "what the runtime resolved".

## [v0.3.17] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration order step 2 ("logging and path safety"). `lib.rs` is at 9759 lines pre-split; this batch trims it by 127 lines into a new `src-tauri/src/app_paths.rs` module (220 lines with doc comments).

### Changed (extracted to `src-tauri/src/app_paths.rs`)
- `APP_ROOT` `OnceLock<PathBuf>` static moved into `app_paths.rs`. `lib.rs::initialize_app_root` now calls `app_paths::try_set_app_root(...)`. Panic-record helper uses `app_paths::app_root_if_initialized()`.
- Path resolution: `app_root`, `resolve_portable_app_root`, `portable_root_from_exe_path`, `data_dir`, `logs_dir`, `human_logs_dir`, `human_log_path_for`, `config_dir`, `bootstrap_config_path`, `ai_provider_config_path`, `sessions_dir`.
- Boot-time path helpers: `early_logs_dir`, `active_or_early_logs_dir` (and new `app_root_if_initialized` returning `Option<PathBuf>` for the panic record).
- Path-safety primitives: `checked_data_child_path`, `is_safe_relative_data_path`, `is_safe_data_file_name`, `safe_run_id_from_entry`, `sanitize_path_segment`.

### Stayed in `lib.rs`
- `initialize_app_root` (touches `&tauri::App`, the runtime boundary).
- `install_process_panic_hook` and `write_early_crash_record` (compose JSON crash records that depend on `Utc`, `serde_json`, and the local `sanitize_text` redactor).
- `create_log_session` and the rest of the diagnostic logger (planned for the next split batch).

### Validation
- `cargo test`: 66 passed (no test count change; same surface — the `mod tests` block re-imports `config_dir`, `portable_root_from_exe_path` from `app_paths` to keep the cargo unused-import warnings clean in non-test builds).
- `npm run typecheck` + `npm run build`: clean.
- `cargo clippy --no-deps --all-targets`: 0 errors. The `#![deny(clippy::disallowed_methods)]` from v0.3.16 is preserved at lib.rs and main.rs.

### Notes
- `lib.rs` line count: 9759 → 9632.
- No public Tauri command name changed. No portable data path changed. No secret redaction logic touched.
- Per `docs/code-split-plan.md` migration order, the next batches will be: AI provider credentials/probes (step 3), Cloudflare D1 + Secrets Store (step 4), editorial orchestration + artifacts (step 5).

## [v0.3.16] - 2026-05-02

Patch release fechando 4 bugs reais surfaceados em logs de produção da v0.3.15 + 2 hardenings (NB-2/NB-5) + clippy hygiene (A+B do parecer pós-v0.3.15). Operator analisou `data/logs/maestro-2026-05-01T18-01-15Z-pid28124.ndjson` rodando v0.3.15 e confirmou: B11 ficou parcialmente casca-vazia + 3 novos bugs (B13/B14/B15). Codex emitiu parecer pedindo NB-2 + NB-5 prioritários.

### Fixed
- **B11 regressão de v0.3.15.** Operator confirmou: "selecionei apenas DeepSeek, mas todos rodaram." Root cause: `App.tsx:1661` (`startResumeSession`) chamava `runRealEditorialSession` com apenas 3 argumentos posicionais — `runOptions` ficava `undefined`, o resume invoke enviava `active_agents: null`, e o backend caía no saved_contract (que tinha os 4 originais). A v0.3.15 fechou o lado backend (`#[serde(default)]`, log `active_agents_resolved`) mas deixou o callsite frontend incompleto. v0.3.16 expande `startResumeSession` para invocar `currentSessionRunOptions()` e propagar `selectedInitialAgent` + `runOptions` de fato. Falha de validação (peers fora dos limites) bloqueia o resume com `setOperation({ status: 'blocked', ... })` em vez de avançar com estado inválido.
- **B13 PROVIDER_NETWORK_ERROR sem retry.** Logs mostravam todos os 4 peers timing out em ~30s sequencialmente, sem retry, perdendo rodadas inteiras a cada blip de rede. Novo helper `send_with_retry(log_session, run_id, provider_label, make_request)` em `lib.rs` envolve cada call site (DeepSeek, OpenAI, Anthropic, Gemini) com 1 retry pós-1.5s backoff em qualquer `Err` de `.send()`. Limita o desperdício a 3s extras por agente em pior caso, recupera transient flakiness em melhor caso. Logs `session.provider.retry_network` warn registram cada retry com `error_is_timeout` / `error_is_connect` flags do reqwest.
- **B14 HTTP 429 sem respeitar Retry-After.** Mesma sessão mostrava Claude retornando 429 às 18:03:10 e Maestro re-tentando 90 segundos depois → 429 novamente. `send_with_retry` agora detecta `response.status() == 429` e dorme por `parse_retry_after_header(headers)` (delta-seconds OU HTTP-date RFC 2822), default 30s, cap 120s. Logs `session.provider.retry_after_429` warn registram a fonte (`header` vs `default`) para auditoria post-hoc.
- **B15 loop infinito em rounds all-error.** Logs mostravam round 74, 75, 76 com todos os 4 peers em tone=error/blocked sem nenhuma escalada. Novo contador `consecutive_all_error_rounds` em `run_editorial_session_inner` incrementa quando `round_results.iter().all(|r| r.tone == "error" || r.tone == "blocked")` e reseta quando ao menos um peer foi ok/warn. Após 3 rounds consecutivos all-error, emite `session.escalation.all_peers_failing` (level=error NDJSON com peer statuses snapshot) e retorna `EditorialSessionResult` com status `ALL_PEERS_FAILING`, pausando a sessão para revisão do operator em vez de queimar quota e tempo.

### Added (Codex NB-2/NB-5 hardenings)
- **NB-2 finalize-running Drop guard.** `FinalizeRunningArtifactsGuard` (RAII) é instanciado no início de `run_editorial_session_inner` com `_finalize_guard = FinalizeRunningArtifactsGuard::new(agent_dir.clone())`. O `Drop` impl chama `finalize_running_agent_artifacts(&self.agent_dir)`, garantindo que `Status: RUNNING` placeholders sejam reescritos para `AGENT_FAILED_NO_OUTPUT` em todos os exit paths — incluindo `?` early returns e panics, que a hook em `editorial_session_result` da v0.3.15 não cobria. `finalize_running_agent_artifacts` permanece idempotente, então o duplo-passo no caminho normal (Drop + chamada explícita em `editorial_session_result`) é no-op no segundo. Teste novo `finalize_running_artifacts_drop_guard_runs_on_panic` usa `std::panic::catch_unwind` para provar que o Drop dispara mesmo em panic mid-session.
- **NB-5 spawn-funnel lint guard.** Novo `src-tauri/clippy.toml` com `disallowed-methods = [{ path = "std::process::Command::new", reason = "..." }]`. `lib.rs` ganha `#![warn(clippy::disallowed_methods)]` no topo. `hidden_command` (a única entrada legítima do funil editorial) e dois test fixtures recebem `#[allow(clippy::disallowed_methods)]` com comentário SAFE-FUNNEL. Qualquer futuro `Command::new` direto em `lib.rs` ou módulos relacionados dispara warn no `cargo clippy` (que já é gate). Substitui também o único `Command::new("xdg-open")` direto restante (linha 712 anterior) por `hidden_command("xdg-open")` para unificar o pattern em todos os OSes.

### Hygiene (advisor recommendation A+B aplicada)
- `manual_pattern_char_comparison` em `lib.rs:6840`: `matches!(char, '.' | ',' | ';' | ':')` colapsado para `['.', ',', ';', ':']` array literal.
- `if_same_then_else` em `lib.rs:5799-5811` (código próprio v0.3.15 do tone derivation B2): `timed_out` e `AGENT_FAILED_EMPTY/AGENT_FAILED_NO_OUTPUT` retornavam ambos "error" em arms separados; merged em uma única condição `||`. Sem perda de legibilidade.
- Clippy warnings: 19 → 17 (2 idiom fixes aplicados; demais 17 são `too_many_arguments` em código autoral de fases anteriores, deferred para o split planejado de `lib.rs` per `docs/code-split-plan.md`).

### Validation
- `cargo test`: **66 passed** (5 new + 61 existing). Novos: `finalize_running_artifacts_drop_guard_runs_on_panic`, `parse_retry_after_header_reads_delta_seconds`, `parse_retry_after_header_reads_http_date`, `parse_retry_after_header_returns_none_when_absent`, `parse_retry_after_header_returns_none_for_garbage`.
- `npm run typecheck`: clean.
- `npm run build`: clean (~1s, 2434 modules).
- `cargo clippy --no-deps`: 17 warnings (todos pré-existentes de `too_many_arguments` ou já anotados com `#[allow(clippy::too_many_arguments)]` em `build_active_agents_resolved_log_context`).

## [v0.3.15] - 2026-05-02

### Fixed — session-log-driven anti-"casca vazia" sweep (12 distinct bugs across run-2026-04-26)
The operator analyzed session `run-2026-04-26T19-28-26-698Z` (75 rounds, 34/219 READY, status blocked, infrastructure cascade collapse around round 072) and surfaced ten recurring failure modes. Two more were caught mid-fix: active-peers selection and session caps lacked an end-to-end verification trail (the operator-coined antipattern *casca vazia* — UI exists, background does not actually fire). v0.3.15 closes all twelve at the wiring level, with 12 new `#[test]` invariants proving each fix is functional, not a label change.

- **B1 — Gemini sandbox trust forced via env.** `--skip-trust` was already in `gemini_args()` but failed silently in some operator environments. Centralized `apply_editorial_agent_environment` helper now sets `GEMINI_CLI_TRUST_WORKSPACE=true` when the spawned binary stem is `gemini`, on top of the existing flag. Belt-and-suspenders.
- **B2 — DeepSeek/API empty-output gets a dedicated classifier.** Successful exit (`exit_code == 0`) with `stdout.trim().is_empty()` now produces `AGENT_FAILED_EMPTY` (tone `error`), not the pre-fix `NOT_READY` which masqueraded as a real editorial parecer. Same fix applied to the CLI path (covers all four agents, but DeepSeek is the most frequent victim because its API contract returns 200 OK with empty `choices[0].message.content` under quota/timeout). Tested via `nonzero_empty_review_with_success_exit_classifies_as_agent_failed_empty`.
- **B3 — Windows pipe error 109 surface and classification.** `read_pipe_to_end_counting_classified` returns both the bytes and an optional classification string; `classify_pipe_error` recognizes `windows_error_109_broken_pipe`, `windows_error_232_pipe_closing`, `windows_error_233_pipe_no_listener`, and the kind-only `broken_pipe`/`unexpected_eof`/`interrupted`/`timed_out` shapes. `TimedCommandOutput` carries `stdout_pipe_error` and `stderr_pipe_error`; the editorial agent artifact now includes a `Stdout pipe error` / `Stderr pipe error` diagnostic line when either is present, so what was previously a silent `Err(_) => break` is now operator-visible.
- **B4 — Codex stderr cap is tail-preserving.** `truncate_text_head_tail(value, head_chars, tail_chars)` keeps the head 1 KiB (preamble identifying the command) plus the tail 60 KiB (where the actual error message lives) with a `[... N chars truncated (head 1024 / tail 61440) ...]` marker between. Replaced the pre-fix `sanitize_text(&stderr, 8000)` call which truncated head-only and lost the tail with the actual ConstrainedLanguage / 429 / sandbox details.
- **B5 — Aggregator classification sweep.** `build_blocked_minutes_decision` and `parse_agent_artifact_result` now route `AGENT_FAILED_EMPTY` and `EMPTY_DRAFT` through the operational-failures branch alongside `AGENT_FAILED_NO_OUTPUT` and `EXEC_ERROR_*`. The `tone` derivation in the artifact-result parser also flips both new statuses to `error`. Pre-fix, `EMPTY_DRAFT` got tone `error` but was skipped by the minutes filter; the aggregator under-reported operational failures.
- **B6 — RUNNING perpetual-state finalization.** `finalize_running_agent_artifacts(agent_dir)` is invoked once at the start of `editorial_session_result(...)` (last common path before the result struct is built). It scans `*.md` artifacts in `agent-runs/` and, for any file still containing the `Status: \`RUNNING\`` placeholder, rewrites it to `AGENT_FAILED_NO_OUTPUT` and appends a one-line note explaining the finalization sweep. No timeout is reintroduced (operator's deliberate v0.3.1 design preserved); this is purely state-cleanup at session end. Tested via `finalize_running_agent_artifacts_rewrites_running_to_failed_no_output`.
- **B7 — Pipe-reader UTF-8 forcing.** Verified the protocolo on disk is valid UTF-8 (no BOM, accents `Versão`/`vigência` display correctly via PowerShell `Get-Content -Encoding UTF8`); the corruption observed in past sessions came from child-process pipe encoding under Windows code page 1252. `apply_editorial_agent_environment` now sets `PYTHONIOENCODING=utf-8`, `PYTHONUTF8=1`, `LC_ALL=C.UTF-8`, `LANG=C.UTF-8` for every spawned editorial CLI (Claude, Codex, Gemini, DeepSeek wrapper). Existing `String::from_utf8_lossy` decoding remains the safety net for non-conformant emitters.
- **B8 — Resume preserves `session-contract.created_at`.** Root cause: `SessionContract`'s `links` and `attachments` fields were required `Vec<...>` without `#[serde(default)]`, so any older contract that pre-dated those fields failed to deserialize, `load_session_contract` returned `None`, and `created_at` fell back to `Utc::now()` — overwriting the original session start time. Fix: `#[serde(default)]` on `active_agents`, `initial_agent`, `max_session_cost_usd`, `max_session_minutes`, `links`, `attachments`, plus a new `default_session_contract_schema_version` for `schema_version`. `load_session_contract` now logs parse failures via `eprintln!` instead of swallowing them. Tested via `session_contract_loads_legacy_payload_without_links_attachments`.
- **B9 — Persistent divergence detection (partial).** New `agent_review_fingerprints: BTreeMap<agent_name, Vec<u64>>` carries a per-agent ring buffer of the last three review fingerprints across rounds. `review_complaint_fingerprint(artifact)` extracts the `## Stdout` block, collapses whitespace, and hashes the first 1024 chars to a stable `u64`. When an agent has 3 consecutive identical fingerprints AND status remains non-READY, a `session.divergence.persistent` warn-level NDJSON event is emitted with the agent, round, status, and fingerprint. Marked **partial**: surfaces the deadlock to the operator; full auto-resolution (escalate-to-operator, force-vote, inject-mediator) is a session-contract amendment deferred to a later release. Tested via `review_complaint_fingerprint_stable_across_whitespace_normalization` and `review_complaint_fingerprint_differs_on_distinct_complaints`.
- **B10 — Default session caps placeholders.** UI placeholders changed from generic "ignorar" to concrete suggestions: `60 (em branco = sem teto)` for max minutes, `5.00 (em branco = sem teto)` for max USD, with tooltips explaining that minutes is checked between rounds + per-spawn timeout, and USD only applies to API peers (CLI peers are subscription-billed). Schema still allows null. Pre-existing wiring in `session_time_exhausted` and `provider_cost_guard_for` confirmed not casca-vazia via grep audit (60+ call sites).
- **B11 — `active_agents` selection wiring (newly identified).** Operator reported "fiz o teste com apenas um agente, e o app chamou todos." Root cause: the resume request from `App.tsx` did not forward `active_agents`, so the backend always fell back to the saved contract (which captured all four agents on first start). Fix: frontend now passes `active_agents`, `max_session_cost_usd`, `max_session_minutes`, `attachments`, `links` through the resume invoke alongside the start invoke. Backend writes a new `session.editorial.active_agents_resolved` log entry recording `active_agents_requested`, `active_agents_saved_contract`, `active_agents_effective`, and `active_agents_source ∈ {request, saved_contract, default_all}`, plus the same shape for `max_session_cost_usd_*` and `max_session_minutes_*`. The operator can now audit post-hoc whether the runtime honored the UI selection.
- **B12 — Cost/time controls visibility (operator hypothesis).** The operator suspected these were also casca vazia. Audit confirmed they ARE wired (60+ call sites of `session_time_exhausted` between rounds, `remaining_session_duration` per spawn, `provider_cost_guard_for` before each API spawn, `COST_LIMIT_REACHED` returned and propagated). The fix here is the same `active_agents_resolved` log entry plus B10's UI clarification — both make the runtime decision auditable.

### Diagnostic surface
- New `eprintln!("session_contract_parse_failed path=... error=...")` in `load_session_contract` so silent schema drift never recurs.
- New NDJSON category `session.editorial.active_agents_resolved` records the full resolution trail for active_agents and both caps.
- New NDJSON category `session.divergence.persistent` (warn level) signals 3-round repeat NOT_READY rebuttals per agent.
- Editorial agent artifacts now include `Stdout pipe error` / `Stderr pipe error` lines when pipe reads classified anything other than clean EOF.

### Validation
- `cargo test` — 61 passed, 0 failed (20 new + 41 existing); no flakes.
- `npm run typecheck` — clean.
- `npm run build` — `tsc --noEmit && vite build`, ~1.7s, 2434 modules transformed (pre-existing PostEditor chunk-size warning unchanged).
- Manual verification of B7 on-disk encoding: PowerShell `Get-Content protocolo.md -Encoding UTF8 | Select-String "Versão|vigência"` displays accents correctly, magic bytes confirm no BOM (`23 20 50 72 6F 74 6F 63` = `# Protoc...`).
- **Cross-review-v2 quadrilateral CONVERGED `unanimous_ready` in R1 of session `1f259a0e-00aa-42d2-aec6-5e32278484ab`** with caller=claude and peers=codex+gemini+deepseek (codex emphasis per operator). Two prior cycles (sessions `52f03bd1` and `176ee784`) had legitimate NEEDS_EVIDENCE / NOT_READY blockers that were closed via real code work each time, not by re-asserting: repo-wide spawn-primitive grep confirming `resolved_command_builder` is the only editorial spawn path (closes earlier "lib.rs-only grep" gap), `resolve_effective_active_agents` extracted as a unit-testable helper with 5 direct tests covering request-overrides-saved / saved-fallback / both-missing / empty-saved-recovery / explicit-empty-rejection, then a 6th test for explicit-empty-with-saved (codex/deepseek R1 BL-2), then `build_active_agents_resolved_log_context` extracted as a pure function so the runtime and tests share a single NDJSON payload source (codex/deepseek R1 BL-1) with 2 shape tests pinning all 13 fields plus the three resolution-source variants (`request`/`saved_contract`/`default_all`/`unset`).

### Helper extractions (anti-drift)
- `resolve_effective_active_agents(request: Option<&Vec<String>>, saved: Option<&Vec<String>>) -> Result<(Vec<String>, &'static str), String>` — single source of the resume contract decision tree. Called from `run_editorial_session_inner`. Handles legacy contract recovery: empty saved Vec falls through to default_all instead of erroring (pre-fix would Err).
- `build_active_agents_resolved_log_context(...) -> serde_json::Value` — single source of the `session.editorial.active_agents_resolved` NDJSON payload shape. Called from `run_editorial_session_inner`; both runtime emission and tests consume the same builder, so payload drift is impossible.

## [v0.3.14] - 2026-05-01

### Added — rigorous security/UX audit closure (parity with admin-app v02.00.00 / mainsite-app v02.18.00)
- Top-level `ErrorBoundary` class component (`src/components/ErrorBoundary.tsx`) wired in `main.tsx` around `<App />`. Pre-fix, `installGlobalDiagnostics()` only captured `window.error` and `unhandledrejection` — both fire AFTER React's reconciler. Render-phase exceptions (throw inside JSX, useState selectors, component init) were silently unmounted by React, blanking the webview with no diagnostic trail. The boundary is strictly additive: it forwards captured exceptions to the SAME `logEvent({ level: 'error', ... })` NDJSON channel, so the audit trail stays single-source. React 19 still requires a class component for `componentDidCatch`.
- `useEscapeKey` hook (`src/hooks/useEscapeKey.ts`) — verbatim port from admin-app v02.00.00. Wired in two custom-portal dialogs that lacked ESC dismissal (Radix-built dialogs, `SearchReplacePanel`, and `SlashCommands` already had ESC):
  - `src/editor/posteditor/editor/PromptModal.tsx`: hook called BEFORE the early `return null` to satisfy Rules of Hooks; `enabled = modal.show` keeps the listener detached when hidden. Mirrors the existing Close button (line 43); no new dismissal path.
  - `App.tsx` `ResumeDialog` block (around lines 2588–2640): in-place edit per `docs/code-split-plan.md` ("future splits should start with pure helpers... without mixing large refactors with behavior changes"). Mirrors the existing Close button at the dialog header — same dismissal semantics, no UX-intent change.

### Calibrated out (advisor catch — regression risk > benefit)
- `Promise.race` timeout on direct-API peers — direct-API editorial calls already have explicit per-session deadlines and 2-retry × 800ms structure; a short blanket timeout would regress legitimate long-wait operator flows.
- `EnvSecretsSchema` Zod migration — `readSecretString` + secret-store routing in `lib.rs` is functional; adding Zod is preference, not fix.
- TLS cert pinning on `reqwest` — relies on system trust store via `rustls-tls`; pinning is engineering preference, not a fix in single-operator desktop context.
- Plaintext credential JSON encryption — operator already has the Cloudflare Secrets Store opt-in for keys (v0.3.11); local plaintext is a known design with OS file-permission fallback. Encrypting at rest needs master-password UX, out of scope for this cycle.

### Validation
- `npm run build` — `tsc --noEmit && vite build` — 786 ms, 2434 modules transformed (pre-existing PostEditor chunk-size warning unchanged).
- `cargo check --locked --manifest-path src-tauri/Cargo.toml` — clean (49.65s).

## [v0.3.13] - 2026-05-01

- Added per-session editorial controls: selectable active peers (1-4), optional time limits, optional direct-API cost limits, prompt attachments, and public source links.
- Added real direct-API editorial runners for OpenAI/Codex, Anthropic/Claude, Google/Gemini, and DeepSeek, with API-only mode no longer falling back to CLIs for non-DeepSeek peers.
- Added a UI-managed provider tariff table for per-million token rates. The session still has one optional USD cost limit; provider tariffs calculate/audit observed API usage and any API peer is blocked with a friendly message until its provider input/output rates are configured.
- Added native attachment delivery for supported direct-API providers: OpenAI receives `input_image`/`input_file`, Anthropic receives image/document content blocks, and Gemini receives `inline_data` parts. Unsupported or native-size-exceeding attachment types remain available through manifest and bounded text previews.
- Added pre-run attachment delivery hints in the session UI, showing per active provider whether each file is expected to be sent natively to OpenAI/Anthropic/Gemini or kept as manifest/previews only.
- Added a human-readable log projection under `data/logs/human/` with additive NDJSON fields, concise summaries, and heartbeat collapse so raw structured logs remain machine-readable without forcing operators to read giant JSON lines.
- Persisted session contracts, attachment manifests, link artifacts, and cost ledgers under each ignored `data/sessions/<run>/` folder.
- Started the conservative code-splitting pass: moved human-log and session-control helpers out of `src-tauri/src/lib.rs`, and changed the Tiptap-heavy PostEditor parity surface to load only after `Criar Post`, reducing the production entry chunk from about 1.30 MB to about 272 KB minified.
- Cross-review for the native attachment continuation converged in `cross-review-v2` session `00b642c0-9f7a-4b85-a1cb-f04384548d61` after round 3 with Claude, Gemini, and DeepSeek READY.
- Cross-review for the per-provider attachment delivery UI follow-up converged in `cross-review-v2` session `121cbad3-81fd-4c84-928f-7e30bfdd5d88` after round 2 with Claude, Gemini, and DeepSeek READY.

## [v0.3.12] - 2026-04-30

- `README.md` now follows the shared organizational opening pattern adopted across the public repositories, while preserving Maestro's Windows/Tauri, logging, and editorial-runtime specific documentation.

## [v0.3.11] - 2026-04-28

- Added DeepSeek as a real API-backed editorial peer: it can be selected as draft lead, participates in review/revision rounds, writes normal session artifacts, and selects the best available authenticated DeepSeek `/models` entry, with `deepseek-v4-pro` preferred when exposed.
- Added DeepSeek credential storage and real model-list verification alongside OpenAI, Anthropic, and Gemini.
- Improved Cloudflare Secrets Store reload behavior: Maestro now restores remote secret references from the local marker and `maestro_db` metadata, while clearly treating raw Secret Store values as non-readable by the desktop app.
- Added the initial `docs/code-split-plan.md` roadmap for splitting the growing Rust and React surfaces without mixing refactor work with behavior changes.

## [v0.3.10] - 2026-04-28

- Fixed broken/paused session display overflow by making the app shell viewport-bound, moving page overflow to the workspace, bounding all repeated status lists, and summarizing agent history instead of rendering every historical agent result as visible UI.
- Made visible session logs more human-readable: the UI now shows a concise latest-round summary while the detailed NDJSON and agent artifacts remain technical for diagnosis.
- Changed the editorial orchestrator so review operational failures no longer end the session as paused; they are logged, fed into the next revision attempt, and the cycle continues until unanimous READY.
- Kept sessions running when no revision agent produces a usable draft by retrying the next review round with the current draft instead of returning a blocked result.
- Reduced CLI pipe failures on very large rounds by writing oversized agent prompts to ignored sidecar input files in `data/sessions/<run>/agent-runs/` and sending the CLIs a compact instruction to read the local file.
- Reduced revision-prompt bloat by passing useful review stdout excerpts instead of whole diagnostic artifacts.
- Fixed Cloudflare Secrets Store upsert when a provider secret already exists beyond the first paginated API page, and added a retry path for `secret_name_already_exists`.

## [v0.3.9] - 2026-04-28

- Fixed Cloudflare API credential persistence so `credential_storage_mode=cloudflare` writes AI provider keys to Cloudflare Secrets Store and stores only non-secret markers/metadata locally and in D1.
- Reused the existing Cloudflare Secrets Store when the account already has one, without renaming it, and linked the effective store plus secret references in `maestro_db`.
- Reclassified empty/failed agent review artifacts as operational failures instead of treating them as usable editorial review notes.
- Kept the session activity feed bounded with internal scrolling so long runs no longer stretch the whole app vertically.
- Expanded `ata-da-sessao.md` blocked decisions with concrete operational failures and editorial divergences.

## [v0.3.8] - 2026-04-28

- Added operator choice for the editorial draft lead. Claude, Codex, or Gemini can now be selected before a session starts; the selected agent is saved with the session, writes the first version, and leads revision fallback order instead of Claude being hardcoded first.
- Isolated editorial CLI child processes inside the session `agent-runs` folder instead of the portable app root, preventing stray files such as root-level `draft.md` from looking like approved final output.
- Clarified the UI and logs around the selected draft lead while preserving the rule that `texto-final.md` is created only after unanimous review approval.
- Expanded `.gitignore` for root-level runtime draft spills and `.tmp/` working directories.
- Enabled automatic release publication from `main`: the release workflow now derives `vX.X.X` from `package.json`, refuses to reuse an existing tag, creates the GitHub Release for the merge commit, and keeps the existing tag/manual release paths.
- Bumped app metadata to `v0.3.8` for the draft-lead and release-automation build.

## [v0.3.7] - 2026-04-27

- Closed the CodeQL `rust/path-injection` findings opened after v0.3.6 by changing the portable app-root regression test to use the current test executable path instead of creating and deleting a dynamic temporary directory.
- Preserved the v0.3.6 runtime startup fix and early crash logger unchanged.
- Replaced the sidebar brand subtitle `Windows 11+ portable` with the current app version label in `vX.X.X` format, sourced from `package.json`.
- Made the API provider settings real: Maestro now loads and saves `data/config/ai-providers.json`, exposes a visible `Salvar APIs` action, and verifies OpenAI, Anthropic, and Gemini credentials against their model-list endpoints without logging raw keys.
- Changed Cloudflare token validation into a provisioning action: `Verificar e preparar` now creates missing `maestro_db`, initializes Maestro tables, reuses an existing account Secrets Store when the plan allows only one, and creates `maestro` only when no store exists and the token has permission.
- Replaced UI-only buttons with real actions for runtime revalidation, agent CLI checks, opening the session minutes file, and link auditing through the native backend.
- Added in-flight editorial diagnostics: each CLI execution now writes a parseable `RUNNING` artifact before launch, logs the child PID after spawn, and records native `session.agent.running` heartbeats with elapsed time plus stdout/stderr byte counters while long agent calls are still active.
- Hardened interrupted-session resume so an unfinished `RUNNING` revision does not become the latest usable draft.
- Hardened CLI startup failure handling so a child process spawned for an editorial agent is killed and reaped if stdin delivery fails before the agent can begin work.
- Sanitized provider API network errors so failed Gemini/OpenAI/Anthropic verification cannot echo request URLs containing API keys.
- Hardened the link-audit public URL gate against private, reserved, documentation, multicast, CGNAT, metadata, IPv6 loopback/link-local/ULA, and IPv4-mapped or IPv4-compatible IPv6 targets while avoiding false positives on public hostnames that merely contain private-looking numbers.

## [v0.3.6] - 2026-04-26

- Fixed a startup crash in the Windows portable build by resolving Maestro's app folder from the running executable instead of the Tauri `BaseDirectory::Executable` resolver, which returned `unknown path` on this environment.
- Added an early native panic/crash logger that writes `data/logs/maestro-crash-*.json` even when the normal NDJSON logger has not completed startup yet.
- Added regression tests for portable app-root resolution and early crash-log writing, then verified the rebuilt release executable stays alive and creates a per-execution NDJSON log.

## [v0.3.5] - 2026-04-26

- Hardened resumable session filesystem scans so session folders and agent artifacts are reconstructed only from validated safe names.
- Replaced recursive markdown/activity discovery with known session artifact accounting to remove CodeQL `rust/path-injection` findings without weakening the resume feature.
- Counted timestamped `protocolo-anterior-*.md` backups through a safe single-level scan and added regression tests for canonical artifact names, dotted session folders, and protocol-backup accounting.

## [v0.3.4] - 2026-04-26

- Reworked the running session status card to use an indeterminate activity meter instead of an artificial completion percentage.
- Changed long-running session time display to `hh:mm:ss` and Brazilian date/time formatting with the Brasilia timezone.
- Humanized visible session, agent, phase, and review labels so the UI no longer exposes internal status codes or development-only wording.
- Added native log-write locking so concurrent agent events cannot interleave and corrupt NDJSON lines.
- Added session resume support: Maestro reads interrupted work under `data/sessions/`, auto-resumes when only one session is available, asks the operator when several sessions exist, and can resume with a newly loaded protocol or the protocol saved in the session.

## [v0.3.3] - 2026-04-26

- Hardened native filesystem access so logs, bootstrap configuration, and editorial session artifacts are validated against a canonical Tauri-resolved app root and confirmed as children of Maestro's local `data/` directory before reads or writes.
- Sanitized editorial `run_id` values into safe path segments before creating session folders, preventing path traversal through session artifact names.
- Updated GitHub release attestation action to `actions/attest-build-provenance@v4.1.0` through the merged Dependabot PR.

## [v0.3.2] - 2026-04-26

- Transferred the public repository from `lcv-leo/maestro-app` to `lcv-ideas-software/maestro-app`.
- Updated README workflow badges, GitHub Pages funding link, site organization links, and release engineering docs for the organization namespace.
- Bumped app/package metadata so the post-migration release verifies GitHub Releases and GHCR publication under the organization owner.

## [v0.3.1] - 2026-04-26

- Fixed Windows child-process spawning so CLI probes, registry env-var reads, `.cmd` shims, PowerShell wrappers, and editorial agents run with `CREATE_NO_WINDOW` and no visible terminal windows.
- Moved long editorial execution onto a native background worker so the Tauri/WebView UI remains responsive while agents run.
- Removed editorial-agent timeouts: real drafting, revision, and review calls may take as long as needed under the inviolable unanimity rule.
- Split startup bootstrap into fast config/env loading plus background dependency preflight so app startup no longer waits for every CLI version probe.
- Added visible no-timeout heartbeat updates while an editorial session is still running, including elapsed time and diagnostic log events.
- Changed non-unanimous or operationally incomplete sessions from final-abort semantics to paused/no-final-delivery semantics.
- Added fallback draft generation across Claude, Codex, and Gemini if the first draft agent fails to produce usable text.
- Added multi-round review/revision behavior: `NOT_READY` review output feeds a new revision round instead of ending the task as failed.
- Updated session minutes and UI language so divergence means continuing/paused work, while `texto-final.md` is still created only after unanimous `READY`.

## [v0.3.0] - 2026-04-26

- Replaced the UI-only CLI smoke path with a real first-pass editorial session command that runs Claude for draft generation and Claude, Codex, and Gemini for review in background.
- Added strict protocol text gating: sessions now block if the full Markdown protocol content was not imported, instead of proceeding with metadata only.
- Added session artifacts under ignored `data/sessions/<run>/`: `prompt.md`, `protocolo.md`, agent run outputs, `ata-da-sessao.md`, and `texto-final.md` only when unanimity is reached.
- Expanded NDJSON diagnostics with frontend runtime context, UI/network/visibility events, console warn/error capture, native panic capture, native log sequence, resolved command paths, and per-agent start/finish records.
- Improved diagnostic redaction so logs keep safe metadata such as token presence, env-var name, source, and scope while continuing to redact raw tokens, API keys, authorization values, cookies, and private material.
- Added live running/completed UI states and agent progress styling so long background operations no longer look static.
- Added Rust coverage for diagnostic redaction rules to protect against future regression.
- Removed previously tracked local AI handoff files from the repository index while preserving them on disk, and expanded `.gitignore` so local AI memory/instruction folders stay out of the public remote.
- Adjusted the Codex CLI adapter away from stdin-only `-` mode to a short prompt argument plus appended stdin payload, avoiding hangs observed in local headless probes.
- Fixed cross-review blockers before release: failed draft generation now stops before reviewer calls, command stdout/stderr are drained concurrently to avoid pipe-buffer stalls, native redaction now matches secret-shaped values inside URLs/JSON/header text, and the UI starts with no protocol loaded until a real Markdown import occurs.

## [v0.2.1] - 2026-04-26

- Fixed Windows CLI preflight detection for npm-style `.cmd` shims and known user install paths so Codex, Gemini, npm, and similar CLIs are not incorrectly shown as missing when they are installed.
- Added real Cloudflare credential validation from the settings screen, including token verification, account reachability, D1 database listing, and Secrets Store reachability without logging raw tokens.
- Fixed Cloudflare token verification for account-owned `cfat_` tokens by using `/accounts/{account_id}/tokens/verify`; user tokens continue to use `/user/tokens/verify`.
- Added Cloudflare env-var scope reporting so the UI distinguishes process, user, and machine environment sources.
- Expanded native, frontend, and CI secret redaction/detection to include Cloudflare account-token and global-key prefixes.
- Updated release engineering policy and end-user instructions to treat beta as a tag suffix (`vX.X.X-betaN`) instead of GitHub prerelease mode.
- Fixed the repository hygiene secret-shape scanner to avoid false positives on normal identifiers such as `cloudflare_persistence_database` while preserving common token/key detection.
- Fixed GitHub Release note generation so Markdown code spans are not treated as shell command substitutions and existing releases receive refreshed notes when the release workflow is rerun.
- Removed GitHub prerelease publishing entirely: beta builds must use explicit `vX.X.X-betaN` tags while still being published as normal GitHub Releases.

## [v0.2.0] - 2026-04-26

- Updated the release artifact upload/download actions to Node 24-capable versions to remove GitHub Actions Node 20 deprecation warnings.
- Expanded the GitHub Release notes template so future releases include download, verification, scope, and current-limit sections.
- Removed the empty Windows console window from release builds by setting the Rust Windows subsystem to `windows`.
- Replaced static/fake session status with a live session monitor that changes state after prompt submission, records visible preflight stages, and explicitly blocks delivery when the real Claude/Codex/Gemini adapters are not connected.
- Made the sidebar navigation functional so session, protocols, evidence, agents, settings, and setup render as separate app sections instead of dumping all configuration panels on the main screen.
- Changed diagnostic logging to create one NDJSON log file per app execution, with a per-run `log_session_id` recorded in each event.
- Added button hover, active, focus, and busy animations so operator actions have immediate visual feedback.
- Added `data/config/bootstrap.json` as the local non-secret pointer file that tells Maestro which persistence backend to use on each app start.
- Added automatic Cloudflare env-var discovery for account ID and API token presence without exposing token values in the UI or logs.
- Added native dependency preflight for Setup so the app reports detected CLI/runtime versions instead of static checklist text.
- Added `LEIAME.md` to the portable release package with first-run configuration, Cloudflare bootstrap, env-var, and logging instructions.

## [v0.1.0] - 2026-04-26

- Initial architecture planning for a portable Windows editorial workbench.
- Added day-zero repository hygiene, CodeQL, CI hygiene checks, Dependabot scaffold, and private protocol ignore policy.
- Added GitHub release engineering plan, governance docs, generated release-note configuration, and Windows 11+ modern stack baseline.
- Added minimal TypeScript project marker so CodeQL has a source tree to analyze before the app scaffold lands.
- Switched CodeQL to GitHub Default Setup only, added modern GitHub Pages artifact deployment, and enabled GitHub Sponsors funding metadata.
- Added the first Tauri/React scaffold and structured NDJSON diagnostic logging under ignored `data/logs/`.
- Added the first background-orchestration UI concept with friendly progress indicators and selectable interface verbosity.
- Added editorial session workflow documentation for prompt intake, full protocol reading, trilateral unanimity, final Markdown, and session minutes.
- Added MainSite compatibility and Cloudflare D1 import/export planning for `bigdata_db.mainsite_posts`.
- Added the integrated editor decision: Maestro uses PostEditor parity over generic TipTap so functionality and final HTML match the existing MainSite editor.
- Added a Maestro-local PostEditor parity module copied from the current `admin-app/MainSite/PostEditor` support surface.
- Added Web Evidence Engine planning for fetch, curl-compatible replay, web search, rendered collection, and human-assisted browser capture for CAPTCHA/login/consent workflows.
- Added ABNT Citation Engine planning and defined Maestro as a deterministic fourth peer that can block final delivery independently of the three AI agents.
- Added Link Integrity Engine planning for link extraction, checking, sanitization, correction proposals, and cross-review escalation.
- Reinforced the inviolable unanimity rule: no final text is delivered while any peer divergence remains, regardless of time, cost, or round count.
- Added Runtime Bootstrapper planning for first-run dependency checks, authorized background installation/update, CLI configuration, and authentication flows inside Maestro UI.
- Added Cloudflare credential settings planning with account ID/API token fields, permission validation, API-first D1 operations, and Wrangler fallback.
- Added official AI provider API/SDK credential planning so Claude, Codex/OpenAI, and Gemini can run through CLI, API, or hybrid transports.
- Added operator-selectable credential storage planning for encrypted vault, Windows user environment variables, or warned local JSON config.
- Established the project versioning and changelog convention as `vX.X.X`.
- Required every Wrangler fallback invocation to use `wrangler@latest`, with automatic update authorization when the fallback CLI path is needed.
- Added a CLI Agent Audit for Codex, Claude, and Gemini, including official documentation links, local headless smoke results, structured-output risks, and adapter hard gates.
- Enabled GitHub Pages first-run provisioning through `configure-pages` and Dependabot Cargo updates now that the Tauri manifest exists.
- Added a tag-driven release workflow that builds a portable Windows ZIP, publishes GitHub Releases, mirrors the release archive to GitHub Packages through GHCR/OCI, and avoids NuGet/installer distribution.
- Triaged the initial Rust Dependabot alerts, constrained Tauri features for the Windows 11+ target, removed unnecessary X11 dependencies from the lockfile, and documented the remaining transitive Tauri risk register.
- Added the three-mode configuration persistence contract: JSON local, Windows env-var hybrid, and Cloudflare remote persistence through D1 `maestro_db` plus Cloudflare Secrets Store.
