// Modulo: src-tauri/src/api_payloads.rs
// Descricao: Provider API request payload builders + native attachment
// support detection extracted from lib.rs in v0.3.44 per
// `docs/code-split-plan.md` migration step 3 (provider API surfaces).
//
// This module owns the per-provider JSON shapes for the chat-completions
// request bodies (`openai_api_input`, `anthropic_api_user_content`,
// `gemini_api_user_parts`), the attachment-by-attachment dispatch table
// (`provider_supports_native_attachment` + 4 per-provider helpers + the
// 20 MiB payload cap), and the input-cost preflight estimator
// (`api_input_estimate_chars`). All consumed by the API peer runners in
// `provider_runners.rs` (openai/anthropic/gemini) and by
// `provider_deepseek.rs`.
//
// What's here (10 items):
//   - `pub(crate) const API_NATIVE_ATTACHMENT_MAX_FILE_BYTES: u64 =
//     20 * 1024 * 1024` — single-file cap on inline base64 payloads sent
//     in the API request body. Files above the cap are silently skipped
//     by the per-provider supported() helpers.
//   - `pub(crate) fn api_input_estimate_chars(prompt, attachments,
//     provider) -> usize` — sums prompt chars + per-attachment overhead
//     (base64 chars + filename chars + media-type chars + 96 bytes for
//     JSON envelope). Used by cost preflight to estimate input tokens
//     before spending API budget.
//   - `provider_supports_native_attachment(provider, entry) -> bool` —
//     dispatches to the 3 per-provider helpers; unknown provider → false.
//   - `openai_api_attachment_supported(entry) -> bool` — image OR file
//     (known document type), gated by payload cap.
//   - `openai_api_file_attachment_supported(entry) -> bool` — known
//     document attachment proxy.
//   - `anthropic_api_attachment_supported(entry) -> bool` — image OR
//     PDF, gated by payload cap.
//   - `gemini_api_attachment_supported(entry) -> bool` — image | audio
//     | video | PDF | text-like | known document, gated by payload cap.
//   - `attachment_within_native_payload_cap(entry) -> bool` — single
//     point of truth for the 20 MiB inline-payload limit.
//   - `pub(crate) fn openai_api_input(prompt, attachments) ->
//     Result<Value, String>` — Responses API input shape:
//     `[{"role":"user","content":[{type:input_text,text:...},
//     {type:input_image,image_url:...}, {type:input_file,filename,
//     file_data}]}]`. Skips attachments above payload cap.
//   - `pub(crate) fn anthropic_api_user_content(prompt, attachments) ->
//     Result<Value, String>` — Messages API user content shape:
//     `[{"type":"text",text:...}, {"type":"image","source":{"type":
//     "base64","media_type":..,"data":..}}, {"type":"document",
//     "source":..,"title":..}]`. Skips attachments above payload cap.
//   - `pub(crate) fn gemini_api_user_parts(prompt, attachments) ->
//     Result<Vec<Value>, String>` — generateContent parts shape:
//     `[{text:..}, {inline_data:{mime_type:..,data:..}}]`. Inline_data
//     entries gated by `gemini_api_attachment_supported`.
//
// What stayed in lib.rs:
//   - `AttachmentManifestEntry` struct lives in `session_evidence.rs`
//     and is consumed via `pub(crate)` cross-module imports here. Same
//     for the 10 attachment helpers (`is_image_attachment` etc.) and
//     the attachment payload helpers (`attachment_base64`,
//     `attachment_data_url`, `normalized_attachment_media_type`,
//     `attachment_payload_base64_chars`).
//
// v0.3.44 is a pure move: every signature, JSON key string, MIME type
// literal, and match arm is identical to the v0.3.43 lib.rs source
// (commit f7beeb7).

use serde_json::{json, Value};

use crate::session_evidence::{
    attachment_base64, attachment_data_url, attachment_payload_base64_chars, is_audio_attachment,
    is_image_attachment, is_known_document_attachment, is_pdf_attachment, is_text_like_attachment,
    is_video_attachment, normalized_attachment_media_type, AttachmentManifestEntry,
};

pub(crate) const API_NATIVE_ATTACHMENT_MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

pub(crate) fn api_input_estimate_chars(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
    provider: &str,
) -> usize {
    let attachment_chars = attachments
        .iter()
        .filter(|entry| provider_supports_native_attachment(provider, entry))
        .map(|entry| {
            attachment_payload_base64_chars(entry)
                + entry.file_name.chars().count()
                + normalized_attachment_media_type(entry).chars().count()
                + 96
        })
        .sum::<usize>();
    prompt.chars().count().saturating_add(attachment_chars)
}

fn provider_supports_native_attachment(provider: &str, entry: &AttachmentManifestEntry) -> bool {
    match provider {
        "openai" => openai_api_attachment_supported(entry),
        "anthropic" => anthropic_api_attachment_supported(entry),
        "gemini" => gemini_api_attachment_supported(entry),
        _ => false,
    }
}

fn openai_api_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    if !attachment_within_native_payload_cap(entry) {
        return false;
    }
    is_image_attachment(entry) || openai_api_file_attachment_supported(entry)
}

fn openai_api_file_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    is_known_document_attachment(entry)
}

fn anthropic_api_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    if !attachment_within_native_payload_cap(entry) {
        return false;
    }
    is_image_attachment(entry) || is_pdf_attachment(entry)
}

fn gemini_api_attachment_supported(entry: &AttachmentManifestEntry) -> bool {
    if !attachment_within_native_payload_cap(entry) {
        return false;
    }
    is_image_attachment(entry)
        || is_audio_attachment(entry)
        || is_video_attachment(entry)
        || is_pdf_attachment(entry)
        || is_text_like_attachment(entry)
        || is_known_document_attachment(entry)
}

fn attachment_within_native_payload_cap(entry: &AttachmentManifestEntry) -> bool {
    entry.size_bytes <= API_NATIVE_ATTACHMENT_MAX_FILE_BYTES
}

pub(crate) fn openai_api_input(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
) -> Result<Value, String> {
    let mut content = vec![json!({ "type": "input_text", "text": prompt })];
    for entry in attachments {
        if !attachment_within_native_payload_cap(entry) {
            continue;
        }
        if is_image_attachment(entry) {
            content.push(json!({
                "type": "input_image",
                "image_url": attachment_data_url(entry)?
            }));
        } else if openai_api_file_attachment_supported(entry) {
            content.push(json!({
                "type": "input_file",
                "filename": entry.file_name.as_str(),
                "file_data": attachment_data_url(entry)?
            }));
        }
    }
    Ok(json!([
        {
            "role": "user",
            "content": content
        }
    ]))
}

pub(crate) fn anthropic_api_user_content(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
) -> Result<Value, String> {
    let mut content = vec![json!({ "type": "text", "text": prompt })];
    for entry in attachments {
        if !attachment_within_native_payload_cap(entry) {
            continue;
        }
        if is_image_attachment(entry) {
            content.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": normalized_attachment_media_type(entry),
                    "data": attachment_base64(entry)?
                }
            }));
        } else if is_pdf_attachment(entry) {
            content.push(json!({
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": attachment_base64(entry)?
                },
                "title": entry.file_name.as_str()
            }));
        }
    }
    Ok(Value::Array(content))
}

pub(crate) fn gemini_api_user_parts(
    prompt: &str,
    attachments: &[AttachmentManifestEntry],
) -> Result<Vec<Value>, String> {
    let mut parts = vec![json!({ "text": prompt })];
    for entry in attachments {
        if gemini_api_attachment_supported(entry) {
            parts.push(json!({
                "inline_data": {
                    "mime_type": normalized_attachment_media_type(entry),
                    "data": attachment_base64(entry)?
                }
            }));
        }
    }
    Ok(parts)
}
