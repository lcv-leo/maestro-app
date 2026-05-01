//! App path resolution and data-directory safety primitives.
//!
//! Extracted from `lib.rs` in the v0.3.16+ split per `docs/code-split-plan.md`
//! migration order step 2 ("logging and path safety"). Behavior preserved:
//! every public function in this module returns the same `PathBuf` and exposes
//! the same `Result<_, String>` shape as the pre-extraction inline code. The
//! existing tests (`sanitizes_run_ids_for_path_segments`,
//! `rejects_paths_outside_data_dir`, `ignores_dotted_session_folder_names`,
//! `resolves_portable_root_from_current_exe_parent`) still cover this surface
//! from `mod tests` in `lib.rs` via `pub(crate)` re-exports.
//!
//! Tauri-bound `initialize_app_root` and the panic/crash record helpers stay
//! in `lib.rs` because they need access to the runtime `&tauri::App`,
//! diagnostic logger, and JSON shape that mixes process metadata.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

/// One-shot global storage for the resolved portable app root.
///
/// `lib.rs::initialize_app_root` writes this once during Tauri setup; every
/// path helper below reads it via `app_root()`. Tests bypass setup and rely
/// on the `#[cfg(test)]` branch in `app_root()` that returns a deterministic
/// per-target sandbox under `target/maestro-editorial-ai-tests`.
static APP_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set the portable app root. Idempotent: subsequent calls are silent no-ops
/// because `OnceLock::set` returns `Err` when already initialized. Used by
/// `lib.rs::initialize_app_root`.
pub(crate) fn try_set_app_root(root: PathBuf) {
    let _ = APP_ROOT.set(root);
}

/// Read the resolved app root if Tauri setup has completed; in tests, return
/// a deterministic per-target sandbox under `CARGO_MANIFEST_DIR/target` so
/// tests can exercise the data layout without ever touching the operator's
/// real `data/` directory. Outside of tests, panics if called before setup.
pub(crate) fn app_root() -> PathBuf {
    if let Some(path) = APP_ROOT.get() {
        return path.clone();
    }

    #[cfg(test)]
    {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("maestro-editorial-ai-tests");
    }

    #[cfg(not(test))]
    {
        panic!("Maestro app root must be initialized by Tauri setup before use");
    }
}

/// Return the portable executable's parent directory canonicalized. Used by
/// Tauri setup to anchor `data/`, `data/logs/`, etc.
pub(crate) fn resolve_portable_app_root() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    portable_root_from_exe_path(&exe)
}

/// Pure helper: extract the canonical parent directory of an executable path.
/// Errors if the path has no parent or canonicalization fails.
pub(crate) fn portable_root_from_exe_path(exe: &Path) -> Result<PathBuf, String> {
    let parent = exe
        .parent()
        .ok_or_else(|| "current executable path has no parent directory".to_string())?;
    parent
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize portable executable dir: {error}"))
}

/// Logs directory derived from the executable's parent BEFORE Tauri setup
/// completes. Used by the early panic hook so that crashes during boot still
/// land somewhere on disk.
pub(crate) fn early_logs_dir() -> PathBuf {
    resolve_portable_app_root()
        .unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("maestro-editorial-ai")
        })
        .join("data")
        .join("logs")
}

/// Logs directory after Tauri setup if available, otherwise the early-boot
/// fallback. Lets the panic hook produce diagnostics whether or not setup
/// completed before the panic.
pub(crate) fn active_or_early_logs_dir() -> PathBuf {
    APP_ROOT
        .get()
        .map(|root| root.join("data").join("logs"))
        .unwrap_or_else(early_logs_dir)
}

/// Read the global app root, optionally — used by the panic hook to surface
/// `app_root` only if Tauri setup ran successfully.
pub(crate) fn app_root_if_initialized() -> Option<PathBuf> {
    APP_ROOT.get().cloned()
}

pub(crate) fn data_dir() -> PathBuf {
    app_root().join("data")
}

pub(crate) fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub(crate) fn human_logs_dir() -> PathBuf {
    logs_dir().join("human")
}

/// Map a raw NDJSON log path to its human-readable projection path under
/// `data/logs/human/`. Pure: only stem extraction + suffix swap.
pub(crate) fn human_log_path_for(raw_log_path: &Path) -> PathBuf {
    let stem = raw_log_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("maestro-log");
    human_logs_dir().join(format!("{stem}.log"))
}

pub(crate) fn config_dir() -> PathBuf {
    data_dir().join("config")
}

pub(crate) fn bootstrap_config_path() -> PathBuf {
    config_dir().join("bootstrap.json")
}

pub(crate) fn ai_provider_config_path() -> PathBuf {
    config_dir().join("ai-providers.json")
}

pub(crate) fn sessions_dir() -> PathBuf {
    data_dir().join("sessions")
}

/// Validate and canonicalize a candidate child path inside `data_dir()`.
/// Rejects: non-absolute paths, paths outside `data_dir()` (including via
/// `..` traversal), and paths whose components contain unsafe characters
/// per `is_safe_data_file_name`. This is the universal gate for every IPC
/// command that writes to disk.
pub(crate) fn checked_data_child_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("internal data path must be absolute".to_string());
    }

    let data_root = data_dir();
    fs::create_dir_all(&data_root)
        .map_err(|error| format!("failed to create Maestro data root: {error}"))?;
    let relative = path
        .strip_prefix(&data_root)
        .map_err(|_| "internal data path escaped Maestro data directory".to_string())?;

    if !is_safe_relative_data_path(relative) {
        return Err("internal data path contains unsafe segments".to_string());
    }

    Ok(data_root.join(relative))
}

/// Predicate: every component of a relative data path must be a Normal
/// (non-traversal, non-prefix) component AND match the safe-name whitelist.
pub(crate) fn is_safe_relative_data_path(path: &Path) -> bool {
    path.components().all(|component| match component {
        Component::Normal(value) => value.to_str().map(is_safe_data_file_name).unwrap_or(false),
        _ => false,
    })
}

/// Whitelist for individual file/directory names inside `data/`.
///
/// General data filenames may contain dots for extensions; run IDs stay
/// stricter through `sanitize_path_segment` because they become directory
/// names that must not collide with `.` or `..` in any context.
pub(crate) fn is_safe_data_file_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 255
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

/// Promote a session directory name to a run_id only if the name round-trips
/// through `sanitize_path_segment` unchanged. Used to skip stray dotfiles or
/// renamed directories without rejecting genuine sessions.
pub(crate) fn safe_run_id_from_entry(entry: &fs::DirEntry) -> Option<String> {
    let name = entry.file_name();
    let name = name.to_str()?;
    let sanitized = sanitize_path_segment(name, 120);
    if sanitized == name {
        Some(sanitized)
    } else {
        None
    }
}

/// Stricter-than-`is_safe_data_file_name` filter for path SEGMENTS that will
/// become directory names (typically run IDs). Strips dots, slashes, and
/// any character that could break a data path or shell. Truncates to
/// `max_len` chars and trims leading/trailing `_` or `-`.
pub(crate) fn sanitize_path_segment(value: &str, max_len: usize) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(max_len)
        .collect::<String>()
        .trim_matches(['_', '-'])
        .to_string()
}
