//! Diagnostic NDJSON logger surface.
//!
//! Extracted from `lib.rs` in the v0.3.19 split per `docs/code-split-plan.md`
//! migration order step 2 ("logging and path safety"). Behavior preserved:
//! the NDJSON record schema (schema_version=2) and the human-log projection
//! companion are unchanged. Test surface in `lib.rs::mod tests` continues to
//! exercise this module via `pub(crate)` re-exports.
//!
//! Stayed in `lib.rs` for this batch (still tightly coupled or pending a
//! later move):
//! - `install_process_panic_hook` and `write_early_crash_record` (they
//!   compose JSON crash records using a different schema and are wired into
//!   process-level panic state, not the per-session logger).
//! - `log_editorial_agent_*` helpers (depend on `EditorialAgentResult` and
//!   move with the editorial orchestration batch).
//! - `sanitize_text`, `sanitize_short`, `sanitize_value`, `redact_secrets`
//!   (general redaction utilities used app-wide; planned for the separate
//!   `text_utils.rs` / `redaction.rs` extraction).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use crate::app_paths::{app_root, checked_data_child_path, logs_dir};
use crate::human_logs::{human_log_summary, severity_number_for, write_human_log_projection};
use crate::{sanitize_short, sanitize_text, sanitize_value};

pub(crate) type LogEventEmitter = Arc<dyn Fn(Value) + Send + Sync + 'static>;

/// Monotonic sequence stamped into every NDJSON record so log consumers can
/// detect dropped lines or out-of-order writes. Process-scoped (resets on
/// each Tauri runtime start).
pub(crate) static NATIVE_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Per-process log session state shared across Tauri commands via
/// `tauri::State<LogSession>`. The `write_lock` mutex serializes record
/// writes so concurrent `write_log_record` calls produce valid line-
/// delimited NDJSON. Cloning the struct shares the same `Arc<Mutex<()>>`.
#[derive(Clone)]
pub(crate) struct LogSession {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) write_lock: Arc<Mutex<()>>,
    pub(crate) event_emitter: Option<LogEventEmitter>,
}

/// Frontend → backend payload for `write_log_event`. `level` is normalized
/// via `sanitize_short` then mapped to `severity_number` per the human-log
/// projection table. `context` is sanitized recursively up to depth 8 by
/// `sanitize_value` before being embedded in the record.
#[derive(Deserialize)]
pub(crate) struct LogEventInput {
    pub(crate) level: String,
    pub(crate) category: String,
    pub(crate) message: String,
    pub(crate) context: Option<Value>,
}

/// Returned to the frontend so caller can confirm the line was written and
/// surface the absolute path / session id for diagnostics.
#[derive(Serialize)]
pub(crate) struct LogWriteResult {
    pub(crate) path: String,
    pub(crate) session_id: String,
}

/// Build a new `LogSession` anchored at the current process's logs_dir.
/// The id is `<UTC timestamp>-pid<pid>` so every run produces a unique
/// log file under `data/logs/maestro-<id>.ndjson`.
#[cfg(test)]
pub(crate) fn create_log_session() -> LogSession {
    create_log_session_with_emitter(None)
}

pub(crate) fn create_log_session_with_emitter(
    event_emitter: Option<LogEventEmitter>,
) -> LogSession {
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let id = format!("{timestamp}-pid{}", process::id());
    LogSession {
        id: id.clone(),
        path: logs_dir().join(format!("maestro-{id}.ndjson")),
        write_lock: Arc::new(Mutex::new(())),
        event_emitter,
    }
}

/// Append one structured NDJSON record to the session's log file plus a
/// human-readable projection line. Holds the session's `write_lock` for
/// the entire write to keep concurrent writers from interleaving lines.
///
/// Schema version 2 fields:
/// - `timestamp`, `native_log_sequence`, `level`, `severity_text`, `severity_number`
/// - `category`, `event_name` (same value), `message`, `human_summary`
/// - `context` (sanitized to depth 8 via `sanitize_value`)
/// - `app` (name/version/target/arch)
/// - `process` (pid/cwd/app_root)
/// - `session` (id, log_file)
pub(crate) fn write_log_record(
    log_session: &LogSession,
    event: LogEventInput,
) -> Result<LogWriteResult, String> {
    let sequence = NATIVE_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    let log_path = checked_data_child_path(&log_session.path)?;

    let timestamp = Utc::now().to_rfc3339();
    let context = sanitize_value(event.context.unwrap_or(Value::Null), 8);
    let human_summary = human_log_summary(&event.category, &event.message, &context);
    let level = sanitize_short(&event.level, 16);
    let severity_number = severity_number_for(&level);
    let record = json!({
        "schema_version": 2,
        "timestamp": timestamp,
        "native_log_sequence": sequence,
        "level": level,
        "severity_text": level.to_ascii_uppercase(),
        "severity_number": severity_number,
        "category": sanitize_short(&event.category, 80),
        "event_name": sanitize_short(&event.category, 80),
        "message": sanitize_text(&event.message, 500),
        "human_summary": human_summary,
        "context": context,
        "app": {
            "name": "Maestro Editorial AI",
            "version": env!("CARGO_PKG_VERSION"),
            "target": std::env::consts::OS,
            "arch": std::env::consts::ARCH
        },
        "process": {
            "pid": process::id(),
            "cwd": std::env::current_dir().ok().map(|path| path.to_string_lossy().to_string()),
            "app_root": app_root().to_string_lossy().to_string()
        },
        "session": {
            "id": log_session.id.clone(),
            "log_file": log_path.to_string_lossy().to_string()
        }
    });
    let event_payload = json!({
        "timestamp": record["timestamp"].clone(),
        "level": record["level"].clone(),
        "category": record["category"].clone(),
        "message": record["message"].clone(),
        "context": record["context"].clone()
    });

    {
        let _guard = log_session
            .write_lock
            .lock()
            .map_err(|_| "failed to lock log writer".to_string())?;
        let dir = checked_data_child_path(&logs_dir())?;
        fs::create_dir_all(&dir).map_err(|error| format!("failed to create log dir: {error}"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|error| format!("failed to open log file: {error}"))?;
        writeln!(file, "{record}").map_err(|error| format!("failed to write log record: {error}"))?;
        let _ = write_human_log_projection(&log_session.path, &record);
    }
    if let Some(emitter) = &log_session.event_emitter {
        emitter(event_payload);
    }

    Ok(LogWriteResult {
        path: log_path.to_string_lossy().to_string(),
        session_id: log_session.id.clone(),
    })
}
