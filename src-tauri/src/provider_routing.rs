// Modulo: src-tauri/src/provider_routing.rs
// Descricao: Per-agent provider routing helpers + env-var lookup helpers
// extracted from lib.rs in v0.3.43 per `docs/code-split-plan.md` migration
// step 3 (provider routing tail) and step 4 (env helpers).
//
// This module owns the small per-agent label/CLI/key lookups that
// `provider_runners.rs`, `provider_deepseek.rs`, and `editorial_agent_runners.rs`
// all consume, plus the env-var fallback chain (`first_env_value` ->
// `env_value_with_scope` -> `windows_registry_env_value`) that backs the
// "config OR env" credential resolution rule.
//
// What's here (8 functions):
//   - `api_cli_for_agent` — agent_key -> static label like "anthropic-api".
//     Used as the `cli` field in NDJSON `session.agent.started` /
//     `session.agent.finished` log records when a peer runs through the API
//     runner instead of a local CLI.
//   - `provider_label_for_agent` — agent_key -> human label "Anthropic /
//     Claude" etc. Used in PT-BR error strings and UI surfaces.
//   - `provider_remote_present` — boolean: is there a remote (Cloudflare
//     Secrets Store) credential for this agent? Reads the `*_api_key_remote`
//     flags on AiProviderConfig.
//   - `provider_key_for_agent` — config.<provider>_api_key first, then env
//     var fallback via `effective_provider_key`. Returns
//     `Option<(value, source_label)>` where source_label is "config" or
//     "<env_name>:<scope>".
//   - `first_env_value` — walks the candidate env-var name list and returns
//     the first present value as `(name, scope, value)`.
//   - `env_value_with_scope` — reads `std::env::var` first ("process"),
//     then on Windows tries `HKCU\Environment` ("user") and
//     `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment`
//     ("machine") via `reg.exe query`. Trims values, drops empty.
//   - `windows_registry_env_value` (Windows-only) — `reg.exe` parser that
//     extracts the value column for a given REG_* type row. Calls
//     `crate::hidden_command("reg.exe")` so spawn stays non-window-popping.
//   - `effective_provider_key` — config_value (trimmed) wins; otherwise
//     `first_env_value` over the env-name candidates with source label
//     "<env_name>:<scope>".
//
// What stayed in lib.rs:
//   - `hidden_command` — the Windows process-spawn primitive that
//     `windows_registry_env_value` calls; tightly coupled with the lib.rs
//     command-spawn surface (also used by Tauri command handlers and other
//     spawn paths).
//   - `AiProviderConfig` struct definition itself.
//
// v0.3.43 is a pure move: every signature, match arm, env-var name, and
// registry path is identical to the v0.3.42 lib.rs source (commit 0f672cf).

use crate::hidden_command;
use crate::AiProviderConfig;

pub(crate) fn api_cli_for_agent(agent_key: &str) -> &'static str {
    match agent_key {
        "claude" => "anthropic-api",
        "codex" => "openai-api",
        "gemini" => "gemini-api",
        "deepseek" => "deepseek-api",
        _ => "provider-api",
    }
}

pub(crate) fn provider_label_for_agent(agent_key: &str) -> &'static str {
    match agent_key {
        "claude" => "Anthropic / Claude",
        "codex" => "OpenAI / Codex",
        "gemini" => "Google / Gemini",
        "deepseek" => "DeepSeek",
        _ => "Provedor API",
    }
}

pub(crate) fn provider_remote_present(config: &AiProviderConfig, agent_key: &str) -> bool {
    match agent_key {
        "claude" => config.anthropic_api_key_remote,
        "codex" => config.openai_api_key_remote,
        "gemini" => config.gemini_api_key_remote,
        "deepseek" => config.deepseek_api_key_remote,
        _ => false,
    }
}

pub(crate) fn provider_key_for_agent(
    config: &AiProviderConfig,
    agent_key: &str,
) -> Option<(String, String)> {
    match agent_key {
        "claude" => effective_provider_key(
            config.anthropic_api_key.as_deref(),
            &["MAESTRO_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"],
        ),
        "codex" => effective_provider_key(
            config.openai_api_key.as_deref(),
            &["MAESTRO_OPENAI_API_KEY", "OPENAI_API_KEY"],
        ),
        "gemini" => effective_provider_key(
            config.gemini_api_key.as_deref(),
            &["MAESTRO_GEMINI_API_KEY", "GEMINI_API_KEY"],
        ),
        "deepseek" => effective_provider_key(
            config.deepseek_api_key.as_deref(),
            &["MAESTRO_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"],
        ),
        _ => None,
    }
}

pub(crate) fn first_env_value(candidates: &[&str]) -> Option<(String, String, String)> {
    candidates.iter().find_map(|name| {
        env_value_with_scope(name).map(|(scope, value)| ((*name).to_string(), scope, value))
    })
}

pub(crate) fn env_value_with_scope(name: &str) -> Option<(String, String)> {
    if let Some(value) = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(("process".to_string(), value));
    }

    #[cfg(windows)]
    {
        if let Some(value) = windows_registry_env_value(r"HKCU\Environment", name) {
            return Some(("user".to_string(), value));
        }

        if let Some(value) = windows_registry_env_value(
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            name,
        ) {
            return Some(("machine".to_string(), value));
        }
    }

    None
}

#[cfg(windows)]
fn windows_registry_env_value(key: &str, name: &str) -> Option<String> {
    let output = hidden_command("reg.exe")
        .args(["query", key, "/v", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with(name) {
            return None;
        }
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        let type_index = parts.iter().position(|part| part.starts_with("REG_"))?;
        let value = parts
            .iter()
            .skip(type_index + 1)
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

pub(crate) fn effective_provider_key(
    config_value: Option<&str>,
    env_candidates: &[&str],
) -> Option<(String, String)> {
    if let Some(value) = config_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
    {
        return Some((value, "config".to_string()));
    }
    first_env_value(env_candidates).map(|(name, scope, value)| (value, format!("{name}:{scope}")))
}
