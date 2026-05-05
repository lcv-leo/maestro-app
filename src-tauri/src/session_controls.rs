use serde_json::Value;

use crate::{
    editorial_agent_specs, resolve_initial_agent_key, sanitize_text, CostLedger, EditorialAgentSpec,
};

const REVIEW_MAX_TOKENS: u64 = 4096;
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
    if selected.is_empty() || selected.len() > 4 {
        return Err("selecione de 1 a 4 peers editoriais".to_string());
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

#[cfg(test)]
mod tests {
    use super::{
        can_agent_review_current_draft, independent_review_agent_specs,
        selected_review_agent_specs, ReviewPanelSelectionError,
    };

    #[test]
    fn selected_review_agent_specs_excludes_current_draft_author() {
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
        ];

        let selected = selected_review_agent_specs("deepseek", &active, Some("deepseek"));
        let keys = selected
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["claude", "codex", "gemini"]);
    }

    #[test]
    fn selected_review_agent_specs_excludes_author_aliases_case_and_whitespace() {
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
        ];

        let selected = selected_review_agent_specs("claude", &active, Some("  OpenAI  "));
        let keys = selected
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["claude", "gemini", "deepseek"]);
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
}
