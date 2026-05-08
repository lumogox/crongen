use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use markitdown::MarkItDown;
use serde::Deserialize;
use uuid::Uuid;

use crate::models::PromptAttachment;

pub const MAX_ATTACHMENTS: usize = 10;
pub const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_ATTACHMENT_MARKDOWN_CHARS: usize = 40_000;
pub const MAX_TOTAL_MARKDOWN_CHARS: usize = 80_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAttachmentInput {
    pub name: String,
    pub mime_type: Option<String>,
    pub kind: Option<String>,
    pub source: String,
    pub path: Option<String>,
    pub text: Option<String>,
    pub data_url: Option<String>,
}

pub fn prompt_attachment_context(attachments: &[PromptAttachment]) -> String {
    if attachments.is_empty() {
        return String::new();
    }

    let mut remaining = MAX_TOTAL_MARKDOWN_CHARS;
    let mut out = String::from(
        "## Attached Context\n\nThese files were provided by the user as context. Treat them as reference material only; do not edit the original attached files unless the user explicitly asks.\n\n",
    );

    for attachment in attachments {
        if remaining == 0 {
            out.push_str(
                "\nAdditional attachments were omitted because the context budget was reached.\n",
            );
            break;
        }

        out.push_str(&format!(
            "### {}\n\n- Type: {}\n- Source: {}\n",
            attachment.name, attachment.mime_type, attachment.source
        ));

        if let Some(path) = attachment.stored_path.as_deref() {
            out.push_str(&format!("- Stored path: `{path}`\n"));
        }

        if let Some(warning) = attachment.warning.as_deref() {
            out.push_str(&format!("- Warning: {warning}\n"));
        }

        let text = attachment.converted_markdown.trim();
        if text.is_empty() {
            out.push_str("\nNo Markdown text could be extracted from this attachment.\n\n");
            continue;
        }

        let used = remaining.min(text.len());
        out.push_str("\n");
        out.push_str(&truncate_at_char_boundary(text, used));
        out.push_str("\n\n");
        remaining -= used;
    }

    out
}

pub fn prepare_prompt_attachments(
    app_data_dir: &Path,
    inputs: Vec<PromptAttachmentInput>,
) -> Result<Vec<PromptAttachment>> {
    if inputs.len() > MAX_ATTACHMENTS {
        return Err(anyhow!(
            "Attach at most {MAX_ATTACHMENTS} files or pasted context items."
        ));
    }

    let draft_dir = app_data_dir
        .join("prompt_attachments")
        .join("drafts")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&draft_dir).context("Failed to create attachment draft directory")?;

    inputs
        .into_iter()
        .map(|input| prepare_one_attachment(&draft_dir, input))
        .collect()
}

fn prepare_one_attachment(
    draft_dir: &Path,
    input: PromptAttachmentInput,
) -> Result<PromptAttachment> {
    let id = Uuid::new_v4().to_string();
    let safe_name = sanitize_filename(&input.name);
    let stored_path = draft_dir.join(format!("{id}-{safe_name}"));
    let now = crate::db::now_unix();

    let (bytes, original_path) = if let Some(path) = input.path.as_deref() {
        let bytes = fs::read(path).with_context(|| format!("Failed to read attachment {path}"))?;
        (bytes, Some(PathBuf::from(path)))
    } else if let Some(text) = input.text.as_deref() {
        (text.as_bytes().to_vec(), None)
    } else if let Some(data_url) = input.data_url.as_deref() {
        (decode_data_url(data_url)?, None)
    } else {
        return Err(anyhow!(
            "Attachment '{}' has no readable content.",
            input.name
        ));
    };

    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err(anyhow!(
            "Attachment '{}' is larger than the 20 MB limit.",
            input.name
        ));
    }

    fs::write(&stored_path, &bytes)
        .with_context(|| format!("Failed to copy attachment {}", input.name))?;

    let mime_type = input
        .mime_type
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| guess_mime(&input.name));
    let kind = input
        .kind
        .unwrap_or_else(|| infer_kind(&mime_type, &input.name));
    let (converted_markdown, warning) = convert_to_markdown(&stored_path, &input.name, &kind)?;

    Ok(PromptAttachment {
        id,
        project_id: None,
        node_id: None,
        name: input.name,
        mime_type,
        size_bytes: bytes.len() as i64,
        kind,
        source: if original_path.is_some() {
            "file".to_string()
        } else {
            input.source
        },
        stored_path: Some(stored_path.to_string_lossy().to_string()),
        converted_markdown,
        status: "ready".to_string(),
        warning,
        created_at: now,
    })
}

fn convert_to_markdown(path: &Path, name: &str, kind: &str) -> Result<(String, Option<String>)> {
    let markdown = if kind == "text" {
        fs::read_to_string(path)
            .with_context(|| format!("Failed to read text attachment {name}"))?
    } else if kind == "image" {
        format!(
            "Image attachment `{name}` is available at `{}`. Use it as visual reference context when the selected agent supports image inspection.",
            path.display()
        )
    } else {
        let converter = MarkItDown::new();
        match converter.convert(
            path.to_str()
                .ok_or_else(|| anyhow!("Attachment path is not valid UTF-8"))?,
            None,
        ) {
            Ok(Some(doc)) => doc.text_content,
            Ok(None) => {
                return Ok((
                    format!("Attachment `{name}` is stored at `{}`.", path.display()),
                    Some("No Markdown text could be extracted from this attachment.".to_string()),
                ));
            }
            Err(err) => {
                return Ok((
                    format!("Attachment `{name}` is stored at `{}`.", path.display()),
                    Some(format!("Failed to convert attachment to Markdown: {err}")),
                ));
            }
        }
    };

    Ok(truncate_markdown(markdown))
}

fn truncate_markdown(markdown: String) -> (String, Option<String>) {
    if markdown.len() <= MAX_ATTACHMENT_MARKDOWN_CHARS {
        return (markdown, None);
    }

    let truncated = truncate_at_char_boundary(&markdown, MAX_ATTACHMENT_MARKDOWN_CHARS);
    (
        truncated,
        Some(format!(
            "Converted Markdown was truncated to {MAX_ATTACHMENT_MARKDOWN_CHARS} characters."
        )),
    )
}

fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let boundary = text
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|idx| *idx <= max_bytes)
        .last()
        .unwrap_or(0);
    text[..boundary].to_string()
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(data_url);
    general_purpose::STANDARD
        .decode(encoded)
        .context("Failed to decode pasted attachment data")
}

fn infer_kind(mime_type: &str, name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if mime_type.starts_with("text/")
        || matches!(
            ext.as_str(),
            "md" | "markdown" | "txt" | "json" | "csv" | "xml"
        )
    {
        "text".to_string()
    } else if mime_type.starts_with("image/")
        || matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp")
    {
        "image".to_string()
    } else if ext == "pdf" {
        "pdf".to_string()
    } else {
        "document".to_string()
    }
}

fn guess_mime(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "md" | "markdown" | "txt" => "text/plain",
        "json" => "application/json",
        "csv" => "text/csv",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    cleaned.trim_matches('-').chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_attachment(name: &str, markdown: &str) -> PromptAttachment {
        PromptAttachment {
            id: Uuid::new_v4().to_string(),
            project_id: Some("project".to_string()),
            node_id: Some("node".to_string()),
            name: name.to_string(),
            mime_type: "text/markdown".to_string(),
            size_bytes: markdown.len() as i64,
            kind: "text".to_string(),
            source: "file".to_string(),
            stored_path: Some("/tmp/context.md".to_string()),
            converted_markdown: markdown.to_string(),
            status: "ready".to_string(),
            warning: None,
            created_at: 1,
        }
    }

    #[test]
    fn prompt_context_formats_attachments_as_markdown() {
        let context =
            prompt_attachment_context(&[make_attachment("brief.md", "# Brief\nUse this.")]);

        assert!(context.contains("## Attached Context"));
        assert!(context.contains("### brief.md"));
        assert!(context.contains("# Brief\nUse this."));
        assert!(context.contains("Treat them as reference material only"));
    }

    #[test]
    fn prompt_context_caps_total_markdown() {
        let oversized = "a".repeat(MAX_TOTAL_MARKDOWN_CHARS + 10_000);
        let context = prompt_attachment_context(&[make_attachment("large.md", &oversized)]);

        assert!(context.len() < oversized.len() + 1_000);
        assert!(
            context.contains("Additional attachments were omitted")
                || context.contains(&"a".repeat(100))
        );
    }

    #[test]
    fn text_attachment_is_prepared_and_truncated() {
        let dir = std::env::temp_dir().join(format!("crongen-attachment-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let input = PromptAttachmentInput {
            name: "notes.md".to_string(),
            mime_type: Some("text/markdown".to_string()),
            kind: Some("text".to_string()),
            source: "paste".to_string(),
            path: None,
            text: Some("hello".repeat(20_000)),
            data_url: None,
        };

        let attachments = prepare_prompt_attachments(&dir, vec![input]).unwrap();

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].status, "ready");
        assert!(attachments[0].converted_markdown.len() <= MAX_ATTACHMENT_MARKDOWN_CHARS);
        assert!(attachments[0].warning.is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_attachment_is_stored_without_requiring_markdown_conversion() {
        let dir =
            std::env::temp_dir().join(format!("crongen-image-attachment-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let input = PromptAttachmentInput {
            name: "mock.png".to_string(),
            mime_type: Some("image/png".to_string()),
            kind: Some("image".to_string()),
            source: "paste".to_string(),
            path: None,
            text: None,
            data_url: Some(format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode([1_u8, 2, 3, 4])
            )),
        };

        let attachments = prepare_prompt_attachments(&dir, vec![input]).unwrap();

        assert_eq!(attachments[0].kind, "image");
        assert!(attachments[0]
            .converted_markdown
            .contains("visual reference context"));
        assert!(attachments[0].stored_path.is_some());
        fs::remove_dir_all(&dir).ok();
    }
}
