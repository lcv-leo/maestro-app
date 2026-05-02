// Modulo: src-tauri/src/sanitize.rs
// Descricao: Foundational text sanitization + secret redaction helpers
// extracted from lib.rs in v0.3.34 per `docs/code-split-plan.md`
// migration step 5.
//
// What's here (7 items):
//   - `sanitize_short(value, max_len)` — strips to ASCII alphanumerics
//     plus `_-.:`, ideal for IDs/labels/short logs.
//   - `sanitize_text(value, max_len)` — redacts secrets first, then
//     truncates by char count. The default sanitizer for any free-form
//     operator-facing text.
//   - `truncate_text_head_tail(value, head, tail)` — preserves the head
//     and tail of large stderr/stdout, useful for CLIs that emit large
//     preambles followed by the actual error tail.
//   - `sanitize_value(value, depth)` — recursive JSON sanitizer with
//     depth + array (80) + object (120) caps; flips to `<redacted>` for
//     keys matching the secret/token/credential heuristic.
//   - `should_redact_key` (private) — keyname-based redaction predicate
//     with safe-suffix allowlist (`_present`/`_source`/`_scope`/etc.).
//   - `redact_secrets(value)` — replaces matches of the secret regex with
//     `<redacted>`. Handles sk-ant/sk_live/sk-/cfut_/cfat_/cfk_/xox[baprs]/
//     gh[pousr]/AIza/re_/AKIA/PEM private-key patterns.
//   - `secret_value_regex` (private) — `OnceLock<Regex>` cache.
//
// Re-export shim in lib.rs (v0.3.34): `pub(crate) use crate::sanitize::{
// redact_secrets, sanitize_short, sanitize_text, sanitize_value,
// truncate_text_head_tail };` — preserves the existing
// `crate::sanitize_text` import path across all 18 sibling modules so no
// downstream `use` statements need to change.
//
// v0.3.34 is a pure move: every signature, secret pattern, char filter,
// and depth/size cap is identical to the v0.3.33 lib.rs source (commit
// e296d89).

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

pub(crate) fn sanitize_short(value: &str, max_len: usize) -> String {
    sanitize_text(value, max_len)
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
        })
        .collect::<String>()
}

pub(crate) fn sanitize_text(value: &str, max_len: usize) -> String {
    let redacted = redact_secrets(value);
    redacted.chars().take(max_len).collect()
}

/// Truncates large stderr/stdout text preserving head and tail with a marker in the middle.
/// Useful for CLIs (Codex, others) that emit large preambles followed by the actual error tail.
pub(crate) fn truncate_text_head_tail(
    value: &str,
    head_chars: usize,
    tail_chars: usize,
) -> String {
    let redacted = redact_secrets(value);
    let total = redacted.chars().count();
    let cap = head_chars + tail_chars;
    if total <= cap {
        return redacted;
    }
    let head: String = redacted.chars().take(head_chars).collect();
    let tail: String = redacted
        .chars()
        .skip(total - tail_chars)
        .collect();
    let dropped = total - cap;
    format!(
        "{head}\n\n[... {dropped} chars truncated (head {head_chars} / tail {tail_chars}) ...]\n\n{tail}"
    )
}

pub(crate) fn sanitize_value(value: Value, depth: usize) -> Value {
    if depth == 0 {
        return Value::String("<max_depth_reached>".to_string());
    }

    match value {
        Value::String(text) => Value::String(sanitize_text(&text, 1200)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .take(80)
                .map(|item| sanitize_value(item, depth - 1))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .take(120)
                .map(|(key, value)| {
                    if should_redact_key(&key) {
                        (
                            sanitize_text(&key, 80),
                            Value::String("<redacted>".to_string()),
                        )
                    } else {
                        (sanitize_text(&key, 80), sanitize_value(value, depth - 1))
                    }
                })
                .collect(),
        ),
        primitive => primitive,
    }
}

pub(crate) fn should_redact_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "credential_storage_mode"
            | "cloudflare_api_token_source"
            | "cloudflare_api_token_env_var"
            | "cloudflare_api_token_env_scope"
            | "cloudflare_api_token_present"
            | "token_source"
            | "token_env_var"
            | "token_present"
            | "secret_store"
    ) {
        return false;
    }

    let safe_suffixes = [
        "_present",
        "_source",
        "_scope",
        "_env_var",
        "_env_scope",
        "_mode",
        "_label",
        "_name",
        "_status",
        "_tone",
        "_kind",
        "_prefix",
    ];
    if safe_suffixes.iter().any(|suffix| lowered.ends_with(suffix)) {
        return false;
    }

    lowered.contains("secret")
        || lowered.contains("token")
        || lowered.contains("password")
        || lowered.contains("credential")
        || lowered.contains("api_key")
        || lowered.contains("api-key")
        || lowered.contains("authorization")
        || lowered.contains("cookie")
        || lowered.contains("private")
}

pub(crate) fn redact_secrets(value: &str) -> String {
    secret_value_regex()
        .replace_all(value, "<redacted>")
        .to_string()
}

fn secret_value_regex() -> &'static Regex {
    static SECRET_VALUE_REGEX: OnceLock<Regex> = OnceLock::new();
    SECRET_VALUE_REGEX.get_or_init(|| {
        Regex::new(
            r"(?m)(sk-ant-[A-Za-z0-9_-]{8,}|sk_live_[A-Za-z0-9_-]{8,}|sk-[A-Za-z0-9_-]{8,}|cfut_[A-Za-z0-9_-]{8,}|cfat_[A-Za-z0-9_-]{8,}|cfk_[A-Za-z0-9_-]{8,}|xox[baprs]-[A-Za-z0-9-]{8,}|gh[pousr]_[A-Za-z0-9_]{8,}|AIza[0-9A-Za-z_-]{8,}|re_[A-Za-z0-9_-]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN[^\r\n]*(?:\r?\n[^\r\n]*){0,80})",
        )
        .expect("valid secret redaction regex")
    })
}
