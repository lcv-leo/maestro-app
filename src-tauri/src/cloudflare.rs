// Modulo: src-tauri/src/cloudflare.rs
// Descricao: Cloudflare API client + D1 + Secrets Store operations extracted from
// lib.rs in v0.3.23 per `docs/code-split-plan.md` migration step 4.
//
// What's here (29 functions + 1 struct):
//   - HTTP layer: `cloudflare_client`, `cloudflare_get`, `cloudflare_post_json`,
//     `cloudflare_patch_json`, `cloudflare_get_paginated_results`,
//     `cloudflare_page_path`, `cloudflare_verify_path`, `cloudflare_token_kind`,
//     `cloudflare_error_summary`.
//   - Token resolution: `token_from_probe_request`, `token_source_label`,
//     `cloudflare_token_from_provider_request`.
//   - JSON helpers: `cloudflare_result_names`, `cloudflare_result_id_for_name`,
//     `cloudflare_store_records`, `cloudflare_store_for_target_or_existing`,
//     `cloudflare_secret_ids_by_name`, `cloudflare_secret_id_from_response`,
//     `cloudflare_created_result_id`, `CloudflareStoreRecord` struct.
//   - D1 + Secrets Store ensure logic: `ensure_cloudflare_d1_database`,
//     `ensure_cloudflare_secret_store`, `provision_maestro_d1_schema`,
//     `link_secret_store_reference`.
//   - AI provider bridge: `ai_provider_secret_values`,
//     `upsert_ai_provider_secrets`, `write_ai_provider_metadata_to_cloudflare`.
//   - Probe entry point: `run_cloudflare_probe`, `probe_row`.
//
// What stayed in lib.rs (consumed via `pub(crate)` upgrades):
//   - `cloudflare_env_snapshot`, `verify_cloudflare_credentials` (Tauri commands).
//   - `persist_ai_provider_cloudflare_marker`,
//     `persist_ai_provider_config_to_cloudflare`,
//     `enrich_ai_provider_config_from_cloudflare`,
//     `read_ai_provider_cloudflare_metadata` (AI provider <-> CF bridges).
//   - The Cloudflare* request/result structs (Tauri command argument types).
//
// v0.3.23 is a pure move: every signature, log line, format string and JSON
// envelope is identical to the v0.3.22 lib.rs source (commit 73b0766).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::{
    env_value_with_scope, first_env_value, sanitize_short, sanitize_text, AiProviderConfig,
    CloudflareProbeRequest, CloudflareProbeResult, CloudflareProbeRow,
    CloudflareProviderStorageRequest,
};

pub(crate) fn token_source_label(request: &CloudflareProbeRequest) -> String {
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

pub(crate) fn run_cloudflare_probe(request: &CloudflareProbeRequest) -> CloudflareProbeResult {
    let token = token_from_probe_request(request);
    let account_id = sanitize_short(request.account_id.trim(), 80);
    let persistence_database = sanitize_short(&request.persistence_database, 80);
    let publication_database = sanitize_short(&request.publication_database, 80);
    let secret_store = sanitize_short(&request.secret_store, 80);
    let mut rows = Vec::new();
    let mut maestro_database_id: Option<String> = None;

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
            if !persistence_database.is_empty() && names.contains(&persistence_database) {
                maestro_database_id = cloudflare_result_id_for_name(&value, &persistence_database);
                if let Some(database_id) = maestro_database_id.as_deref() {
                    let _ = provision_maestro_d1_schema(
                        &client,
                        &token_value,
                        &account_id,
                        database_id,
                    );
                }
            }
            let publication_missing =
                !publication_database.is_empty() && !names.contains(&publication_database);
            let persistence_missing =
                !persistence_database.is_empty() && !names.contains(&persistence_database);

            if !persistence_missing && !publication_missing {
                rows.push(probe_row(
                    "D1 Read/Edit",
                    format!("{persistence_database} + {publication_database} acessiveis"),
                    "ok",
                ));
            } else if persistence_missing {
                let create_result = cloudflare_post_json(
                    &client,
                    &token_value,
                    &d1_path,
                    json!({ "name": persistence_database }),
                );

                match create_result {
                    Ok(created) => {
                        let database_id = cloudflare_created_result_id(&created).or_else(|| {
                            cloudflare_result_id_for_name(&value, &persistence_database)
                        });
                        maestro_database_id = database_id.clone();
                        let (schema_status, schema_ok) = if let Some(database_id) = database_id {
                            match provision_maestro_d1_schema(
                                &client,
                                &token_value,
                                &account_id,
                                &database_id,
                            ) {
                                Ok(_) => ("schema Maestro criado".to_string(), true),
                                Err(error) => (format!("schema pendente: {error}"), false),
                            }
                        } else {
                            (
                                "schema pendente: id da base nao retornado".to_string(),
                                false,
                            )
                        };

                        if publication_missing {
                            rows.push(probe_row(
                                "D1 Read/Edit",
                                format!(
                                    "{persistence_database} criada; {schema_status}; {publication_database} ausente"
                                ),
                                "warn",
                            ));
                        } else {
                            rows.push(probe_row(
                                "D1 Read/Edit",
                                format!("{persistence_database} criada; {schema_status}"),
                                if schema_ok { "ok" } else { "warn" },
                            ));
                        }
                    }
                    Err(error) => rows.push(probe_row(
                        "D1 Read/Edit",
                        format!("{persistence_database} ausente e criacao falhou: {error}"),
                        "error",
                    )),
                }
            } else {
                rows.push(probe_row(
                    "D1 Read/Edit",
                    format!("{persistence_database} acessivel; {publication_database} ausente"),
                    "warn",
                ));
            }
        }
        Err(error) => rows.push(probe_row("D1 Read/Edit", error, "error")),
    }

    let stores_path = format!("/accounts/{account_id}/secrets_store/stores");
    match cloudflare_get(&client, &token_value, &stores_path) {
        Ok(value) => {
            if secret_store.is_empty() {
                rows.push(probe_row("Secrets Store", "endpoint acessivel", "ok"));
            } else if let Some(store) =
                cloudflare_store_for_target_or_existing(&value, &secret_store)
            {
                let link_status = link_secret_store_reference(
                    &client,
                    &token_value,
                    &account_id,
                    maestro_database_id.as_deref(),
                    &secret_store,
                    &store.name,
                    &store.id,
                );
                if store.name == secret_store {
                    rows.push(probe_row(
                        "Secrets Store",
                        format!("store {secret_store} acessivel; {link_status}"),
                        "ok",
                    ));
                } else {
                    rows.push(probe_row(
                        "Secrets Store",
                        format!("usando store existente {}; {link_status}", store.name),
                        "ok",
                    ));
                }
            } else {
                match cloudflare_post_json(
                    &client,
                    &token_value,
                    &stores_path,
                    json!({ "name": secret_store }),
                ) {
                    Ok(created) => {
                        let store_id = cloudflare_created_result_id(&created)
                            .unwrap_or_else(|| "id-nao-retornado".to_string());
                        let link_status = link_secret_store_reference(
                            &client,
                            &token_value,
                            &account_id,
                            maestro_database_id.as_deref(),
                            &secret_store,
                            &secret_store,
                            &store_id,
                        );
                        rows.push(probe_row(
                            "Secrets Store",
                            format!("store {secret_store} criado; {link_status}"),
                            "ok",
                        ));
                    }
                    Err(error) => rows.push(probe_row(
                        "Secrets Store",
                        format!("nenhum store existente e criacao falhou: {error}"),
                        "error",
                    )),
                }
            }
        }
        Err(error) => rows.push(probe_row("Secrets Store", error, "error")),
    }

    CloudflareProbeResult { rows }
}

pub(crate) fn cloudflare_get(client: &Client, token: &str, path: &str) -> Result<Value, String> {
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

fn cloudflare_get_paginated_results(
    client: &Client,
    token: &str,
    path: &str,
) -> Result<Value, String> {
    let mut merged = cloudflare_get(client, token, &cloudflare_page_path(path, 1, 50))?;
    let total_pages = merged
        .pointer("/result_info/total_pages")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if total_pages <= 1 {
        return Ok(merged);
    }

    let mut all_items = merged
        .get("result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for page in 2..=total_pages {
        let page_value = cloudflare_get(
            client,
            token,
            &cloudflare_page_path(path, page as usize, 50),
        )?;
        if let Some(items) = page_value.get("result").and_then(Value::as_array) {
            all_items.extend(items.iter().cloned());
        }
    }

    if let Some(object) = merged.as_object_mut() {
        object.insert("result".to_string(), Value::Array(all_items));
    }
    Ok(merged)
}

pub(crate) fn cloudflare_page_path(path: &str, page: usize, per_page: usize) -> String {
    let separator = if path.contains('?') { '&' } else { '?' };
    format!("{path}{separator}page={page}&per_page={per_page}")
}

pub(crate) fn cloudflare_post_json(
    client: &Client,
    token: &str,
    path: &str,
    body: Value,
) -> Result<Value, String> {
    let url = format!("https://api.cloudflare.com/client/v4{path}");
    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
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

fn cloudflare_patch_json(
    client: &Client,
    token: &str,
    path: &str,
    body: Value,
) -> Result<Value, String> {
    let url = format!("https://api.cloudflare.com/client/v4{path}");
    let response = client
        .patch(url)
        .bearer_auth(token)
        .json(&body)
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

pub(crate) fn cloudflare_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!(
            "Maestro Editorial AI/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| format!("cliente HTTP Cloudflare falhou: {error}"))
}

pub(crate) fn cloudflare_verify_path(token: &str, account_id: &str) -> String {
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

#[derive(Clone)]
pub(crate) struct CloudflareStoreRecord {
    pub(crate) name: String,
    pub(crate) id: String,
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

pub(crate) fn cloudflare_result_id_for_name(value: &Value, target_name: &str) -> Option<String> {
    value
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .find_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?;
            if name != target_name {
                return None;
            }
            ["uuid", "id"]
                .into_iter()
                .find_map(|key| item.get(key).and_then(Value::as_str))
                .map(|id| id.to_string())
        })
}

pub(crate) fn cloudflare_store_for_target_or_existing(
    value: &Value,
    target_name: &str,
) -> Option<CloudflareStoreRecord> {
    let stores = cloudflare_store_records(value);
    stores
        .iter()
        .find(|store| store.name == target_name)
        .cloned()
        .or_else(|| stores.into_iter().next())
}

fn cloudflare_store_records(value: &Value) -> Vec<CloudflareStoreRecord> {
    value
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?;
            let id = ["id", "uuid"]
                .into_iter()
                .find_map(|key| item.get(key).and_then(Value::as_str))
                .unwrap_or(name);
            Some(CloudflareStoreRecord {
                name: sanitize_short(name, 80),
                id: sanitize_short(id, 80),
            })
        })
        .collect()
}

pub(crate) fn cloudflare_token_from_provider_request(
    request: &CloudflareProviderStorageRequest,
) -> Result<String, String> {
    if let Some(token) = request
        .api_token
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(token);
    }

    let requested_env = request.api_token_env_var.trim();
    if !requested_env.is_empty() {
        if let Some((_, value)) = env_value_with_scope(requested_env) {
            return Ok(value);
        }
    }

    first_env_value(&[
        "MAESTRO_CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_API_TOKEN",
        "CF_API_TOKEN",
    ])
    .map(|(_, _, value)| value)
    .ok_or_else(|| "token Cloudflare ausente: informe no campo ou em env var".to_string())
}

pub(crate) fn ensure_cloudflare_d1_database(
    client: &Client,
    token: &str,
    account_id: &str,
    database_name: &str,
) -> Result<String, String> {
    let d1_path = format!("/accounts/{account_id}/d1/database");
    let listed = cloudflare_get(client, token, &d1_path)?;
    if let Some(database_id) = cloudflare_result_id_for_name(&listed, database_name) {
        provision_maestro_d1_schema(client, token, account_id, &database_id)?;
        return Ok(database_id);
    }

    let created = cloudflare_post_json(client, token, &d1_path, json!({ "name": database_name }))?;
    let database_id = cloudflare_created_result_id(&created)
        .or_else(|| cloudflare_result_id_for_name(&listed, database_name))
        .ok_or_else(|| "Cloudflare criou/listou D1 sem retornar id da base".to_string())?;
    provision_maestro_d1_schema(client, token, account_id, &database_id)?;
    Ok(database_id)
}

pub(crate) fn ensure_cloudflare_secret_store(
    client: &Client,
    token: &str,
    account_id: &str,
    requested_store_name: &str,
    database_id: Option<&str>,
) -> Result<CloudflareStoreRecord, String> {
    let stores_path = format!("/accounts/{account_id}/secrets_store/stores");
    let listed = cloudflare_get(client, token, &stores_path)?;
    if let Some(store) = cloudflare_store_for_target_or_existing(&listed, requested_store_name) {
        let _ = link_secret_store_reference(
            client,
            token,
            account_id,
            database_id,
            requested_store_name,
            &store.name,
            &store.id,
        );
        return Ok(store);
    }

    let created = cloudflare_post_json(
        client,
        token,
        &stores_path,
        json!({ "name": requested_store_name }),
    )?;
    let store = CloudflareStoreRecord {
        name: requested_store_name.to_string(),
        id: cloudflare_created_result_id(&created)
            .ok_or_else(|| "Cloudflare criou Secrets Store sem retornar id".to_string())?,
    };
    let _ = link_secret_store_reference(
        client,
        token,
        account_id,
        database_id,
        requested_store_name,
        &store.name,
        &store.id,
    );
    Ok(store)
}

pub(crate) fn ai_provider_secret_values(config: &AiProviderConfig) -> BTreeMap<&'static str, String> {
    let mut values = BTreeMap::new();
    if let Some(value) = config.openai_api_key.as_ref() {
        values.insert("MAESTRO_OPENAI_API_KEY", value.clone());
    }
    if let Some(value) = config.anthropic_api_key.as_ref() {
        values.insert("MAESTRO_ANTHROPIC_API_KEY", value.clone());
    }
    if let Some(value) = config.gemini_api_key.as_ref() {
        values.insert("MAESTRO_GEMINI_API_KEY", value.clone());
    }
    if let Some(value) = config.deepseek_api_key.as_ref() {
        values.insert("MAESTRO_DEEPSEEK_API_KEY", value.clone());
    }
    values
}

pub(crate) fn upsert_ai_provider_secrets(
    client: &Client,
    token: &str,
    account_id: &str,
    store: &CloudflareStoreRecord,
    secrets: &BTreeMap<&'static str, String>,
) -> Result<Vec<Value>, String> {
    let secrets_path = format!(
        "/accounts/{account_id}/secrets_store/stores/{}/secrets",
        store.id
    );
    let listed = cloudflare_get_paginated_results(client, token, &secrets_path)?;
    let mut existing = cloudflare_secret_ids_by_name(&listed);
    let mut records = Vec::new();

    for (name, value) in secrets {
        let comment = format!("Maestro Editorial AI provider credential: {name}");
        let response = if let Some(secret_id) = existing.get(*name) {
            cloudflare_patch_json(
                client,
                token,
                &format!(
                    "/accounts/{account_id}/secrets_store/stores/{}/secrets/{secret_id}",
                    store.id
                ),
                json!({
                    "value": value,
                    "scopes": ["workers", "ai_gateway"],
                    "comment": comment
                }),
            )
        } else {
            match cloudflare_post_json(
                client,
                token,
                &secrets_path,
                json!([{
                    "name": name,
                    "value": value,
                    "scopes": ["workers", "ai_gateway"],
                    "comment": comment
                }]),
            ) {
                Ok(response) => Ok(response),
                Err(error) if error.contains("secret_name_already_exists") => {
                    let relisted = cloudflare_get_paginated_results(client, token, &secrets_path)?;
                    existing = cloudflare_secret_ids_by_name(&relisted);
                    let Some(secret_id) = existing.get(*name) else {
                        return Err(format!(
                            "{error}; segredo existente nao apareceu na listagem paginada"
                        ));
                    };
                    cloudflare_patch_json(
                        client,
                        token,
                        &format!(
                            "/accounts/{account_id}/secrets_store/stores/{}/secrets/{secret_id}",
                            store.id
                        ),
                        json!({
                            "value": value,
                            "scopes": ["workers", "ai_gateway"],
                            "comment": comment
                        }),
                    )
                }
                Err(error) => Err(error),
            }
        }?;

        let secret_id = cloudflare_secret_id_from_response(&response)
            .or_else(|| existing.get(*name).cloned())
            .unwrap_or_else(|| "id-nao-retornado".to_string());
        records.push(json!({
            "name": name,
            "secret_id": secret_id,
            "store_id": store.id,
            "store_name": store.name,
            "updated_at": Utc::now().to_rfc3339()
        }));
    }

    Ok(records)
}

fn cloudflare_secret_ids_by_name(value: &Value) -> BTreeMap<String, String> {
    value
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?;
            let id = item.get("id").and_then(Value::as_str)?;
            Some((name.to_string(), id.to_string()))
        })
        .collect()
}

fn cloudflare_secret_id_from_response(value: &Value) -> Option<String> {
    value
        .get("result")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("id").and_then(Value::as_str))
        .or_else(|| value.pointer("/result/id").and_then(Value::as_str))
        .map(|id| id.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_ai_provider_metadata_to_cloudflare(
    client: &Client,
    token: &str,
    account_id: &str,
    database_id: &str,
    config: &AiProviderConfig,
    requested_store_name: &str,
    store: &CloudflareStoreRecord,
    secret_records: &[Value],
) -> Result<(), String> {
    let raw_path = format!("/accounts/{account_id}/d1/database/{database_id}/raw");
    let updated_at = Utc::now().to_rfc3339();
    let metadata = json!({
        "schema_version": 1,
        "provider_mode": config.provider_mode,
        "credential_storage_mode": "cloudflare",
        "openai_input_usd_per_million": config.openai_input_usd_per_million,
        "openai_output_usd_per_million": config.openai_output_usd_per_million,
        "anthropic_input_usd_per_million": config.anthropic_input_usd_per_million,
        "anthropic_output_usd_per_million": config.anthropic_output_usd_per_million,
        "gemini_input_usd_per_million": config.gemini_input_usd_per_million,
        "gemini_output_usd_per_million": config.gemini_output_usd_per_million,
        "deepseek_input_usd_per_million": config.deepseek_input_usd_per_million,
        "deepseek_output_usd_per_million": config.deepseek_output_usd_per_million,
        "requested_store_name": requested_store_name,
        "effective_store_name": store.name,
        "effective_store_id": store.id,
        "secrets": secret_records,
        "updated_at": updated_at
    });

    cloudflare_post_json(
        client,
        token,
        &raw_path,
        json!({
            "sql": "INSERT OR REPLACE INTO maestro_settings (key, value_json, updated_at) VALUES (?, ?, ?)",
            "params": ["ai.providers", metadata.to_string(), updated_at]
        }),
    )?;

    for record in secret_records {
        let Some(name) = record.get("name").and_then(Value::as_str) else {
            continue;
        };
        let store_id = record
            .get("store_id")
            .and_then(Value::as_str)
            .unwrap_or(&store.id);
        let secret_id = record
            .get("secret_id")
            .and_then(Value::as_str)
            .unwrap_or("id-nao-retornado");
        cloudflare_post_json(
            client,
            token,
            &raw_path,
            json!({
                "sql": "INSERT OR REPLACE INTO maestro_secret_refs (name, store_id, secret_id, updated_at) VALUES (?, ?, ?, ?)",
                "params": [name, store_id, secret_id, updated_at]
            }),
        )?;
    }

    Ok(())
}

fn link_secret_store_reference(
    client: &Client,
    token: &str,
    account_id: &str,
    database_id: Option<&str>,
    requested_store_name: &str,
    effective_store_name: &str,
    effective_store_id: &str,
) -> String {
    let Some(database_id) = database_id.filter(|value| !value.trim().is_empty()) else {
        return "vinculo local pendente: maestro_db indisponivel".to_string();
    };

    let raw_path = format!("/accounts/{account_id}/d1/database/{database_id}/raw");
    let value_json = json!({
        "requested_store_name": requested_store_name,
        "effective_store_name": effective_store_name,
        "effective_store_id": effective_store_id,
        "linked_at": Utc::now().to_rfc3339(),
        "note": "Maestro usa o Secrets Store existente quando o plano Cloudflare permite apenas um store."
    })
    .to_string();
    let updated_at = Utc::now().to_rfc3339();

    match cloudflare_post_json(
        client,
        token,
        &raw_path,
        json!({
            "sql": "INSERT OR REPLACE INTO maestro_settings (key, value_json, updated_at) VALUES (?, ?, ?)",
            "params": ["cloudflare.secrets_store", value_json, updated_at]
        }),
    ) {
        Ok(_) => "vinculado em maestro_db".to_string(),
        Err(error) => format!("vinculo pendente: {error}"),
    }
}

fn cloudflare_created_result_id(value: &Value) -> Option<String> {
    ["uuid", "id"]
        .into_iter()
        .find_map(|key| {
            value
                .pointer(&format!("/result/{key}"))
                .and_then(Value::as_str)
        })
        .map(|id| id.to_string())
}

fn provision_maestro_d1_schema(
    client: &Client,
    token: &str,
    account_id: &str,
    database_id: &str,
) -> Result<(), String> {
    let raw_path = format!("/accounts/{account_id}/d1/database/{database_id}/raw");
    let statements = [
        "CREATE TABLE IF NOT EXISTS maestro_settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS maestro_sessions (run_id TEXT PRIMARY KEY, status TEXT NOT NULL, metadata_json TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS maestro_artifacts (run_id TEXT NOT NULL, name TEXT NOT NULL, content TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY (run_id, name))",
        "CREATE TABLE IF NOT EXISTS maestro_secret_refs (name TEXT PRIMARY KEY, store_id TEXT, secret_id TEXT, updated_at TEXT NOT NULL)",
    ];

    for sql in statements {
        cloudflare_post_json(
            client,
            token,
            &raw_path,
            json!({
                "sql": sql,
                "params": []
            }),
        )?;
    }

    Ok(())
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
