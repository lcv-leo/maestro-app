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
use crate::provider_grok::run_grok_api_agent;
use crate::provider_perplexity::run_perplexity_api_agent;
use crate::provider_runners::{
    run_anthropic_api_agent, run_gemini_api_agent, run_openai_api_agent,
    write_provider_error_result, write_provider_failure_result, EditorialAgentRequest,
    ProviderInvocation,
};
use crate::session_controls::ProviderCostGuard;
use crate::session_evidence::AttachmentManifestEntry;
use crate::{
    api_cli_for_agent, command_working_dir_for_output, extract_maestro_status,
    log_editorial_agent_finished, sanitize_short, sanitize_text, strip_process_management_noise,
    truncate_text_head_tail, write_text_file, AiProviderConfig, EditorialAgentResult,
    EditorialAgentSpec,
};
use tokio_util::sync::CancellationToken;

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
    cancel_token: &CancellationToken,
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
            cancel_token,
        );
    }

    if matches!(spec.key, "deepseek" | "grok" | "perplexity") {
        let invocation = ProviderInvocation {
            log_session,
            run_id,
            name: spec.name,
            cli: api_cli_for_agent(spec.key),
            provider: spec.key,
            role,
            output_path,
        };
        return write_provider_failure_result(
            &invocation,
            "unknown",
            "API_ONLY_AGENT_DISABLED_IN_CLI_MODE",
            "blocked",
            "Este agente opera somente via API. Troque o modo para Hibrido ou API para inclui-lo.",
            0,
            None,
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
        cancel_token,
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
    cancel_token: &CancellationToken,
) -> EditorialAgentResult {
    let request = EditorialAgentRequest {
        log_session,
        run_id,
        role,
        prompt,
        attachments,
        output_path,
        timeout,
        config,
        cost_guard,
    };
    match spec.key {
        "claude" => tauri::async_runtime::block_on(run_anthropic_api_agent(request, cancel_token)),
        "codex" => tauri::async_runtime::block_on(run_openai_api_agent(request, cancel_token)),
        "gemini" => tauri::async_runtime::block_on(run_gemini_api_agent(request, cancel_token)),
        "deepseek" => tauri::async_runtime::block_on(run_deepseek_api_agent(request, cancel_token)),
        "grok" => tauri::async_runtime::block_on(run_grok_api_agent(request, cancel_token)),
        "perplexity" => {
            tauri::async_runtime::block_on(run_perplexity_api_agent(request, cancel_token))
        }
        _ => {
            let invocation = ProviderInvocation {
                log_session: request.log_session,
                run_id: request.run_id,
                name: spec.name,
                cli: api_cli_for_agent(spec.key),
                provider: "unknown",
                role: request.role,
                output_path: request.output_path,
            };
            write_provider_error_result(&invocation, "unknown", "API_PROVIDER_NOT_SUPPORTED", 0)
        }
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
    cancel_token: &CancellationToken,
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
            cache: None,
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
        Some(cancel_token),
    );

    match command_result {
        Ok(result) => {
            let raw_stdout = String::from_utf8_lossy(&result.output.stdout).to_string();
            let stdout = strip_process_management_noise(&raw_stdout);
            let stderr = String::from_utf8_lossy(&result.output.stderr).to_string();
            let exit_code = result.output.status.code();
            // CLI cancel artifact status refinement (v0.5.3, deepseek follow-up
            // from v0.5.0 R1): when the run completed via timed_out + cancel
            // signaled, classify the agent artifact as `STOPPED_BY_USER`
            // explicitly instead of routing through the generic
            // `EMPTY_DRAFT`/`AGENT_FAILED_NO_OUTPUT` path. Differentiates real
            // session-deadline timeouts (cancel_token never fired) from
            // operator-driven stops (cancel_token cancelled, then poll loop
            // killed the child).
            let stopped_by_user = result.timed_out && cancel_token.is_cancelled();
            let status = if stopped_by_user {
                "STOPPED_BY_USER"
            } else if role == "review" {
                if stdout.trim().is_empty() {
                    let base = if result.output.status.success() {
                        "AGENT_FAILED_EMPTY"
                    } else {
                        "AGENT_FAILED_NO_OUTPUT"
                    };
                    classify_upstream_cli_failure(name, &stderr).unwrap_or(base)
                } else {
                    extract_maestro_status(&stdout).unwrap_or("NOT_READY")
                }
            } else if stdout.trim().is_empty() {
                classify_upstream_cli_failure(name, &stderr).unwrap_or("EMPTY_DRAFT")
            } else {
                "DRAFT_CREATED"
            };
            let tone = if status == "STOPPED_BY_USER" {
                "blocked"
            } else if result.timed_out
                || status == "AGENT_FAILED_EMPTY"
                || status == "CODEX_CLI_NO_FINAL_OUTPUT"
                || status == "AGENT_FAILED_NO_OUTPUT"
                || status == "CODEX_WINDOWS_SANDBOX_UPSTREAM"
                || status == "GEMINI_CLI_NO_FINAL_OUTPUT"
                || status == "GEMINI_RIPGREP_UNAVAILABLE"
                || status == "GEMINI_WORKSPACE_VIOLATION"
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
            let note = if status == "STOPPED_BY_USER" {
                "\n> Sessao interrompida pelo operador via botao 'Parar sessao'. CLI peer foi encerrado com `taskkill /T /F`; partial output preservado abaixo. Retome a sessao via `Retomar` para continuar do mesmo run_id.\n"
            } else if status == "CODEX_WINDOWS_SANDBOX_UPSTREAM" {
                "\n> Codex CLI 0.128.0+ no Windows roda o sandbox em PowerShell ConstrainedLanguage e trava ao desmontar o tree de processos (stderr mostra `ConstrainedLanguage`, `Cannot set property` ou `ERRO: o processo` do taskkill). Bug upstream conhecido (rastreado no cross-review-mcp v1.5.0+). Tente outro peer ou ambiente sem o sandbox.\n"
            } else if status == "GEMINI_WORKSPACE_VIOLATION" {
                "\n> Gemini CLI bloqueou uma chamada de ferramenta porque o agente tentou acessar caminho fora do workspace (`Path not in workspace` / `resolves outside the allowed workspace directories`). Esperado quando o protocolo pede recursos no diretorio pai. Tente outro peer.\n"
            } else if status == "GEMINI_RIPGREP_UNAVAILABLE" {
                "\n> Gemini CLI indicou que `rg`/ripgrep nao esta disponivel no ambiente do processo. A revisao foi tratada como falha operacional recuperavel; ajuste o PATH efetivo ou tente outro peer antes de retomar.\n"
            } else if status == "GEMINI_CLI_NO_FINAL_OUTPUT" {
                "\n> Gemini CLI encerrou sem stdout nem diagnostico util. A revisao foi tratada como falha operacional recuperavel, nao como parecer editorial.\n"
            } else if status == "CODEX_CLI_NO_FINAL_OUTPUT" {
                "\n> Codex CLI encerrou sem entregar parecer final em stdout. Quando o transcript aparece no stderr, ele e diagnostico operacional e nao substitui o parecer editorial estruturado.\n"
            } else if status == "AGENT_FAILED_NO_OUTPUT" {
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
            let pipe_diagnostic_line =
                if result.stdout_pipe_error.is_some() || result.stderr_pipe_error.is_some() {
                    format!(
                        "- Stdout pipe error: `{}`\n- Stderr pipe error: `{}`\n",
                        result.stdout_pipe_error.as_deref().unwrap_or("none"),
                        result.stderr_pipe_error.as_deref().unwrap_or("none"),
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
                cache: None,
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
                cache: None,
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

/// Classify CLI failures whose stderr matches a known upstream-bug pattern.
///
/// Returns a more specific status code when the stderr fingerprint matches a
/// documented upstream issue, so the operator-facing artifact distinguishes
/// "agent CLI failed silently" from "agent CLI hit a known platform bug".
///
/// **Codex Windows sandbox bug** (Codex CLI 0.128.0+ on Windows): the
/// PowerShell sandbox runs in `ConstrainedLanguage` mode, trips on
/// `Cannot set property` while resolving classifier state, and the
/// process-tree teardown emits the Portuguese `ERRO: o processo "<pid>"
/// nao foi encontrado` from `taskkill`. Documented in the workspace memory
/// at `reference_codex_cli_sandbox_constrained_language.md`. Tracked
/// upstream; deferred from cross-review-mcp v1.4.0 to v1.5.0+.
///
/// **Gemini workspace violation** (Gemini CLI with `--skip-trust`): the CLI
/// resolves the workspace as the agent's CWD (`agent-runs/`) and refuses
/// any file-system tool that touches the parent session directory; emits
/// `Error executing tool list_directory: Path not in workspace` /
/// `resolves outside the allowed workspace directories`. Surfaces when
/// the protocol prompt asks the agent to read sibling files (the input
/// file lives in `agent-runs/` but the protocol references the parent).
fn classify_upstream_cli_failure(name: &str, stderr: &str) -> Option<&'static str> {
    match name {
        "Codex" => {
            if stderr.contains("ConstrainedLanguage")
                || stderr.contains("Cannot set property")
                || stderr.contains("ERRO: o processo")
            {
                Some("CODEX_WINDOWS_SANDBOX_UPSTREAM")
            } else if stderr.trim().is_empty()
                || stderr.contains("Reading additional input from stdin")
                || stderr.contains("Get-Content")
            {
                Some("CODEX_CLI_NO_FINAL_OUTPUT")
            } else {
                None
            }
        }
        "Gemini" => {
            if stderr.contains("Path not in workspace")
                || stderr.contains("resolves outside the allowed workspace directories")
            {
                Some("GEMINI_WORKSPACE_VIOLATION")
            } else if stderr.contains("Ripgrep is not available") {
                Some("GEMINI_RIPGREP_UNAVAILABLE")
            } else if stderr.trim().is_empty() {
                Some("GEMINI_CLI_NO_FINAL_OUTPUT")
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::classify_upstream_cli_failure;

    #[test]
    fn classify_upstream_cli_failure_detects_codex_windows_sandbox_taskkill() {
        let stderr = "OpenAI Codex v0.128.0 (research preview)\nReading additional input from stdin...\nERRO: o processo \"4232\" nao foi encontrado.\n";
        assert_eq!(
            classify_upstream_cli_failure("Codex", stderr),
            Some("CODEX_WINDOWS_SANDBOX_UPSTREAM"),
        );
    }

    #[test]
    fn classify_upstream_cli_failure_detects_codex_constrained_language() {
        let stderr = "InvalidOperation: Cannot set property. Property setting is supported only on core types in this language mode.\nConstrainedLanguage mode\n";
        assert_eq!(
            classify_upstream_cli_failure("Codex", stderr),
            Some("CODEX_WINDOWS_SANDBOX_UPSTREAM"),
        );
    }

    #[test]
    fn classify_upstream_cli_failure_detects_gemini_workspace_violation() {
        let stderr = "Error executing tool list_directory: Path not in workspace: Attempted path \"C:\\Users\\leona\\OneDrive\\Downloads\\maestro-editorial-ai\\data\\sessions\\run-2026-05-02T11-39-41-113Z\" resolves outside the allowed workspace directories: C:\\Users\\leona\\OneDrive\\Downloads\\maestro-editorial-ai\\data\\sessions\\run-2026-05-02T11-39-41-113Z\\agent-runs\n";
        assert_eq!(
            classify_upstream_cli_failure("Gemini", stderr),
            Some("GEMINI_WORKSPACE_VIOLATION"),
        );
    }

    #[test]
    fn classify_upstream_cli_failure_returns_none_when_stderr_is_clean() {
        assert_eq!(
            classify_upstream_cli_failure("Codex", ""),
            Some("CODEX_CLI_NO_FINAL_OUTPUT")
        );
        assert_eq!(
            classify_upstream_cli_failure("Codex", "OpenAI Codex v0.128.0\nworkdir: C:\\\n"),
            None,
        );
        assert_eq!(
            classify_upstream_cli_failure("Gemini", ""),
            Some("GEMINI_CLI_NO_FINAL_OUTPUT")
        );
        assert_eq!(
            classify_upstream_cli_failure("Gemini", "Warning: 256-color support not detected.\n"),
            None,
        );
    }

    #[test]
    fn classify_upstream_cli_failure_detects_gemini_missing_ripgrep() {
        assert_eq!(
            classify_upstream_cli_failure(
                "Gemini",
                "Warning: 256-color support not detected.\nRipgrep is not available. Falling back to GrepTool.\n"
            ),
            Some("GEMINI_RIPGREP_UNAVAILABLE")
        );
    }

    #[test]
    fn classify_upstream_cli_failure_detects_codex_no_final_output() {
        assert_eq!(
            classify_upstream_cli_failure("Codex", "Reading additional input from stdin...\n"),
            Some("CODEX_CLI_NO_FINAL_OUTPUT")
        );
        assert_eq!(
            classify_upstream_cli_failure(
                "Codex",
                "\"pwsh.exe\" -Command \"Get-Content -Raw -LiteralPath 'round-input.md'\"\n"
            ),
            Some("CODEX_CLI_NO_FINAL_OUTPUT")
        );
    }

    #[test]
    fn classify_upstream_cli_failure_does_not_misclassify_other_agents() {
        // Claude/DeepSeek do not have classified upstream-bug fingerprints; even
        // when their stderr includes substrings that would match Codex/Gemini
        // patterns, they must return None to preserve the generic
        // EMPTY_DRAFT/AGENT_FAILED_EMPTY classification.
        assert_eq!(
            classify_upstream_cli_failure("Claude", "ERRO: o processo nao foi encontrado"),
            None,
        );
        assert_eq!(
            classify_upstream_cli_failure("DeepSeek", "Path not in workspace"),
            None,
        );
        assert_eq!(
            classify_upstream_cli_failure("Unknown", "ConstrainedLanguage"),
            None,
        );
    }
}
