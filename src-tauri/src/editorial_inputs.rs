// Modulo: src-tauri/src/editorial_inputs.rs
// Descricao: Editorial agent input preparation + active-agents log context +
// time-budget anchor helpers extracted from lib.rs in v0.3.26 per
// `docs/code-split-plan.md` migration step 5.
//
// What's here (4 functions):
//   - `effective_agent_input` — gemini-aware adapter that places the prepared
//     prompt into argv (`--prompt <text>`) when a sidecar input file is
//     written; other CLIs continue receiving stdin.
//   - `prepare_agent_input` — write large prompts (> 48k chars) to a
//     `<output>-input.md` sidecar and return a short stdin pointer; smaller
//     prompts pass through untouched.
//   - `build_active_agents_resolved_log_context` — assembles the JSON
//     payload for the `session.editorial.active_agents_resolved` NDJSON log
//     entry. Pinned by unit tests in `lib.rs::tests`.
//   - `resolve_time_budget_anchor` — picks the wall-clock anchor for the
//     `max_session_minutes` cap. **B18 fix (v0.3.18)**: per-call now-anchored
//     on resume; created_at-anchored on fresh start. `created_at` continues
//     to be the persisted source of truth for cumulative metrics.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `PreparedAgentInput`, `EffectiveAgentInput` structs (pub(crate) +
//     fields upgraded in v0.3.26).
//   - `SessionContract` struct (already pub(crate)).
//   - `write_text_file`, `sanitize_text` — already pub(crate).
//
// v0.3.26 is a pure move: every signature, format string and JSON shape is
// identical to the v0.3.25 lib.rs source (commit dd8e923).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::{write_text_file, EffectiveAgentInput, PreparedAgentInput, SessionContract};

pub(crate) fn effective_agent_input(
    command: &str,
    args: Vec<String>,
    prepared: &PreparedAgentInput,
) -> EffectiveAgentInput {
    if command == "gemini" && prepared.input_path.is_some() {
        let mut next_args = args;
        if let Some(prompt_index) = next_args.iter().position(|arg| arg == "--prompt") {
            if let Some(prompt) = next_args.get_mut(prompt_index + 1) {
                *prompt = prepared.stdin_text.clone();
            }
        }

        return EffectiveAgentInput {
            args: next_args,
            stdin_text: None,
            stdin_chars: 0,
            delivery: "prompt_arg_sidecar",
        };
    }

    EffectiveAgentInput {
        args,
        stdin_text: Some(prepared.stdin_text.clone()),
        stdin_chars: prepared.stdin_text.chars().count(),
        delivery: if prepared.input_path.is_some() {
            "stdin_sidecar"
        } else {
            "stdin_inline"
        },
    }
}

pub(crate) fn prepare_agent_input(
    name: &str,
    role: &str,
    input: &str,
    output_path: &Path,
) -> PreparedAgentInput {
    const INLINE_PROMPT_LIMIT_CHARS: usize = 48_000;
    let original_chars = input.chars().count();
    if original_chars <= INLINE_PROMPT_LIMIT_CHARS {
        return PreparedAgentInput {
            stdin_text: input.to_string(),
            original_chars,
            input_path: None,
        };
    }

    let input_path = output_path.with_file_name(format!(
        "{}-input.md",
        output_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("maestro-agent")
    ));
    match write_text_file(&input_path, input) {
        Ok(()) => {
            let file_name = input_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("arquivo-de-entrada.md");
            PreparedAgentInput {
                stdin_text: format!(
                    "# Maestro Editorial AI - entrada por arquivo\n\nAgente: {name}\nTarefa: {role}\n\nLeia integralmente o arquivo local `{file_name}` no diretorio de trabalho atual antes de responder.\nO arquivo contem a solicitacao, o protocolo editorial integral, o rascunho e as instrucoes obrigatorias para esta rodada.\nExecute exatamente as instrucoes do arquivo e escreva a resposta final somente na saida padrao.\n"
                ),
                original_chars,
                input_path: Some(input_path),
            }
        }
        Err(_) => PreparedAgentInput {
            stdin_text: input.to_string(),
            original_chars,
            input_path: None,
        },
    }
}

/// Build the JSON context payload for the `session.editorial.active_agents_resolved`
/// NDJSON log entry. Extracted so unit tests can pin the field shape and source-label
/// derivation independently of the surrounding session loop.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_active_agents_resolved_log_context(
    run_id: &str,
    request_active_agents: Option<&Vec<String>>,
    saved_contract: Option<&SessionContract>,
    active_agent_keys: &[String],
    active_agents_source: &str,
    draft_lead_key: &str,
    invalid_initial_agent: Option<&str>,
    request_max_session_cost_usd: Option<f64>,
    request_max_session_minutes: Option<u64>,
) -> Value {
    let max_session_cost_usd_source = if request_max_session_cost_usd.is_some() {
        "request"
    } else if saved_contract
        .and_then(|contract| contract.max_session_cost_usd)
        .is_some()
    {
        "saved_contract"
    } else {
        "unset"
    };
    let max_session_minutes_source = if request_max_session_minutes.is_some() {
        "request"
    } else if saved_contract
        .and_then(|contract| contract.max_session_minutes)
        .is_some()
    {
        "saved_contract"
    } else {
        "unset"
    };
    json!({
        "run_id": run_id,
        "active_agents_requested": request_active_agents.cloned(),
        "active_agents_saved_contract": saved_contract
            .map(|contract| contract.active_agents.clone()),
        "active_agents_effective": active_agent_keys.to_vec(),
        "active_agents_source": active_agents_source,
        "draft_lead_key": draft_lead_key,
        "invalid_initial_agent": invalid_initial_agent,
        "max_session_cost_usd_requested": request_max_session_cost_usd,
        "max_session_cost_usd_saved": saved_contract
            .and_then(|contract| contract.max_session_cost_usd),
        "max_session_cost_usd_source": max_session_cost_usd_source,
        "max_session_minutes_requested": request_max_session_minutes,
        "max_session_minutes_saved": saved_contract
            .and_then(|contract| contract.max_session_minutes),
        "max_session_minutes_source": max_session_minutes_source,
    })
}

/// Decide which clock to anchor `max_session_minutes` against.
///
/// **B18 fix (v0.3.18)**: the optional `max_session_minutes` cap counts wall-
/// clock time from THIS session-call (resume or fresh start), not from the
/// original session's `created_at` which may be days old. Without this anchor,
/// an operator setting `max_minutes=5` on a session created 5 days ago would
/// see `TIME_LIMIT_REACHED` fire instantly without any peer ever running.
/// `created_at` remains the single source of truth for cumulative metrics and
/// is still persisted in the session contract; only the time-budget gate
/// switches to the per-call anchor.
pub(crate) fn resolve_time_budget_anchor(
    created_at: DateTime<Utc>,
    is_resume: bool,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    if is_resume {
        now
    } else {
        created_at
    }
}
