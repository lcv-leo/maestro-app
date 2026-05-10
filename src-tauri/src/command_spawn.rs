// Modulo: src-tauri/src/command_spawn.rs
// Descricao: Child-process spawn machinery (timeout, progress logging, pipe
// readers, command builders, environment policy) extracted from lib.rs in
// v0.3.35 per `docs/code-split-plan.md` migration step 5.
//
// What's here (8 items):
//   - `CommandProgressContext<'a>` — per-spawn context (log_session,
//     run_id, agent, role, cli, output_path) used to emit
//     `session.agent.spawned` / `session.agent.running` NDJSON entries.
//   - `command_check` — diagnostic helper called by `dependency_preflight`
//     to verify each peer CLI is on PATH and answers `--version`.
//   - `run_resolved_command_with_timeout` — convenience wrapper that
//     forwards to `run_resolved_command_observed` with no progress context.
//   - `run_resolved_command_observed` — the heavy spawn loop: builds the
//     command via `resolved_command_builder`, sets working dir from the
//     progress's output_path (or `app_root()` fallback), spawns, drains
//     stdout/stderr in 2 reader threads with byte counters, polls every
//     250ms, emits `session.agent.running` every 30s, honors optional
//     timeout, and returns `TimedCommandOutput`.
//   - `read_pipe_to_end_counting_classified` — pipe reader that increments
//     a shared atomic byte counter and classifies any I/O error.
//   - `classify_pipe_error` — Windows-aware classifier (raw_os_error 109/
//     232/233 + std ErrorKind variants).
//   - `resolved_command_builder` — Windows: routes `.cmd`/`.bat` through
//     `cmd.exe /C` and `.ps1` through `powershell.exe -NoProfile
//     -ExecutionPolicy Bypass -File`; everything else via `hidden_command`.
//     Always applies `apply_editorial_agent_environment`.
//   - `apply_editorial_agent_environment` — sets UTF-8 (`PYTHONIOENCODING`/
//     `PYTHONUTF8`/`LC_ALL`/`LANG`) on every child + `GEMINI_CLI_TRUST_WORKSPACE`
//     when the executable's stem is `gemini` (Gemini sandbox-trust env from B1).
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `TimedCommandOutput` struct (pub(crate) since v0.3.35 with all 5
//     fields).
//   - `hidden_command` (pub(crate)) — only entry point that funnels through
//     `apply_hidden_window_policy` per the v0.3.16 `clippy.toml`
//     `disallowed-methods` policy on `Command::new`.
//   - `app_root` (already pub(crate)).
//   - `command_working_dir_for_output` (pub(crate)) — wrapper around the
//     output_path's parent dir.
//   - `log_editorial_agent_spawned`, `log_editorial_agent_running` (both
//     pub(crate)) — NDJSON helpers tightly coupled with the editorial
//     orchestration log schema.
//   - `sanitize_text` (already pub(crate) via v0.3.34 re-export).
//   - `resolve_command` from `crate::command_path` (pub(crate) since v0.3.33).
//
// v0.3.35 is a pure move: every signature, format string, sleep cadence,
// 30-second progress interval, and Windows error code is identical to the
// v0.3.34 lib.rs source (commit e00538e).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::command_path::{command_search_dirs, resolve_command};
use crate::logging::LogSession;
use crate::{
    app_root, command_working_dir_for_output, hidden_command, log_editorial_agent_running,
    log_editorial_agent_spawned, sanitize_text, TimedCommandOutput,
};

/// Cap per-pipe buffer at 64 MiB. Beyond the cap, bytes continue to be drained
/// from the pipe (so the child does not block on a full pipe), but they are not
/// retained; the pipe_error field gets the truncation marker so the artifact
/// surfaces the cause.
pub(crate) const MAX_PIPE_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn command_check(label: &str, command: &str, args: &[&str]) -> Value {
    let Some(path) = resolve_command(command) else {
        return json!({
            "label": label,
            "value": "nao encontrado no PATH efetivo",
            "tone": "blocked"
        });
    };
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = run_resolved_command_with_timeout(&path, &args, Duration::from_secs(12), None);

    match output {
        Ok(result) if result.timed_out => json!({
            "label": label,
            "value": sanitize_text("diagnostico excedeu 12s; CLI pode exigir login ou inicializacao lenta", 220),
            "tone": "warn"
        }),
        Ok(result) if result.output.status.success() => {
            let stdout = String::from_utf8_lossy(&result.output.stdout);
            let stderr = String::from_utf8_lossy(&result.output.stderr);
            let detail = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("detectado")
                .trim();
            let resolved_note = format!(" via {}", path.to_string_lossy());
            json!({
                "label": label,
                "value": sanitize_text(&format!("{detail}{resolved_note}"), 220),
                "tone": "ok"
            })
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.output.stderr);
            let stdout = String::from_utf8_lossy(&result.output.stdout);
            let detail = stderr
                .lines()
                .chain(stdout.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("comando retornou falha")
                .trim();
            json!({
                "label": label,
                "value": sanitize_text(detail, 220),
                "tone": "warn"
            })
        }
        Err(error) => json!({
            "label": label,
            "value": sanitize_text(&format!("nao encontrado/executado: {error}"), 220),
            "tone": "blocked"
        }),
    }
}

pub(crate) struct CommandProgressContext<'a> {
    pub(crate) log_session: &'a LogSession,
    pub(crate) run_id: &'a str,
    pub(crate) agent: &'a str,
    pub(crate) role: &'a str,
    pub(crate) cli: &'a str,
    pub(crate) output_path: &'a Path,
}

pub(crate) fn run_resolved_command_with_timeout(
    path: &Path,
    args: &[String],
    timeout: Duration,
    stdin_text: Option<&str>,
) -> std::io::Result<TimedCommandOutput> {
    run_resolved_command_observed(path, args, Some(timeout), stdin_text, None, None)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_resolved_command_observed(
    path: &Path,
    args: &[String],
    timeout: Option<Duration>,
    stdin_text: Option<&str>,
    progress: Option<CommandProgressContext<'_>>,
    cancel_token: Option<&CancellationToken>,
) -> std::io::Result<TimedCommandOutput> {
    let started = Instant::now();
    let mut command = resolved_command_builder(path, args);
    let working_dir = progress
        .as_ref()
        .map(|progress| command_working_dir_for_output(progress.output_path))
        .unwrap_or_else(app_root);
    command
        .current_dir(&working_dir)
        .stdin(if stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let child_id = child.id();
    if let Some(progress) = progress.as_ref() {
        log_editorial_agent_spawned(progress, child_id, path, &working_dir);
    }
    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(text.as_bytes()) {
                kill_process_tree(&mut child);
                let _ = child.wait();
                return Err(error);
            }
        }
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_bytes = Arc::new(AtomicU64::new(0));
    let stderr_bytes = Arc::new(AtomicU64::new(0));
    let stdout_counter = Arc::clone(&stdout_bytes);
    let stderr_counter = Arc::clone(&stderr_bytes);
    let stdout_handle =
        thread::spawn(move || read_pipe_to_end_counting_classified(stdout, stdout_counter));
    let stderr_handle =
        thread::spawn(move || read_pipe_to_end_counting_classified(stderr, stderr_counter));
    let mut last_progress = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            let (stdout, stdout_pipe_error) = stdout_handle
                .join()
                .unwrap_or_else(|_| (Vec::new(), Some("stdout_thread_panic".to_string())));
            let (stderr, stderr_pipe_error) = stderr_handle
                .join()
                .unwrap_or_else(|_| (Vec::new(), Some("stderr_thread_panic".to_string())));
            return Ok(TimedCommandOutput {
                output: Output {
                    status,
                    stdout,
                    stderr,
                },
                duration_ms: started.elapsed().as_millis(),
                timed_out: false,
                stdout_pipe_error,
                stderr_pipe_error,
            });
        }

        if let Some(timeout) = timeout {
            if started.elapsed() >= timeout {
                kill_process_tree(&mut child);
                let status = child.wait()?;
                let (stdout, stdout_pipe_error) = stdout_handle
                    .join()
                    .unwrap_or_else(|_| (Vec::new(), Some("stdout_thread_panic".to_string())));
                let (stderr, stderr_pipe_error) = stderr_handle
                    .join()
                    .unwrap_or_else(|_| (Vec::new(), Some("stderr_thread_panic".to_string())));
                return Ok(TimedCommandOutput {
                    output: Output {
                        status,
                        stdout,
                        stderr,
                    },
                    duration_ms: started.elapsed().as_millis(),
                    timed_out: true,
                    stdout_pipe_error,
                    stderr_pipe_error,
                });
            }
        }

        // Cancellation check fires every 250ms (next loop iteration). When
        // operator presses "Stop session" the token is signaled; we kill the
        // child process tree and return with `timed_out: true` so the caller
        // surfaces it as a STOPPED_BY_USER artifact.
        if let Some(token) = cancel_token {
            if token.is_cancelled() {
                kill_process_tree(&mut child);
                let status = child.wait()?;
                let (stdout, stdout_pipe_error) = stdout_handle
                    .join()
                    .unwrap_or_else(|_| (Vec::new(), Some("stdout_thread_panic".to_string())));
                let (stderr, stderr_pipe_error) = stderr_handle
                    .join()
                    .unwrap_or_else(|_| (Vec::new(), Some("stderr_thread_panic".to_string())));
                return Ok(TimedCommandOutput {
                    output: Output {
                        status,
                        stdout,
                        stderr,
                    },
                    duration_ms: started.elapsed().as_millis(),
                    timed_out: true,
                    stdout_pipe_error,
                    stderr_pipe_error,
                });
            }
        }

        if last_progress.elapsed() >= Duration::from_secs(30) {
            if let Some(progress) = progress.as_ref() {
                log_editorial_agent_running(
                    progress,
                    child_id,
                    started.elapsed(),
                    stdout_bytes.load(Ordering::Relaxed),
                    stderr_bytes.load(Ordering::Relaxed),
                );
            }
            last_progress = Instant::now();
        }

        thread::sleep(Duration::from_millis(250));
    }
}

fn read_pipe_to_end_counting_classified(
    pipe: Option<impl Read>,
    byte_counter: Arc<AtomicU64>,
) -> (Vec<u8>, Option<String>) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut pipe_error: Option<String> = None;
    let mut truncated = false;
    if let Some(mut pipe) = pipe {
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    byte_counter.fetch_add(count as u64, Ordering::Relaxed);
                    if !truncated {
                        let projected = buffer.len() as u64 + count as u64;
                        if projected <= MAX_PIPE_BYTES {
                            buffer.extend_from_slice(&chunk[..count]);
                        } else {
                            // Append only the prefix that fits within MAX_PIPE_BYTES, then
                            // mark truncated and keep draining without retaining further bytes.
                            // Draining (instead of breaking) keeps the child unblocked when the
                            // OS pipe fills up, so the timeout branch can still reap it cleanly.
                            let remaining = MAX_PIPE_BYTES.saturating_sub(buffer.len() as u64);
                            if remaining > 0 {
                                let remaining = remaining as usize;
                                buffer.extend_from_slice(&chunk[..remaining]);
                            }
                            truncated = true;
                            pipe_error = Some(format!(
                                "stdout_truncated_oversize (cap={MAX_PIPE_BYTES} bytes; further output drained but not retained)"
                            ));
                        }
                    }
                }
                Err(error) => {
                    if pipe_error.is_none() {
                        pipe_error = Some(classify_pipe_error(&error));
                    }
                    break;
                }
            }
        }
    }
    (buffer, pipe_error)
}

pub(crate) fn classify_pipe_error(error: &std::io::Error) -> String {
    let raw = error.raw_os_error();
    let kind = error.kind();
    let label = match (raw, kind) {
        (Some(109), _) => "windows_error_109_broken_pipe",
        (Some(232), _) => "windows_error_232_pipe_closing",
        (Some(233), _) => "windows_error_233_pipe_no_listener",
        (_, std::io::ErrorKind::BrokenPipe) => "broken_pipe",
        (_, std::io::ErrorKind::UnexpectedEof) => "unexpected_eof",
        (_, std::io::ErrorKind::Interrupted) => "interrupted",
        (_, std::io::ErrorKind::TimedOut) => "timed_out",
        _ => "other",
    };
    let raw_label = raw
        .map(|code| code.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!("{label} (kind={kind:?}, raw_os_error={raw_label})")
}

fn resolved_command_builder(path: &Path, args: &[String]) -> Command {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    #[cfg(windows)]
    {
        if extension == "cmd" || extension == "bat" {
            let mut command =
                hidden_command(std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string()));
            command.arg("/C").arg(path).args(args);
            apply_editorial_agent_environment(&mut command, path);
            return command;
        }

        if extension == "ps1" {
            let mut command = hidden_command("powershell.exe");
            command
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(path)
                .args(args);
            apply_editorial_agent_environment(&mut command, path);
            return command;
        }
    }

    let mut command = hidden_command(path);
    command.args(args);
    apply_editorial_agent_environment(&mut command, path);
    command
}

/// Kill a child process AND its descendant tree.
///
/// On Windows, `child.kill()` only terminates the direct PID. When a peer is
/// reached through `cmd.exe /C <peer>.cmd` (or any other shim), the actual
/// peer process is a grandchild of `child` and survives the direct kill. We
/// invoke `taskkill /T /F /PID <pid>` to walk the descendant tree, then call
/// `child.wait()` to reap the cmd.exe wrapper. On non-Windows platforms,
/// `child.kill()` already sends SIGKILL to the process group when the child
/// was set up as a session leader; we keep the simple direct kill there.
pub(crate) fn kill_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let pid = child.id();
        let mut taskkill = hidden_command("taskkill");
        taskkill.args(["/T", "/F", "/PID", &pid.to_string()]);
        // Best-effort: if taskkill itself fails (rare; e.g. PATH stripped), we
        // still fall back to the direct kill below so the child does not leak.
        let _ = taskkill.output();
        let _ = child.kill();
    }
    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }
}

pub(crate) fn apply_editorial_agent_environment(command: &mut Command, path: &Path) {
    command
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env("LC_ALL", "C.UTF-8")
        .env("LANG", "C.UTF-8")
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("CI", "1");

    if let Ok(path) = std::env::join_paths(command_search_dirs()) {
        command.env("PATH", path);
    }

    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if stem == "gemini" {
        command.env("GEMINI_CLI_TRUST_WORKSPACE", "true");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn pipe_reader_retains_short_payloads_without_truncation() {
        let payload = b"hello world".to_vec();
        let counter = Arc::new(AtomicU64::new(0));
        let (buffer, pipe_error) = read_pipe_to_end_counting_classified(
            Some(Cursor::new(payload.clone())),
            Arc::clone(&counter),
        );
        assert_eq!(buffer, payload);
        assert!(pipe_error.is_none());
        assert_eq!(counter.load(Ordering::Relaxed), payload.len() as u64);
    }

    #[test]
    fn pipe_reader_caps_buffer_at_max_pipe_bytes_and_keeps_draining() {
        let oversize = (MAX_PIPE_BYTES as usize) + 4096;
        let payload = vec![b'x'; oversize];
        let counter = Arc::new(AtomicU64::new(0));
        let (buffer, pipe_error) = read_pipe_to_end_counting_classified(
            Some(Cursor::new(payload.clone())),
            Arc::clone(&counter),
        );
        assert_eq!(buffer.len(), MAX_PIPE_BYTES as usize);
        let marker = pipe_error.expect("truncation marker must be set");
        assert!(
            marker.contains("stdout_truncated_oversize"),
            "marker must surface root cause: {marker}"
        );
        assert!(
            marker.contains(&MAX_PIPE_BYTES.to_string()),
            "marker must include cap value: {marker}"
        );
        // Counter must reflect the FULL input the child wrote, not the truncated retained slice.
        assert_eq!(counter.load(Ordering::Relaxed), oversize as u64);
    }

    #[test]
    fn pipe_reader_classifies_io_error_when_no_truncation_yet() {
        // A pipe that returns Err before any bytes is the classify-only path.
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "boom"))
            }
        }
        let counter = Arc::new(AtomicU64::new(0));
        let (buffer, pipe_error) =
            read_pipe_to_end_counting_classified(Some(FailingReader), Arc::clone(&counter));
        assert!(buffer.is_empty());
        let marker = pipe_error.expect("classifier must run on first-error path");
        assert!(
            marker.contains("broken_pipe"),
            "io error must be classified, not silenced: {marker}"
        );
    }

    #[test]
    fn pipe_reader_truncation_marker_takes_precedence_over_late_io_error() {
        // After the cap is reached, an Err should not overwrite the truncation marker; the
        // operator needs to see WHY the buffer was capped, not a downstream pipe close.
        struct CapThenError {
            remaining: usize,
        }
        impl Read for CapThenError {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.remaining > 0 {
                    let n = buf.len().min(self.remaining);
                    buf[..n].fill(b'x');
                    self.remaining -= n;
                    Ok(n)
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "late"))
                }
            }
        }
        let oversize = (MAX_PIPE_BYTES as usize) + 8192;
        let counter = Arc::new(AtomicU64::new(0));
        let (buffer, pipe_error) = read_pipe_to_end_counting_classified(
            Some(CapThenError {
                remaining: oversize,
            }),
            Arc::clone(&counter),
        );
        assert_eq!(buffer.len(), MAX_PIPE_BYTES as usize);
        let marker = pipe_error.expect("marker must be set");
        assert!(
            marker.contains("stdout_truncated_oversize"),
            "truncation cause must win over late io error: {marker}"
        );
    }

    #[test]
    fn max_pipe_bytes_is_64_mib() {
        // Pin the cap so accidental edits surface in CI.
        assert_eq!(MAX_PIPE_BYTES, 64 * 1024 * 1024);
    }
}
