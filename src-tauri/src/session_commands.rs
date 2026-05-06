// Modulo: src-tauri/src/session_commands.rs
// Descricao: Editorial session orchestration entry points extracted from
// lib.rs in v0.5.7 per `docs/code-split-plan.md`. Holds the four
// `#[tauri::command]` thin async wrappers (run/resume/list/stop) that the
// frontend invokes plus the three blocking workers they dispatch onto
// `tauri::async_runtime::spawn_blocking`:
//
//   Tauri commands (4):
//   - `list_resumable_sessions`
//   - `resume_editorial_session`
//   - `run_editorial_session`
//   - `stop_editorial_session` (sync — signals the cancellation token map)
//
//   Blocking workers (3):
//   - `run_editorial_session_blocking` — wraps NDJSON start/finish logging
//     and registers the cancellation token before delegating to
//     `run_editorial_session_inner` (still in lib.rs until v0.5.8).
//   - `resume_editorial_session_blocking` — reads saved prompt/protocol/
//     contract from `data/sessions/<run_id>/`, applies B22 fix semantics
//     (request is source of truth for caps + initial_agent; saved is
//     reference only) and dispatches into `run_editorial_session_core`.
//   - `list_resumable_sessions_blocking` — walks `data/sessions/`, returns
//     `ResumableSessionInfo` records sorted by last activity descending.
//
// Pure move from lib.rs v0.5.6 (commit e477ba3): every NDJSON shape, log
// category, sanitize_* call, B22 comment block, RAII cancel guard, and
// resume contract resolution preserved verbatim. The only behavioral
// change versus the pre-move state is that lib.rs visibility for the
// types and the `*_inner` / `*_core` helpers was upgraded to `pub(crate)`
// in this same commit so the cross-module call still resolves.

use chrono::Utc;
use serde_json::json;
use std::fs;

use crate::app_paths::{
    checked_data_child_path, safe_run_id_from_entry, sanitize_path_segment, sessions_dir,
};
use crate::editorial_io::{read_text_file, write_text_file};
use crate::editorial_prompts::resolve_initial_agent_key;
use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::session_artifacts::{inspect_resumable_session_dir, load_resume_session_state};
use crate::session_cancel;
use crate::session_persistence::load_session_contract;
use crate::session_resume::{
    extract_saved_initial_agent, extract_saved_prompt, extract_saved_session_name,
    stable_text_fingerprint,
};
use crate::{
    run_editorial_session_core, run_editorial_session_inner, sanitize_short, sanitize_text,
    EditorialSessionRequest, EditorialSessionResult, ResumableSessionInfo, ResumeSessionRequest,
};

#[tauri::command]
pub(crate) async fn list_resumable_sessions(
    log_session: tauri::State<'_, LogSession>,
) -> Result<Vec<ResumableSessionInfo>, String> {
    let log_session = log_session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || list_resumable_sessions_blocking(&log_session))
        .await
        .map_err(|error| format!("resume session listing worker join failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn resume_editorial_session(
    log_session: tauri::State<'_, LogSession>,
    request: ResumeSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let log_session = log_session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        resume_editorial_session_blocking(log_session, request)
    })
    .await
    .map_err(|error| format!("resume editorial worker join failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn run_editorial_session(
    log_session: tauri::State<'_, LogSession>,
    request: EditorialSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let log_session = log_session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        run_editorial_session_blocking(log_session, request)
    })
    .await
    .map_err(|error| format!("editorial worker join failed: {error}"))?
}

/// Operator-driven session stop. Returns `true` when a matching active
/// run_id was found and signaled, `false` when no matching run is active
/// (already finished, never started, or unknown id). Idempotent: repeated
/// calls on a still-running id keep returning `true`. Sync command —
/// returns immediately even if the session loop is mid-API-call; the loop
/// observes the cancellation token at its next checkpoint (between rounds
/// or inside `tokio::select!` for in-flight HTTP requests).
#[tauri::command]
pub(crate) fn stop_editorial_session(
    log_session: tauri::State<'_, LogSession>,
    run_id: String,
) -> Result<bool, String> {
    let signaled = session_cancel::signal_session_cancel(&run_id);
    let _ = write_log_record(
        log_session.inner(),
        LogEventInput {
            level: "info".to_string(),
            category: "session.user.stop_requested".to_string(),
            message: "operator requested editorial session stop".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(&run_id, 120),
                "signaled": signaled
            })),
        },
    );
    Ok(signaled)
}

fn run_editorial_session_blocking(
    log_session: LogSession,
    request: EditorialSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.editorial.started".to_string(),
            message: "real editorial session command received".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(&request.run_id, 120),
                "session_name": sanitize_text(&request.session_name, 200),
                "prompt_chars": request.prompt.chars().count(),
                "protocol_name": sanitize_text(&request.protocol_name, 200),
                "protocol_chars": request.protocol_text.chars().count(),
                "protocol_lines": request.protocol_text.lines().count(),
                "protocol_hash_prefix": sanitize_short(&request.protocol_hash, 16),
                "initial_agent": resolve_initial_agent_key(request.initial_agent.as_deref()).0,
                "active_agents_requested": request.active_agents.clone(),
                "max_session_cost_usd": request.max_session_cost_usd,
                "max_session_minutes": request.max_session_minutes,
                "attachment_count": request.attachments.as_ref().map(|items| items.len()).unwrap_or_default(),
                "link_count": request.links.as_ref().map(|items| items.len()).unwrap_or_default(),
                "artifact_policy": "raw agent outputs are written under data/sessions, not embedded in NDJSON"
            })),
        },
    );

    // Register cancellation token before entering the orchestration loop. The
    // RAII guard drops it when this function returns (success, error, panic),
    // so the static map does not grow unbounded across many sessions.
    let cancel_token = session_cancel::register_session_cancel(&request.run_id);
    let _cancel_guard = session_cancel::CancelTokenGuard::new(request.run_id.clone());

    let result = match run_editorial_session_inner(&request, &log_session, &cancel_token) {
        Ok(result) => result,
        Err(error) => {
            let _ = write_log_record(
                &log_session,
                LogEventInput {
                    level: "error".to_string(),
                    category: "session.editorial.failed".to_string(),
                    message: "real editorial session failed before structured result".to_string(),
                    context: Some(json!({
                        "run_id": sanitize_short(&request.run_id, 120),
                        "error": sanitize_text(&error, 500),
                        "session_name": sanitize_text(&request.session_name, 200),
                        "prompt_chars": request.prompt.chars().count(),
                        "protocol_chars": request.protocol_text.chars().count(),
                        "protocol_hash_prefix": sanitize_short(&request.protocol_hash, 16)
                    })),
                },
            );
            return Err(error);
        }
    };
    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: if result.consensus_ready {
                "info"
            } else {
                "warn"
            }
            .to_string(),
            category: "session.editorial.completed".to_string(),
            message: "real editorial session completed".to_string(),
            context: Some(json!({
                "run_id": result.run_id,
                "status": result.status,
                "consensus_ready": result.consensus_ready,
                "active_agents": result.active_agents.clone(),
                "observed_cost_usd": result.observed_cost_usd,
                "human_log_path": result.human_log_path.clone(),
                "session_dir": result.session_dir,
                "final_markdown_path": result.final_markdown_path,
                "session_minutes_path": result.session_minutes_path,
                "agents": result.agents.iter().map(|agent| json!({
                    "name": agent.name,
                    "role": agent.role,
                    "tone": agent.tone,
                    "duration_ms": agent.duration_ms,
                    "exit_code": agent.exit_code,
                    "output_path": agent.output_path
                })).collect::<Vec<_>>()
            })),
        },
    );
    Ok(result)
}

fn resume_editorial_session_blocking(
    log_session: LogSession,
    request: ResumeSessionRequest,
) -> Result<EditorialSessionResult, String> {
    let run_id = sanitize_path_segment(&request.run_id, 120);
    if run_id.is_empty() {
        return Err("run_id vazio".to_string());
    }

    let session_dir = checked_data_child_path(&sessions_dir().join(&run_id))?;
    if !session_dir.is_dir() {
        return Err("sessao nao encontrada em data/sessions".to_string());
    }

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    let saved_prompt = read_text_file(&prompt_path)?;
    let saved_protocol = read_text_file(&protocol_path)?;
    let prompt = extract_saved_prompt(&saved_prompt)
        .unwrap_or_else(|| saved_prompt.trim().to_string())
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("prompt salvo da sessao esta vazio".to_string());
    }

    let session_name =
        extract_saved_session_name(&saved_prompt).unwrap_or_else(|| format!("Sessao {run_id}"));
    let saved_contract = load_session_contract(&session_dir);
    let saved_initial_agent = extract_saved_initial_agent(&saved_prompt);
    let requested_initial_agent = request.initial_agent.clone();
    // B22 fix (v0.5.2): request is source of truth. Pre-fix this was
    // `saved_initial_agent.or_else(requested_initial_agent)`, which silently
    // overrode the operator's UI choice with whatever was extracted from the
    // saved prompt.md. Mirrors v0.3.42 B20 (caps) and v0.5.1 B21 (peers):
    // saved values are reference only.
    let effective_initial_agent = requested_initial_agent
        .clone()
        .or_else(|| saved_initial_agent.clone());
    let override_protocol = request
        .protocol_text
        .as_deref()
        .map(str::trim)
        .filter(|value| value.len() >= 100)
        .map(str::to_string);
    let using_protocol_override = override_protocol.is_some();
    let protocol_text = override_protocol.unwrap_or_else(|| saved_protocol.trim().to_string());
    let protocol_name = if using_protocol_override {
        request
            .protocol_name
            .as_deref()
            .map(|value| sanitize_text(value, 200))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "protocolo-atualizado.md".to_string())
    } else {
        "protocolo.md".to_string()
    };
    let protocol_hash = request
        .protocol_hash
        .as_deref()
        .map(|value| sanitize_short(value, 80))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| stable_text_fingerprint(&protocol_text));

    let protocol_backup_path =
        if using_protocol_override && saved_protocol.trim() != protocol_text.trim() {
            let backup_path = session_dir.join(format!(
                "protocolo-anterior-{}.md",
                Utc::now().format("%Y%m%dT%H%M%SZ")
            ));
            write_text_file(&backup_path, &saved_protocol)?;
            Some(backup_path)
        } else {
            None
        };

    let agent_dir = checked_data_child_path(&session_dir.join("agent-runs"))?;
    fs::create_dir_all(&agent_dir)
        .map_err(|error| format!("failed to create agent run dir: {error}"))?;
    let resume_state = load_resume_session_state(&agent_dir)?;

    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.resume.started".to_string(),
            message: "operator requested editorial session resume".to_string(),
            context: Some(json!({
                "run_id": &run_id,
                "session_name": sanitize_text(&session_name, 200),
                "using_protocol_override": using_protocol_override,
                "protocol_name": sanitize_text(&protocol_name, 200),
                "protocol_chars": protocol_text.chars().count(),
                "protocol_lines": protocol_text.lines().count(),
                "protocol_hash_prefix": sanitize_short(&protocol_hash, 16),
                "saved_initial_agent": saved_initial_agent.clone(),
                "requested_initial_agent": requested_initial_agent.clone(),
                "effective_initial_agent": effective_initial_agent.clone(),
                "resume_draft_path": resume_state
                    .current_draft_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                "protocol_backup_path": protocol_backup_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                "next_review_round": resume_state.next_review_round,
                "existing_agent_artifacts": resume_state.existing_agents.len()
            })),
        },
    );

    let request = EditorialSessionRequest {
        run_id,
        session_name,
        prompt,
        protocol_name,
        protocol_text,
        protocol_hash,
        initial_agent: effective_initial_agent,
        // B22 fix (v0.5.2): caps are NEVER carried forward from the saved
        // contract. Pre-fix this was `request.X.or_else(saved.X)` which
        // silently substituted the operator's "no cap" (None) with the saved
        // contract's prior values, matching the v0.3.42 B20 bug pattern in a
        // path that the v0.3.42 fix missed (run_editorial_session_blocking
        // was patched; resume_editorial_session_blocking was overlooked).
        // Operator's request is source of truth: None means "no cap".
        active_agents: request.active_agents.clone(),
        max_session_cost_usd: request.max_session_cost_usd,
        max_session_minutes: request.max_session_minutes,
        attachments: request.attachments.clone(),
        // Links is a content field (not a cap); keep saved fallback so resume
        // without explicit links carries the original session's links forward.
        links: request.links.clone().or_else(|| {
            saved_contract
                .as_ref()
                .map(|contract| contract.links.clone())
        }),
    };

    let cancel_token = session_cancel::register_session_cancel(&request.run_id);
    let _cancel_guard = session_cancel::CancelTokenGuard::new(request.run_id.clone());

    let result =
        match run_editorial_session_core(&request, &log_session, Some(resume_state), &cancel_token)
        {
            Ok(result) => result,
            Err(error) => {
                let _ = write_log_record(
                    &log_session,
                    LogEventInput {
                        level: "error".to_string(),
                        category: "session.resume.failed".to_string(),
                        message: "editorial session resume failed before structured result"
                            .to_string(),
                        context: Some(json!({
                            "run_id": sanitize_short(&request.run_id, 120),
                            "error": sanitize_text(&error, 500)
                        })),
                    },
                );
                return Err(error);
            }
        };

    let _ = write_log_record(
        &log_session,
        LogEventInput {
            level: if result.consensus_ready {
                "info"
            } else {
                "warn"
            }
            .to_string(),
            category: "session.resume.completed".to_string(),
            message: "editorial session resume completed".to_string(),
            context: Some(json!({
                "run_id": result.run_id,
                "status": result.status,
                "consensus_ready": result.consensus_ready,
                "active_agents": result.active_agents.clone(),
                "observed_cost_usd": result.observed_cost_usd,
                "human_log_path": result.human_log_path.clone(),
                "session_dir": result.session_dir,
                "final_markdown_path": result.final_markdown_path,
                "session_minutes_path": result.session_minutes_path
            })),
        },
    );

    Ok(result)
}

fn list_resumable_sessions_blocking(
    log_session: &LogSession,
) -> Result<Vec<ResumableSessionInfo>, String> {
    let root = checked_data_child_path(&sessions_dir())?;
    fs::create_dir_all(&root).map_err(|error| format!("failed to create sessions dir: {error}"))?;

    let mut sessions = Vec::new();
    for entry in
        fs::read_dir(&root).map_err(|error| format!("failed to read sessions dir: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to read session entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read session entry type: {error}"))?;
        if !file_type.is_dir() {
            continue;
        }
        let Some(run_id) = safe_run_id_from_entry(&entry) else {
            continue;
        };
        let path = root.join(run_id);
        if let Some(info) = inspect_resumable_session_dir(&path)? {
            sessions.push(info);
        }
    }

    sessions.sort_by(|left, right| {
        right
            .last_activity_unix
            .cmp(&left.last_activity_unix)
            .then_with(|| left.run_id.cmp(&right.run_id))
    });

    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.resume.listed".to_string(),
            message: "resumable sessions listed from data/sessions".to_string(),
            context: Some(json!({
                "count": sessions.len(),
                "run_ids": sessions.iter().take(30).map(|session| session.run_id.clone()).collect::<Vec<_>>()
            })),
        },
    );

    Ok(sessions)
}
