# Maestro Code Split Plan

Status: planning baseline for the v0.4.x stabilization line.

Maestro is now large enough that feature work in single files increases review cost and regression risk. The first split must be conservative: move code along existing responsibility boundaries, preserve behavior, and keep tests green after each small extraction.

Progress on 2026-05-01: the first split batch extracted `human_logs.rs` and `session_controls.rs` from the native `lib.rs`, then moved the Tiptap-heavy PostEditor parity surface behind a `React.lazy` boundary that is rendered only after the operator clicks `Criar Post`. The production entry chunk dropped from about 1.30 MB to about 272 KB minified; the remaining large PostEditor chunk is intentionally isolated for on-demand loading.

## Current Pressure Points

- `src-tauri/src/lib.rs` mixes Tauri command registration, runtime bootstrap, logging, Cloudflare provisioning, credential persistence, AI provider probes, editorial orchestration, link audit, session resume, artifact parsing, and tests.
- `src/App.tsx` mixes global app state, navigation, orchestration UI, protocol UI, settings UI, Cloudflare UI, provider credentials UI, helpers, and rendering.
- New peers such as DeepSeek add provider-specific behavior that should not live beside unrelated Cloudflare or session code.

## Rust Module Target

Recommended backend layout:

```text
src-tauri/src/
  lib.rs
  app_paths.rs
  logging.rs
  bootstrap.rs
  credentials/
    mod.rs
    ai_providers.rs
    cloudflare.rs
    windows_env.rs
  cloudflare/
    mod.rs
    d1.rs
    secrets_store.rs
    probes.rs
  editorial/
    mod.rs
    agents.rs
    artifacts.rs
    orchestration.rs
    prompts.rs
    resume.rs
  providers/
    mod.rs
    deepseek.rs
    cli.rs
  link_audit.rs
  import_export/
    mod.rs
    markdown.rs
    pdf.rs
    mainsite_d1.rs
```

`lib.rs` should become the Tauri boundary: command exports, setup hooks, panic/crash guard, and module wiring only.

## Frontend Module Target

Recommended frontend layout:

```text
src/
  App.tsx
  app/
    state.ts
    types.ts
    formatters.ts
  components/
    Shell.tsx
    StatusPanel.tsx
    ActivityLedger.tsx
  features/
    session/
    protocols/
    evidence/
    agents/
    settings/
    setup/
  services/
    tauri.ts
    logs.ts
```

`App.tsx` should become route/state composition, not the home of every screen.

## Migration Order

1. Extract pure helpers and types first.
2. Extract logging and path safety next, because they are used everywhere and already have tests.
3. Extract AI provider credentials/probes, including DeepSeek.
4. Extract Cloudflare D1 and Secrets Store operations.
5. Extract editorial orchestration and artifacts.
6. Split React settings/setup screens into feature components.
7. Add focused unit tests around each extracted module before changing behavior again.

## Completed Split Batches

- 2026-05-01: extracted native human-log projection helpers into `src-tauri/src/human_logs.rs`.
- 2026-05-01: extracted selected-peer, optional limit, and provider cost helpers into `src-tauri/src/session_controls.rs`.
- 2026-05-01: converted the MainSite-compatible PostEditor parity surface from a static `App.tsx` import into an on-demand lazy import opened by `Criar Post`, matching the admin-app loading pattern.
- 2026-05-02 (v0.3.17): extracted `APP_ROOT` static + 18 path resolution and safety helpers (`app_root`, `data_dir`, `sessions_dir`, `checked_data_child_path`, `sanitize_path_segment`, `is_safe_data_file_name`, `safe_run_id_from_entry`, etc.) into `src-tauri/src/app_paths.rs`. `initialize_app_root` (Tauri-bound) and the panic/crash record helpers stayed in `lib.rs` because they touch the runtime or compose JSON records using `Utc`/`serde_json`/`sanitize_text`. lib.rs went from 9759 → 9632 lines. Migration order step 2 partially complete; logging extraction (`logging.rs`) is the next target for the same step.
- 2026-05-02 (v0.3.19): extracted `NATIVE_LOG_SEQUENCE` static + `LogSession`/`LogEventInput`/`LogWriteResult` structs + `create_log_session` factory + `write_log_record` (with NDJSON record schema v2 + human-log projection companion call) into `src-tauri/src/logging.rs` (157 lines with doc comments). Tauri command shells (`runtime_profile`, `write_log_event`), panic/crash record helpers (`install_process_panic_hook`, `write_early_crash_record`), and `log_editorial_agent_*` helpers stayed in `lib.rs` (panic helpers compose a separate JSON schema and tie to process panic state; editorial helpers depend on `EditorialAgentResult` and move with the editorial orchestration batch). Sanitization helpers (`sanitize_short`, `sanitize_value`) upgraded from `fn` to `pub(crate) fn` for cross-module access; `sanitize_text` was already `pub(crate)`. Migration step 2 ("logging and path safety") is now COMPLETE for the items that don't have orchestration coupling. lib.rs went from 9930 → 9873 lines.
- 2026-05-02 (v0.3.20): extracted shared provider HTTP networking primitives into `src-tauri/src/provider_retry.rs` (137 lines with doc comments): `build_api_client(timeout)` factory, `send_with_retry<F>` retry policy with Retry-After respect, `parse_retry_after_header` (RFC 7231 + RFC 2822), and 4 `PROVIDER_RETRY_*` constants. The 4 provider runner functions (`run_deepseek_api_agent`, `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent`) stayed in `lib.rs` because each has provider-specific request body shapes and response parsers; they move in v0.3.21+ batches. Migration step 3 ("AI provider credentials/probes") partially complete; runner-by-runner extraction is the next target. lib.rs went from 9873 → 9776 lines.
- 2026-05-02 (v0.3.23): extracted the Cloudflare/D1/Secrets Store surface into `src-tauri/src/cloudflare.rs` (~960 lines with doc header). Migration step 4 ("Cloudflare D1 and Secrets Store operations") underway. Extracted: HTTP layer (`cloudflare_get`/`post_json`/`patch_json`/`get_paginated_results`/`client`/`page_path`/`verify_path`/`token_kind`/`error_summary`), token resolvers (`token_from_probe_request`, `token_source_label`, `cloudflare_token_from_provider_request`), JSON helpers (`cloudflare_result_names`, `cloudflare_result_id_for_name`, `cloudflare_store_records`, `cloudflare_store_for_target_or_existing`, `cloudflare_secret_ids_by_name`, `cloudflare_secret_id_from_response`, `cloudflare_created_result_id`, `CloudflareStoreRecord`), D1+Secrets ensure logic (`ensure_cloudflare_d1_database`, `ensure_cloudflare_secret_store`, `provision_maestro_d1_schema`, `link_secret_store_reference`), AI provider bridge (`ai_provider_secret_values`, `upsert_ai_provider_secrets`, `write_ai_provider_metadata_to_cloudflare`), probe entry (`run_cloudflare_probe`, `probe_row`). Visibility upgrades in lib.rs: `env_value_with_scope` + 4 Cloudflare* structs (CloudflareProbeRequest/Row/Result + CloudflareProviderStorageRequest) all marked `pub(crate)` along with their fields. Tests in `lib.rs::tests` re-import `cloudflare_page_path`/`cloudflare_store_for_target_or_existing`/`cloudflare_verify_path` via `use crate::cloudflare::*` block. Stayed in lib.rs: `cloudflare_env_snapshot`/`verify_cloudflare_credentials` (Tauri command boundary), AI provider <-> Cloudflare bridges (`persist_ai_provider_cloudflare_marker`, `persist_ai_provider_config_to_cloudflare`, `enrich_ai_provider_config_from_cloudflare`, `read_ai_provider_cloudflare_metadata`). Followed v0.3.22's "delete first, edit second" sequence (build new file → fresh grep → single sed → mod+use+pub upgrades). lib.rs went from 8236 → 7260 lines (−976).
- 2026-05-02 (v0.3.22): bundled the 3 isomorphic provider runners (OpenAI / Anthropic / Gemini) + their shared helper family into `src-tauri/src/provider_runners.rs` (~1100 lines with doc header). Migration step 3 ("AI provider credentials/probes, including DeepSeek") is now COMPLETE for the runner surface — DeepSeek (v0.3.21) + the 3 isomorphic runners (v0.3.22) all live in dedicated modules. Extracted: `run_{openai,anthropic,gemini}_api_agent`, the unified helper family (`editorial_api_system_prompt`, `api_cost_preflight_result`, `write_provider_missing_key_result`, `write_provider_error_result`, `write_provider_failure_result`, `write_provider_success_result`, `log_provider_api_started`), 3 model resolvers (`resolve_{openai,anthropic,gemini}_model` + private `choose_preferred_model`/`api_model_ids`/`gemini_model_ids`) and 3 response parsers (`openai_response_text` + private `anthropic_response_text`/`gemini_response_text`/`gemini_usage_tokens`). Visibility upgrades in lib.rs: `provider_label_for_agent`, `provider_remote_present`, `provider_key_for_agent`, `api_input_estimate_chars`, `openai_api_input`, `anthropic_api_user_content`, `gemini_api_user_parts` (the last 4 keep their unit tests in `lib.rs::tests`). Stayed in lib.rs: the attachment-shape predicates and payload builders (move later if test ownership migrates). Followed the advisor's "delete first, edit second" sequence (build new file → fresh grep → single `sed -i 3886,5003d` → add `mod`+`use`+`pub(crate)` upgrades) to eliminate the line-shift class that bit v0.3.21. lib.rs went from 9351 → 8233 lines (−1118). Migration step 3 (runner surface) COMPLETE.
- 2026-05-02 (v0.3.21): extracted the DeepSeek API runner + 4 helpers (`run_deepseek_api_agent`, `write_deepseek_error_result`, `deepseek_model`, `resolve_deepseek_model`, `deepseek_model_ids`) + the `deepseek_model_ids_extract_current_api_shape` test into `src-tauri/src/provider_deepseek.rs` (467 lines with doc header). DeepSeek was extracted ahead of openai/anthropic/gemini because it is the structural outlier — it predates the unified `write_provider_failure_result` family and keeps its own custom error helper; bundling it with the 3 isomorphic runners would have hidden the asymmetry behind a uniform-looking diff. Visibility upgrades in lib.rs (consumed from `provider_deepseek.rs`): `AiProviderConfig` struct + 4 secret fields, `EditorialAgentResult` struct + all 12 fields, `first_env_value`, `effective_provider_key`, `log_editorial_agent_finished`, `extract_maestro_status`, `api_error_message`. Migration step 3 advances; the remaining 3 isomorphic runners + their shared helpers (`api_cost_preflight_result`, `write_provider_missing_key_result`, `write_provider_error_result`, `write_provider_failure_result`, `editorial_api_system_prompt`, `log_provider_api_started`, `write_provider_success_result`, `api_input_estimate_chars`, `provider_key_for_agent`, `provider_remote_present`, `openai_response_text`) move together in v0.3.22 as a single bundled batch (advisor-recommended given the isomorphism). lib.rs went from 9776 → 9351 lines (−425).

## Rules

- Do not combine code split with behavior changes unless the behavior change is the reason for the extraction.
- Keep public Tauri command names stable.
- Keep portable data paths stable.
- Preserve secret redaction tests before and after every extraction.
- Run `cargo test`, `npm run typecheck`, and `npm run build` after each completed split batch.
