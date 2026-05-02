// Modulo: src-tauri/src/app_init.rs
// Descricao: App boot helpers (Tauri root initialization, panic hook,
// early-crash NDJSON record writer, hidden Command primitive) extracted
// from lib.rs in v0.3.48 per `docs/code-split-plan.md` migration order
// step 2 ("logging and path safety" tail items deferred from v0.3.17).
//
// What's here (5 items):
//   - `initialize_app_root` — invoked from `pub fn run()` in lib.rs at
//     Tauri `setup` time; resolves portable root via `app_paths` and
//     stores it in the `OnceLock`.
//   - `install_process_panic_hook` — installs `std::panic::set_hook` that
//     forwards every native panic to `write_early_crash_record` so we
//     have a JSON crash trail even when the normal NDJSON logger has
//     not finished startup yet.
//   - `write_early_crash_record` — writes `data/logs/maestro-crash-
//     <timestamp>-pid<pid>.json` with payload + location + app/process
//     metadata. Cap-limited via `sanitize_text` (1000 char payload, 500
//     char location).
//   - `hidden_command` — the SAFE-FUNNEL `Command::new` allowed by
//     `clippy.toml` `disallowed-methods`. Always passes through
//     `apply_hidden_window_policy`, which on Windows sets
//     `CREATE_NO_WINDOW` (0x08000000) so editorial peer spawns never
//     flash a console window.
//   - `apply_hidden_window_policy` — Windows-only flag setter; no-op on
//     non-Windows. Module-private.
//
// What stays in lib.rs:
//   - `pub fn run()` — Tauri 2 binary entry point with the
//     `#[cfg_attr(mobile, tauri::mobile_entry_point)]` attribute, which
//     prefers to live in lib.rs.
//   - The corresponding `tests::writes_early_crash_record_before_normal_logger`
//     test continues to work via the re-exported function.
//
// v0.3.48 is a pure move: every signature, format string, JSON shape,
// and Windows creation flag is identical to the v0.3.47 lib.rs source
// (commit 4b56e0d).

use chrono::Utc;
use serde_json::json;
use std::fs;
use std::process;
use std::process::Command;

use crate::app_paths::{
    active_or_early_logs_dir, app_root_if_initialized, resolve_portable_app_root, try_set_app_root,
};
use crate::sanitize::sanitize_text;

pub(crate) fn initialize_app_root(app: &tauri::App) -> Result<(), String> {
    let _ = app;
    let root = resolve_portable_app_root()?;
    try_set_app_root(root);
    Ok(())
}

pub(crate) fn install_process_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("unknown panic payload");
        let location = panic_info.location().map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        });
        let _ = write_early_crash_record(payload, location.as_deref());
    }));
}

pub(crate) fn write_early_crash_record(
    payload: &str,
    location: Option<&str>,
) -> Result<(), String> {
    let dir = active_or_early_logs_dir();
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create early crash log dir: {error}"))?;
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let path = dir.join(format!(
        "maestro-crash-{timestamp}-pid{}.json",
        process::id()
    ));
    let record = json!({
        "schema_version": 1,
        "timestamp": Utc::now().to_rfc3339(),
        "level": "fatal",
        "category": "native.panic",
        "message": "native panic captured before normal diagnostic logger completed startup",
        "panic": {
            "payload": sanitize_text(payload, 1000),
            "location": location.map(|value| sanitize_text(value, 500))
        },
        "app": {
            "name": "Maestro Editorial AI",
            "version": env!("CARGO_PKG_VERSION"),
            "target": std::env::consts::OS,
            "arch": std::env::consts::ARCH
        },
        "process": {
            "pid": process::id(),
            "cwd": std::env::current_dir().ok().map(|path| path.to_string_lossy().to_string()),
            "current_exe": std::env::current_exe().ok().map(|path| path.to_string_lossy().to_string()),
            "app_root": app_root_if_initialized().map(|path| path.to_string_lossy().to_string())
        }
    });
    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|error| format!("failed to serialize early crash log: {error}"))?;
    fs::write(&path, bytes).map_err(|error| format!("failed to write early crash log: {error}"))
}

pub(crate) fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    // SAFE-FUNNEL: this is the single allowed Command::new call site for editorial spawns.
    // See `clippy.toml` and the `#![warn(clippy::disallowed_methods)]` at the top of lib.rs.
    #[allow(clippy::disallowed_methods)]
    let mut command = Command::new(program);
    apply_hidden_window_policy(&mut command);
    command
}

#[cfg(windows)]
fn apply_hidden_window_policy(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn apply_hidden_window_policy(_command: &mut Command) {}
