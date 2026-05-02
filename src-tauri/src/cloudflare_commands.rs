// Modulo: src-tauri/src/cloudflare_commands.rs
// Descricao: Tauri commands for Cloudflare environment + credential
// diagnostics, plus the dependency_preflight command + inner that
// surfaces CLI presence and Cloudflare env state on the Settings panel.
// Extracted from lib.rs in v0.5.5 per `docs/code-split-plan.md`.
//
// What's here (4 items):
//   - `cloudflare_env_snapshot` — Tauri command that probes process env
//     for `MAESTRO_CLOUDFLARE_ACCOUNT_ID` / `CLOUDFLARE_ACCOUNT_ID` /
//     `CF_ACCOUNT_ID` and the matching API_TOKEN family. Returns scope
//     (process / HKCU / HKLM) for each detected variable.
//   - `verify_cloudflare_credentials` — Tauri command wrapping
//     `cloudflare::run_cloudflare_probe` with the `settings.cloudflare.
//     verify_completed` NDJSON log emission.
//   - `dependency_preflight` (async) + `dependency_preflight_inner` —
//     Settings panel command that runs CLI/version checks for Claude /
//     Codex / Gemini / Node / npm / cargo / gh plus Cloudflare env state
//     and Wrangler hint. Async wrapper uses `spawn_blocking` to keep the
//     IPC thread free.
//
// Pure move from lib.rs v0.5.4 (commit 812d988): function bodies, NDJSON
// shape, env-var precedence chains, and CLI label strings preserved
// byte-identical.

use serde_json::{json, Value};

use crate::cloudflare::{run_cloudflare_probe, token_source_label};
use crate::command_spawn::command_check;
use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::provider_routing::first_env_value;
use crate::sanitize::{sanitize_short, sanitize_text};
use crate::{CloudflareEnvSnapshot, CloudflareProbeRequest, CloudflareProbeResult};

#[tauri::command]
pub(crate) fn cloudflare_env_snapshot() -> CloudflareEnvSnapshot {
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
pub(crate) async fn dependency_preflight() -> Value {
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
pub(crate) fn verify_cloudflare_credentials(
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
