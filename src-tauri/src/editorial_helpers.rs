// Modulo: src-tauri/src/editorial_helpers.rs
// Descricao: Editorial session lifecycle helpers (active-agent filtering and
// resolution, review-complaint fingerprinting, RUNNING-artifact finalization,
// per-attempt running/error artifact writers) extracted from lib.rs in v0.3.24
// per `docs/code-split-plan.md` migration step 5 (editorial orchestration).
//
// What's here (7 items):
//   - `filter_existing_agents_to_active_set` — resume-side filter that keeps
//     only the agents in the active set, normalizing aliases the same way as
//     `normalize_active_agents` does on the request side (closes the trim
//     asymmetry blocker raised by claude+deepseek in the v0.3.18 cross-review).
//   - `resolve_effective_active_agents` — request/saved/default decision tree
//     for the effective active_agents list, plus the audit-log source label.
//   - `review_complaint_fingerprint` — stable u64 hash of the agent's stdout
//     block (whitespace-collapsed, first 1024 chars), used to detect three
//     consecutive identical NOT_READY rebuttals as persistent divergence.
//   - `FinalizeRunningArtifactsGuard` (struct + impl + `Drop`) — RAII guard
//     that runs `finalize_running_agent_artifacts` on every exit path of the
//     session loop, including panics and early `?` returns (Codex NB-2 from
//     the v0.3.15 handoff).
//   - `finalize_running_agent_artifacts` — final-pass safety net for
//     `agent-runs/*.md` files left at `Status: RUNNING`; idempotent.
//   - `write_editorial_agent_running_artifact` — initial RUNNING placeholder
//     before child process spawn.
//   - `write_editorial_agent_error_artifact` — error envelope for command
//     failures that did not produce a structured result.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `extract_stdout_block`, `read_text_file` — used by
//     `review_complaint_fingerprint` and `finalize_running_agent_artifacts`.
//
// v0.3.24 is a pure move: every signature, log line, format string and
// status string is identical to the v0.3.23 lib.rs source (commit 7b687a0).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::session_controls::normalize_active_agents;
use crate::{
    extract_stdout_block, read_text_file, sanitize_text, write_text_file, EditorialAgentResult,
};

pub(crate) fn filter_existing_agents_to_active_set(
    existing: Vec<EditorialAgentResult>,
    active_agent_keys: &[String],
) -> Vec<EditorialAgentResult> {
    let active_set: BTreeSet<String> = active_agent_keys.iter().cloned().collect();
    existing
        .into_iter()
        .filter(|agent| {
            // Mirror `normalize_active_agents` exactly: trim whitespace BEFORE
            // lowercasing so inputs like " Claude\n" or "\tdeepseek-api" map
            // to the same key in both the request-side normalizer and this
            // resume-side filter (closes the v0.3.18 cross-review R2 trim
            // asymmetry blocker raised by claude+deepseek).
            let key = agent.name.trim().to_ascii_lowercase();
            let normalized = match key.as_str() {
                "claude" | "anthropic" => "claude",
                "codex" | "openai" | "chatgpt" => "codex",
                "gemini" | "google" => "gemini",
                "deepseek" | "deepseek-api" => "deepseek",
                "grok" | "xai" | "grok-api" => "grok",
                _ => key.as_str(),
            };
            active_set.contains(normalized)
        })
        .collect()
}

/// Decide the effective active_agents list and the source label for the audit log.
/// Mirrors the pre-existing decision tree but is unit-testable in isolation.
/// Returns Err only when normalize_active_agents rejects the chosen list.
pub(crate) fn resolve_effective_active_agents(
    request_active_agents: Option<&Vec<String>>,
    saved_active_agents: Option<&Vec<String>>,
) -> Result<(Vec<String>, &'static str), String> {
    if request_active_agents.is_some() {
        let normalized = normalize_active_agents(request_active_agents)?;
        return Ok((normalized, "request"));
    }
    if let Some(saved) = saved_active_agents {
        if !saved.is_empty() {
            let normalized = normalize_active_agents(Some(saved))?;
            return Ok((normalized, "saved_contract"));
        }
    }
    let normalized = normalize_active_agents(None)?;
    Ok((normalized, "default_all"))
}

/// Hash the actionable portion of a review artifact (the agent's stdout body)
/// so consecutive identical NOT_READY rebuttals can be detected as persistent
/// divergence. Walks the artifact's `## Stdout` block, collapses whitespace,
/// and returns a stable u64 fingerprint based on the first 1024 chars.
pub(crate) fn review_complaint_fingerprint(artifact: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let stdout = extract_stdout_block(artifact).unwrap_or(artifact);
    let normalized: String = stdout
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let head: String = normalized.chars().take(1024).collect();
    let mut hasher = DefaultHasher::new();
    head.hash(&mut hasher);
    hasher.finish()
}

/// RAII guard that runs `finalize_running_agent_artifacts(&self.agent_dir)` from
/// its Drop impl. This makes the RUNNING-placeholder cleanup pass fire on every
/// exit path of the session loop, including panics and early `?` returns —
/// closing the gap that the v0.3.15 hook in `editorial_session_result(...)` left
/// open by only running when the result struct was successfully built. The
/// finalize routine is idempotent, so when both this guard and the
/// `editorial_session_result` call fire on a normal completion path the second
/// pass is a no-op (Codex NB-2 from the v0.3.15 cross-review handoff).
pub(crate) struct FinalizeRunningArtifactsGuard {
    agent_dir: PathBuf,
}

impl FinalizeRunningArtifactsGuard {
    pub(crate) fn new(agent_dir: PathBuf) -> Self {
        Self { agent_dir }
    }
}

impl Drop for FinalizeRunningArtifactsGuard {
    fn drop(&mut self) {
        // Wrap in catch_unwind so an unforeseen panic inside the finalize routine
        // (today: none, since all `Result` returns are swallowed via `let _ = `
        // and `if let Ok(...)` patterns) cannot escape the Drop while the stack
        // is already unwinding from another panic. A panic-during-unwind is
        // process abort by default; this guard renders that abort impossible.
        let agent_dir = self.agent_dir.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            finalize_running_agent_artifacts(&agent_dir);
        }));
    }
}

/// Final-pass safety net for agent-runs/*.md files left at `Status: RUNNING`.
/// Writes only when the file actually contains the RUNNING placeholder,
/// rewriting it to AGENT_FAILED_NO_OUTPUT and appending a diagnostic note.
/// This protects against the rare case where a process exits without producing
/// stdout (the structured-result path normally overwrites the placeholder).
pub(crate) fn finalize_running_agent_artifacts(agent_dir: &Path) {
    let entries = match fs::read_dir(agent_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let Ok(contents) = read_text_file(&path) else {
            continue;
        };
        if !contents.contains("- Status: `RUNNING`") {
            continue;
        }
        let rewritten = contents.replacen(
            "- Status: `RUNNING`",
            "- Status: `AGENT_FAILED_NO_OUTPUT`",
            1,
        );
        let with_note = if rewritten
            .contains("\n> Sessao finalizada com este artefato ainda em RUNNING")
        {
            rewritten
        } else {
            format!(
                "{rewritten}\n> Sessao finalizada com este artefato ainda em RUNNING. Reclassificado para AGENT_FAILED_NO_OUTPUT na finalizacao.\n"
            )
        };
        let _ = write_text_file(&path, &with_note);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_editorial_agent_running_artifact(
    output_path: &Path,
    name: &str,
    role: &str,
    command: &str,
    resolved_path: &Path,
    args: &[String],
    stdin_chars: usize,
    original_chars: usize,
    input_path: Option<&Path>,
) -> Result<(), String> {
    let input_line = input_path
        .map(|path| format!("- Input file: `{}`\n", path.to_string_lossy()))
        .unwrap_or_else(|| "- Input file: `inline stdin`\n".to_string());
    write_text_file(
        output_path,
        &format!(
            "# {name} - {role}\n\n- CLI: `{command}`\n- Resolved path: `{}`\n- Args: `{}`\n- Status: `RUNNING`\n- Exit code: `unknown`\n- Duration ms: `0`\n- Timed out: `false`\n- Stdin chars: `{stdin_chars}`\n- Original prompt chars: `{original_chars}`\n{input_line}- Stdout chars: `0`\n- Stderr chars: `0`\n- Started at: `{}`\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n\n```\n",
            resolved_path.to_string_lossy(),
            sanitize_text(&args.join(" "), 1000),
            Utc::now().to_rfc3339(),
        ),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_editorial_agent_error_artifact(
    output_path: &Path,
    name: &str,
    role: &str,
    command: &str,
    resolved_path: &Path,
    args: &[String],
    status: &str,
    duration_ms: u128,
    stdin_chars: usize,
    original_chars: usize,
    input_path: Option<&Path>,
) -> Result<(), String> {
    let input_line = input_path
        .map(|path| format!("- Input file: `{}`\n", path.to_string_lossy()))
        .unwrap_or_else(|| "- Input file: `inline stdin`\n".to_string());
    write_text_file(
        output_path,
        &format!(
            "# {name} - {role}\n\n- CLI: `{command}`\n- Resolved path: `{}`\n- Args: `{}`\n- Status: `{}`\n- Exit code: `unknown`\n- Duration ms: `{duration_ms}`\n- Timed out: `false`\n- Stdin chars: `{stdin_chars}`\n- Original prompt chars: `{original_chars}`\n{input_line}- Stdout chars: `0`\n- Stderr chars: `0`\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n{}\n```\n",
            resolved_path.to_string_lossy(),
            sanitize_text(&args.join(" "), 1000),
            sanitize_text(status, 240),
            sanitize_text(status, 2000),
        ),
    )
}
