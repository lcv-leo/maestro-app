// Modulo: src-tauri/src/provider_config.rs
// Descricao: AI provider config validation, sanitization, mode-normalization,
// and routing helpers extracted from lib.rs in v0.3.40.
//
// This module owns the value-level logic that decides which provider runs each
// peer (`should_run_agent_via_api`), normalizes operator-supplied free-form
// strings into a closed enum surface (`normalize_provider_mode`,
// `normalize_storage_mode`, `normalize_cloudflare_token_source`), produces the
// canonical `AiProviderConfig` artifact (`sanitize_ai_provider_config`), and
// reads provider tariff rates and env-var fallbacks. The v0.3.38 bugfix landed
// here too (`should_run_agent_via_api` is now strict and deterministic per
// `provider_mode`, with hybrid sending API-only peers to the API runner and
// Claude/Codex/Gemini to CLI).
//
// What's here:
//   - `normalize_storage_mode` / `normalize_provider_mode` /
//     `normalize_cloudflare_token_source` — collapse free-form strings into
//     fixed string literals; unknown values fall through to a safe default
//     ("local_json" / "hybrid" / "prompt_each_launch").
//   - `sanitize_optional_secret` (trims, caps to 4096 chars, drops empties)
//     and `sanitize_optional_cost_rate` (filters Option<f64> to finite > 0
//     and <= 10_000.0).
//   - `sanitize_ai_provider_config` — full per-field config builder used by
//     read/write Tauri commands and by the env-merge layer.
//   - `merge_ai_provider_env_values` + `provider_env_value` — env var fallback
//     when individual API keys are absent in the config (reads
//     MAESTRO_<PROVIDER>_API_KEY then <PROVIDER>_API_KEY).
//   - `provider_cost_rates_from_config` — builds the per-agent
//     `ProviderCostRates` for cost-guard preflight, returning a Portuguese
//     error string keyed to the operator UI when a tariff is missing.
//   - `api_provider_for_agent` — the agent_key -> provider_label mapping
//     ("claude" -> "anthropic", "codex" -> "openai", etc.).
//   - `should_run_agent_via_api` — the routing decision called once per
//     session start; returns false when the agent has no API provider OR
//     `provider_mode == "cli"`, true when "api", and the
//     identity-deterministic hybrid branch otherwise.
//
// What stayed in lib.rs:
//   - `AiProviderConfig` and `BootstrapConfig` structs (still consumed by
//     Tauri commands at the registry boundary).
//   - The CLI peer routing helpers (`api_cli_for_agent`,
//     `provider_label_for_agent`, `provider_remote_present`,
//     `provider_key_for_agent`) — already `pub(crate)` and tightly coupled
//     with `provider_runners.rs`.
//   - `effective_provider_key` — used by both `provider_key_for_agent`
//     (lib.rs) and `provider_runners.rs`; not part of the routing layer.
//
// v0.3.40 is a pure move: every signature, status string, format string and
// match arm is identical to the v0.3.39 lib.rs source (commit dc60061). The
// Unit tests pin the mode-routing invariants plus provider config sanitization.

use chrono::Utc;

use crate::session_controls::ProviderCostRates;
use crate::{first_env_value, sanitize_short, sanitize_text, AiProviderConfig};

pub(crate) fn normalize_storage_mode(value: &str) -> &'static str {
    match value {
        "windows_env" => "windows_env",
        "cloudflare" => "cloudflare",
        _ => "local_json",
    }
}

pub(crate) fn normalize_provider_mode(value: &str) -> &'static str {
    match value {
        "cli" => "cli",
        "api" => "api",
        _ => "hybrid",
    }
}

pub(crate) fn normalize_cloudflare_token_source(value: &str) -> &'static str {
    match value {
        "windows_env" => "windows_env",
        "local_encrypted" => "local_encrypted",
        _ => "prompt_each_launch",
    }
}

pub(crate) fn sanitize_ai_provider_config(config: AiProviderConfig) -> AiProviderConfig {
    AiProviderConfig {
        schema_version: 1,
        provider_mode: normalize_provider_mode(&config.provider_mode).to_string(),
        credential_storage_mode: normalize_storage_mode(&config.credential_storage_mode)
            .to_string(),
        openai_api_key: sanitize_optional_secret(config.openai_api_key),
        anthropic_api_key: sanitize_optional_secret(config.anthropic_api_key),
        gemini_api_key: sanitize_optional_secret(config.gemini_api_key),
        deepseek_api_key: sanitize_optional_secret(config.deepseek_api_key),
        grok_api_key: sanitize_optional_secret(config.grok_api_key),
        perplexity_api_key: sanitize_optional_secret(config.perplexity_api_key),
        openai_api_key_remote: config.openai_api_key_remote,
        anthropic_api_key_remote: config.anthropic_api_key_remote,
        gemini_api_key_remote: config.gemini_api_key_remote,
        deepseek_api_key_remote: config.deepseek_api_key_remote,
        grok_api_key_remote: config.grok_api_key_remote,
        perplexity_api_key_remote: config.perplexity_api_key_remote,
        openai_input_usd_per_million: sanitize_optional_cost_rate(
            config.openai_input_usd_per_million,
        ),
        openai_output_usd_per_million: sanitize_optional_cost_rate(
            config.openai_output_usd_per_million,
        ),
        anthropic_input_usd_per_million: sanitize_optional_cost_rate(
            config.anthropic_input_usd_per_million,
        ),
        anthropic_output_usd_per_million: sanitize_optional_cost_rate(
            config.anthropic_output_usd_per_million,
        ),
        gemini_input_usd_per_million: sanitize_optional_cost_rate(
            config.gemini_input_usd_per_million,
        ),
        gemini_output_usd_per_million: sanitize_optional_cost_rate(
            config.gemini_output_usd_per_million,
        ),
        deepseek_input_usd_per_million: sanitize_optional_cost_rate(
            config.deepseek_input_usd_per_million,
        ),
        deepseek_output_usd_per_million: sanitize_optional_cost_rate(
            config.deepseek_output_usd_per_million,
        ),
        grok_input_usd_per_million: sanitize_optional_cost_rate(config.grok_input_usd_per_million),
        grok_output_usd_per_million: sanitize_optional_cost_rate(
            config.grok_output_usd_per_million,
        ),
        perplexity_input_usd_per_million: sanitize_optional_cost_rate(
            config.perplexity_input_usd_per_million,
        ),
        perplexity_output_usd_per_million: sanitize_optional_cost_rate(
            config.perplexity_output_usd_per_million,
        ),
        cloudflare_secret_store_id: config
            .cloudflare_secret_store_id
            .map(|value| sanitize_short(&value, 80))
            .filter(|value| !value.is_empty()),
        cloudflare_secret_store_name: config
            .cloudflare_secret_store_name
            .map(|value| sanitize_short(&value, 80))
            .filter(|value| !value.is_empty()),
        updated_at: Utc::now().to_rfc3339(),
    }
}

pub(crate) fn sanitize_optional_cost_rate(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0 && *value <= 10_000.0)
}

pub(crate) fn provider_cost_rates_from_config(
    agent_key: &str,
    config: &AiProviderConfig,
) -> Result<ProviderCostRates, String> {
    let (label, input, output) = match agent_key {
        "claude" => (
            "Anthropic / Claude",
            config.anthropic_input_usd_per_million,
            config.anthropic_output_usd_per_million,
        ),
        "codex" => (
            "OpenAI / Codex",
            config.openai_input_usd_per_million,
            config.openai_output_usd_per_million,
        ),
        "gemini" => (
            "Google / Gemini",
            config.gemini_input_usd_per_million,
            config.gemini_output_usd_per_million,
        ),
        "deepseek" => (
            "DeepSeek",
            config.deepseek_input_usd_per_million,
            config.deepseek_output_usd_per_million,
        ),
        "grok" => (
            "Grok / xAI",
            config.grok_input_usd_per_million,
            config.grok_output_usd_per_million,
        ),
        "perplexity" => (
            "Perplexity / Sonar",
            config.perplexity_input_usd_per_million,
            config.perplexity_output_usd_per_million,
        ),
        _ => {
            return Err(format!(
                "Peer editorial sem provedor de tarifa conhecido: {}.",
                sanitize_text(agent_key, 80)
            ))
        }
    };
    let input = input.ok_or_else(|| {
        format!(
            "Configure a tarifa de entrada do provedor {label} em Configuracoes > Agentes via API > Tabela de tarifas."
        )
    })?;
    let output = output.ok_or_else(|| {
        format!(
            "Configure a tarifa de saida do provedor {label} em Configuracoes > Agentes via API > Tabela de tarifas."
        )
    })?;
    Ok(ProviderCostRates {
        input_usd_per_million: input,
        output_usd_per_million: output,
    })
}

pub(crate) fn api_provider_for_agent(agent_key: &str) -> Option<&'static str> {
    match agent_key {
        "claude" => Some("anthropic"),
        "codex" => Some("openai"),
        "gemini" => Some("gemini"),
        "deepseek" => Some("deepseek"),
        "grok" => Some("grok"),
        "perplexity" => Some("perplexity"),
        _ => None,
    }
}

pub(crate) fn should_run_agent_via_api(agent_key: &str, config: &AiProviderConfig) -> bool {
    if api_provider_for_agent(agent_key).is_none() {
        return false;
    }
    match normalize_provider_mode(&config.provider_mode) {
        "api" => true,
        "cli" => false,
        // "hybrid" is deterministic by agent identity, not credential availability:
        // DeepSeek, Grok and Perplexity run via API (no maestro-app CLI
        // integration exists); the other peers run via CLI. If a user wants
        // Claude/Codex/Gemini on API, they pick "api" mode explicitly.
        _ => matches!(agent_key, "deepseek" | "grok" | "perplexity"),
    }
}

pub(crate) fn sanitize_optional_secret(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().chars().take(4096).collect::<String>())
        .filter(|text| !text.is_empty())
}

pub(crate) fn merge_ai_provider_env_values(mut config: AiProviderConfig) -> AiProviderConfig {
    if config.openai_api_key.is_none() {
        config.openai_api_key = provider_env_value(&["MAESTRO_OPENAI_API_KEY", "OPENAI_API_KEY"]);
    }
    if config.anthropic_api_key.is_none() {
        config.anthropic_api_key =
            provider_env_value(&["MAESTRO_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"]);
    }
    if config.gemini_api_key.is_none() {
        config.gemini_api_key = provider_env_value(&["MAESTRO_GEMINI_API_KEY", "GEMINI_API_KEY"]);
    }
    if config.deepseek_api_key.is_none() {
        config.deepseek_api_key =
            provider_env_value(&["MAESTRO_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"]);
    }
    if config.grok_api_key.is_none() {
        config.grok_api_key =
            provider_env_value(&["MAESTRO_GROK_API_KEY", "GROK_API_KEY", "XAI_API_KEY"]);
    }
    if config.perplexity_api_key.is_none() {
        config.perplexity_api_key =
            provider_env_value(&["MAESTRO_PERPLEXITY_API_KEY", "PERPLEXITY_API_KEY"]);
    }
    sanitize_ai_provider_config(config)
}

pub(crate) fn provider_env_value(candidates: &[&str]) -> Option<String> {
    first_env_value(candidates).map(|(_, _, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_provider_config_trims_empty_secret_fields() {
        let config = sanitize_ai_provider_config(AiProviderConfig {
            schema_version: 99,
            provider_mode: "api".to_string(),
            credential_storage_mode: "windows_env".to_string(),
            openai_api_key: Some("  sk-test-value  ".to_string()),
            anthropic_api_key: Some("   ".to_string()),
            gemini_api_key: None,
            deepseek_api_key: Some("  ds-test-value  ".to_string()),
            grok_api_key: Some("  xai-test-value  ".to_string()),
            perplexity_api_key: Some("  pplx-test-value  ".to_string()),
            openai_api_key_remote: false,
            anthropic_api_key_remote: false,
            gemini_api_key_remote: false,
            deepseek_api_key_remote: false,
            grok_api_key_remote: false,
            perplexity_api_key_remote: false,
            openai_input_usd_per_million: Some(2.50),
            openai_output_usd_per_million: Some(10.0),
            anthropic_input_usd_per_million: Some(3.0),
            anthropic_output_usd_per_million: Some(15.0),
            gemini_input_usd_per_million: Some(1.25),
            gemini_output_usd_per_million: Some(5.0),
            deepseek_input_usd_per_million: Some(0.55),
            deepseek_output_usd_per_million: Some(2.19),
            grok_input_usd_per_million: Some(3.0),
            grok_output_usd_per_million: Some(15.0),
            perplexity_input_usd_per_million: Some(1.0),
            perplexity_output_usd_per_million: Some(1.0),
            cloudflare_secret_store_id: None,
            cloudflare_secret_store_name: None,
            updated_at: "old".to_string(),
        });

        assert_eq!(config.schema_version, 1);
        assert_eq!(config.provider_mode, "api");
        assert_eq!(config.credential_storage_mode, "windows_env");
        assert_eq!(config.openai_api_key.as_deref(), Some("sk-test-value"));
        assert!(config.anthropic_api_key.is_none());
        assert!(config.gemini_api_key.is_none());
        assert_eq!(config.deepseek_api_key.as_deref(), Some("ds-test-value"));
        assert_eq!(config.grok_api_key.as_deref(), Some("xai-test-value"));
        assert_eq!(
            config.perplexity_api_key.as_deref(),
            Some("pplx-test-value")
        );
    }

    #[test]
    fn should_run_agent_via_api_api_mode_routes_all_to_api() {
        let config = AiProviderConfig {
            provider_mode: "api".to_string(),
            ..AiProviderConfig::default()
        };
        for agent in [
            "claude",
            "codex",
            "gemini",
            "deepseek",
            "grok",
            "perplexity",
        ] {
            assert!(
                should_run_agent_via_api(agent, &config),
                "{agent} must run via API in api mode"
            );
        }
    }

    #[test]
    fn should_run_agent_via_api_cli_mode_routes_all_to_cli() {
        let config = AiProviderConfig {
            provider_mode: "cli".to_string(),
            ..AiProviderConfig::default()
        };
        for agent in [
            "claude",
            "codex",
            "gemini",
            "deepseek",
            "grok",
            "perplexity",
        ] {
            assert!(
                !should_run_agent_via_api(agent, &config),
                "{agent} must run via CLI in cli mode (API-only peers are gated before spawn)"
            );
        }
    }

    #[test]
    fn should_run_agent_via_api_hybrid_mode_routes_only_api_only_peers_to_api() {
        let config = AiProviderConfig {
            provider_mode: "hybrid".to_string(),
            ..AiProviderConfig::default()
        };
        assert!(!should_run_agent_via_api("claude", &config));
        assert!(!should_run_agent_via_api("codex", &config));
        assert!(!should_run_agent_via_api("gemini", &config));
        assert!(should_run_agent_via_api("deepseek", &config));
        assert!(should_run_agent_via_api("grok", &config));
        assert!(should_run_agent_via_api("perplexity", &config));
    }

    #[test]
    fn should_run_agent_via_api_hybrid_mode_is_deterministic_regardless_of_keys() {
        // Pre-v0.3.38, hybrid sent any agent with a configured key to the API runner.
        // Post-v0.3.38 it is deterministic by agent identity: hybrid lets
        // API-only peers (DeepSeek, Grok and Perplexity) join a CLI session. Other peers stay
        // on CLI even when their API key is set.
        let config = AiProviderConfig {
            provider_mode: "hybrid".to_string(),
            openai_api_key: Some("sk-test".to_string()),
            anthropic_api_key: Some("sk-ant-test".to_string()),
            gemini_api_key: Some("AIza-test".to_string()),
            deepseek_api_key: None,
            grok_api_key: None,
            perplexity_api_key: None,
            ..AiProviderConfig::default()
        };
        assert!(!should_run_agent_via_api("claude", &config));
        assert!(!should_run_agent_via_api("codex", &config));
        assert!(!should_run_agent_via_api("gemini", &config));
        // API-only peers go API even without keys — the API runner emits clear
        // missing-key failure artifacts instead of silently falling back.
        assert!(should_run_agent_via_api("deepseek", &config));
        assert!(should_run_agent_via_api("grok", &config));
        assert!(should_run_agent_via_api("perplexity", &config));
    }
}
