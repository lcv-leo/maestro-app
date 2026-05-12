// Modulo: src-tauri/src/config_persistence.rs
// Descricao: Bootstrap and AI provider config persistence (disk + Cloudflare)
// extracted from lib.rs in v0.3.41 per `docs/code-split-plan.md` migration
// step 2 (config persistence).
//
// This module owns reading/writing config artifacts to local disk
// (`data/config/bootstrap.json` and `data/config/ai-providers.json`),
// publishing the AiProviderConfig to Cloudflare Secrets Store + D1 metadata,
// reading the metadata back, and merging Cloudflare-derived flags into a
// locally-loaded config (the "enrich" step).
//
// What's here (7 functions):
//   - `persist_bootstrap_config` — atomic JSON write to disk via the
//     `checked_data_child_path` safety gate from `app_paths.rs`.
//   - `persist_ai_provider_config` — same shape, for the AI provider config.
//   - `persist_ai_provider_cloudflare_marker` — writes a marker JSON to
//     disk that records `credential_storage_mode = "cloudflare"` and which
//     remote secrets are present, with the actual API keys cleared. Used
//     when the operator opts in to Cloudflare-managed secrets so the local
//     file no longer holds plaintext.
//   - `persist_ai_provider_config_to_cloudflare` — the full upload path:
//     ensures the D1 database + Secrets Store exist via the
//     `cloudflare.rs` ensure_* helpers, upserts the per-provider secrets,
//     writes the metadata row to D1.
//   - `enrich_ai_provider_config_from_cloudflare` — best-effort merge of
//     remote `provider_mode` / remote-secret-present flags / store id+name
//     into a locally-loaded config (no error propagation; falls through
//     when the remote read fails).
//   - `read_ai_provider_cloudflare_metadata` — reads the JSON value blob
//     from `maestro_settings WHERE key='ai.providers'` in D1 and rebuilds
//     an `AiProviderConfig` with `credential_storage_mode="cloudflare"`,
//     remote presence flags, store id+name, and per-provider tariff rates.
//   - `json_find_first_string` — recursive helper to find the first
//     string value for a given key anywhere in a serde_json `Value`.
//     Local to this module (no other callers in the crate).
//
// What stayed in lib.rs:
//   - `read_bootstrap_config` and `read_ai_provider_config` Tauri commands
//     (registry boundary; consume the helpers here via crate-level imports).
//   - `BootstrapConfig` and `AiProviderConfig` structs themselves.
//   - The `enrich`-callers in lib.rs (the `read_ai_provider_config` Tauri
//     command and `loadAiProviderConfig`-equivalent path).
//
// v0.3.41 is a pure move: every signature, error string, JSON path,
// SQL string, and Cloudflare endpoint is identical to the v0.3.40 lib.rs
// source (commit 254e5a3).

use std::path::Path;

use serde_json::{json, Value};

use crate::cloudflare::{
    ai_provider_secret_values, cloudflare_client, cloudflare_get, cloudflare_post_json,
    cloudflare_result_id_for_name, cloudflare_token_from_provider_request,
    ensure_cloudflare_d1_database, ensure_cloudflare_secret_store, upsert_ai_provider_secrets,
    write_ai_provider_metadata_to_cloudflare,
};
use crate::{
    sanitize_ai_provider_config, sanitize_short, write_text_file, AiProviderConfig,
    BootstrapConfig, CloudflareProviderStorageRequest,
};

pub(crate) fn persist_bootstrap_config(
    path: &Path,
    config: &BootstrapConfig,
) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config)
        .map_err(|error| format!("failed to serialize bootstrap config: {error}"))?;
    write_text_file(path, &text)
        .map_err(|error| format!("failed to write bootstrap config: {error}"))
}

pub(crate) fn persist_ai_provider_config(
    path: &Path,
    config: &AiProviderConfig,
) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config)
        .map_err(|error| format!("failed to serialize AI provider config: {error}"))?;
    write_text_file(path, &text)
        .map_err(|error| format!("failed to write AI provider config: {error}"))
}

pub(crate) fn persist_ai_provider_cloudflare_marker(
    path: &Path,
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
        grok_api_key: None,
        perplexity_api_key: None,
        openai_api_key_remote: config.openai_api_key.is_some() || config.openai_api_key_remote,
        anthropic_api_key_remote: config.anthropic_api_key.is_some()
            || config.anthropic_api_key_remote,
        gemini_api_key_remote: config.gemini_api_key.is_some() || config.gemini_api_key_remote,
        deepseek_api_key_remote: config.deepseek_api_key.is_some()
            || config.deepseek_api_key_remote,
        grok_api_key_remote: config.grok_api_key.is_some() || config.grok_api_key_remote,
        perplexity_api_key_remote: config.perplexity_api_key.is_some()
            || config.perplexity_api_key_remote,
        openai_input_usd_per_million: config.openai_input_usd_per_million,
        openai_output_usd_per_million: config.openai_output_usd_per_million,
        anthropic_input_usd_per_million: config.anthropic_input_usd_per_million,
        anthropic_output_usd_per_million: config.anthropic_output_usd_per_million,
        gemini_input_usd_per_million: config.gemini_input_usd_per_million,
        gemini_output_usd_per_million: config.gemini_output_usd_per_million,
        deepseek_input_usd_per_million: config.deepseek_input_usd_per_million,
        deepseek_output_usd_per_million: config.deepseek_output_usd_per_million,
        grok_input_usd_per_million: config.grok_input_usd_per_million,
        grok_output_usd_per_million: config.grok_output_usd_per_million,
        perplexity_input_usd_per_million: config.perplexity_input_usd_per_million,
        perplexity_output_usd_per_million: config.perplexity_output_usd_per_million,
        cloudflare_secret_store_id: config.cloudflare_secret_store_id.clone(),
        cloudflare_secret_store_name: config.cloudflare_secret_store_name.clone(),
        updated_at: config.updated_at.clone(),
    };
    persist_ai_provider_config(path, &marker)
}

pub(crate) fn persist_ai_provider_config_to_cloudflare(
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

pub(crate) fn enrich_ai_provider_config_from_cloudflare(
    mut config: AiProviderConfig,
    bootstrap: &BootstrapConfig,
) -> AiProviderConfig {
    if let Ok(remote) = read_ai_provider_cloudflare_metadata(bootstrap) {
        config.provider_mode = remote.provider_mode;
        config.openai_api_key_remote |= remote.openai_api_key_remote;
        config.anthropic_api_key_remote |= remote.anthropic_api_key_remote;
        config.gemini_api_key_remote |= remote.gemini_api_key_remote;
        config.deepseek_api_key_remote |= remote.deepseek_api_key_remote;
        config.grok_api_key_remote |= remote.grok_api_key_remote;
        config.perplexity_api_key_remote |= remote.perplexity_api_key_remote;
        config.cloudflare_secret_store_id = remote
            .cloudflare_secret_store_id
            .or(config.cloudflare_secret_store_id);
        config.cloudflare_secret_store_name = remote
            .cloudflare_secret_store_name
            .or(config.cloudflare_secret_store_name);
    }
    config
}

pub(crate) fn read_ai_provider_cloudflare_metadata(
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
        grok_input_usd_per_million: metadata
            .get("grok_input_usd_per_million")
            .and_then(Value::as_f64),
        grok_output_usd_per_million: metadata
            .get("grok_output_usd_per_million")
            .and_then(Value::as_f64),
        perplexity_input_usd_per_million: metadata
            .get("perplexity_input_usd_per_million")
            .and_then(Value::as_f64),
        perplexity_output_usd_per_million: metadata
            .get("perplexity_output_usd_per_million")
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
                "MAESTRO_GROK_API_KEY" => config.grok_api_key_remote = true,
                "MAESTRO_PERPLEXITY_API_KEY" => config.perplexity_api_key_remote = true,
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
