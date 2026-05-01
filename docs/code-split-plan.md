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

## Rules

- Do not combine code split with behavior changes unless the behavior change is the reason for the extraction.
- Keep public Tauri command names stable.
- Keep portable data paths stable.
- Preserve secret redaction tests before and after every extraction.
- Run `cargo test`, `npm run typecheck`, and `npm run build` after each completed split batch.
