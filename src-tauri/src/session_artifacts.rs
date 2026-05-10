// Modulo: src-tauri/src/session_artifacts.rs
// Descricao: Resumable-session inspection + agent-runs/* artifact reading
// helpers extracted from lib.rs in v0.3.29 per `docs/code-split-plan.md`
// migration step 5.
//
// What's here (9 functions):
//   - `inspect_resumable_session_dir` — top-level entry that decides whether
//     a session directory is resumable (`prompt.md` + `protocolo.md` present
//     and no `texto-final.md`); enriches the result with saved-contract
//     defaults (active_agents/initial_agent/caps) for the picker UI.
//   - `load_resume_session_state` — reads the latest draft + existing agent
//     results so the orchestrator can pick up mid-session.
//   - `find_latest_draft_artifact`, `find_latest_draft_artifact_from_artifacts`,
//     `artifact_resume_rank` — find the most-advanced (round, role) draft in
//     the agent-runs/ directory.
//   - `load_agent_results_from_dir`, `read_agent_artifacts` — recover the
//     per-round agent result vector from disk.
//   - `parse_agent_artifact_name`, `parse_agent_artifact_result` — parse the
//     canonical `round-NNN-{peer}-{role}.md` filename and the bullet-list
//     metadata at the top of the artifact body.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `SessionArtifact` (pub(crate) struct) — referenced by both
//     `session_resume.rs` (extracted in v0.3.28) and this module.
//   - `ResumableSessionInfo`, `ResumeSessionState` (v0.3.29 upgrades fields
//     to pub(crate) so the migrated functions can construct values).
//   - `EditorialAgentResult` (already pub(crate)).
//   - `extract_stdout_block`, `read_text_file` (already pub(crate)).
//
// v0.3.29 is a pure move: every signature, format string, and bullet label
// is identical to the v0.3.28 lib.rs source (commit 5f35960).

use std::fs;
use std::path::Path;

use crate::app_paths::{checked_data_child_path, sanitize_path_segment};
use crate::session_persistence::load_session_contract;
use crate::session_resume::{
    count_known_session_markdown_artifacts, extract_bullet_code_value, extract_saved_session_name,
    humanize_agent_name, known_session_activity_unix,
};
use crate::{
    extract_stdout_block, read_text_file, EditorialAgentResult, ProviderCacheTelemetry,
    ResumableSessionInfo, ResumeSessionState, SessionArtifact,
};

pub(crate) fn inspect_resumable_session_dir(
    path: &Path,
) -> Result<Option<ResumableSessionInfo>, String> {
    let session_dir = checked_data_child_path(path)?;
    let run_id = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| sanitize_path_segment(value, 120))
        .unwrap_or_default();
    if run_id.is_empty() {
        return Ok(None);
    }

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    if !prompt_path.is_file() || !protocol_path.is_file() {
        return Ok(None);
    }

    let final_path = session_dir.join("texto-final.md");
    if final_path.is_file() {
        return Ok(None);
    }

    let prompt_text = read_text_file(&prompt_path)?;
    let protocol_text = read_text_file(&protocol_path)?;
    let agent_dir = checked_data_child_path(&session_dir.join("agent-runs"))?;
    let artifacts = read_agent_artifacts(&agent_dir)?;
    let latest_draft = find_latest_draft_artifact_from_artifacts(&artifacts)?;
    let next_round = latest_draft
        .as_ref()
        .map(|artifact| artifact.round.max(1))
        .unwrap_or(1);
    let artifact_count = count_known_session_markdown_artifacts(&session_dir, &artifacts)?;
    let last_activity_unix =
        known_session_activity_unix(&session_dir, &prompt_path, &protocol_path, &artifacts)
            .unwrap_or(0);
    let status = if latest_draft.is_some() {
        "pronta para continuar".to_string()
    } else {
        "aguardando primeiro rascunho".to_string()
    };

    let saved_contract = load_session_contract(&session_dir);
    let saved_active_agents = saved_contract
        .as_ref()
        .map(|contract| contract.active_agents.clone())
        .unwrap_or_default();
    let saved_initial_agent = saved_contract
        .as_ref()
        .and_then(|contract| {
            contract
                .original_initial_agent
                .clone()
                .or_else(|| Some(contract.initial_agent.clone()))
        })
        .filter(|value| !value.trim().is_empty());
    let saved_max_session_cost_usd = saved_contract
        .as_ref()
        .and_then(|contract| contract.max_session_cost_usd);
    let saved_max_session_minutes = saved_contract
        .as_ref()
        .and_then(|contract| contract.max_session_minutes);

    Ok(Some(ResumableSessionInfo {
        run_id,
        session_name: extract_saved_session_name(&prompt_text)
            .unwrap_or_else(|| "Sessao editorial".to_string()),
        session_dir: session_dir.to_string_lossy().to_string(),
        prompt_path: prompt_path.to_string_lossy().to_string(),
        protocol_path: protocol_path.to_string_lossy().to_string(),
        draft_path: latest_draft
            .as_ref()
            .map(|artifact| artifact.path.to_string_lossy().to_string()),
        final_markdown_path: None,
        next_round,
        last_activity_unix,
        artifact_count,
        protocol_lines: protocol_text.lines().count(),
        status,
        saved_active_agents,
        saved_initial_agent,
        saved_max_session_cost_usd,
        saved_max_session_minutes,
    }))
}

pub(crate) fn load_resume_session_state(agent_dir: &Path) -> Result<ResumeSessionState, String> {
    let agent_dir = checked_data_child_path(agent_dir)?;
    let latest_draft = find_latest_draft_artifact(&agent_dir)?;
    let existing_agents = load_agent_results_from_dir(&agent_dir)?;

    if let Some(artifact) = latest_draft {
        let text = read_text_file(&artifact.path)?;
        let draft = extract_stdout_block(&text)
            .unwrap_or(text.as_str())
            .trim()
            .to_string();
        if !draft.is_empty() {
            return Ok(ResumeSessionState {
                current_draft: draft,
                current_draft_path: Some(artifact.path),
                next_review_round: artifact.round.max(1),
                existing_agents,
            });
        }
    }

    Ok(ResumeSessionState {
        current_draft: String::new(),
        current_draft_path: None,
        next_review_round: 1,
        existing_agents,
    })
}

pub(crate) fn find_latest_draft_artifact(
    agent_dir: &Path,
) -> Result<Option<SessionArtifact>, String> {
    let artifacts = read_agent_artifacts(agent_dir)?;
    find_latest_draft_artifact_from_artifacts(&artifacts)
}

fn find_latest_draft_artifact_from_artifacts(
    artifacts: &[SessionArtifact],
) -> Result<Option<SessionArtifact>, String> {
    let mut artifacts = artifacts
        .iter()
        .filter(|artifact| artifact.role == "revision" || artifact.role == "draft")
        .cloned()
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| {
        artifact_resume_rank(right)
            .cmp(&artifact_resume_rank(left))
            .then_with(|| right.agent.cmp(&left.agent))
    });

    for artifact in artifacts {
        let text = read_text_file(&artifact.path).unwrap_or_default();
        let draft = extract_stdout_block(&text).unwrap_or(text.as_str());
        if !draft.trim().is_empty() {
            return Ok(Some(artifact));
        }
    }

    Ok(None)
}

fn artifact_resume_rank(artifact: &SessionArtifact) -> (usize, usize, usize) {
    let role_rank = if artifact.role == "revision" { 1 } else { 0 };
    (artifact.round, role_rank, artifact.attempt)
}

pub(crate) fn load_agent_results_from_dir(
    agent_dir: &Path,
) -> Result<Vec<EditorialAgentResult>, String> {
    let mut artifacts = read_agent_artifacts(agent_dir)?;
    artifacts.sort_by(|left, right| {
        left.round
            .cmp(&right.round)
            .then_with(|| left.role.cmp(&right.role))
            .then_with(|| left.attempt.cmp(&right.attempt))
            .then_with(|| left.agent.cmp(&right.agent))
    });

    let mut agents = Vec::new();
    for artifact in artifacts {
        if let Some(result) = parse_agent_artifact_result(&artifact) {
            agents.push(result);
        }
    }
    Ok(agents)
}

pub(crate) fn read_agent_artifacts(agent_dir: &Path) -> Result<Vec<SessionArtifact>, String> {
    let agent_dir = checked_data_child_path(agent_dir)?;
    if !agent_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut artifacts = Vec::new();
    for entry in
        fs::read_dir(&agent_dir).map_err(|error| format!("failed to read agent dir: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to read agent artifact entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read agent artifact type: {error}"))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(artifact) = parse_agent_artifact_name(&agent_dir, name) {
            artifacts.push(artifact);
        }
    }
    Ok(artifacts)
}

pub(crate) fn parse_agent_artifact_name(agent_dir: &Path, name: &str) -> Option<SessionArtifact> {
    let rest = name.strip_prefix("round-")?;
    let (round_text, rest) = rest.split_once('-')?;
    let round = round_text.parse::<usize>().ok()?;
    let mut stem = rest.strip_suffix(".md")?;
    let mut attempt = 1usize;
    if let Some((base, attempt_text)) = stem.rsplit_once("-attempt-") {
        attempt = attempt_text.parse::<usize>().ok()?;
        if attempt < 2 {
            return None;
        }
        stem = base;
    }
    let (agent, role) = stem.rsplit_once('-')?;
    let agent = match agent {
        "claude" | "codex" | "gemini" | "deepseek" | "grok" => agent,
        _ => return None,
    };
    if !matches!(role, "draft" | "review" | "revision") {
        return None;
    }
    let canonical_name = if attempt == 1 {
        format!("round-{round:03}-{agent}-{role}.md")
    } else {
        format!("round-{round:03}-{agent}-{role}-attempt-{attempt:03}.md")
    };
    if canonical_name != name {
        return None;
    }
    Some(SessionArtifact {
        round,
        attempt,
        agent: agent.to_string(),
        role: role.to_string(),
        path: agent_dir.join(canonical_name),
    })
}

pub(crate) fn parse_agent_artifact_result(
    artifact: &SessionArtifact,
) -> Option<EditorialAgentResult> {
    let text = read_text_file(&artifact.path).ok()?;
    let cli = extract_bullet_code_value(&text, "CLI").unwrap_or_else(|| artifact.agent.clone());
    let status = extract_bullet_code_value(&text, "Status").unwrap_or_else(|| {
        if artifact.role == "draft" || artifact.role == "revision" {
            "DRAFT_CREATED".to_string()
        } else {
            "NOT_READY".to_string()
        }
    });
    let duration_ms = extract_bullet_code_value(&text, "Duration ms")
        .and_then(|value| value.parse::<u128>().ok())
        .unwrap_or(0);
    let exit_code =
        extract_bullet_code_value(&text, "Exit code").and_then(|value| value.parse::<i32>().ok());
    let usage_input_tokens = extract_bullet_code_value(&text, "Usage input tokens")
        .and_then(|value| value.parse::<u64>().ok());
    let usage_output_tokens = extract_bullet_code_value(&text, "Usage output tokens")
        .and_then(|value| value.parse::<u64>().ok());
    let cost_usd =
        extract_bullet_code_value(&text, "Cost USD").and_then(|value| value.parse::<f64>().ok());
    let cache = parse_cache_telemetry_from_artifact(&text);
    let tone = if status == "READY" || status == "DRAFT_CREATED" {
        "ok"
    } else if status == "CLI_NOT_FOUND"
        || status == "API_KEY_NOT_AVAILABLE"
        || status == "REMOTE_SECRET_NOT_READABLE"
    {
        "blocked"
    } else if status.starts_with("EXEC_ERROR")
        || status.starts_with("PROVIDER_")
        || status == "AGENT_FAILED_NO_OUTPUT"
        || status == "AGENT_FAILED_EMPTY"
        || status == "EMPTY_DRAFT"
        || status == "RUNNING"
        || status == "STOPPED_BY_USER"
        || status == "COST_LIMIT_REACHED"
        || status == "CODEX_WINDOWS_SANDBOX_UPSTREAM"
        || status == "GEMINI_WORKSPACE_VIOLATION"
    {
        "error"
    } else {
        "warn"
    };

    Some(EditorialAgentResult {
        name: humanize_agent_name(&artifact.agent),
        role: artifact.role.clone(),
        cli,
        tone: tone.to_string(),
        status,
        duration_ms,
        exit_code,
        output_path: artifact.path.to_string_lossy().to_string(),
        usage_input_tokens,
        usage_output_tokens,
        cost_usd,
        cost_estimated: cost_usd.map(|_| true),
        cache,
    })
}

fn optional_cache_u64(text: &str, label: &str) -> Option<u64> {
    extract_bullet_code_value(text, label)
        .filter(|value| value != "unknown")
        .and_then(|value| value.parse::<u64>().ok())
}

fn parse_cache_telemetry_from_artifact(text: &str) -> Option<ProviderCacheTelemetry> {
    let provider_mode = extract_bullet_code_value(text, "Cache provider mode")?;
    if provider_mode == "none" || provider_mode == "unknown" {
        return None;
    }
    Some(ProviderCacheTelemetry {
        provider_mode,
        cache_key_hash: extract_bullet_code_value(text, "Cache key hash")
            .filter(|value| value != "unknown"),
        cache_control_status: extract_bullet_code_value(text, "Cache control status")
            .filter(|value| value != "unknown"),
        cache_retention: extract_bullet_code_value(text, "Cache retention")
            .filter(|value| value != "unknown"),
        cached_input_tokens: optional_cache_u64(text, "Cache cached input tokens"),
        cache_hit_tokens: optional_cache_u64(text, "Cache hit tokens"),
        cache_miss_tokens: optional_cache_u64(text, "Cache miss tokens"),
        cache_read_input_tokens: optional_cache_u64(text, "Cache read input tokens"),
        cache_creation_input_tokens: optional_cache_u64(text, "Cache creation input tokens"),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_agent_artifact_name;
    use std::path::PathBuf;

    #[test]
    fn parse_agent_artifact_name_accepts_append_only_attempt_suffix() {
        let agent_dir = PathBuf::from("agent-runs");
        let artifact =
            parse_agent_artifact_name(&agent_dir, "round-018-codex-revision-attempt-002.md")
                .expect("attempt artifact should parse");

        assert_eq!(artifact.round, 18);
        assert_eq!(artifact.attempt, 2);
        assert_eq!(artifact.agent, "codex");
        assert_eq!(artifact.role, "revision");
        assert_eq!(
            artifact.path,
            agent_dir.join("round-018-codex-revision-attempt-002.md")
        );
    }

    #[test]
    fn parse_agent_artifact_name_rejects_invalid_attempt_suffixes() {
        let agent_dir = PathBuf::from("agent-runs");

        assert!(
            parse_agent_artifact_name(&agent_dir, "round-018-codex-revision-attempt-001.md")
                .is_none()
        );
        assert!(
            parse_agent_artifact_name(&agent_dir, "round-018-codex-revision-attempt-two.md")
                .is_none()
        );
    }
}
