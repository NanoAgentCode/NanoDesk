use super::ocr::{image_mime_from_path, is_supported_ocr_image, run_paddle_ocr};
use crate::error::{AppError, AppResult};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OcrProvider {
    #[default]
    Local,
    External,
    Mineru,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OcrConfig {
    pub provider: OcrProvider,
    pub external_url: String,
    pub external_key: String,
    pub model: String,
    pub mineru_url: String,
    pub mineru_key: String,
}

impl OcrConfig {
    pub(crate) fn validate(&self) -> AppResult<()> {
        let endpoint = match self.provider {
            OcrProvider::Local => return Ok(()),
            OcrProvider::External => {
                if self.model.trim().is_empty() {
                    return Err("请填写 OCR 模型名称".into());
                }
                &self.external_url
            }
            OcrProvider::Mineru => &self.mineru_url,
        };
        let url = reqwest::Url::parse(endpoint.trim())
            .map_err(|_| AppError::from("请填写完整的 OCR HTTP 接口地址"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("OCR 地址必须是 HTTP/HTTPS 地址，凭据请填写在 API Key 中".into());
        }
        Ok(())
    }
}

pub(crate) async fn run_ocr(
    config: OcrConfig,
    project: &str,
    relative: &str,
    format: &str,
) -> AppResult<String> {
    config.validate()?;
    if config.provider == OcrProvider::Local {
        let (project, relative, format) =
            (project.to_owned(), relative.to_owned(), format.to_owned());
        return tokio::task::spawn_blocking(move || run_paddle_ocr(&project, &relative, &format))
            .await
            .map_err(|e| AppError::from(e.to_string()))?;
    }
    let root = crate::project_files::project_root(project)?;
    let path = crate::project_files::resolve_project_relative_path(&root, relative)?;
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() || !is_supported_ocr_image(&path) {
        return Err("OCR 只能处理项目内普通图片文件".into());
    }
    if metadata.len() > 8 * 1024 * 1024 {
        return Err("OCR 图片超过 8MB，请先压缩或裁剪后再识别".into());
    }
    let bytes = std::fs::read(&path)?;
    request_remote(
        &config,
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("image.png"),
        image_mime_from_path(&path),
        &bytes,
        format,
    )
    .await
}

async fn request_remote(
    config: &OcrConfig,
    filename: &str,
    mime: &str,
    bytes: &[u8],
    format: &str,
) -> AppResult<String> {
    request_remote_with_polling(
        config,
        filename,
        mime,
        bytes,
        format,
        60,
        Duration::from_secs(1),
    )
    .await
}

async fn request_remote_with_polling(
    config: &OcrConfig,
    filename: &str,
    mime: &str,
    bytes: &[u8],
    format: &str,
    max_polls: usize,
    poll_interval: Duration,
) -> AppResult<String> {
    config.validate()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(320))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let (endpoint, key) = match config.provider {
        OcrProvider::External => (&config.external_url, &config.external_key),
        OcrProvider::Mineru => (&config.mineru_url, &config.mineru_key),
        OcrProvider::Local => return Err("本地 OCR 不使用 HTTP 接口".into()),
    };
    let endpoint = endpoint.trim();
    let mut request = client.post(endpoint);
    if !key.trim().is_empty() {
        request = request.bearer_auth(key.trim());
    }
    if config.provider == OcrProvider::External {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        request = request.json(&json!({"model":config.model.trim(),"stream":false,"messages":[{"role":"user","content":[{"type":"text","text":"提取图片中全部文字，保留阅读顺序，仅返回识别内容，不要解释。"},{"type":"image_url","image_url":{"url":format!("data:{mime};base64,{encoded}")}}]}]}));
    } else {
        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name(filename.to_owned())
            .mime_str(mime)?;
        let form = reqwest::multipart::Form::new().part("files", part);
        let url = reqwest::Url::parse(endpoint).map_err(|e| AppError::from(e.to_string()))?;
        if url.path().trim_end_matches('/').ends_with("/v1/file_parse") {
            request = request
                .query(&[
                    ("response_format", "json"),
                    ("output_formats", "markdown"),
                    ("wait_timeout", "300"),
                ])
                .multipart(form);
        } else {
            request = request.multipart(
                form.text("return_md", "true")
                    .text("return_images", "false")
                    .text("response_format_zip", "false"),
            );
        }
    }
    let response = request.send().await?;
    let result = tokio::time::timeout(Duration::from_secs(320), async {
        let mut response = response;
        for attempt in 0..=max_polls {
            let status = response.status();
            if !status.is_success() {
                return Err(AppError::from(format!("OCR 服务请求失败：HTTP {status}")));
            }
            let value: Value = response.json().await?;
            if status != reqwest::StatusCode::ACCEPTED {
                let text = extract_text(&value, config.provider)?;
                return if format == "raw" {
                    Ok(serde_json::to_string_pretty(&value)?)
                } else {
                    Ok(text)
                };
            }
            if config.provider != OcrProvider::Mineru {
                return Err("OCR 服务返回了不支持的异步响应".into());
            }
            if attempt == max_polls {
                return Err("MinerU 任务等待超时，请稍后重试".into());
            }
            let id = value
                .get("task_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| AppError::from("MinerU 等待响应缺少 task_id"))?;
            let mut url =
                reqwest::Url::parse(endpoint).map_err(|e| AppError::from(e.to_string()))?;
            let prefix = url
                .path()
                .strip_suffix("/file_parse")
                .ok_or_else(|| AppError::from("MinerU 异步响应需要 /v1/file_parse 接口"))?
                .to_owned();
            url.set_query(None);
            url.set_path(&prefix);
            url.path_segments_mut()
                .map_err(|_| AppError::from("MinerU 地址无效"))?
                .push("tasks")
                .push(id)
                .push("result");
            url.set_query(Some("response_format=json"));
            tokio::time::sleep(poll_interval).await;
            let mut poll = client.get(url);
            if !key.trim().is_empty() {
                poll = poll.bearer_auth(key.trim());
            }
            response = poll.send().await?;
        }
        Err("MinerU 任务等待超时，请稍后重试".into())
    })
    .await
    .map_err(|_| AppError::from("OCR 任务等待超时"))?;
    result
}

fn extract_text(value: &Value, provider: OcrProvider) -> AppResult<String> {
    let mut texts = Vec::new();
    if provider == OcrProvider::External {
        if let Some(text) = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
        {
            texts.push(text.to_owned());
        }
    } else {
        if matches!(
            value.get("status").and_then(Value::as_str),
            Some("failed" | "canceled" | "partial")
        ) {
            return Err("MinerU 解析失败或结果不完整，请检查服务日志".into());
        }
        if let Some(results) = value.get("results").and_then(Value::as_object) {
            for result in results.values() {
                if let Some(text) = result.get("md_content").and_then(Value::as_str) {
                    texts.push(text.to_owned());
                }
            }
        }
        if let Some(files) = value.get("files").and_then(Value::as_array) {
            for file in files {
                if matches!(
                    file.get("status").and_then(Value::as_str),
                    Some("failed" | "canceled")
                ) {
                    return Err("MinerU 文件解析失败".into());
                }
                if let Some(text) = file.pointer("/content/markdown").and_then(Value::as_str) {
                    texts.push(text.to_owned());
                }
            }
        }
    }
    let text = texts.join("\n\n");
    if text.trim().is_empty() {
        return Err("OCR 服务未返回识别文字，请检查接口协议或图片内容".into());
    }
    Ok(text)
}

#[cfg(test)]
#[path = "remote_ocr_tests.rs"]
mod tests;
