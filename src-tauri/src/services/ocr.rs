use crate::brand;
use crate::error::AppResult;
use crate::project_files::project_root;
use crate::project_files::resolve_project_relative_path;
use crate::services::environment::find_paddleocr_binary;
use std::io::Read;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::thread;
use std::time::Duration;

pub(crate) fn is_supported_ocr_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn run_paddle_ocr(
    project_path: &str,
    relative_path: &str,
    output_format: &str,
) -> AppResult<String> {
    const MAX_OCR_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
    const OCR_TIMEOUT: Duration = Duration::from_secs(90);

    let root = project_root(project_path)?;
    let target_path = resolve_project_relative_path(&root, relative_path)?;
    let metadata = std::fs::metadata(&target_path)?;
    if !metadata.is_file() {
        return Err(crate::error::AppError::Message(
            "OCR 只能处理项目内普通图片文件".to_string(),
        ));
    }
    if metadata.len() > MAX_OCR_IMAGE_BYTES {
        return Err(crate::error::AppError::Message(
            "OCR 图片超过 8MB，请先压缩或裁剪后再识别".to_string(),
        ));
    }
    if !is_supported_ocr_image(&target_path) {
        return Err(crate::error::AppError::Message(
            "OCR 仅支持 png、jpg、jpeg、bmp、webp、tif、tiff 图片".to_string(),
        ));
    }

    let paddleocr_bin = find_paddleocr_binary(None).ok_or_else(|| {
        crate::error::AppError::Message(
            "未检测到 PaddleOCR CLI。请在环境页安装 OCR，或将 paddleocr.exe 加入 PATH，也可以设置 NANODESK_PADDLEOCR_BIN。".to_string(),
        )
    })?;
    let paddle_cache_dir = root
        .join(brand::PROJECT_DATA_DIRECTORY)
        .join("paddlex-cache");
    std::fs::create_dir_all(&paddle_cache_dir)?;

    let target_path_arg = target_path.to_string_lossy().to_string();
    let mut command = std::process::Command::new(&paddleocr_bin);
    command.args([
        "ocr",
        "-i",
        target_path_arg.as_str(),
        "--device",
        "cpu",
        "--text_detection_model_name",
        "PP-OCRv6_small_det",
        "--text_recognition_model_name",
        "PP-OCRv6_small_rec",
        "--use_doc_orientation_classify",
        "False",
        "--use_doc_unwarping",
        "False",
        "--use_textline_orientation",
        "False",
        "--text_det_limit_side_len",
        "960",
        "--text_det_limit_type",
        "max",
        "--text_recognition_batch_size",
        "1",
        "--cpu_threads",
        "2",
        "--enable_mkldnn",
        "False",
        "--mkldnn_cache_capacity",
        "1",
        "--enable_hpi",
        "False",
        "--enable_cinn",
        "False",
    ]);
    command.env("PADDLE_PDX_CACHE_HOME", paddle_cache_dir);
    command.env("OMP_NUM_THREADS", "2");
    command.env("MKL_NUM_THREADS", "2");
    command.env("OPENBLAS_NUM_THREADS", "2");
    command.env("NUMEXPR_NUM_THREADS", "2");
    command.env("KMP_BLOCKTIME", "0");
    command.env("FLAGS_allocator_strategy", "auto_growth");
    command.env("FLAGS_use_mkldnn", "0");
    // Paddle/PaddleX on some Windows + Python 3.12 setups can fail in the PIR
    // predictor path with ConvertPirAttribute2RuntimeAttribute. Keep OCR on the
    // legacy inference path unless the user overrides it in the process env.
    if std::env::var_os("FLAGS_enable_pir_api").is_none() {
        command.env("FLAGS_enable_pir_api", "0");
    }
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    let output = run_paddleocr_with_timeout(&mut command, OCR_TIMEOUT)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (false, false) => format!("{}\n{}", stdout.trim(), stderr.trim()),
        (false, true) => stdout.trim().to_string(),
        (true, false) => stderr.trim().to_string(),
        (true, true) => String::new(),
    };

    if !output.status.success() {
        return Err(crate::error::AppError::Message(format!(
            "PaddleOCR 执行失败，退出码 {:?}\n{}",
            output.status.code(),
            combined
        )));
    }

    if output_format == "raw" {
        return Ok(if combined.trim().is_empty() {
            "PaddleOCR 已完成，但没有输出。".to_string()
        } else {
            combined
        });
    }

    let text = extract_paddleocr_text(&combined);
    if text.trim().is_empty() {
        Ok(if combined.trim().is_empty() {
            "PaddleOCR 已完成，但没有识别到文字。".to_string()
        } else {
            combined
        })
    } else {
        Ok(text)
    }
}

pub(crate) fn run_paddleocr_with_timeout(
    command: &mut std::process::Command,
    timeout: Duration,
) -> AppResult<std::process::Output> {
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|err| {
        crate::error::AppError::Message(format!(
            "未能启动 PaddleOCR。请先安装：python -m pip install paddleocr paddlepaddle；如 paddleocr 不在 PATH，可设置 NANODESK_PADDLEOCR_BIN。原始错误：{err}"
        ))
    })?;

    let mut stdout = child.stdout.take().ok_or_else(|| {
        crate::error::AppError::Message("未能读取 PaddleOCR 标准输出".to_string())
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        crate::error::AppError::Message("未能读取 PaddleOCR 错误输出".to_string())
    })?;
    let stdout_handle = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        output
    });
    let stderr_handle = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stderr.read_to_end(&mut output);
        output
    });

    let started_at = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| crate::error::AppError::Message(format!("等待 PaddleOCR 失败：{err}")))?
        {
            let stdout = stdout_handle.join().unwrap_or_default();
            let stderr = stderr_handle.join().unwrap_or_default();
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = stdout_handle.join().unwrap_or_default();
            let stderr = stderr_handle.join().unwrap_or_default();
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&stdout).trim(),
                String::from_utf8_lossy(&stderr).trim()
            );
            return Err(crate::error::AppError::Message(format!(
                "PaddleOCR 执行超过 {} 秒，已自动终止。请裁剪/压缩图片后重试。\n{}",
                timeout.as_secs(),
                combined.trim()
            )));
        }

        thread::sleep(Duration::from_millis(200));
    }
}

pub(crate) fn extract_paddleocr_text(output: &str) -> String {
    let mut values = Vec::new();
    let mut search_start = 0;
    while let Some(relative_index) = output[search_start..].find("rec_texts") {
        let marker_index = search_start + relative_index;
        let Some(list_start_relative) = output[marker_index..].find('[') else {
            break;
        };
        let mut chars = output[marker_index + list_start_relative + 1..]
            .chars()
            .peekable();
        while let Some(ch) = chars.next() {
            if ch == ']' {
                break;
            }
            if ch != '\'' && ch != '"' {
                continue;
            }
            let quote = ch;
            let mut value = String::new();
            let mut escaped = false;
            for next in chars.by_ref() {
                if escaped {
                    value.push(next);
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    continue;
                }
                if next == quote {
                    break;
                }
                value.push(next);
            }
            let value = value.trim();
            if !value.is_empty() {
                values.push(value.to_string());
            }
        }
        search_start = marker_index + "rec_texts".len();
    }

    if values.is_empty() {
        return String::new();
    }
    values.join("\n")
}
pub(crate) fn image_mime_from_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
        Some(ext) if ext == "bmp" => "image/bmp",
        Some(ext) if ext == "webp" => "image/webp",
        Some(ext) if ext == "tif" || ext == "tiff" => "image/tiff",
        _ => "image/png",
    }
}
