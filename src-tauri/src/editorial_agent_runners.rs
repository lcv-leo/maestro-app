// Modulo: src-tauri/src/editorial_agent_runners.rs
// Descricao: Per-spec dispatchers + the CLI-path editorial agent runner,
// extracted from lib.rs in v0.3.36 per `docs/code-split-plan.md`
// migration step 5.
//
// What's here (3 functions):
//   - `run_editorial_agent_for_spec` — top-level dispatcher: routes the
//     given EditorialAgentSpec to the API peer when `use_api_agent=true`,
//     otherwise to the CLI runner.
//   - `run_provider_api_agent` — match-by-spec.key dispatcher to the 4
//     pub(crate) API runners (`run_anthropic_api_agent`,
//     `run_openai_api_agent`, `run_gemini_api_agent`,
//     `run_deepseek_api_agent`); falls back to
//     `write_provider_error_result(API_PROVIDER_NOT_SUPPORTED)` on
//     unknown keys.
//   - `run_editorial_agent` — the CLI-path runner: prepares input
//     (sidecar over 48 KB), resolves command (CLI_NOT_FOUND short-circuit),
//     writes RUNNING placeholder, spawns via
//     `run_resolved_command_observed`, classifies result (READY /
//     NOT_READY / DRAFT_CREATED / EMPTY_DRAFT / AGENT_FAILED_EMPTY /
//     AGENT_FAILED_NO_OUTPUT / EXEC_ERROR) and emits the artifact +
//     `EditorialAgentResult`.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `api_cli_for_agent` (v0.3.36 upgrade) — provider-id → CLI-name map.
//   - `AiProviderConfig`, `EditorialAgentResult`, `EditorialAgentSpec`,
//     `LogSession` (all already pub(crate)).
//   - `sanitize_text`, `truncate_text_head_tail`, `write_text_file`,
//     `extract_maestro_status`, `log_editorial_agent_finished`,
//     `command_working_dir_for_output` (all already pub(crate)).
//   - `command_search_dirs`, `resolve_command` from `command_path`.
//   - `run_resolved_command_observed`, `CommandProgressContext` from
//     `command_spawn`.
//   - `prepare_agent_input`, `effective_agent_input` from
//     `editorial_inputs`.
//   - `write_editorial_agent_running_artifact`,
//     `write_editorial_agent_error_artifact` from `editorial_helpers`.
//   - `run_anthropic_api_agent`, `run_openai_api_agent`,
//     `run_gemini_api_agent`, `write_provider_error_result` from
//     `provider_runners`; `run_deepseek_api_agent` from
//     `provider_deepseek`.
//   - `ProviderCostGuard` from `session_controls`.
//   - `AttachmentManifestEntry` from `session_evidence`.
//   - `LogEventInput`, `write_log_record` from `logging`.
//
// v0.3.36 is a pure move: every signature, status string
// (CLI_NOT_FOUND / AGENT_FAILED_EMPTY / AGENT_FAILED_NO_OUTPUT /
// EMPTY_DRAFT / DRAFT_CREATED / EXEC_ERROR / API_PROVIDER_NOT_SUPPORTED),
// log line category (`session.agent.started`), tone classification, and
// format string is identical to the v0.3.35 lib.rs source (commit
// 05a7a0f).

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::command_path::{command_search_dirs, resolve_command};
use crate::command_spawn::{run_resolved_command_observed, CommandProgressContext};
use crate::editorial_helpers::{
    write_editorial_agent_error_artifact, write_editorial_agent_running_artifact,
};
use crate::editorial_inputs::{effective_agent_input, prepare_agent_input};
use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::provider_deepseek::run_deepseek_api_agent;
use crate::provider_runners::{
    run_anthropic_api_agent, run_gemini_api_agent, run_openai_api_agent, write_provider_error_result,
};
use crate::session_controls::ProviderCostGuard;
use crate::session_evidence::AttachmentManifestEntry;
use crate::{
    api_cli_for_agent, command_working_dir_for_output, extract_maestro_status,
    log_editorial_agent_finished, sanitize_short, sanitize_text, truncate_text_head_tail,
    write_text_file, AiProviderConfig, EditorialAgentResult, EditorialAgentSpec,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_editorial_agent_for_spec(
    log_session: &LogSession,
    run_id: &str,
    spec: EditorialAgentSpec,
    role: &str,
    stdin_text: String,
    attachments: &[AttachmentManifestEntry],
    output_path: &Path,
    timeout: Option<Duration>,
    config: &AiProviderConfig,
    cost_guard: Option<ProviderCostGuard>,
    use_api_agent: bool,
) -> EditorialAgentResult {
    if use_api_agent {
        return run_provider_api_agent(
            log_session,
            run_id,
            spec,
            role,
            stdin_text,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        );
    }

    run_editorial_agent(
        log_session,
        run_id,
        spec.name,
        role,
        spec.command,
        (spec.args)(),
        stdin_text,
        output_path,
        timeout,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_provider_api_agent(
    log_session: &LogSession,
    run_id: &str,
    spec: EditorialAgentSpec,
    role: &str,
    prompt: String,
    attachments: &[AttachmentManifestEntry],
    output_path: &Path,
    timeout: Option<Duration>,
    config: &AiProviderConfig,
    cost_guard: Option<ProviderCostGuard>,
) -> EditorialAgentResult {
    match spec.key {
        "claude" => run_anthropic_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        "codex" => run_openai_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        "gemini" => run_gemini_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            attachments,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        "deepseek" => run_deepseek_api_agent(
            log_session,
            run_id,
            role,
            prompt,
            output_path,
            timeout,
            config,
            cost_guard,
        ),
        _ => write_provider_error_result(
            log_session,
            run_id,
            spec.name,
            api_cli_for_agent(spec.key),
            "unknown",
            role,
            output_path,
            "unknown",
            "API_PROVIDER_NOT_SUPPORTED",
            0,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_editorial_agent(
    log_session: &LogSession,
    run_id: &str,
    name: &str,
    role: &str,
    command: &str,
    args: Vec<String>,
    stdin_text: String,
    output_path: &Path,
    timeout: Option<Duration>,
) -> EditorialAgentResult {
    let started = Instant::now();
    let working_dir = command_working_dir_for_output(output_path);
    let prepared_input = prepare_agent_input(name, role, &stdin_text, output_path);
    let effective_input = effective_agent_input(command, args, &prepared_input);
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.agent.started".to_string(),
            message: "editorial agent process starting".to_string(),
            context: Some(json!({
                "run_id": sanitize_short(run_id, 120),
                "agent": name,
                "role": role,
                "cli": command,
                "stdin_chars": effective_input.stdin_chars,
                "original_prompt_chars": prepared_input.original_chars,
                "input_path": prepared_input
                    .input_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                "input_delivery": effective_input.delivery,
                "timeout_seconds": timeout.map(|value| value.as_secs()),
                "timeout_policy": if timeout.is_some() { "diagnostic_or_limited" } else { "none_editorial_session" },
                "working_dir": working_dir.to_string_lossy().to_string(),
                "output_path": output_path.to_string_lossy().to_string()
            })),
        },
    );
    let Some(path) = resolve_command(command) else {
        let _ = write_text_file(
            output_path,
            &format!(
                "# {name} - {role}\n\n- CLI: `{command}`\n- Status: `CLI_NOT_FOUND`\n- PATH dirs checked: `{}`\n\nCLI nao encontrada no PATH efetivo.\n",
                command_search_dirs().len()
            ),
        );
        let result = EditorialAgentResult {
            name: name.to_string(),
            role: role.to_string(),
            cli: command.to_string(),
            tone: "blocked".to_string(),
            status: "CLI_NOT_FOUND".to_string(),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            output_path: output_path.to_string_lossy().to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
        };
        log_editorial_agent_finished(log_session, run_id, &result, None, None, None, false);
        return result;
    };

    let _ = write_editorial_agent_running_artifact(
        output_path,
        name,
        role,
        command,
        &path,
        &effective_input.args,
        effective_input.stdin_chars,
        prepared_input.original_chars,
        prepared_input.input_path.as_deref(),
    );

    let progress = CommandProgressContext {
        log_session,
        run_id,
        agent: name,
        role,
        cli: command,
        output_path,
    };
    let command_result = run_resolved_command_observed(
        &path,
        &effective_input.args,
        timeout,
        effective_input.stdin_text.as_deref(),
        Some(progress),
    );

    match command_result {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.output.stderr).to_string();
            let exit_code = result.output.status.code();
            let status = if role == "review" {
                if stdout.trim().is_empty() {
                    if result.output.status.success() {
                        "AGENT_FAILED_EMPTY"
                    } else {
                        "AGENT_FAILED_NO_OUTPUT"
                    }
                } else {
                    extract_maestro_status(&stdout).unwrap_or("NOT_READY")
                }
            } else if stdout.trim().is_empty() {
                "EMPTY_DRAFT"
            } else {
                "DRAFT_CREATED"
            };
            let tone = if result.timed_out
                || status == "AGENT_FAILED_EMPTY"
                || status == "AGENT_FAILED_NO_OUTPUT"
            {
                "error"
            } else if result.output.status.success()
                && (status == "READY" || status == "DRAFT_CREATED")
            {
                "ok"
            } else if result.output.status.success() {
                "warn"
            } else {
                "error"
            };
            let note = if status == "AGENT_FAILED_NO_OUTPUT" {
                "\n> O agente encerrou sem entregar avaliacao editorial em stdout. Este arquivo e diagnostico operacional, nao parecer de revisao.\n"
            } else if status == "AGENT_FAILED_EMPTY" {
                "\n> O agente encerrou com exit code 0 mas devolveu stdout vazio. Tratado como falha operacional (NAO READY), nao como parecer editorial.\n"
            } else {
                ""
            };
            let input_line = prepared_input
                .input_path
                .as_ref()
                .map(|input_path| format!("- Input file: `{}`\n", input_path.to_string_lossy()))
                .unwrap_or_else(|| "- Input file: `inline stdin`\n".to_string());
            let pipe_diagnostic_line = if result.stdout_pipe_error.is_some()
                || result.stderr_pipe_error.is_some()
            {
                format!(
                    "- Stdout pipe error: `{}`\n- Stderr pipe error: `{}`\n",
                    result
                        .stdout_pipe_error
                        .as_deref()
                        .unwrap_or("none"),
                    result
                        .stderr_pipe_error
                        .as_deref()
                        .unwrap_or("none"),
                )
            } else {
                String::new()
            };
            let artifact = format!(
                "# {name} - {role}\n\n- CLI: `{command}`\n- Resolved path: `{}`\n- Args: `{}`\n- Status: `{status}`\n- Exit code: `{}`\n- Duration ms: `{}`\n- Timed out: `{}`\n- Stdin chars: `{}`\n- Original prompt chars: `{}`\n{input_line}- Stdout chars: `{}`\n- Stderr chars: `{}`\n{pipe_diagnostic_line}{note}\n## Stdout\n\n```text\n{}\n```\n\n## Stderr\n\n```text\n{}\n```\n",
                path.to_string_lossy(),
                sanitize_text(&effective_input.args.join(" "), 1000),
                exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                result.duration_ms,
                result.timed_out,
                effective_input.stdin_chars,
                prepared_input.original_chars,
                stdout.chars().count(),
                stderr.chars().count(),
                stdout,
                truncate_text_head_tail(&stderr, 1024, 60 * 1024)
            );
            let _ = write_text_file(output_path, &artifact);

            let agent_result = EditorialAgentResult {
                name: name.to_string(),
                role: role.to_string(),
                cli: command.to_string(),
                tone: tone.to_string(),
                status: status.to_string(),
                duration_ms: result.duration_ms,
                exit_code,
                output_path: output_path.to_string_lossy().to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            };
            log_editorial_agent_finished(
                log_session,
                run_id,
                &agent_result,
                Some(stdout.chars().count()),
                Some(stderr.chars().count()),
                Some(path.to_string_lossy().to_string()),
                result.timed_out,
            );
            agent_result
        }
        Err(error) => {
            let status = sanitize_text(&format!("EXEC_ERROR: {error}"), 240);
            let _ = write_editorial_agent_error_artifact(
                output_path,
                name,
                role,
                command,
                &path,
                &effective_input.args,
                &status,
                started.elapsed().as_millis(),
                effective_input.stdin_chars,
                prepared_input.original_chars,
                prepared_input.input_path.as_deref(),
            );
            let agent_result = EditorialAgentResult {
                name: name.to_string(),
                role: role.to_string(),
                cli: command.to_string(),
                tone: "error".to_string(),
                status,
                duration_ms: started.elapsed().as_millis(),
                exit_code: None,
                output_path: output_path.to_string_lossy().to_string(),
                usage_input_tokens: None,
                usage_output_tokens: None,
                cost_usd: None,
                cost_estimated: None,
            };
            log_editorial_agent_finished(
                log_session,
                run_id,
                &agent_result,
                None,
                None,
                Some(path.to_string_lossy().to_string()),
                false,
            );
            agent_result
        }
    }
}
