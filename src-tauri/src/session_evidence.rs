use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    checked_data_child_path, is_public_http_url, sanitize_path_segment, sanitize_text,
    write_text_file, SessionContract,
};

const ATTACHMENT_MAX_FILES: usize = 8;
const ATTACHMENT_MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
const ATTACHMENT_MAX_TOTAL_BYTES: u64 = 75 * 1024 * 1024;
const ATTACHMENT_MAX_INLINE_PREVIEW_BYTES: usize = 128 * 1024;
const ATTACHMENT_MAX_TOTAL_INLINE_PREVIEW_BYTES: usize = 512 * 1024;

#[derive(Clone, Deserialize)]
pub(crate) struct PromptAttachmentRequest {
    name: String,
    media_type: Option<String>,
    size_bytes: Option<u64>,
    data_base64: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AttachmentManifestEntry {
    pub(crate) original_name: String,
    pub(crate) file_name: String,
    pub(crate) media_type: String,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: String,
    pub(crate) path: String,
    pub(crate) inline_preview_chars: usize,
    pub(crate) inline_preview_truncated: bool,
}

pub(crate) struct SessionEvidence {
    pub(crate) links: Vec<String>,
    pub(crate) attachments: Vec<AttachmentManifestEntry>,
    pub(crate) block: String,
    pub(crate) links_path: Option<PathBuf>,
    pub(crate) attachments_manifest_path: Option<PathBuf>,
}

pub(crate) fn process_session_evidence(
    session_dir: &Path,
    links: Option<&Vec<String>>,
    attachments: Option<&Vec<PromptAttachmentRequest>>,
    saved: Option<&SessionContract>,
) -> Result<SessionEvidence, String> {
    let normalized_links = if let Some(links) = links {
        normalize_session_links(links)?
    } else {
        saved
            .map(|contract| contract.links.clone())
            .unwrap_or_default()
    };
    let attachment_entries = if let Some(attachments) = attachments {
        persist_session_attachments(session_dir, attachments)?
    } else {
        saved
            .map(|contract| contract.attachments.clone())
            .unwrap_or_default()
    };

    let links_path = if normalized_links.is_empty() {
        None
    } else {
        let json_path = session_dir.join("links.json");
        write_text_file(
            &json_path,
            &serde_json::to_string_pretty(&normalized_links)
                .map_err(|error| format!("failed to serialize links: {error}"))?,
        )?;
        let md_path = session_dir.join("links.md");
        let mut md = "# Links da Sessao\n\n".to_string();
        for link in &normalized_links {
            md.push_str(&format!("- <{}>\n", link));
        }
        write_text_file(&md_path, &md)?;
        Some(json_path)
    };

    let attachments_manifest_path = if attachment_entries.is_empty() {
        None
    } else {
        let path = session_dir.join("attachments").join("manifest.json");
        write_text_file(
            &path,
            &serde_json::to_string_pretty(&attachment_entries)
                .map_err(|error| format!("failed to serialize attachment manifest: {error}"))?,
        )?;
        Some(path)
    };

    let block = build_evidence_prompt_block(&normalized_links, &attachment_entries, session_dir)?;
    Ok(SessionEvidence {
        links: normalized_links,
        attachments: attachment_entries,
        block,
        links_path,
        attachments_manifest_path,
    })
}

pub(crate) fn normalize_session_links(values: &[String]) -> Result<Vec<String>, String> {
    let mut links = BTreeSet::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !is_public_http_url(trimmed) {
            return Err(format!(
                "link rejeitado para anexos da sessao: {}",
                sanitize_text(trimmed, 160)
            ));
        }
        let parsed = Url::parse(trimmed).map_err(|error| format!("link invalido: {error}"))?;
        links.insert(parsed.to_string());
    }
    Ok(links.into_iter().collect())
}

pub(crate) fn persist_session_attachments(
    session_dir: &Path,
    attachments: &[PromptAttachmentRequest],
) -> Result<Vec<AttachmentManifestEntry>, String> {
    if attachments.len() > ATTACHMENT_MAX_FILES {
        return Err(format!(
            "limite de anexos excedido: maximo de {} arquivos",
            ATTACHMENT_MAX_FILES
        ));
    }
    let attachment_dir = checked_data_child_path(&session_dir.join("attachments"))?;
    fs::create_dir_all(&attachment_dir)
        .map_err(|error| format!("failed to create attachment dir: {error}"))?;

    let mut total_bytes = 0u64;
    let mut entries = Vec::new();
    for (index, item) in attachments.iter().enumerate() {
        let original_name = sanitize_text(&item.name, 240);
        let data = BASE64_STANDARD
            .decode(item.data_base64.trim())
            .map_err(|error| format!("anexo {} nao esta em base64 valido: {error}", index + 1))?;
        let size_bytes = data.len() as u64;
        if let Some(declared) = item.size_bytes {
            if declared != size_bytes {
                return Err(format!(
                    "tamanho declarado do anexo {} diverge do payload recebido",
                    original_name
                ));
            }
        }
        if size_bytes > ATTACHMENT_MAX_FILE_BYTES {
            return Err(format!(
                "anexo {} excede o limite de {} MiB",
                original_name,
                ATTACHMENT_MAX_FILE_BYTES / 1024 / 1024
            ));
        }
        total_bytes += size_bytes;
        if total_bytes > ATTACHMENT_MAX_TOTAL_BYTES {
            return Err(format!(
                "anexos excedem o limite total de {} MiB",
                ATTACHMENT_MAX_TOTAL_BYTES / 1024 / 1024
            ));
        }

        let file_name = attachment_file_name(index, &original_name);
        let path = attachment_dir.join(&file_name);
        fs::write(&path, &data).map_err(|error| format!("failed to write attachment: {error}"))?;
        let sha256 = format!("{:x}", Sha256::digest(&data));
        let media_type = item
            .media_type
            .as_deref()
            .map(|value| sanitize_text(value, 120))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let mut entry = AttachmentManifestEntry {
            original_name,
            file_name,
            media_type,
            size_bytes,
            sha256,
            path: path.to_string_lossy().to_string(),
            inline_preview_chars: 0,
            inline_preview_truncated: false,
        };
        if is_text_like_attachment(&entry) {
            let preview_bytes = data.len().min(ATTACHMENT_MAX_INLINE_PREVIEW_BYTES);
            entry.inline_preview_chars = String::from_utf8_lossy(&data[..preview_bytes])
                .chars()
                .count();
            entry.inline_preview_truncated = data.len() > preview_bytes;
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn attachment_file_name(index: usize, original_name: &str) -> String {
    let path = Path::new(original_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| sanitize_path_segment(value, 80))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "anexo".to_string());
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| sanitize_path_segment(value, 16))
        .filter(|value| !value.is_empty());
    match extension {
        Some(extension) => format!("{:02}-{}.{}", index + 1, stem, extension),
        None => format!("{:02}-{}", index + 1, stem),
    }
}

fn build_evidence_prompt_block(
    links: &[String],
    attachments: &[AttachmentManifestEntry],
    session_dir: &Path,
) -> Result<String, String> {
    if links.is_empty() && attachments.is_empty() {
        return Ok(String::new());
    }
    let mut block = String::from("\n## Fontes, links e anexos fornecidos pelo operador\n\n");
    if !links.is_empty() {
        block.push_str("### Links\n\n");
        for link in links {
            block.push_str(&format!("- <{}>\n", link));
        }
        block.push('\n');
    }
    if !attachments.is_empty() {
        block.push_str("### Anexos\n\n");
        block.push_str("Os anexos ficam no disco local da sessao. Peers CLI recebem caminhos locais, manifesto e previews textuais limitados. Peers API recebem anexos nativos quando o provedor aceita o tipo de arquivo; tipos sem suporte nativo permanecem disponiveis por manifesto e preview textual quando aplicavel.\n\n");
        let mut preview_budget = ATTACHMENT_MAX_TOTAL_INLINE_PREVIEW_BYTES;
        for entry in attachments {
            block.push_str(&format!(
                "- `{}`: `{}`; tipo `{}`; {} bytes; sha256 `{}`; caminho `{}`\n",
                entry.original_name,
                entry.file_name,
                entry.media_type,
                entry.size_bytes,
                entry.sha256,
                entry.path
            ));
            if preview_budget > 0 && is_text_like_attachment(entry) {
                let path = Path::new(&entry.path);
                let data = fs::read(checked_data_child_path(path)?).unwrap_or_default();
                let take = data
                    .len()
                    .min(ATTACHMENT_MAX_INLINE_PREVIEW_BYTES)
                    .min(preview_budget);
                let preview = String::from_utf8_lossy(&data[..take]).to_string();
                if !preview.trim().is_empty() {
                    preview_budget = preview_budget.saturating_sub(take);
                    block.push_str("\nPreview limitado:\n\n```text\n");
                    block.push_str(&sanitize_text(
                        &preview,
                        ATTACHMENT_MAX_INLINE_PREVIEW_BYTES,
                    ));
                    if data.len() > take {
                        block.push_str("\n[PREVIEW_TRUNCADO]\n");
                    }
                    block.push_str("```\n\n");
                }
            }
        }
        block.push_str(&format!(
            "\nManifesto de anexos: `{}`\n",
            session_dir
                .join("attachments")
                .join("manifest.json")
                .to_string_lossy()
        ));
    }
    Ok(block)
}

pub(crate) fn read_attachment_bytes(entry: &AttachmentManifestEntry) -> Result<Vec<u8>, String> {
    let path = Path::new(&entry.path);
    fs::read(checked_data_child_path(path)?)
        .map_err(|error| format!("failed to read attachment {}: {error}", entry.file_name))
}

pub(crate) fn attachment_base64(entry: &AttachmentManifestEntry) -> Result<String, String> {
    Ok(BASE64_STANDARD.encode(read_attachment_bytes(entry)?))
}

pub(crate) fn attachment_data_url(entry: &AttachmentManifestEntry) -> Result<String, String> {
    Ok(format!(
        "data:{};base64,{}",
        normalized_attachment_media_type(entry),
        attachment_base64(entry)?
    ))
}

pub(crate) fn normalized_attachment_media_type(entry: &AttachmentManifestEntry) -> String {
    let media = entry.media_type.trim().to_ascii_lowercase();
    if media == "image/jpg" {
        "image/jpeg".to_string()
    } else if media.is_empty() {
        "application/octet-stream".to_string()
    } else {
        media
    }
}

pub(crate) fn is_text_like_attachment(entry: &AttachmentManifestEntry) -> bool {
    let media = entry.media_type.to_ascii_lowercase();
    if media.starts_with("text/")
        || media.contains("json")
        || media.contains("xml")
        || media.contains("markdown")
        || media.contains("csv")
        || media.contains("yaml")
    {
        return true;
    }
    let extension = Path::new(&entry.file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "txt"
            | "md"
            | "markdown"
            | "json"
            | "csv"
            | "tsv"
            | "html"
            | "htm"
            | "xml"
            | "yaml"
            | "yml"
            | "log"
    )
}

pub(crate) fn is_pdf_attachment(entry: &AttachmentManifestEntry) -> bool {
    normalized_attachment_media_type(entry) == "application/pdf"
        || attachment_extension(entry) == "pdf"
}

pub(crate) fn is_image_attachment(entry: &AttachmentManifestEntry) -> bool {
    matches!(
        normalized_attachment_media_type(entry).as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    )
}

pub(crate) fn is_audio_attachment(entry: &AttachmentManifestEntry) -> bool {
    normalized_attachment_media_type(entry).starts_with("audio/")
}

pub(crate) fn is_video_attachment(entry: &AttachmentManifestEntry) -> bool {
    normalized_attachment_media_type(entry).starts_with("video/")
}

pub(crate) fn is_known_document_attachment(entry: &AttachmentManifestEntry) -> bool {
    if is_text_like_attachment(entry) || is_pdf_attachment(entry) {
        return true;
    }
    // Conservative document allow-list for providers that support file/document inputs.
    let media = normalized_attachment_media_type(entry);
    if matches!(
        media.as_str(),
        "application/msword"
            | "application/rtf"
            | "application/vnd.ms-excel"
            | "application/vnd.ms-powerpoint"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            | "application/vnd.oasis.opendocument.text"
            | "application/vnd.oasis.opendocument.spreadsheet"
            | "application/vnd.oasis.opendocument.presentation"
    ) {
        return true;
    }
    matches!(
        attachment_extension(entry).as_str(),
        "doc" | "docx" | "rtf" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
    )
}

pub(crate) fn attachment_payload_base64_chars(entry: &AttachmentManifestEntry) -> usize {
    ((entry.size_bytes as usize + 2) / 3) * 4
}

fn attachment_extension(entry: &AttachmentManifestEntry) -> String {
    Path::new(&entry.file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions_dir;

    #[test]
    fn session_links_reject_local_targets() {
        let links = normalize_session_links(&[
            "https://example.com/a".to_string(),
            "https://example.com/a".to_string(),
        ])
        .unwrap();
        assert_eq!(links, vec!["https://example.com/a".to_string()]);
        assert!(normalize_session_links(&["http://localhost:8787/x".to_string()]).is_err());
        assert!(normalize_session_links(&["file:///C:/secret.txt".to_string()]).is_err());
    }

    #[test]
    fn attachment_count_cap_is_enforced_before_writing_payloads() {
        let session_dir = sessions_dir().join("run-attachment-cap-test");
        let attachments = (0..=ATTACHMENT_MAX_FILES)
            .map(|index| PromptAttachmentRequest {
                name: format!("file-{index}.txt"),
                media_type: Some("text/plain".to_string()),
                size_bytes: Some(1),
                data_base64: BASE64_STANDARD.encode("x"),
            })
            .collect::<Vec<_>>();
        let result = persist_session_attachments(&session_dir, &attachments);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn session_evidence_persists_links_manifest_and_prompt_block() {
        let session_dir = sessions_dir().join("run-evidence-module-test");
        let _ = fs::remove_dir_all(&session_dir);
        fs::create_dir_all(&session_dir).unwrap();
        let attachments = vec![PromptAttachmentRequest {
            name: "notes.md".to_string(),
            media_type: Some("text/markdown".to_string()),
            size_bytes: Some(14),
            data_base64: BASE64_STANDARD.encode("hello evidence"),
        }];
        let evidence = process_session_evidence(
            &session_dir,
            Some(&vec!["https://example.com/source".to_string()]),
            Some(&attachments),
            None,
        )
        .unwrap();

        assert_eq!(
            evidence.links,
            vec!["https://example.com/source".to_string()]
        );
        assert!(evidence.links_path.unwrap().ends_with("links.json"));
        assert!(evidence
            .attachments_manifest_path
            .unwrap()
            .ends_with(Path::new("attachments").join("manifest.json")));
        assert!(evidence.block.contains("https://example.com/source"));
        assert!(evidence.block.contains("notes.md"));
        assert!(evidence.block.contains("hello evidence"));
        assert!(session_dir.join("links.md").exists());
        assert!(session_dir
            .join("attachments")
            .join("manifest.json")
            .exists());
        let _ = fs::remove_dir_all(&session_dir);
    }
}
