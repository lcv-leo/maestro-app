// Modulo: src-tauri/src/provider_deepseek.rs
// Descricao: DeepSeek API peer runner extracted from lib.rs in v0.3.21.
//
// This module owns the DeepSeek-specific surface of the editorial peer pipeline:
//   - `run_deepseek_api_agent` performs the chat-completions call with retry, cost
//     pre-flight, output sanitization and NDJSON instrumentation.
//   - `write_deepseek_error_result` builds the error artifact + result envelope
//     reused on client / network / HTTP-error branches.
//   - `deepseek_model`, `resolve_deepseek_model`, and `deepseek_model_ids` honor the
//     env override (`MAESTRO_DEEPSEEK_MODEL` / `CROSS_REVIEW_DEEPSEEK_MODEL`) and
//     fall back to the live `/models` listing.
//
// The DeepSeek runner predates the unified provider helper family
// (`write_provider_failure_result` and friends used by openai/anthropic/gemini) and
// keeps its own custom error helper to preserve byte-for-byte parity with the v0.3.20
// artifact shape. v0.3.21 is a pure move: every signature, log line, format string
// and status string is identical to the lib.rs source it replaces.

use std::path::Path;
use std::time::Instant;

use reqwest::blocking::Client;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::provider_retry::{
    build_api_client_async, provider_http_error_status, provider_reqwest_error_status,
    send_with_retry_async, ProviderRequestOutcome,
};
use crate::provider_runners::{
    editorial_api_system_prompt, log_provider_cache_configured, provider_cache_artifact_lines,
    EditorialAgentRequest,
};
use crate::session_controls::{
    api_role_max_tokens, estimate_provider_cost, provider_cache_plan, provider_cache_telemetry,
    provider_cache_telemetry_with_plan, provider_cost, usage_tokens,
};
use crate::{
    api_error_message, effective_provider_key, extract_maestro_status, first_env_value,
    log_editorial_agent_finished, sanitize_short, sanitize_text, write_text_file,
    EditorialAgentResult,
};

pub(crate) async fn run_deepseek_api_agent(
    request: EditorialAgentRequest<'_>,
    cancel_token: &CancellationToken,
) -> EditorialAgentResult {
    let EditorialAgentRequest {
        log_session,
        run_id,
        role,
        prompt,
        attachments: _,
        output_path,
        timeout,
        config,
        cost_guard,
    } = request;
    let started = Instant::now();
    let model_hint = deepseek_model();
    let name = "DeepSeek";
    let cli = "deepseek-api";

    let Some((api_key, key_source)) = effective_provider_key(
        config.deepseek_api_key.as_deref(),
        &["MAESTRO_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"],
    ) else {
        let status = if config.deepseek_api_key_remote {
            "REMOTE_SECRET_NOT_READABLE"
        } else {
            "API_KEY_NOT_AVAILABLE"
        };
        let note = if config.deepseek_api_key_remote {
            "A referencia do segredo existe no Cloudflare Secrets Store, mas a Cloudflare nao devolve o valor bruto ao app local. Informe MAESTRO_DEEPSEEK_API_KEY/DEEPSEEK_API_KEY ou configure um broker Cloudflare para consumir o segredo."
        } else {
            "DeepSeek precisa de MAESTRO_DEEPSEEK_API_KEY, DEEPSEEK_API_KEY ou chave informada na tela Ajustes > Agentes via API."
        };
        let _ = write_text_file(
            output_path,
            &format!(
                "# {name} - {role}\n\n- CLI: `{cli}`\n- Status: `{status}`\n- Exit code: `unknown`\n- Duration ms: `{}`\n- Model: `{model}`\n\n{note}\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n{note}\n```\n",
                started.elapsed().as_millis(),
                model = model_hint
            ),
        );
        let result = EditorialAgentResult {
            name: name.to_string(),
            role: role.to_string(),
            cli: cli.to_string(),
            tone: "blocked".to_string(),
            status: status.to_string(),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            output_path: output_path.to_string_lossy().to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
            cache: None,
        };
        log_editorial_agent_finished(
            log_session,
            run_id,
            &result,
            None,
            Some(note.len()),
            None,
            false,
        );
        return result;
    };

    let max_tokens = api_role_max_tokens(role);
    if let Some(guard) = cost_guard.as_ref() {
        let estimated_cost = estimate_provider_cost(&prompt, max_tokens, guard.rates);
        if let Some(max_session_cost_usd) = guard.max_session_cost_usd {
            if guard.observed_cost_usd + estimated_cost > max_session_cost_usd {
                let status = "COST_LIMIT_REACHED";
                let note = format!(
                "DeepSeek nao foi chamado: custo projetado ${:.6} somado ao observado ${:.6} excede o limite ${:.6}.",
                estimated_cost, guard.observed_cost_usd, max_session_cost_usd
            );
                let _ = write_text_file(
                output_path,
                &format!(
                    "# {name} - {role}\n\n- CLI: `{cli}`\n- Provider: `deepseek`\n- Status: `{status}`\n- Exit code: `unknown`\n- Duration ms: `{}`\n- Cost projected USD: `{:.6}`\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n{}\n```\n",
                    started.elapsed().as_millis(),
                    estimated_cost,
                    note
                ),
            );
                let result = EditorialAgentResult {
                    name: name.to_string(),
                    role: role.to_string(),
                    cli: cli.to_string(),
                    tone: "blocked".to_string(),
                    status: status.to_string(),
                    duration_ms: started.elapsed().as_millis(),
                    exit_code: None,
                    output_path: output_path.to_string_lossy().to_string(),
                    usage_input_tokens: None,
                    usage_output_tokens: None,
                    cost_usd: None,
                    cost_estimated: None,
                    cache: None,
                };
                log_editorial_agent_finished(
                    log_session,
                    run_id,
                    &result,
                    None,
                    Some(note.len()),
                    None,
                    false,
                );
                return result;
            }
        }
    }

    // Two clients: blocking for resolve_deepseek_model (short /models call),
    // async for the main editorial request supporting tokio cancellation.
    let mut blocking_builder = Client::builder().user_agent(format!(
        "Maestro Editorial AI/{}",
        env!("CARGO_PKG_VERSION")
    ));
    if let Some(timeout) = timeout {
        blocking_builder = blocking_builder.timeout(timeout);
    }
    let blocking_client = match blocking_builder.build() {
        Ok(client) => client,
        Err(error) => {
            let status = provider_reqwest_error_status("CLIENT_ERROR", error);
            return write_deepseek_error_result(
                log_session,
                run_id,
                role,
                output_path,
                &model_hint,
                &status,
                started.elapsed().as_millis(),
            );
        }
    };
    let async_client = match build_api_client_async(timeout) {
        Ok(client) => client,
        Err(error) => {
            let status = provider_reqwest_error_status("CLIENT_ERROR", error);
            return write_deepseek_error_result(
                log_session,
                run_id,
                role,
                output_path,
                &model_hint,
                &status,
                started.elapsed().as_millis(),
            );
        }
    };

    let model = resolve_deepseek_model(&blocking_client, &api_key);
    let system_prompt = editorial_api_system_prompt(name);
    let cache_plan = provider_cache_plan("deepseek", &model, role, name, &system_prompt);
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.started".to_string(),
            message: "editorial API peer request starting".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(run_id, 120),
                "agent": name,
                "role": role,
                "cli": cli,
                "provider": "deepseek",
                "model": model,
                "prompt_chars": prompt.chars().count(),
                "output_path": output_path.to_string_lossy().to_string(),
                "timeout_seconds": timeout.map(|value| value.as_secs()),
                "timeout_policy": if timeout.is_some() { "session_deadline" } else { "none_editorial_session" },
                "cost_limit_usd": cost_guard.as_ref().and_then(|guard| guard.max_session_cost_usd),
                "observed_cost_usd": cost_guard.as_ref().map(|guard| guard.observed_cost_usd)
            })),
        },
    );
    log_provider_cache_configured(
        log_session,
        run_id,
        "deepseek",
        &model,
        role,
        output_path,
        prompt.chars().count(),
        &cache_plan,
    );

    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            { "role": "user", "content": prompt }
        ],
        "stream": false,
        "max_tokens": max_tokens
    });
    let request_builder = async_client
        .post("https://api.deepseek.com/chat/completions")
        .bearer_auth(&api_key)
        .json(&body);
    let response = match send_with_retry_async(
        log_session,
        run_id,
        "deepseek",
        cancel_token,
        request_builder,
    )
    .await
    {
        Ok(response) => response,
        Err(ProviderRequestOutcome::Cancelled) => {
            return write_deepseek_error_result(
                log_session,
                run_id,
                role,
                output_path,
                &model,
                "STOPPED_BY_USER",
                started.elapsed().as_millis(),
            );
        }
        Err(ProviderRequestOutcome::Network(error)) => {
            let status = provider_reqwest_error_status("PROVIDER_NETWORK_ERROR", error);
            return write_deepseek_error_result(
                log_session,
                run_id,
                role,
                output_path,
                &model,
                &status,
                started.elapsed().as_millis(),
            );
        }
    };

    let http_status = response.status();
    let body_text = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => {
            return write_deepseek_error_result(
                log_session,
                run_id,
                role,
                output_path,
                &model,
                "STOPPED_BY_USER",
                started.elapsed().as_millis(),
            );
        }
        r = response.text() => r.unwrap_or_default(),
    };

    {
        if !http_status.is_success() {
            let status =
                provider_http_error_status(http_status.as_u16(), &api_error_message(&body_text));
            return write_deepseek_error_result(
                log_session,
                run_id,
                role,
                output_path,
                &model,
                &status,
                started.elapsed().as_millis(),
            );
        }

        let parsed: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| json!({}));
        let (usage_input_tokens, usage_output_tokens) = usage_tokens(&parsed);
        let cache = Some(provider_cache_telemetry_with_plan(
            &cache_plan,
            provider_cache_telemetry("deepseek", &parsed, usage_input_tokens),
        ));
        let cost_usd = cost_guard.as_ref().and_then(|guard| {
            usage_input_tokens
                .zip(usage_output_tokens)
                .map(|(input, output)| provider_cost(input, output, guard.rates))
        });
        let stdout = match deepseek_assistant_content(&parsed) {
            Ok(content) => content,
            Err(status) => {
                return write_deepseek_error_result(
                    log_session,
                    run_id,
                    role,
                    output_path,
                    &model,
                    &status,
                    started.elapsed().as_millis(),
                );
            }
        };
        let model_reported = parsed
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&model);
        let status = if role == "review" {
            if stdout.trim().is_empty() {
                "AGENT_FAILED_EMPTY"
            } else {
                extract_maestro_status(&stdout).unwrap_or("NOT_READY")
            }
        } else if stdout.trim().is_empty() {
            "EMPTY_DRAFT"
        } else {
            "DRAFT_CREATED"
        };
        let tone = if status == "READY" || status == "DRAFT_CREATED" {
            "ok"
        } else if status == "EMPTY_DRAFT" || status == "AGENT_FAILED_EMPTY" {
            "error"
        } else {
            "warn"
        };
        let artifact = format!(
            "# {name} - {role}\n\n- CLI: `{cli}`\n- Provider: `deepseek`\n- Model: `{}`\n- Model reported: `{}`\n- Key source: `{}`\n- Status: `{status}`\n- Exit code: `0`\n- Duration ms: `{}`\n- Prompt chars: `{}`\n- Stdout chars: `{}`\n- Usage input tokens: `{}`\n- Usage output tokens: `{}`\n{}- Cost USD: `{}`\n- Stderr chars: `0`\n\n## Stdout\n\n```text\n{}\n```\n\n## Stderr\n\n```text\n\n```\n",
            model,
            sanitize_text(model_reported, 120),
            sanitize_text(&key_source, 120),
            started.elapsed().as_millis(),
            prompt.chars().count(),
            stdout.chars().count(),
            usage_input_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            usage_output_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            provider_cache_artifact_lines(cache.as_ref()),
            cost_usd
                .map(|value| format!("{value:.8}"))
                .unwrap_or_else(|| "unknown".to_string()),
            stdout
        );
        let _ = write_text_file(output_path, &artifact);
        let result = EditorialAgentResult {
            name: name.to_string(),
            role: role.to_string(),
            cli: cli.to_string(),
            tone: tone.to_string(),
            status: status.to_string(),
            duration_ms: started.elapsed().as_millis(),
            exit_code: Some(0),
            output_path: output_path.to_string_lossy().to_string(),
            usage_input_tokens,
            usage_output_tokens,
            cost_usd,
            cost_estimated: cost_usd.map(|_| true),
            cache,
        };
        log_editorial_agent_finished(
            log_session,
            run_id,
            &result,
            Some(stdout.chars().count()),
            Some(0),
            Some("https://api.deepseek.com/chat/completions".to_string()),
            false,
        );
        result
    }
}

pub(crate) fn write_deepseek_error_result(
    log_session: &LogSession,
    run_id: &str,
    role: &str,
    output_path: &Path,
    model: &str,
    status: &str,
    duration_ms: u128,
) -> EditorialAgentResult {
    let name = "DeepSeek";
    let cli = "deepseek-api";
    let safe_status = sanitize_text(status, 240);
    let _ = write_text_file(
        output_path,
        &format!(
            "# {name} - {role}\n\n- CLI: `{cli}`\n- Provider: `deepseek`\n- Model: `{}`\n- Status: `{}`\n- Exit code: `unknown`\n- Duration ms: `{duration_ms}`\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n{}\n```\n",
            sanitize_text(model, 120),
            safe_status,
            safe_status
        ),
    );
    let result = EditorialAgentResult {
        name: name.to_string(),
        role: role.to_string(),
        cli: cli.to_string(),
        tone: deepseek_error_tone(&safe_status).to_string(),
        status: safe_status,
        duration_ms,
        exit_code: None,
        output_path: output_path.to_string_lossy().to_string(),
        usage_input_tokens: None,
        usage_output_tokens: None,
        cost_usd: None,
        cost_estimated: None,
        cache: None,
    };
    log_editorial_agent_finished(
        log_session,
        run_id,
        &result,
        None,
        Some(status.len()),
        None,
        false,
    );
    result
}

fn deepseek_assistant_content(parsed: &Value) -> Result<String, String> {
    if let Some(content) = parsed
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(content.to_string());
    }

    let finish_reason = parsed
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let reasoning_chars = parsed
        .pointer("/choices/0/message/reasoning_content")
        .and_then(Value::as_str)
        .map(|value| value.trim().chars().count())
        .unwrap_or(0);
    let status = if finish_reason.eq_ignore_ascii_case("length") || reasoning_chars > 0 {
        "PROVIDER_INCOMPLETE_RESPONSE"
    } else {
        "PROVIDER_EMPTY_CONTENT"
    };

    Err(sanitize_text(
        &format!(
            "{status}: DeepSeek did not return final assistant content; finish_reason={finish_reason}; reasoning_content_chars={reasoning_chars}; raw provider JSON omitted from artifact"
        ),
        240,
    ))
}

fn deepseek_error_tone(status: &str) -> &'static str {
    if status.trim().eq_ignore_ascii_case("STOPPED_BY_USER") {
        "blocked"
    } else {
        "error"
    }
}

pub(crate) fn deepseek_model() -> String {
    first_env_value(&["MAESTRO_DEEPSEEK_MODEL", "CROSS_REVIEW_DEEPSEEK_MODEL"])
        .map(|(_, _, value)| sanitize_short(&value, 120))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "deepseek-v4-pro".to_string())
}

pub(crate) fn resolve_deepseek_model(client: &Client, api_key: &str) -> String {
    if let Some((_, _, value)) =
        first_env_value(&["MAESTRO_DEEPSEEK_MODEL", "CROSS_REVIEW_DEEPSEEK_MODEL"])
    {
        let model = sanitize_short(&value, 120);
        if !model.is_empty() {
            return model;
        }
    }

    let response = client
        .get("https://api.deepseek.com/models")
        .bearer_auth(api_key)
        .send();
    if let Ok(response) = response {
        if response.status().is_success() {
            let body = response.text().unwrap_or_default();
            if let Ok(value) = serde_json::from_str::<Value>(&body) {
                let models = deepseek_model_ids(&value);
                for candidate in [
                    "deepseek-v4-pro",
                    "deepseek-reasoner",
                    "deepseek-chat",
                    "deepseek-v4-flash",
                ] {
                    if models.iter().any(|model| model == candidate) {
                        return candidate.to_string();
                    }
                }
                if let Some(first) = models.first() {
                    return first.clone();
                }
            }
        }
    }

    "deepseek-reasoner".to_string()
}

pub(crate) fn deepseek_model_ids(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .map(|id| sanitize_short(id, 120))
                })
                .filter(|id| !id.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deepseek_model_ids_extract_current_api_shape() {
        let value = json!({
            "object": "list",
            "data": [
                { "id": "deepseek-v4-flash", "object": "model" },
                { "id": "deepseek-v4-pro", "object": "model" }
            ]
        });

        assert_eq!(
            deepseek_model_ids(&value),
            vec![
                "deepseek-v4-flash".to_string(),
                "deepseek-v4-pro".to_string()
            ]
        );
    }

    #[test]
    fn deepseek_assistant_content_extracts_final_content_only() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "reasoning_content": "internal reasoning",
                        "content": "MAESTRO_STATUS: READY\nFinal text."
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        assert_eq!(
            deepseek_assistant_content(&value).unwrap(),
            "MAESTRO_STATUS: READY\nFinal text."
        );
    }

    #[test]
    fn deepseek_assistant_content_rejects_reasoning_only_json() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "reasoning_content": "reasoned for a long time",
                        "content": ""
                    },
                    "finish_reason": "length"
                }
            ],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        });

        let error = deepseek_assistant_content(&value).unwrap_err();

        assert!(error.starts_with("PROVIDER_INCOMPLETE_RESPONSE"));
        assert!(error.contains("raw provider JSON omitted"));
    }

    #[test]
    fn deepseek_assistant_content_rejects_empty_content_without_reasoning() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "content": ""
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        let error = deepseek_assistant_content(&value).unwrap_err();

        assert!(error.starts_with("PROVIDER_EMPTY_CONTENT"));
        assert!(error.contains("raw provider JSON omitted"));
    }

    #[test]
    fn deepseek_stop_by_user_is_blocked_not_error() {
        assert_eq!(deepseek_error_tone("STOPPED_BY_USER"), "blocked");
        assert_eq!(deepseek_error_tone("PROVIDER_EMPTY_CONTENT"), "error");
    }
}
