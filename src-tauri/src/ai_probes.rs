// Modulo: src-tauri/src/ai_probes.rs
// Descricao: AI provider credential probes (OpenAI / Anthropic / Gemini /
// DeepSeek) extracted from lib.rs in v0.3.30 per `docs/code-split-plan.md`
// migration step 5.
//
// What's here (8 functions):
//   - `run_ai_provider_probe` — top-level entry that builds the HTTP client
//     once and dispatches to the four per-provider probes.
//   - `probe_openai_api`, `probe_anthropic_api`, `probe_gemini_api`,
//     `probe_deepseek_api` — per-provider GET to `/models` (or equivalent)
//     authenticated with the resolved key.
//   - `missing_provider_key_row` — uniform row for "no key informed" / "key
//     lives in Cloudflare Secrets Store" cases.
//   - `summarize_ai_probe_response` — translates HTTP status into an
//     operator-readable tone (ok / warn / error) with `api_error_message`
//     enrichment.
//   - `ai_probe_row` — small `AiProviderProbeRow` builder with sanitization.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `AiProviderConfig`, `AiProviderProbeRow`, `AiProviderProbeResult`
//     (the structs; v0.3.30 upgrades fields to pub(crate)).
//   - `effective_provider_key`, `api_error_message`, `sanitize_text`,
//     `sanitize_short` (already pub(crate)).
//
// v0.3.30 is a pure move: every signature, format string, and HTTP shape is
// identical to the v0.3.29 lib.rs source (commit fd77a4c).

use std::time::Duration;

use chrono::Utc;
use reqwest::blocking::Client;

use crate::{
    api_error_message, effective_provider_key, sanitize_short, sanitize_text, AiProviderConfig,
    AiProviderProbeResult, AiProviderProbeRow,
};

pub(crate) fn run_ai_provider_probe(config: &AiProviderConfig) -> AiProviderProbeResult {
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
