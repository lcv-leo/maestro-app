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

const LEGACY_UNSCOPED_RUN_ID: &str = "__legacy_unscoped__";

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

pub(crate) fn load_cost_ledger(
    session_dir: &Path,
    session_run_id: &str,
    cost_scope_id: &str,
) -> CostLedger {
    let path = cost_ledger_path(session_dir);
    let mut ledger = read_text_file(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<CostLedger>(&text).ok())
        .unwrap_or_else(|| CostLedger {
            schema_version: 1,
            run_id: session_run_id.to_string(),
            total_observed_cost_usd: 0.0,
            entries: Vec::new(),
        });
    let legacy_run_id = if ledger.run_id.trim().is_empty() {
        LEGACY_UNSCOPED_RUN_ID.to_string()
    } else {
        ledger.run_id.clone()
    };
    for entry in &mut ledger.entries {
        if entry.run_id.trim().is_empty() {
            entry.run_id = legacy_run_id.clone();
        }
    }
    ledger.run_id = session_run_id.to_string();
    ledger.total_observed_cost_usd = observed_cost_for_run(&ledger.entries, cost_scope_id);
    ledger
}

pub(crate) fn write_cost_ledger(session_dir: &Path, ledger: &CostLedger) -> Result<(), String> {
    let bytes = serde_json::to_string_pretty(ledger)
        .map_err(|error| format!("failed to serialize cost ledger: {error}"))?;
    write_text_file(&cost_ledger_path(session_dir), &bytes)
}

pub(crate) fn append_agent_cost_to_ledger(
    session_dir: &Path,
    ledger: &mut CostLedger,
    cost_scope_id: &str,
    agent: &EditorialAgentResult,
) -> Result<(), String> {
    let Some(cost_usd) = agent.cost_usd else {
        return Ok(());
    };
    let input_tokens = agent.usage_input_tokens.unwrap_or_default();
    let output_tokens = agent.usage_output_tokens.unwrap_or_default();
    let provider = api_provider_from_cli(&agent.cli).unwrap_or("cli");
    ledger.entries.push(CostLedgerEntry {
        run_id: cost_scope_id.to_string(),
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
    ledger.total_observed_cost_usd = observed_cost_for_run(&ledger.entries, cost_scope_id);
    write_cost_ledger(session_dir, ledger)
}

fn observed_cost_for_run(entries: &[CostLedgerEntry], run_id: &str) -> f64 {
    entries
        .iter()
        .filter(|entry| entry.run_id == run_id)
        .map(|entry| entry.cost_usd)
        .sum::<f64>()
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

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{
        append_agent_cost_to_ledger, cost_ledger_path, load_cost_ledger, LEGACY_UNSCOPED_RUN_ID,
    };
    use crate::app_paths::data_dir;
    use crate::EditorialAgentResult;

    fn unique_temp_dir(name: &str) -> PathBuf {
        data_dir().join("sessions").join(format!(
            "maestro-{name}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn cost_ledger_resume_scopes_observed_total_to_current_run() {
        let dir = unique_temp_dir("cost-ledger-resume");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            cost_ledger_path(&dir),
            r#"{
  "schema_version": 1,
  "run_id": "old-run",
  "total_observed_cost_usd": 9.5,
  "entries": [
    {
      "at": "2026-05-03T00:00:00Z",
      "provider": "deepseek",
      "agent": "DeepSeek",
      "role": "review",
      "model": "deepseek",
      "input_tokens": 100,
      "output_tokens": 200,
      "cost_usd": 9.5,
      "estimated": true
    }
  ]
}"#,
        )
        .unwrap();

        let mut ledger = load_cost_ledger(&dir, "session-run", "new-attempt");
        assert_eq!(ledger.run_id, "session-run");
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].run_id, "old-run");
        assert_eq!(ledger.total_observed_cost_usd, 0.0);

        append_agent_cost_to_ledger(
            &dir,
            &mut ledger,
            "new-attempt",
            &EditorialAgentResult {
                name: "DeepSeek".to_string(),
                role: "review".to_string(),
                cli: "deepseek-api".to_string(),
                tone: "ok".to_string(),
                status: "READY".to_string(),
                duration_ms: 1,
                exit_code: Some(0),
                output_path: "agent-runs/round-001-deepseek-review.md".to_string(),
                usage_input_tokens: Some(10),
                usage_output_tokens: Some(20),
                cost_usd: Some(1.25),
                cost_estimated: Some(true),
            },
        )
        .unwrap();

        let new_run_ledger = load_cost_ledger(&dir, "session-run", "new-attempt");
        assert_eq!(new_run_ledger.entries.len(), 2);
        assert_eq!(new_run_ledger.entries[0].run_id, "old-run");
        assert_eq!(new_run_ledger.entries[1].run_id, "new-attempt");
        assert_eq!(new_run_ledger.total_observed_cost_usd, 1.25);

        let old_run_ledger = load_cost_ledger(&dir, "session-run", "old-run");
        assert_eq!(old_run_ledger.total_observed_cost_usd, 9.5);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cost_ledger_same_session_resume_uses_fresh_attempt_scope() {
        let dir = unique_temp_dir("cost-ledger-same-session-resume");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            cost_ledger_path(&dir),
            r#"{
  "schema_version": 1,
  "run_id": "session-run",
  "total_observed_cost_usd": 9.5,
  "entries": [
    {
      "at": "2026-05-03T00:00:00Z",
      "provider": "deepseek",
      "agent": "DeepSeek",
      "role": "review",
      "model": "deepseek",
      "input_tokens": 100,
      "output_tokens": 200,
      "cost_usd": 9.5,
      "estimated": true
    }
  ]
}"#,
        )
        .unwrap();

        let mut ledger = load_cost_ledger(&dir, "session-run", "resume-attempt");
        assert_eq!(ledger.run_id, "session-run");
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].run_id, "session-run");
        assert_eq!(ledger.total_observed_cost_usd, 0.0);

        append_agent_cost_to_ledger(
            &dir,
            &mut ledger,
            "resume-attempt",
            &EditorialAgentResult {
                name: "DeepSeek".to_string(),
                role: "review".to_string(),
                cli: "deepseek-api".to_string(),
                tone: "ok".to_string(),
                status: "READY".to_string(),
                duration_ms: 1,
                exit_code: Some(0),
                output_path: "agent-runs/round-088-deepseek-review.md".to_string(),
                usage_input_tokens: Some(10),
                usage_output_tokens: Some(20),
                cost_usd: Some(1.25),
                cost_estimated: Some(true),
            },
        )
        .unwrap();

        let resume_ledger = load_cost_ledger(&dir, "session-run", "resume-attempt");
        assert_eq!(resume_ledger.entries.len(), 2);
        assert_eq!(resume_ledger.entries[0].run_id, "session-run");
        assert_eq!(resume_ledger.entries[1].run_id, "resume-attempt");
        assert_eq!(resume_ledger.total_observed_cost_usd, 1.25);

        let session_ledger = load_cost_ledger(&dir, "session-run", "session-run");
        assert_eq!(session_ledger.total_observed_cost_usd, 9.5);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cost_ledger_missing_root_run_id_preserves_history_without_charging_resume() {
        let dir = unique_temp_dir("cost-ledger-rootless-resume");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            cost_ledger_path(&dir),
            r#"{
  "schema_version": 1,
  "total_observed_cost_usd": 9.5,
  "entries": [
    {
      "at": "2026-05-03T00:00:00Z",
      "provider": "deepseek",
      "agent": "DeepSeek",
      "role": "review",
      "model": "deepseek",
      "input_tokens": 100,
      "output_tokens": 200,
      "cost_usd": 9.5,
      "estimated": true
    }
  ]
}"#,
        )
        .unwrap();

        let mut ledger = load_cost_ledger(&dir, "session-run", "new-attempt");
        assert_eq!(ledger.run_id, "session-run");
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].run_id, LEGACY_UNSCOPED_RUN_ID);
        assert_eq!(ledger.total_observed_cost_usd, 0.0);

        append_agent_cost_to_ledger(
            &dir,
            &mut ledger,
            "new-attempt",
            &EditorialAgentResult {
                name: "DeepSeek".to_string(),
                role: "review".to_string(),
                cli: "deepseek-api".to_string(),
                tone: "ok".to_string(),
                status: "READY".to_string(),
                duration_ms: 1,
                exit_code: Some(0),
                output_path: "agent-runs/round-001-deepseek-review.md".to_string(),
                usage_input_tokens: Some(10),
                usage_output_tokens: Some(20),
                cost_usd: Some(1.25),
                cost_estimated: Some(true),
            },
        )
        .unwrap();

        let new_run_ledger = load_cost_ledger(&dir, "session-run", "new-attempt");
        assert_eq!(new_run_ledger.entries.len(), 2);
        assert_eq!(new_run_ledger.entries[0].run_id, LEGACY_UNSCOPED_RUN_ID);
        assert_eq!(new_run_ledger.entries[1].run_id, "new-attempt");
        assert_eq!(new_run_ledger.total_observed_cost_usd, 1.25);

        fs::remove_dir_all(&dir).unwrap();
    }
}
