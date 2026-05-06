// Modulo: src-tauri/src/session_resume.rs
// Descricao: Session-time + extract/parse helpers + protocol-backup utilities
// extracted from lib.rs in v0.3.28 per `docs/code-split-plan.md` migration step 5.
//
// What's here (12 functions + 1 struct):
//   - `parse_created_at`, `remaining_session_duration`, `session_time_exhausted`
//     — wall-clock helpers around the optional `max_session_minutes` cap.
//   - `extract_bullet_code_value` — pulls `- {label}: \`value\`` markdown
//     bullets out of artifact bodies.
//   - `humanize_agent_name` — renders peer keys as the user-facing label.
//   - `extract_saved_session_name`, `extract_saved_initial_agent`,
//     `extract_saved_prompt` — parse fields back out of a saved `prompt.md`.
//   - `stable_text_fingerprint` — FNV-64 hash of arbitrary text used as a
//     stable per-prompt identifier across resume cycles.
//   - `count_known_session_markdown_artifacts` and
//     `known_session_activity_unix` — session-directory inspection helpers.
//   - `ProtocolBackupStats` struct + `protocol_backup_stats` —
//     `protocolo-anterior-*.md` summary (count + latest mtime) used by both
//     of the inspection helpers above.
//   - `is_protocol_backup_file_name` — name-based classifier for the protocol
//     backup files.
//   - `system_time_to_unix` — small SystemTime → seconds converter.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `SessionArtifact` struct (v0.3.28 upgrades fields to pub(crate)).
//   - `checked_data_child_path`, `is_safe_data_file_name` (already pub(crate)
//     from `app_paths.rs`).
//   - `resolve_initial_agent_key` (already pub(crate) from `editorial_prompts.rs`).
//
// v0.3.28 is a pure move: every signature and body byte is identical to the
// v0.3.27 lib.rs source (commit 065ac2b). Only the visibility prefix changes.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};

use crate::app_paths::{checked_data_child_path, is_safe_data_file_name};
use crate::editorial_prompts::resolve_initial_agent_key;
use crate::SessionArtifact;

pub(crate) fn parse_created_at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub(crate) fn remaining_session_duration(
    created_at: DateTime<Utc>,
    max_session_minutes: Option<u64>,
) -> Option<Duration> {
    let minutes = max_session_minutes?;
    let deadline = created_at + chrono::Duration::minutes(minutes as i64);
    let remaining = deadline - Utc::now();
    if remaining.num_milliseconds() <= 0 {
        Some(Duration::from_secs(0))
    } else {
        Some(Duration::from_millis(remaining.num_milliseconds() as u64))
    }
}

pub(crate) fn session_time_exhausted(
    created_at: DateTime<Utc>,
    max_session_minutes: Option<u64>,
) -> bool {
    remaining_session_duration(created_at, max_session_minutes)
        .map(|duration| duration.as_secs() < 2)
        .unwrap_or(false)
}

pub(crate) fn extract_bullet_code_value(text: &str, label: &str) -> Option<String> {
    let prefix = format!("- {label}: `");
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?;
        let end = value.find('`')?;
        Some(value[..end].trim().to_string())
    })
}

pub(crate) fn humanize_agent_name(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        "deepseek" => "DeepSeek".to_string(),
        "grok" => "Grok".to_string(),
        other => other
            .replace('_', " ")
            .split_whitespace()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub(crate) fn extract_saved_session_name(prompt_file: &str) -> Option<String> {
    prompt_file.lines().find_map(|line| {
        let value = line.strip_prefix("Sessao: ")?;
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

pub(crate) fn extract_saved_initial_agent(prompt_file: &str) -> Option<String> {
    prompt_file.lines().find_map(|line| {
        let value = line.strip_prefix("Agente redator inicial: ")?;
        let value = value.trim().trim_matches('`');
        let (agent, invalid) = resolve_initial_agent_key(Some(value));
        if invalid.is_some() {
            None
        } else {
            Some(agent.to_string())
        }
    })
}

pub(crate) fn extract_saved_prompt(prompt_file: &str) -> Option<String> {
    let marker = "\n\n";
    let (_, prompt) = prompt_file.split_once(marker)?;
    let (_, prompt) = prompt.split_once(marker)?;
    let prompt = prompt.trim();
    if prompt.is_empty() {
        None
    } else {
        Some(prompt.to_string())
    }
}

pub(crate) fn stable_text_fingerprint(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64-{hash:016x}")
}

pub(crate) fn count_known_session_markdown_artifacts(
    session_dir: &Path,
    artifacts: &[SessionArtifact],
) -> Result<usize, String> {
    let session_dir = checked_data_child_path(session_dir)?;
    let backup_stats = protocol_backup_stats(&session_dir)?;
    let known_session_files = [
        "prompt.md",
        "protocolo.md",
        "ata-da-sessao.md",
        "texto-final.md",
    ];
    let session_file_count = known_session_files
        .iter()
        .filter(|file_name| session_dir.join(file_name).is_file())
        .count();
    Ok(session_file_count + backup_stats.count + artifacts.len())
}

pub(crate) fn known_session_activity_unix(
    session_dir: &Path,
    prompt_path: &Path,
    protocol_path: &Path,
    artifacts: &[SessionArtifact],
) -> Option<u64> {
    let backup_latest = protocol_backup_stats(session_dir)
        .ok()
        .and_then(|stats| stats.latest_activity_unix);
    [
        checked_data_child_path(session_dir).ok(),
        checked_data_child_path(prompt_path).ok(),
        checked_data_child_path(protocol_path).ok(),
        checked_data_child_path(&session_dir.join("ata-da-sessao.md")).ok(),
        checked_data_child_path(&session_dir.join("texto-final.md")).ok(),
    ]
    .into_iter()
    .flatten()
    .chain(artifacts.iter().map(|artifact| artifact.path.clone()))
    .filter_map(|path| {
        path.metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_to_unix)
    })
    .chain(backup_latest)
    .max()
}

pub(crate) struct ProtocolBackupStats {
    pub(crate) count: usize,
    pub(crate) latest_activity_unix: Option<u64>,
}

pub(crate) fn protocol_backup_stats(session_dir: &Path) -> Result<ProtocolBackupStats, String> {
    let session_dir = checked_data_child_path(session_dir)?;
    if !session_dir.is_dir() {
        return Ok(ProtocolBackupStats {
            count: 0,
            latest_activity_unix: None,
        });
    }

    let mut count = 0;
    let mut latest_activity_unix = None;
    for entry in fs::read_dir(&session_dir)
        .map_err(|error| format!("failed to read session backup dir: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to read session backup entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read session backup entry type: {error}"))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_protocol_backup_file_name(name) {
            continue;
        }
        count += 1;
        if let Ok(metadata) = entry.metadata() {
            if let Some(modified) = metadata.modified().ok().and_then(system_time_to_unix) {
                latest_activity_unix = Some(
                    latest_activity_unix
                        .map(|current: u64| current.max(modified))
                        .unwrap_or(modified),
                );
            }
        }
    }

    Ok(ProtocolBackupStats {
        count,
        latest_activity_unix,
    })
}

pub(crate) fn is_protocol_backup_file_name(name: &str) -> bool {
    is_safe_data_file_name(name) && name.starts_with("protocolo-anterior-") && name.ends_with(".md")
}

pub(crate) fn system_time_to_unix(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}
