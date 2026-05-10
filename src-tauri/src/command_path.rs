// Modulo: src-tauri/src/command_path.rs
// Descricao: PATH-resolution helpers for child command spawn extracted from
// lib.rs in v0.3.33 per `docs/code-split-plan.md` migration step 5.
//
// What's here (3 functions):
//   - `resolve_command` — locates a CLI by name on the effective PATH
//     (absolute and relative paths bypass the search). Returns the first
//     candidate that exists as a file.
//   - `command_candidate_paths` — on Windows, expands a bare `<command>`
//     stem into `[<command>.exe, <command>.cmd, <command>.bat,
//     <command>.ps1, <command>]` so the resolver can match any common
//     extension; on POSIX, returns the path unchanged.
//   - `command_search_dirs` — assembles the effective PATH-like search
//     order: process PATH first, then well-known Windows install
//     locations (USERPROFILE\.cargo\bin, APPDATA\npm, LOCALAPPDATA\Programs\
//     nodejs, LOCALAPPDATA\Microsoft\WinGet\Links, C:\npm-global,
//     WinGet ripgrep package dirs, C:\Program Files\nodejs, C:\nvm4w\nodejs,
//     C:\Program Files\GitHub CLI). Deduplicates by
//     case-insensitive path string.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `command_check`, `run_resolved_command_with_timeout`,
//     `run_resolved_command_observed`, `read_pipe_to_end_counting_classified`,
//     `classify_pipe_error`, `resolved_command_builder`,
//     `apply_editorial_agent_environment` — the spawn machinery is tightly
//     coupled to editorial orchestration via `CommandProgressContext` /
//     `TimedCommandOutput` / `log_editorial_agent_*` helpers; planned for a
//     follow-up batch when the editorial orchestration core is split.
//
// v0.3.33 is a pure move: every signature, format string, and PATH order
// is identical to the v0.3.32 lib.rs source (commit e149e9c).

use std::collections::BTreeSet;
#[cfg(windows)]
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn resolve_command(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.is_absolute() || command.contains('\\') || command.contains('/') {
        return command_candidate_paths(command_path)
            .into_iter()
            .find(|path| path.is_file());
    }

    command_search_dirs()
        .into_iter()
        .flat_map(|dir| command_candidate_paths(&dir.join(command)))
        .find(|path| path.is_file())
}

fn command_candidate_paths(path: &Path) -> Vec<PathBuf> {
    if path.extension().is_some() {
        return vec![path.to_path_buf()];
    }

    #[cfg(windows)]
    {
        ["exe", "cmd", "bat", "ps1", ""]
            .into_iter()
            .map(|ext| {
                if ext.is_empty() {
                    path.to_path_buf()
                } else {
                    path.with_extension(ext)
                }
            })
            .collect()
    }

    #[cfg(not(windows))]
    {
        vec![path.to_path_buf()]
    }
}

pub(crate) fn command_search_dirs() -> Vec<PathBuf> {
    let mut dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    #[cfg(windows)]
    {
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let user_profile = PathBuf::from(user_profile);
            dirs.push(user_profile.join(".cargo").join("bin"));
        }
        if let Some(app_data) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(app_data).join("npm"));
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let local_app_data = PathBuf::from(local_app_data);
            dirs.push(local_app_data.join("Programs").join("nodejs"));
            dirs.push(
                local_app_data
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
            );
            append_winget_ripgrep_dirs(
                &mut dirs,
                &local_app_data
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Packages"),
            );
        }
        dirs.push(PathBuf::from(r"C:\npm-global"));
        dirs.push(PathBuf::from(r"C:\Program Files\nodejs"));
        dirs.push(PathBuf::from(r"C:\nvm4w\nodejs"));
        dirs.push(PathBuf::from(r"C:\Program Files\GitHub CLI"));
    }

    let mut seen = BTreeSet::new();
    dirs.into_iter()
        .filter(|dir| seen.insert(dir.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

#[cfg(windows)]
fn append_winget_ripgrep_dirs(dirs: &mut Vec<PathBuf>, packages_dir: &Path) {
    let Ok(packages) = fs::read_dir(packages_dir) else {
        return;
    };
    for package in packages.flatten() {
        let package_path = package.path();
        let package_name = package_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !package_name.contains("ripgrep") {
            continue;
        }
        if package_path.join("rg.exe").is_file() {
            dirs.push(package_path.clone());
        }
        if let Ok(children) = fs::read_dir(&package_path) {
            for child in children.flatten() {
                let child_path = child.path();
                if child_path.join("rg.exe").is_file() {
                    dirs.push(child_path);
                }
            }
        }
    }
}
