use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    editorial_agent_specs, resolve_initial_agent_key, sanitize_text, CostLedger,
    EditorialAgentSpec, ProviderCacheTelemetry,
};

const REVIEW_MAX_TOKENS: u64 = 20_000;
const DRAFT_MAX_TOKENS: u64 = 20_000;

#[derive(Clone, Copy)]
pub(crate) struct ProviderCostRates {
    pub(crate) input_usd_per_million: f64,
    pub(crate) output_usd_per_million: f64,
}

#[derive(Clone)]
pub(crate) struct ProviderCostGuard {
    pub(crate) max_session_cost_usd: Option<f64>,
    pub(crate) observed_cost_usd: f64,
    pub(crate) rates: ProviderCostRates,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderCachePlan {
    pub(crate) provider_mode: String,
    pub(crate) cache_key: String,
    pub(crate) cache_key_hash: String,
    pub(crate) cache_control_status: String,
    pub(crate) cache_retention: Option<String>,
    pub(crate) stable_prefix_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewPanelSelectionError {
    DraftAuthorUnknown,
}

pub(crate) fn all_agent_keys() -> Vec<String> {
    editorial_agent_specs()
        .into_iter()
        .map(|spec| spec.key.to_string())
        .collect()
}

pub(crate) fn normalize_active_agents(values: Option<&Vec<String>>) -> Result<Vec<String>, String> {
    let mut selected = Vec::new();
    let candidates = values
        .cloned()
        .unwrap_or_else(all_agent_keys)
        .into_iter()
        .collect::<Vec<_>>();
    for value in candidates {
        let key = match canonical_editorial_agent_key(&value) {
            Some(key) => key,
            None if value.trim().is_empty() => continue,
            None => {
                return Err(format!(
                    "agente editorial desconhecido: {}",
                    sanitize_text(&value, 80)
                ))
            }
        };
        if !selected.iter().any(|existing| existing == key) {
            selected.push(key.to_string());
        }
    }
    if selected.is_empty() || selected.len() > 6 {
        return Err("selecione de 1 a 6 peers editoriais".to_string());
    }
    Ok(selected)
}

pub(crate) fn canonical_editorial_agent_key(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "claude" | "anthropic" => Some("claude"),
        "codex" | "openai" | "chatgpt" => Some("codex"),
        "gemini" | "google" => Some("gemini"),
        "deepseek" | "deepseek-api" => Some("deepseek"),
        "grok" | "xai" | "grok-api" => Some("grok"),
        "perplexity" | "sonar" | "perplexity-api" => Some("perplexity"),
        _ => None,
    }
}

pub(crate) fn selected_editorial_agent_specs(
    first_key: &str,
    active_agents: &[String],
) -> Vec<EditorialAgentSpec> {
    let active = active_agents
        .iter()
        .map(|value| value.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    crate::ordered_editorial_agent_specs(first_key)
        .into_iter()
        .filter(|spec| active.contains(spec.key))
        .collect()
}

pub(crate) fn selected_review_agent_specs(
    first_key: &str,
    active_agents: &[String],
    current_draft_author_key: Option<&str>,
) -> Vec<EditorialAgentSpec> {
    let author = current_draft_author_key.and_then(canonical_editorial_agent_key);
    selected_editorial_agent_specs(first_key, active_agents)
        .into_iter()
        .filter(|spec| can_agent_review_current_draft(spec.key, author))
        .collect()
}

pub(crate) fn independent_review_agent_specs(
    first_key: &str,
    active_agents: &[String],
    current_draft_author_key: Option<&str>,
) -> Result<Vec<EditorialAgentSpec>, ReviewPanelSelectionError> {
    let Some(author) = current_draft_author_key.and_then(canonical_editorial_agent_key) else {
        return Err(ReviewPanelSelectionError::DraftAuthorUnknown);
    };
    Ok(selected_review_agent_specs(
        first_key,
        active_agents,
        Some(author),
    ))
}

pub(crate) fn can_agent_review_current_draft(
    candidate_key: &str,
    current_draft_author_key: Option<&str>,
) -> bool {
    // Colegiate-review invariant: the current draft/revision author is the
    // petitioner for this cycle, not a voting reviewer of that same text.
    // Fail closed when the candidate and author normalize to the same agent.
    let Some(candidate) = canonical_editorial_agent_key(candidate_key) else {
        return false;
    };
    let author = current_draft_author_key.and_then(canonical_editorial_agent_key);
    author != Some(candidate)
}

pub(crate) fn effective_draft_lead(
    requested: Option<&str>,
    active_agents: &[String],
) -> (&'static str, Option<String>) {
    let (requested_key, invalid) = resolve_initial_agent_key(requested);
    if active_agents.iter().any(|key| key == requested_key) {
        return (requested_key, invalid);
    }
    let fallback = active_agents
        .iter()
        .find_map(|key| match key.as_str() {
            "claude" => Some("claude"),
            "codex" => Some("codex"),
            "gemini" => Some("gemini"),
            "deepseek" => Some("deepseek"),
            "grok" => Some("grok"),
            "perplexity" => Some("perplexity"),
            _ => None,
        })
        .unwrap_or("claude");
    (
        fallback,
        requested.map(|value| sanitize_text(value, 80)).or(invalid),
    )
}

pub(crate) fn sanitize_optional_positive_f64(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

pub(crate) fn sanitize_optional_positive_u64(value: Option<u64>) -> Option<u64> {
    value.filter(|value| *value > 0)
}

pub(crate) fn api_role_max_tokens(role: &str) -> u64 {
    if role == "review" {
        REVIEW_MAX_TOKENS
    } else {
        DRAFT_MAX_TOKENS
    }
}

pub(crate) fn estimate_provider_cost(
    prompt: &str,
    max_output_tokens: u64,
    rates: ProviderCostRates,
) -> f64 {
    estimate_provider_cost_from_input_chars(prompt.chars().count(), max_output_tokens, rates)
}

pub(crate) fn estimate_provider_cost_from_input_chars(
    input_chars: usize,
    max_output_tokens: u64,
    rates: ProviderCostRates,
) -> f64 {
    let input_tokens = ((input_chars as f64) / 4.0).ceil() as u64;
    provider_cost(input_tokens, max_output_tokens, rates)
}

pub(crate) fn provider_cost(
    input_tokens: u64,
    output_tokens: u64,
    rates: ProviderCostRates,
) -> f64 {
    (input_tokens as f64 / 1_000_000.0 * rates.input_usd_per_million)
        + (output_tokens as f64 / 1_000_000.0 * rates.output_usd_per_million)
}

pub(crate) fn provider_cost_guard_for(
    max_session_cost_usd: Option<f64>,
    rates: Option<ProviderCostRates>,
    ledger: &CostLedger,
) -> Option<ProviderCostGuard> {
    Some(ProviderCostGuard {
        max_session_cost_usd,
        observed_cost_usd: ledger.total_observed_cost_usd,
        rates: rates?,
    })
}

fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn openai_supports_extended_prompt_cache(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.starts_with("gpt-5.2")
        || model.starts_with("gpt-5.1")
        || model == "gpt-5"
        || model.starts_with("gpt-5-codex")
        || model.starts_with("gpt-4.1")
}

pub(crate) fn provider_cache_plan(
    provider: &str,
    model: &str,
    role: &str,
    agent_name: &str,
    system_prompt: &str,
) -> ProviderCachePlan {
    let stable_prefix = format!("{provider}\n{model}\n{role}\n{agent_name}\n{system_prompt}");
    let cache_key_hash = sha256_hex(&stable_prefix);
    let cache_key = format!(
        "maestro-{provider}-{role}-{}",
        cache_key_hash.chars().take(32).collect::<String>()
    );
    let (provider_mode, cache_control_status, cache_retention) = match provider {
        "openai" if openai_supports_extended_prompt_cache(model) => (
            "prompt_cache_key".to_string(),
            "prompt_cache_key_24h".to_string(),
            Some("24h".to_string()),
        ),
        "openai" => (
            "prompt_cache_key".to_string(),
            "prompt_cache_key_default_retention".to_string(),
            None,
        ),
        "anthropic" => (
            "cache_control".to_string(),
            "system_block_cache_control_5m".to_string(),
            Some("5m".to_string()),
        ),
        "gemini" => (
            "implicit_prefix".to_string(),
            "implicit_cache_thinking_preserved".to_string(),
            None,
        ),
        "deepseek" => (
            "automatic_disk_prefix".to_string(),
            "automatic_context_cache".to_string(),
            None,
        ),
        "grok" => (
            "prompt_cache_key".to_string(),
            "prompt_cache_key".to_string(),
            None,
        ),
        "perplexity" => (
            "automatic_prefix".to_string(),
            "sonar_no_documented_prompt_cache_control".to_string(),
            None,
        ),
        _ => (
            "automatic_prefix".to_string(),
            "automatic_or_unconfigured".to_string(),
            None,
        ),
    };

    ProviderCachePlan {
        provider_mode,
        cache_key,
        cache_key_hash,
        cache_control_status,
        cache_retention,
        stable_prefix_chars: stable_prefix.chars().count(),
    }
}

pub(crate) fn provider_cache_telemetry_with_plan(
    plan: &ProviderCachePlan,
    observed: Option<ProviderCacheTelemetry>,
) -> ProviderCacheTelemetry {
    let mut cache = observed.unwrap_or_else(|| ProviderCacheTelemetry {
        provider_mode: plan.provider_mode.clone(),
        cache_key_hash: None,
        cache_control_status: None,
        cache_retention: None,
        cached_input_tokens: None,
        cache_hit_tokens: None,
        cache_miss_tokens: None,
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
    });
    cache.cache_key_hash = Some(plan.cache_key_hash.clone());
    cache.cache_control_status = Some(plan.cache_control_status.clone());
    cache.cache_retention = plan.cache_retention.clone();
    cache
}

pub(crate) fn usage_tokens(parsed: &Value) -> (Option<u64>, Option<u64>) {
    let input = parsed
        .pointer("/usage/prompt_tokens")
        .or_else(|| parsed.pointer("/usage/input_tokens"))
        .and_then(Value::as_u64);
    let output = parsed
        .pointer("/usage/completion_tokens")
        .or_else(|| parsed.pointer("/usage/output_tokens"))
        .and_then(Value::as_u64);
    (input, output)
}

fn first_u64_at(value: &Value, pointers: &[&str]) -> Option<u64> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_u64))
}

fn cache_telemetry_if_present(cache: ProviderCacheTelemetry) -> Option<ProviderCacheTelemetry> {
    if cache.cached_input_tokens.is_some()
        || cache.cache_hit_tokens.is_some()
        || cache.cache_miss_tokens.is_some()
        || cache.cache_read_input_tokens.is_some()
        || cache.cache_creation_input_tokens.is_some()
    {
        Some(cache)
    } else {
        None
    }
}

pub(crate) fn provider_cache_telemetry(
    provider: &str,
    parsed: &Value,
    usage_input_tokens: Option<u64>,
) -> Option<ProviderCacheTelemetry> {
    match provider {
        "openai" => {
            let cached = first_u64_at(
                parsed,
                &[
                    "/usage/input_tokens_details/cached_tokens",
                    "/usage/prompt_tokens_details/cached_tokens",
                ],
            );
            cache_telemetry_if_present(ProviderCacheTelemetry {
                provider_mode: "automatic_prefix".to_string(),
                cache_key_hash: None,
                cache_control_status: None,
                cache_retention: None,
                cached_input_tokens: cached,
                cache_hit_tokens: cached,
                cache_miss_tokens: usage_input_tokens
                    .zip(cached)
                    .map(|(input, cached)| input.saturating_sub(cached)),
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            })
        }
        "anthropic" => {
            let read = parsed
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64);
            let creation = parsed
                .pointer("/usage/cache_creation_input_tokens")
                .and_then(Value::as_u64);
            cache_telemetry_if_present(ProviderCacheTelemetry {
                provider_mode: "cache_control".to_string(),
                cache_key_hash: None,
                cache_control_status: None,
                cache_retention: None,
                cached_input_tokens: read,
                cache_hit_tokens: read,
                cache_miss_tokens: None,
                cache_read_input_tokens: read,
                cache_creation_input_tokens: creation,
            })
        }
        "gemini" => {
            let cached = first_u64_at(
                parsed,
                &[
                    "/usageMetadata/cachedContentTokenCount",
                    "/usageMetadata/cacheTokenCount",
                ],
            );
            cache_telemetry_if_present(ProviderCacheTelemetry {
                provider_mode: "explicit_resource_or_implicit".to_string(),
                cache_key_hash: None,
                cache_control_status: None,
                cache_retention: None,
                cached_input_tokens: cached,
                cache_hit_tokens: cached,
                cache_miss_tokens: usage_input_tokens
                    .zip(cached)
                    .map(|(input, cached)| input.saturating_sub(cached)),
                cache_read_input_tokens: cached,
                cache_creation_input_tokens: None,
            })
        }
        "deepseek" => {
            let hit = parsed
                .pointer("/usage/prompt_cache_hit_tokens")
                .and_then(Value::as_u64);
            let miss = parsed
                .pointer("/usage/prompt_cache_miss_tokens")
                .and_then(Value::as_u64);
            cache_telemetry_if_present(ProviderCacheTelemetry {
                provider_mode: "automatic_prefix".to_string(),
                cache_key_hash: None,
                cache_control_status: None,
                cache_retention: None,
                cached_input_tokens: hit,
                cache_hit_tokens: hit,
                cache_miss_tokens: miss,
                cache_read_input_tokens: hit,
                cache_creation_input_tokens: None,
            })
        }
        "grok" => {
            let cached = first_u64_at(
                parsed,
                &[
                    "/usage/cached_prompt_text_tokens",
                    "/usage/input_tokens_details/cached_tokens",
                    "/usage/prompt_tokens_details/cached_tokens",
                    "/usage/cached_tokens",
                ],
            );
            let miss = first_u64_at(
                parsed,
                &[
                    "/usage/prompt_cache_miss_tokens",
                    "/usage/cache_miss_tokens",
                ],
            )
            .or_else(|| {
                usage_input_tokens
                    .zip(cached)
                    .map(|(input, cached)| input.saturating_sub(cached))
            });
            cache_telemetry_if_present(ProviderCacheTelemetry {
                provider_mode: "automatic_prefix".to_string(),
                cache_key_hash: None,
                cache_control_status: None,
                cache_retention: None,
                cached_input_tokens: cached,
                cache_hit_tokens: cached,
                cache_miss_tokens: miss,
                cache_read_input_tokens: cached,
                cache_creation_input_tokens: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        can_agent_review_current_draft, independent_review_agent_specs, provider_cache_plan,
        provider_cache_telemetry, provider_cache_telemetry_with_plan, selected_review_agent_specs,
        ReviewPanelSelectionError,
    };
    use serde_json::json;

    #[test]
    fn selected_review_agent_specs_excludes_current_draft_author() {
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
            "grok".to_string(),
            "perplexity".to_string(),
        ];

        let selected = selected_review_agent_specs("deepseek", &active, Some("deepseek"));
        let keys = selected
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["claude", "codex", "gemini", "grok", "perplexity"]
        );
    }

    #[test]
    fn selected_review_agent_specs_excludes_author_aliases_case_and_whitespace() {
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
            "grok".to_string(),
            "perplexity".to_string(),
        ];

        let selected = selected_review_agent_specs("claude", &active, Some("  OpenAI  "));
        let keys = selected
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["claude", "gemini", "deepseek", "grok", "perplexity"]
        );
    }

    #[test]
    fn selected_review_agent_specs_returns_empty_when_only_author_is_active() {
        let active = vec!["deepseek".to_string()];
        let selected = selected_review_agent_specs("deepseek", &active, Some("deepseek"));

        assert!(selected.is_empty());
    }

    #[test]
    fn can_agent_review_current_draft_fails_closed_for_same_normalized_agent() {
        assert!(!can_agent_review_current_draft("codex", Some("openai")));
        assert!(!can_agent_review_current_draft("gemini", Some(" GOOGLE ")));
        assert!(!can_agent_review_current_draft("grok", Some("xai")));
        assert!(!can_agent_review_current_draft("perplexity", Some("sonar")));
        assert!(can_agent_review_current_draft("claude", Some("codex")));
        assert!(can_agent_review_current_draft("deepseek", None));
    }

    #[test]
    fn independent_review_agent_specs_requires_known_current_author() {
        let active = vec!["claude".to_string(), "codex".to_string()];

        assert!(matches!(
            independent_review_agent_specs("claude", &active, None),
            Err(ReviewPanelSelectionError::DraftAuthorUnknown)
        ));
        assert!(matches!(
            independent_review_agent_specs("claude", &active, Some("unknown")),
            Err(ReviewPanelSelectionError::DraftAuthorUnknown)
        ));
    }

    #[test]
    fn independent_review_agent_specs_returns_only_non_author_panel() {
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
        ];
        let selected = independent_review_agent_specs("claude", &active, Some("anthropic"))
            .unwrap()
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();

        assert_eq!(selected, vec!["codex", "gemini"]);
    }

    #[test]
    fn independent_review_agent_specs_returns_empty_when_only_author_is_active() {
        let active = vec!["claude".to_string()];
        let selected = independent_review_agent_specs("claude", &active, Some("claude")).unwrap();

        assert!(selected.is_empty());
    }

    #[test]
    fn provider_cache_telemetry_reads_openai_cached_tokens() {
        let parsed = json!({
            "usage": {
                "input_tokens": 1200,
                "output_tokens": 100,
                "input_tokens_details": { "cached_tokens": 1024 }
            }
        });

        let cache = provider_cache_telemetry("openai", &parsed, Some(1200)).unwrap();

        assert_eq!(cache.provider_mode, "automatic_prefix");
        assert_eq!(cache.cached_input_tokens, Some(1024));
        assert_eq!(cache.cache_hit_tokens, Some(1024));
        assert_eq!(cache.cache_miss_tokens, Some(176));
    }

    #[test]
    fn provider_cache_telemetry_reads_anthropic_cache_usage() {
        let parsed = json!({
            "usage": {
                "input_tokens": 20,
                "output_tokens": 80,
                "cache_creation_input_tokens": 4096,
                "cache_read_input_tokens": 2048
            }
        });

        let cache = provider_cache_telemetry("anthropic", &parsed, Some(20)).unwrap();

        assert_eq!(cache.provider_mode, "cache_control");
        assert_eq!(cache.cache_creation_input_tokens, Some(4096));
        assert_eq!(cache.cache_read_input_tokens, Some(2048));
        assert_eq!(cache.cached_input_tokens, Some(2048));
    }

    #[test]
    fn provider_cache_telemetry_reads_deepseek_hit_and_miss_tokens() {
        let parsed = json!({
            "usage": {
                "prompt_tokens": 2000,
                "completion_tokens": 100,
                "prompt_cache_hit_tokens": 1500,
                "prompt_cache_miss_tokens": 500
            }
        });

        let cache = provider_cache_telemetry("deepseek", &parsed, Some(2000)).unwrap();

        assert_eq!(cache.provider_mode, "automatic_prefix");
        assert_eq!(cache.cache_hit_tokens, Some(1500));
        assert_eq!(cache.cache_miss_tokens, Some(500));
    }

    #[test]
    fn provider_cache_plan_uses_extended_openai_retention_when_supported() {
        let plan = provider_cache_plan("openai", "gpt-5.2", "draft", "Codex", "system");

        assert_eq!(plan.provider_mode, "prompt_cache_key");
        assert_eq!(plan.cache_control_status, "prompt_cache_key_24h");
        assert_eq!(plan.cache_retention.as_deref(), Some("24h"));
        assert!(plan.cache_key.starts_with("maestro-openai-draft-"));
    }

    #[test]
    fn provider_cache_plan_omits_extended_openai_retention_for_unknown_future_models() {
        let plan = provider_cache_plan("openai", "gpt-5.5", "draft", "Codex", "system");

        assert_eq!(plan.provider_mode, "prompt_cache_key");
        assert_eq!(
            plan.cache_control_status,
            "prompt_cache_key_default_retention"
        );
        assert_eq!(plan.cache_retention, None);
    }

    #[test]
    fn provider_cache_plan_keeps_perplexity_without_invented_payload_fields() {
        let plan = provider_cache_plan(
            "perplexity",
            "sonar-reasoning-pro",
            "review",
            "Perplexity",
            "system",
        );

        assert_eq!(plan.provider_mode, "automatic_prefix");
        assert_eq!(
            plan.cache_control_status,
            "sonar_no_documented_prompt_cache_control"
        );
        assert_eq!(plan.cache_retention, None);
    }

    #[test]
    fn provider_cache_telemetry_with_plan_attaches_nonsecret_cache_identity() {
        let plan = provider_cache_plan("grok", "grok-4.3", "review", "Grok", "system");
        let cache = provider_cache_telemetry_with_plan(&plan, None);

        assert_eq!(cache.provider_mode, "prompt_cache_key");
        assert_eq!(cache.cache_key_hash, Some(plan.cache_key_hash));
        assert_eq!(
            cache.cache_control_status.as_deref(),
            Some("prompt_cache_key")
        );
        assert_eq!(cache.cached_input_tokens, None);
    }

    #[test]
    fn provider_cache_telemetry_reads_gemini_cached_tokens() {
        let parsed = json!({
            "usageMetadata": {
                "promptTokenCount": 4096,
                "candidatesTokenCount": 200,
                "cachedContentTokenCount": 2048
            }
        });

        let cache = provider_cache_telemetry("gemini", &parsed, Some(4096)).unwrap();

        assert_eq!(cache.provider_mode, "explicit_resource_or_implicit");
        assert_eq!(cache.cache_hit_tokens, Some(2048));
        assert_eq!(cache.cache_miss_tokens, Some(2048));
    }

    #[test]
    fn provider_cache_telemetry_reads_grok_cached_tokens() {
        let parsed = json!({
            "usage": {
                "input_tokens": 1200,
                "output_tokens": 100,
                "input_tokens_details": { "cached_tokens": 512 }
            }
        });

        let cache = provider_cache_telemetry("grok", &parsed, Some(1200)).unwrap();

        assert_eq!(cache.provider_mode, "automatic_prefix");
        assert_eq!(cache.cache_hit_tokens, Some(512));
        assert_eq!(cache.cache_miss_tokens, Some(688));
    }
}
