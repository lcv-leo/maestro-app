// Modulo: src-tauri/src/session_persistence.rs
// Descricao: Session contract + cost ledger persistence helpers extracted from
// lib.rs in v0.3.27 per `docs/code-split-plan.md` migration step 5.
//
// What's here (8 functions):
//   - `session_contract_path`, `cost_ledger_path` — canonical paths inside a
//     session directory.
//   - `load_session_contract`, `write_session_contract` — JSON
//     deserialize/serialize of the resume contract; parse failures log to
//     stderr and return None so callers fall through to the request body.
//   - `load_cost_ledger`, `write_cost_ledger` — JSON persistence for the
//     cumulative per-session cost ledger; load returns an empty ledger when
//     the file is missing or malformed.
//   - `append_agent_cost_to_ledger` — appends one entry (provider/role/cost/
//     estimated flag) and rewrites the file with the recomputed total.
//   - `api_provider_from_cli` — short helper that maps a peer CLI name
//     (`openai-api`/`anthropic-api`/`gemini-api`/`deepseek-api`) to the
//     provider id stored in the ledger; returns None for non-API CLIs.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `SessionContract`, `CostLedger`, `CostLedgerEntry` (all pub(crate);
//     v0.3.27 upgrades the remaining private CostLedger* fields).
//   - `EditorialAgentResult` (already pub(crate)).
//   - `read_text_file`, `write_text_file` — already pub(crate).
//
// v0.3.27 is a pure move: every signature, format string and JSON shape is
// identical to the v0.3.26 lib.rs source (commit aaaacff).

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::{
    read_text_file, write_text_file, CostLedger, CostLedgerEntry, EditorialAgentResult,
    SessionContract,
};

pub(crate) fn session_contract_path(session_dir: &Path) -> PathBuf {
    session_dir.join("session-contract.json")
}

pub(crate) fn cost_ledger_path(session_dir: &Path) -> PathBuf {
    session_dir.join("cost-ledger.json")
}

pub(crate) fn load_session_contract(session_dir: &Path) -> Option<SessionContract> {
    let path = session_contract_path(session_dir);
    let text = read_text_file(&path).ok()?;
    match serde_json::from_str::<SessionContract>(&text) {
        Ok(contract) => Some(contract),
        Err(error) => {
            eprintln!(
                "session_contract_parse_failed path={} error={}",
                path.display(),
                error
            );
            None
        }
    }
}

pub(crate) fn write_session_contract(session_dir: &Path, contract: &SessionContract) -> Result<(), String> {
    let bytes = serde_json::to_string_pretty(contract)
        .map_err(|error| format!("failed to serialize session contract: {error}"))?;
    write_text_file(&session_contract_path(session_dir), &bytes)
}

pub(crate) fn load_cost_ledger(session_dir: &Path, run_id: &str) -> CostLedger {
    let path = cost_ledger_path(session_dir);
    read_text_file(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<CostLedger>(&text).ok())
        .unwrap_or_else(|| CostLedger {
            schema_version: 1,
            run_id: run_id.to_string(),
            total_observed_cost_usd: 0.0,
            entries: Vec::new(),
        })
}

pub(crate) fn write_cost_ledger(session_dir: &Path, ledger: &CostLedger) -> Result<(), String> {
    let bytes = serde_json::to_string_pretty(ledger)
        .map_err(|error| format!("failed to serialize cost ledger: {error}"))?;
    write_text_file(&cost_ledger_path(session_dir), &bytes)
}

pub(crate) fn append_agent_cost_to_ledger(
    session_dir: &Path,
    ledger: &mut CostLedger,
    agent: &EditorialAgentResult,
) -> Result<(), String> {
    let Some(cost_usd) = agent.cost_usd else {
        return Ok(());
    };
    let input_tokens = agent.usage_input_tokens.unwrap_or_default();
    let output_tokens = agent.usage_output_tokens.unwrap_or_default();
    let provider = api_provider_from_cli(&agent.cli).unwrap_or("cli");
    ledger.entries.push(CostLedgerEntry {
        at: Utc::now().to_rfc3339(),
        provider: provider.to_string(),
        agent: agent.name.clone(),
        role: agent.role.clone(),
        model: provider.to_string(),
        input_tokens,
        output_tokens,
        cost_usd,
        estimated: agent.cost_estimated.unwrap_or(true),
    });
    ledger.total_observed_cost_usd = ledger
        .entries
        .iter()
        .map(|entry| entry.cost_usd)
        .sum::<f64>();
    write_cost_ledger(session_dir, ledger)
}

pub(crate) fn api_provider_from_cli(cli: &str) -> Option<&'static str> {
    match cli {
        "anthropic-api" => Some("anthropic"),
        "openai-api" => Some("openai"),
        "gemini-api" => Some("gemini"),
        "deepseek-api" => Some("deepseek"),
        _ => None,
    }
}
