use std::io::Read;

use base64::Engine as _;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::{finish_observation, start_observation, AppState, ObservationStart};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AbsoluteFileContent {
    name: String,
    size: u64,
    content: String,
}

#[derive(serde::Deserialize)]
pub struct UploadedFileExtractionRequest {
    name: String,
    content_base64: String,
}

#[tauri::command]
pub async fn read_absolute_file(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<AbsoluteFileContent> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "read_absolute_file",
            category: "tool",
            entity_type: Some("file"),
            entity_id: Some(path.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: None,
        },
    )
    .await;
    let result = (|| -> AppResult<AbsoluteFileContent> {
        const MAX_TEXT_FILE_BYTES: u64 = 10 * 1024 * 1024;

        let target_path = std::path::Path::new(&path);
        let metadata = std::fs::metadata(target_path)?;
        if !metadata.is_file() {
            return Err(AppError::Message("只能读取普通文件".to_string()));
        }
        if metadata.len() > MAX_TEXT_FILE_BYTES {
            return Err(AppError::Message("文件超过 10MB 限制".to_string()));
        }

        let name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let size = metadata.len();
        let content = extract_text_from_file(&path)?;

        Ok(AbsoluteFileContent {
            name,
            size,
            content,
        })
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|res| format!("content_chars={}", res.content.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
pub async fn extract_uploaded_file(
    state: State<'_, AppState>,
    request: UploadedFileExtractionRequest,
) -> AppResult<AbsoluteFileContent> {
    const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "extract_uploaded_file",
            category: "tool",
            entity_type: Some("file"),
            entity_id: Some(request.name.clone()),
            input_summary: Some(format!("encoded_chars={}", request.content_base64.len())),
            metadata: serde_json::json!({}),
            trace_id: None,
        },
    )
    .await;
    let result = extract_uploaded_file_content(request, MAX_FILE_BYTES);
    let summary = result
        .as_ref()
        .ok()
        .map(|file| format!("content_chars={}", file.content.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

fn extract_uploaded_file_content(
    request: UploadedFileExtractionRequest,
    max_file_bytes: usize,
) -> AppResult<AbsoluteFileContent> {
    let encoded = request
        .content_base64
        .split_once(',')
        .map(|(_, payload)| payload)
        .unwrap_or(request.content_base64.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| AppError::Message(format!("文件内容不是有效的 base64: {error}")))?;
    if bytes.len() > max_file_bytes {
        return Err(AppError::Message("文件超过 10MB 限制".to_string()));
    }

    let safe_name = crate::project_files::sanitize_attachment_file_name(&request.name)?;
    let temp_dir = std::env::temp_dir().join("nanodesk-rag-extraction");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_path = temp_dir.join(format!("{}-{safe_name}", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, &bytes)?;
    let extracted = extract_text_from_file(temp_path.to_string_lossy().as_ref());
    if let Err(error) = std::fs::remove_file(&temp_path) {
        crate::logging::warn(
            "rag-extraction",
            "failed to remove temporary uploaded file",
            serde_json::json!({ "error": error.to_string() }),
        );
    }
    extracted.map(|content| AbsoluteFileContent {
        name: request.name,
        size: bytes.len() as u64,
        content,
    })
}

fn extract_text_from_file(path: &str) -> AppResult<String> {
    let path_buf = std::path::Path::new(path);
    let extension = path_buf
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "doc" => {
            let data = std::fs::read(path)?;
            Ok(extract_doc_binary_text(&data))
        }
        "pdf" => {
            let doc = pdf_oxide::PdfDocument::open(path)
                .map_err(|err| AppError::Message(err.to_string()))?;
            let mut text = String::new();
            let num_pages = doc
                .page_count()
                .map_err(|err| AppError::Message(err.to_string()))?;
            for index in 0..num_pages {
                if let Ok(page_text) = doc.extract_text(index) {
                    text.push_str(&page_text);
                    text.push('\n');
                }
            }
            Ok(text)
        }
        "docx" => {
            let file = std::fs::File::open(path)?;
            let mut archive =
                zip::ZipArchive::new(file).map_err(|err| AppError::Message(err.to_string()))?;
            let mut doc_file = archive
                .by_name("word/document.xml")
                .map_err(|err| AppError::Message(err.to_string()))?;
            let mut xml_content = String::new();
            doc_file.read_to_string(&mut xml_content)?;

            Ok(extract_xml_text(&xml_content, "w:t"))
        }
        "pptx" => {
            let file = std::fs::File::open(path)?;
            let mut archive =
                zip::ZipArchive::new(file).map_err(|err| AppError::Message(err.to_string()))?;
            let mut text = String::new();

            let mut slide_names = Vec::new();
            for index in 0..archive.len() {
                if let Ok(archive_file) = archive.by_index(index) {
                    let name = archive_file.name();
                    if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                        slide_names.push(name.to_string());
                    }
                }
            }
            slide_names.sort_by_key(|name| {
                name.strip_prefix("ppt/slides/slide")
                    .and_then(|value| value.strip_suffix(".xml"))
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(0)
            });

            for name in slide_names {
                if let Ok(mut slide_file) = archive.by_name(&name) {
                    let mut xml_content = String::new();
                    if slide_file.read_to_string(&mut xml_content).is_ok() {
                        text.push_str(&extract_xml_text(&xml_content, "a:t"));
                        text.push(' ');
                    }
                }
            }
            Ok(text.trim().to_string())
        }
        "xlsx" => extract_xlsx_as_markdown(path),
        _ => Ok(std::fs::read_to_string(path)?),
    }
}

fn extract_xml_text(xml_content: &str, tag_name: &str) -> String {
    let open_prefix = format!("<{tag_name}");
    let close_tag = format!("</{tag_name}>");
    let mut text = String::new();
    let mut pos = 0;

    while let Some(start) = xml_content[pos..].find(&open_prefix) {
        let absolute_start = pos + start;
        let Some(close_tag_end) = xml_content[absolute_start..].find('>') else {
            break;
        };
        let text_start = absolute_start + close_tag_end + 1;
        let Some(end) = xml_content[text_start..].find(&close_tag) else {
            break;
        };
        let absolute_end = text_start + end;
        text.push_str(&xml_content[text_start..absolute_end]);
        text.push(' ');
        pos = absolute_end + close_tag.len();
    }

    text.trim().to_string()
}

fn extract_xlsx_as_markdown(path: &str) -> AppResult<String> {
    use calamine::{Data, Reader};

    let mut excel =
        calamine::open_workbook_auto(path).map_err(|err| AppError::Message(err.to_string()))?;
    let mut markdown = String::new();

    for sheet_name in excel.sheet_names().to_owned() {
        if let Ok(range) = excel.worksheet_range(&sheet_name) {
            markdown.push_str(&format!("## Sheet: {}\n\n", sheet_name));

            for (row_idx, row) in range.rows().enumerate() {
                markdown.push('|');
                for cell in row {
                    let val = match cell {
                        Data::Empty => "".to_string(),
                        Data::String(value) => value.clone(),
                        Data::Int(value) => value.to_string(),
                        Data::Float(value) => value.to_string(),
                        Data::Bool(value) => value.to_string(),
                        Data::Error(value) => format!("Error({:?})", value),
                        Data::DateTime(value) => value.to_string(),
                        _ => format!("{:?}", cell),
                    };
                    let escaped = val.replace('|', "\\|");
                    markdown.push_str(&format!(" {} |", escaped));
                }
                markdown.push('\n');

                if row_idx == 0 {
                    markdown.push('|');
                    for _ in row {
                        markdown.push_str(" --- |");
                    }
                    markdown.push('\n');
                }
            }
            markdown.push('\n');
        }
    }
    Ok(markdown.trim().to_string())
}

fn extract_doc_binary_text(data: &[u8]) -> String {
    let mut text = String::new();
    let mut i = 0;

    while i < data.len() {
        let mut utf16_chars = Vec::new();
        let mut j = i;
        while j + 1 < data.len() {
            let value = u16::from_le_bytes([data[j], data[j + 1]]);
            if (0x20..=0x7E).contains(&value)
                || value == 0x0A
                || value == 0x0D
                || value == 0x09
                || (0x4E00..=0x9FFF).contains(&value)
            {
                utf16_chars.push(value);
                j += 2;
            } else {
                break;
            }
        }
        if utf16_chars.len() >= 4 {
            if let Ok(value) = String::from_utf16(&utf16_chars) {
                text.push_str(&value);
                text.push(' ');
                i = j;
                continue;
            }
        }

        let mut ascii_chars = Vec::new();
        let mut j = i;
        while j < data.len() {
            let value = data[j];
            if (0x20..=0x7E).contains(&value) || value == 0x0A || value == 0x0D || value == 0x09 {
                ascii_chars.push(value);
                j += 1;
            } else {
                break;
            }
        }
        if ascii_chars.len() >= 4 {
            if let Ok(value) = String::from_utf8(ascii_chars) {
                text.push_str(&value);
                text.push(' ');
                i = j;
                continue;
            }
        }

        i += 1;
    }

    normalize_spacing(&text)
}

fn normalize_spacing(text: &str) -> String {
    let mut cleaned = String::new();
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
                prev_space = true;
            }
        } else {
            cleaned.push(ch);
            prev_space = false;
        }
    }
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn uploaded_docx_uses_the_document_extractor() {
        let cursor = Cursor::new(Vec::new());
        let mut archive = zip::ZipWriter::new(cursor);
        archive
            .start_file("word/document.xml", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(br#"<w:document><w:body><w:p><w:r><w:t>NanoDesk document</w:t></w:r></w:p></w:body></w:document>"#)
            .unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let content_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let extracted = extract_uploaded_file_content(
            UploadedFileExtractionRequest {
                name: "example.docx".to_string(),
                content_base64,
            },
            10 * 1024 * 1024,
        )
        .unwrap();

        assert_eq!(extracted.name, "example.docx");
        assert_eq!(extracted.size, bytes.len() as u64);
        assert!(extracted.content.contains("NanoDesk document"));
    }

    #[test]
    fn uploaded_file_limit_is_enforced_after_base64_decoding() {
        let error = extract_uploaded_file_content(
            UploadedFileExtractionRequest {
                name: "large.txt".to_string(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"too large"),
            },
            3,
        )
        .unwrap_err();
        assert!(error.to_string().contains("10MB"));
    }
}
