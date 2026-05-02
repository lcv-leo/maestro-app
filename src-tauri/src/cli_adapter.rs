// Modulo: src-tauri/src/cli_adapter.rs
// Descricao: CLI adapter smoke probe spec table + per-CLI probe runner
// extracted from lib.rs in v0.3.39.
//
// This module owns the dependency_preflight smoke probe machinery that
// validates each external CLI (Claude, Codex, Gemini) is callable and
// returns the expected marker. The Tauri command wrapper
// (`run_cli_adapter_smoke`) stays in lib.rs because it lives on the
// `#[tauri::command]` registry boundary and orchestrates calls to these
// helpers across all 3 CLI adapters.
//
// What's here:
//   - `cli_adapter_specs` — builds the 3-element spec table (name, command,
//     marker, CLI args, per-CLI timeout). Args differ per CLI: Claude uses
//     `--print --output-format text --permission-mode dontAsk`, Codex uses
//     `--ask-for-approval never exec --skip-git-repo-check --sandbox
//     read-only --color never`, and Gemini uses `--prompt --output-format
//     text --approval-mode yolo --skip-trust`.
//   - `run_cli_adapter_probe` — single-spec runner: resolves the command
//     against the effective PATH (returns `blocked` tone with status "CLI
//     nao encontrada no PATH efetivo" when missing), invokes
//     `run_resolved_command_with_timeout`, then classifies the outcome
//     (timeout/ok+marker/ok-without-marker/nonzero-exit).
//
// What stayed in lib.rs:
//   - `CliAdapterSmokeRequest` / `CliAdapterSmokeResult` /
//     `CliAdapterProbeResult` / `CliAdapterSpec` structs (consumed by both
//     `cli_adapter.rs` and the Tauri command wrapper; live in lib.rs as
//     `pub(crate)` for cross-module access).
//   - `run_cli_adapter_smoke` Tauri command wrapper (registry boundary).
//
// v0.3.39 is a pure move: every signature, log line, format string and
// status string is identical to the v0.3.38 lib.rs source (commit b7509b9).

use std::time::{Duration, Instant};

use crate::command_path::resolve_command;
use crate::command_spawn::run_resolved_command_with_timeout;
use crate::{
    sanitize_short, sanitize_text, CliAdapterProbeResult, CliAdapterSmokeRequest, CliAdapterSpec,
};

pub(crate) fn cli_adapter_specs(request: &CliAdapterSmokeRequest) -> Vec<CliAdapterSpec> {
    let run_id = sanitize_short(&request.run_id, 120);
    let protocol_name = sanitize_text(&request.protocol_name, 160);
    let protocol_hash_prefix = sanitize_short(&request.protocol_hash, 16);
    let prompt_base = format!(
        "Maestro Editorial AI adapter smoke. Run {run_id}. Prompt chars: {}. Protocol: {protocol_name}; lines: {}; hash prefix: {protocol_hash_prefix}. Do not use tools. Reply only with the exact marker requested.",
        request.prompt_chars, request.protocol_lines
    );

    vec![
        CliAdapterSpec {
            name: "Claude",
            command: "claude",
            marker: "MAESTRO_CLI_SMOKE_CLAUDE_READY",
            args: vec![
                "--print".to_string(),
                "--output-format".to_string(),
                "text".to_string(),
                "--permission-mode".to_string(),
                "dontAsk".to_string(),
                format!("{prompt_base} Marker: MAESTRO_CLI_SMOKE_CLAUDE_READY"),
            ],
            timeout: Duration::from_secs(90),
        },
        CliAdapterSpec {
            name: "Codex",
            command: "codex",
            marker: "MAESTRO_CLI_SMOKE_CODEX_READY",
            args: vec![
                "--ask-for-approval".to_string(),
                "never".to_string(),
                "exec".to_string(),
                "--skip-git-repo-check".to_string(),
                "--sandbox".to_string(),
                "read-only".to_string(),
                "--color".to_string(),
                "never".to_string(),
                format!("{prompt_base} Marker: MAESTRO_CLI_SMOKE_CODEX_READY"),
            ],
            timeout: Duration::from_secs(90),
        },
        CliAdapterSpec {
            name: "Gemini",
            command: "gemini",
            marker: "MAESTRO_CLI_SMOKE_GEMINI_READY",
            args: vec![
                "--prompt".to_string(),
                format!("{prompt_base} Marker: MAESTRO_CLI_SMOKE_GEMINI_READY"),
                "--output-format".to_string(),
                "text".to_string(),
                "--approval-mode".to_string(),
                "yolo".to_string(),
                "--skip-trust".to_string(),
            ],
            timeout: Duration::from_secs(90),
        },
    ]
}

pub(crate) fn run_cli_adapter_probe(spec: CliAdapterSpec) -> CliAdapterProbeResult {
    let started = Instant::now();
    let Some(path) = resolve_command(spec.command) else {
        return CliAdapterProbeResult {
            name: spec.name.to_string(),
            cli: spec.command.to_string(),
            tone: "blocked".to_string(),
            status: "CLI nao encontrada no PATH efetivo".to_string(),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            marker_found: false,
        };
    };

    match run_resolved_command_with_timeout(&path, &spec.args, spec.timeout, None) {
        Ok(result) => {
            let exit_code = result.output.status.code();
            let stdout = String::from_utf8_lossy(&result.output.stdout);
            let stderr = String::from_utf8_lossy(&result.output.stderr);
            let marker_found = stdout.contains(spec.marker) || stderr.contains(spec.marker);

            let (tone, status) = if result.timed_out {
                ("error", "timeout aguardando resposta da CLI")
            } else if result.output.status.success() && marker_found {
                ("ok", "CLI executada e marcador recebido")
            } else if result.output.status.success() {
                ("warn", "CLI executada, mas marcador esperado nao apareceu")
            } else {
                ("error", "CLI retornou codigo de saida diferente de zero")
            };

            CliAdapterProbeResult {
                name: spec.name.to_string(),
                cli: spec.command.to_string(),
                tone: tone.to_string(),
                status: status.to_string(),
                duration_ms: result.duration_ms,
                exit_code,
                marker_found,
            }
        }
        Err(error) => CliAdapterProbeResult {
            name: spec.name.to_string(),
            cli: spec.command.to_string(),
            tone: "error".to_string(),
            status: sanitize_text(&format!("falha ao executar CLI: {error}"), 240),
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            marker_found: false,
        },
    }
}
