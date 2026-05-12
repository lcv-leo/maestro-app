// Modulo: src-tauri/src/provider_perplexity.rs
// Descricao: Perplexity Sonar API peer runner for Maestro Editorial AI.
//
// Perplexity is API-only in maestro-app. The integration uses the Sonar
// endpoint directly because Sonar adds web search/citation behavior that is
// materially different from the OpenAI-compatible peers.

use std::time::Instant;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::provider_retry::{
    build_api_client_async, provider_http_error_status, provider_reqwest_error_status,
    send_with_retry_async, ProviderRequestOutcome,
};
use crate::provider_runners::{
    api_cost_preflight_result, editorial_api_system_prompt, log_provider_api_started,
    log_provider_cache_configured, write_provider_error_result, write_provider_failure_result,
    write_provider_missing_key_result, write_provider_success_result, EditorialAgentRequest,
    ProviderInvocation,
};
use crate::session_controls::{
    api_role_max_tokens, provider_cache_plan, provider_cache_telemetry_with_plan, provider_cost,
    usage_tokens,
};
use crate::{
    api_error_message, api_input_estimate_chars, first_env_value, provider_key_for_agent,
    provider_remote_present, sanitize_short, sanitize_text,
};

const PERPLEXITY_ENDPOINT: &str = "https://api.perplexity.ai/v1/sonar";

pub(crate) async fn run_perplexity_api_agent(
    request: EditorialAgentRequest<'_>,
    cancel_token: &CancellationToken,
) -> crate::EditorialAgentResult {
    let EditorialAgentRequest {
        log_session,
        run_id,
        role,
        prompt,
        attachments,
        output_path,
        timeout,
        config,
        cost_guard,
    } = request;
    let started = Instant::now();
    let name = "Perplexity";
    let cli = "perplexity-api";
    let provider = "perplexity";
    let model = perplexity_model();
    let invocation = ProviderInvocation {
        log_session,
        run_id,
        name,
        cli,
        provider,
        role,
        output_path,
    };

    let Some((api_key, key_source)) = provider_key_for_agent(config, "perplexity") else {
        return write_provider_missing_key_result(
            &invocation,
            &model,
            provider_remote_present(config, "perplexity"),
        );
    };

    let max_tokens = api_role_max_tokens(role);
    let input_estimate_chars = api_input_estimate_chars(&prompt, attachments, provider);
    if let Some(result) = api_cost_preflight_result(
        &invocation,
        input_estimate_chars,
        max_tokens,
        cost_guard.as_ref(),
        started.elapsed().as_millis(),
    ) {
        return result;
    }

    let async_client = match build_api_client_async(timeout) {
        Ok(client) => client,
        Err(error) => {
            let status = provider_reqwest_error_status("CLIENT_ERROR", error);
            return write_provider_error_result(
                &invocation,
                &model,
                &status,
                started.elapsed().as_millis(),
            );
        }
    };
    let system_prompt = editorial_api_system_prompt(name);
    let cache_plan = provider_cache_plan(provider, &model, role, name, &system_prompt);
    log_provider_api_started(
        log_session,
        run_id,
        name,
        cli,
        provider,
        role,
        &model,
        prompt.chars().count(),
        output_path,
        timeout,
        cost_guard.as_ref(),
    );
    log_provider_cache_configured(
        log_session,
        run_id,
        provider,
        &model,
        role,
        output_path,
        prompt.chars().count(),
        &cache_plan,
    );

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": prompt }
        ],
        "stream": false,
        "max_tokens": max_tokens,
        "temperature": 0.2,
        "top_p": 0.9,
        "search_mode": "web",
        "reasoning_effort": "high",
        "web_search_options": {
            "search_context_size": "high"
        },
        "return_images": false,
        "return_related_questions": false
    });
    let request_builder = async_client
        .post(PERPLEXITY_ENDPOINT)
        .bearer_auth(&api_key)
        .json(&body);
    let response = match send_with_retry_async(
        log_session,
        run_id,
        "perplexity",
        cancel_token,
        request_builder,
    )
    .await
    {
        Ok(response) => response,
        Err(ProviderRequestOutcome::Cancelled) => {
            return write_provider_failure_result(
                &invocation,
                &model,
                "STOPPED_BY_USER",
                "blocked",
                "Sessao parada pelo operador antes da resposta do provedor.",
                started.elapsed().as_millis(),
                None,
            );
        }
        Err(ProviderRequestOutcome::Network(error)) => {
            let status = provider_reqwest_error_status("PROVIDER_NETWORK_ERROR", error);
            return write_provider_error_result(
                &invocation,
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
            return write_provider_failure_result(
                &invocation,
                &model,
                "STOPPED_BY_USER",
                "blocked",
                "Sessao parada pelo operador durante leitura da resposta do provedor.",
                started.elapsed().as_millis(),
                None,
            );
        }
        r = response.text() => r.unwrap_or_default(),
    };

    if !http_status.is_success() {
        let status =
            provider_http_error_status(http_status.as_u16(), &api_error_message(&body_text));
        return write_provider_error_result(
            &invocation,
            &model,
            &status,
            started.elapsed().as_millis(),
        );
    }

    let parsed: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| json!({}));
    let stdout = perplexity_response_text(&parsed)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();
    if stdout.trim().is_empty() {
        return write_provider_error_result(
            &invocation,
            &model,
            "PROVIDER_EMPTY_CONTENT",
            started.elapsed().as_millis(),
        );
    }
    log_perplexity_sources(log_session, run_id, &parsed, output_path);
    let (usage_input_tokens, usage_output_tokens) = usage_tokens(&parsed);
    let cache = Some(provider_cache_telemetry_with_plan(&cache_plan, None));
    let cost_usd = cost_guard.as_ref().and_then(|guard| {
        usage_input_tokens
            .zip(usage_output_tokens)
            .map(|(input, output)| provider_cost(input, output, guard.rates))
    });
    let model_reported = parsed
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&model);
    write_provider_success_result(
        log_session,
        run_id,
        name,
        cli,
        provider,
        role,
        output_path,
        &model,
        model_reported,
        &key_source,
        &stdout,
        usage_input_tokens,
        usage_output_tokens,
        cost_usd,
        cache,
        started.elapsed().as_millis(),
        prompt.chars().count(),
        PERPLEXITY_ENDPOINT,
    )
}

pub(crate) fn perplexity_model() -> String {
    first_env_value(&["MAESTRO_PERPLEXITY_MODEL", "PERPLEXITY_MODEL"])
        .map(|(_, _, value)| sanitize_short(&value, 120))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "sonar-reasoning-pro".to_string())
}

pub(crate) fn perplexity_response_text(value: &Value) -> Option<String> {
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn log_perplexity_sources(
    log_session: &LogSession,
    run_id: &str,
    parsed: &Value,
    output_path: &std::path::Path,
) {
    let citations = parsed
        .get("citations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|value| sanitize_text(value, 300))
                .filter(|value| !value.is_empty())
                .take(12)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let search_results = parsed
        .get("search_results")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let title = item.get("title").and_then(Value::as_str).unwrap_or("");
                    let url = item.get("url").and_then(Value::as_str).unwrap_or("");
                    if title.trim().is_empty() && url.trim().is_empty() {
                        return None;
                    }
                    Some(json!({
                        "title": sanitize_text(title, 180),
                        "url": sanitize_text(url, 300),
                    }))
                })
                .take(12)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if citations.is_empty() && search_results.is_empty() {
        return;
    }
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.provider.perplexity.sources".to_string(),
            message: "perplexity returned source metadata".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(run_id, 120),
                "provider": "perplexity",
                "citations": citations,
                "search_results": search_results,
                "output_path": output_path.to_string_lossy().to_string(),
            })),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perplexity_response_text_extracts_sonar_message_content() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "MAESTRO_STATUS: READY\nReview approved."
                    }
                }
            ]
        });

        assert_eq!(
            perplexity_response_text(&value).unwrap(),
            "MAESTRO_STATUS: READY\nReview approved."
        );
    }
}
