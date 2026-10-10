use crate::brand;
use crate::error::AppResult;
use crate::models::ChatImageAttachment;
use crate::models::ChatImageAttachmentPreview;
use crate::models::ChatImageAttachmentRequest;
use crate::project_files::normalize_relative_path;
use crate::project_files::project_root;
use crate::project_files::resolve_project_relative_path;
use crate::project_files::sanitize_attachment_file_name;
use crate::services::ocr::image_mime_from_path;
use crate::services::ocr::is_supported_ocr_image;
use base64::Engine as _;
use chrono::Utc;

#[tauri::command]
pub(crate) async fn save_chat_image_attachment(
    request: ChatImageAttachmentRequest,
) -> AppResult<ChatImageAttachment> {
    const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;

    let root = project_root(&request.project_path)?;
    let safe_name = sanitize_attachment_file_name(&request.file_name)?;
    let relative_path = format!(
        "{}/{}-{}-{}",
        brand::IMAGE_UPLOADS_DIRECTORY,
        Utc::now().format("%Y%m%d%H%M%S%3f"),
        uuid::Uuid::new_v4(),
        safe_name
    );
    let target_path = resolve_project_relative_path(&root, &relative_path)?;

    if !is_supported_ocr_image(std::path::Path::new(&safe_name)) {
        return Err(crate::error::AppError::Message(
            "OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff".to_string(),
        ));
    }

    let bytes = if let Some(source_path) = request
        .source_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let source = std::path::PathBuf::from(source_path);
        let metadata = std::fs::metadata(&source).map_err(|err| {
            crate::error::AppError::Message(format!("读取图片文件信息失败: {err}"))
        })?;
        if !metadata.is_file() {
            return Err(crate::error::AppError::Message(
                "只能上传普通图片文件".to_string(),
            ));
        }
        if metadata.len() > MAX_IMAGE_BYTES as u64 {
            return Err(crate::error::AppError::Message(
                "图片超过 25MB，请先压缩或裁剪后再上传".to_string(),
            ));
        }
        if !is_supported_ocr_image(&source) {
            return Err(crate::error::AppError::Message(
                "OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff".to_string(),
            ));
        }
        std::fs::read(&source)
            .map_err(|err| crate::error::AppError::Message(format!("读取图片失败: {err}")))?
    } else {
        let content_base64 = request
            .content_base64
            .as_deref()
            .ok_or_else(|| crate::error::AppError::Message("图片内容不能为空".to_string()))?;
        let data = content_base64
            .split_once(',')
            .map(|(_, data)| data)
            .unwrap_or(content_base64);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|err| crate::error::AppError::Message(format!("解析图片失败: {err}")))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(crate::error::AppError::Message(
                "图片超过 25MB，请先压缩或裁剪后再上传".to_string(),
            ));
        }
        bytes
    };

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| crate::error::AppError::Message(format!("创建图片目录失败: {err}")))?;
    }
    std::fs::write(&target_path, &bytes)
        .map_err(|err| crate::error::AppError::Message(format!("保存图片失败: {err}")))?;

    Ok(ChatImageAttachment {
        name: safe_name,
        relative_path,
        size: bytes.len() as u64,
    })
}
#[tauri::command]
pub(crate) async fn read_chat_image_attachment(
    project_path: String,
    relative_path: String,
) -> AppResult<ChatImageAttachmentPreview> {
    const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;

    let normalized = normalize_relative_path(&relative_path)?;
    if !normalized.starts_with(&format!("{}/", brand::IMAGE_UPLOADS_DIRECTORY)) {
        return Err(crate::error::AppError::Message(
            "只能预览对话图片附件".to_string(),
        ));
    }

    let root = project_root(&project_path)?;
    let file_path = resolve_project_relative_path(&root, &normalized)?;
    if !is_supported_ocr_image(&file_path) {
        return Err(crate::error::AppError::Message(
            "OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff".to_string(),
        ));
    }
    let metadata = std::fs::metadata(&file_path)
        .map_err(|err| crate::error::AppError::Message(format!("读取图片文件信息失败: {err}")))?;
    if !metadata.is_file() {
        return Err(crate::error::AppError::Message(
            "只能预览普通图片文件".to_string(),
        ));
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(crate::error::AppError::Message(
            "图片超过 25MB，无法预览".to_string(),
        ));
    }

    let bytes = std::fs::read(&file_path)
        .map_err(|err| crate::error::AppError::Message(format!("读取图片失败: {err}")))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(ChatImageAttachmentPreview {
        relative_path: normalized,
        absolute_path: file_path.to_string_lossy().to_string(),
        data_url: format!(
            "data:{};base64,{}",
            image_mime_from_path(&file_path),
            encoded
        ),
    })
}
