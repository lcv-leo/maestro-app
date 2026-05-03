// Modulo: src-tauri/src/editorial_io.rs
// Descricao: Editorial session I/O + agent observability primitives extracted
// from lib.rs in v0.5.4 per `docs/code-split-plan.md`. No behavior change —
// every function body, JSON shape, log category, and string parsing rule is
// byte-identical with v0.5.3 lib.rs (commit 116154f).
//
// What's here (10 items, ~234 lines):
//   - File I/O: `write_text_file`, `read_text_file` — sandboxed via
//     `checked_data_child_path` so artifacts can only be written/read under
//     `data/`.
//   - Path helper: `command_working_dir_for_output` — derives the spawn
//     working directory from a per-agent output path, falling back to
//     `app_root` when the parent is missing.
//   - Result builder: `editorial_session_result` + `SessionResultContext`
//     — assembles the final `EditorialSessionResult` returned to the
//     frontend, runs the v0.3.16 NB-2 `finalize_running_agent_artifacts`
//     guard against orphan RUNNING artifacts.
//   - NDJSON loggers: `log_editorial_agent_finished`, `log_editorial_agent_spawned`,
//     `log_editorial_agent_running` — emit `session.agent.finished/spawned/running`
//     with the schema_version=2 NDJSON shape consumed by the human-log
//     renderer and the resume-state inspector.
//   - Output parsers: `extract_maestro_status` (parses the
//     `MAESTRO_STATUS: READY|NOT_READY` review-protocol contract from
//     stdout), `extract_stdout_block` (extracts the fenced `## Stdout` body
//     from a Markdown agent artifact), `api_error_message` (best-effort
//     extraction of provider error message from JSON HTTP-error bodies,
//     with sanitize_text 180-char cap fallback).

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::app_paths::{app_root, checked_data_child_path};
use crate::command_spawn::CommandProgressContext;
use crate::editorial_helpers::finalize_running_agent_artifacts;
use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::sanitize::{sanitize_short, sanitize_text};
use crate::{EditorialAgentResult, EditorialSessionResult};

pub(crate) fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    let path = checked_data_child_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create artifact dir: {error}"))?;
    }
    fs::write(&path, text).map_err(|error| format!("failed to write artifact: {error}"))
}

pub(crate) fn read_text_file(path: &Path) -> Result<String, String> {
    let path = checked_data_child_path(path)?;
    fs::read_to_string(&path).map_err(|error| format!("failed to read artifact: {error}"))
}

pub(crate) struct SessionResultContext<'a> {
    pub(crate) run_id: &'a str,
    pub(crate) session_dir: &'a Path,
    pub(crate) prompt_path: &'a Path,
    pub(crate) protocol_path: &'a Path,
    pub(crate) active_agents: &'a [String],
    pub(crate) max_session_cost_usd: Option<f64>,
    pub(crate) max_session_minutes: Option<u64>,
    pub(crate) observed_cost_usd: f64,
    pub(crate) links_path: Option<&'a PathBuf>,
    pub(crate) attachments_manifest_path: Option<&'a PathBuf>,
    pub(crate) human_log_path: &'a Path,
}

pub(crate) fn editorial_session_result(
    context: &SessionResultContext<'_>,
    final_path: Option<&PathBuf>,
    minutes_path: &Path,
    draft_path: Option<PathBuf>,
    agents: Vec<EditorialAgentResult>,
    consensus_ready: bool,
    status: &str,
) -> EditorialSessionResult {
    finalize_running_agent_artifacts(&context.session_dir.join("agent-runs"));
    EditorialSessionResult {
        run_id: context.run_id.to_string(),
        session_dir: context.session_dir.to_string_lossy().to_string(),
        final_markdown_path: final_path.map(|path| path.to_string_lossy().to_string()),
        session_minutes_path: minutes_path.to_string_lossy().to_string(),
        prompt_path: context.prompt_path.to_string_lossy().to_string(),
        protocol_path: context.protocol_path.to_string_lossy().to_string(),
        draft_path: draft_path.map(|path| path.to_string_lossy().to_string()),
        agents,
        consensus_ready,
        status: status.to_string(),
        active_agents: context.active_agents.to_vec(),
        max_session_cost_usd: context.max_session_cost_usd,
        max_session_minutes: context.max_session_minutes,
        observed_cost_usd: Some(context.observed_cost_usd),
        links_path: context
            .links_path
            .map(|path| path.to_string_lossy().to_string()),
        attachments_manifest_path: context
            .attachments_manifest_path
            .map(|path| path.to_string_lossy().to_string()),
        human_log_path: Some(context.human_log_path.to_string_lossy().to_string()),
    }
}

pub(crate) fn log_editorial_agent_finished(
    log_session: &LogSession,
    run_id: &str,
    result: &EditorialAgentResult,
    stdout_chars: Option<usize>,
    stderr_chars: Option<usize>,
    resolved_path: Option<String>,
    timed_out: bool,
) {
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: if result.tone == "ok" {
                "info".to_string()
            } else if result.tone == "warn" || result.tone == "blocked" {
                "warn".to_string()
            } else {
                "error".to_string()
            },
            category: "session.agent.finished".to_string(),
            message: "editorial agent process finished".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(run_id, 120),
                "agent": result.name,
                "role": result.role,
                "cli": result.cli,
                "tone": result.tone,
                "status": result.status,
                "duration_ms": result.duration_ms,
                "exit_code": result.exit_code,
                "timed_out": timed_out,
                "resolved_path": resolved_path,
                "stdout_chars": stdout_chars,
                "stderr_chars": stderr_chars,
                "usage_input_tokens": result.usage_input_tokens,
                "usage_output_tokens": result.usage_output_tokens,
                "cost_usd": result.cost_usd,
                "cost_estimated": result.cost_estimated,
                "output_path": result.output_path
            })),
        },
    );
}

pub(crate) fn command_working_dir_for_output(output_path: &Path) -> PathBuf {
    output_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(app_root)
}

pub(crate) fn log_editorial_agent_spawned(
    progress: &CommandProgressContext<'_>,
    child_id: u32,
    path: &Path,
    working_dir: &Path,
) {
    let _ = write_log_record(
        progress.log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.spawned".to_string(),
            message: "editorial agent child process spawned".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(progress.run_id, 120),
                "agent": progress.agent,
                "role": progress.role,
                "cli": progress.cli,
                "child_pid": child_id,
                "resolved_path": path.to_string_lossy().to_string(),
                "working_dir": working_dir.to_string_lossy().to_string(),
                "output_path": progress.output_path.to_string_lossy().to_string()
            })),
        },
    );
}

pub(crate) fn log_editorial_agent_running(
    progress: &CommandProgressContext<'_>,
    child_id: u32,
    elapsed: Duration,
    stdout_bytes: u64,
    stderr_bytes: u64,
) {
    let _ = write_log_record(
        progress.log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.running".to_string(),
            message: "editorial agent still running".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(progress.run_id, 120),
                "agent": progress.agent,
                "role": progress.role,
                "cli": progress.cli,
                "child_pid": child_id,
                "elapsed_seconds": elapsed.as_secs(),
                "stdout_bytes_so_far": stdout_bytes,
                "stderr_bytes_so_far": stderr_bytes,
                "working_dir": command_working_dir_for_output(progress.output_path).to_string_lossy().to_string(),
                "output_path": progress.output_path.to_string_lossy().to_string()
            })),
        },
    );
}

pub(crate) fn extract_maestro_status(output: &str) -> Option<&'static str> {
    output.lines().find_map(|line| {
        let normalized = line.trim().to_ascii_uppercase();
        if normalized == "MAESTRO_STATUS: READY" {
            Some("READY")
        } else if normalized == "MAESTRO_STATUS: NOT_READY" {
            Some("NOT_READY")
        } else {
            None
        }
    })
}

pub(crate) fn strip_leading_maestro_status(output: &str) -> String {
    let output = output.trim_start_matches('\u{feff}');
    let mut lines = output.lines();
    let Some(first_line) = lines.next() else {
        return String::new();
    };
    let normalized = first_line.trim().to_ascii_uppercase();
    if normalized == "MAESTRO_STATUS: READY" || normalized == "MAESTRO_STATUS: NOT_READY" {
        lines.collect::<Vec<_>>().join("\n").trim().to_string()
    } else {
        output.trim().to_string()
    }
}

pub(crate) fn extract_stdout_block(artifact: &str) -> Option<&str> {
    let marker = "## Stdout\n\n```text\n";
    let start = artifact.find(marker)? + marker.len();
    let rest = &artifact[start..];
    let end = rest.find("\n```\n\n## Stderr")?;
    Some(rest[..end].trim())
}

pub(crate) fn api_error_message(body: &str) -> String {
    if body.trim().is_empty() {
        return "sem detalhe na resposta".to_string();
    }

    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(message) = value
            .pointer("/error/message")
            .or_else(|| value.pointer("/error/status"))
            .or_else(|| value.pointer("/error/code"))
            .and_then(Value::as_str)
        {
            return sanitize_text(message, 180);
        }

        if let Some(message) = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
        {
            return sanitize_text(message, 180);
        }
    }

    sanitize_text(body, 180)
}

#[cfg(test)]
mod tests {
    use super::strip_leading_maestro_status;

    #[test]
    fn strip_leading_maestro_status_removes_ready_marker_from_final_text() {
        let output = "MAESTRO_STATUS: READY\n# Titulo\n\nCorpo final.";
        assert_eq!(
            strip_leading_maestro_status(output),
            "# Titulo\n\nCorpo final."
        );
    }

    #[test]
    fn strip_leading_maestro_status_handles_utf8_bom_marker() {
        let output = "\u{feff}MAESTRO_STATUS: READY\r\n# Titulo";
        assert_eq!(strip_leading_maestro_status(output), "# Titulo");
    }

    #[test]
    fn strip_leading_maestro_status_preserves_normal_markdown() {
        let output = "# Titulo\n\nMAESTRO_STATUS: READY aparece no corpo.";
        assert_eq!(strip_leading_maestro_status(output), output);
    }
}
