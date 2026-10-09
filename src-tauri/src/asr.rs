use std::time::Duration;

use base64::Engine as _;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct AsrConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub language: String,
}

impl AsrConfig {
    pub fn normalized(mut self) -> AppResult<Self> {
        self.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        self.api_key = self.api_key.trim().to_string();
        self.model = self.model.trim().to_string();
        self.language = self.language.trim().to_lowercase();
        let url = reqwest::Url::parse(&self.base_url)
            .map_err(|_| AppError::from("请填写有效的 ASR HTTP/HTTPS 服务地址"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("ASR 地址必须是无账号、查询参数和片段的 HTTP/HTTPS URL".into());
        }
        if self.model.is_empty() {
            return Err("请填写 ASR 模型名称".into());
        }
        if !self.language.is_empty()
            && (self.language.len() != 2 || !self.language.bytes().all(|b| b.is_ascii_alphabetic()))
        {
            return Err("语言请填写两位代码，例如 zh、en，或留空".into());
        }
        Ok(self)
    }

    fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/audio/transcriptions") {
            base.to_string()
        } else {
            format!("{base}/audio/transcriptions")
        }
    }
}

#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    file_name: String,
    audio_base64: String,
    config: Option<AsrConfig>,
) -> AppResult<String> {
    let config = match config {
        Some(config) => Some(config),
        None => crate::settings::load_asr_config(&app)?,
    }
    .ok_or_else(|| AppError::from("请先在系统设置 → 语音识别中保存 ASR 配置"))?
    .normalized()?;
    if audio_base64.len() > MAX_AUDIO_BYTES.div_ceil(3) * 4 {
        return Err("音频文件不能超过 25 MB".into());
    }
    let audio = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .map_err(|_| AppError::from("音频数据无效"))?;
    request_transcription(&config, &file_name, audio).await
}

fn audio_mime(file_name: &str) -> AppResult<&'static str> {
    match file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => Ok("audio/wav"),
        "mp3" | "mpeg" | "mpga" => Ok("audio/mpeg"),
        "mp4" | "m4a" => Ok("audio/mp4"),
        "webm" => Ok("audio/webm"),
        _ => Err("不支持的音频格式，请使用 WAV、MP3、M4A、MP4、MPEG、MPGA 或 WebM".into()),
    }
}

async fn request_transcription(
    config: &AsrConfig,
    file_name: &str,
    audio: Vec<u8>,
) -> AppResult<String> {
    if audio.is_empty() {
        return Err("音频文件为空".into());
    }
    if audio.len() > MAX_AUDIO_BYTES {
        return Err("音频文件不能超过 25 MB".into());
    }
    let part = Part::bytes(audio)
        .file_name(file_name.to_string())
        .mime_str(audio_mime(file_name)?)?;
    let mut form = Form::new()
        .part("file", part)
        .text("model", config.model.clone());
    if !config.language.is_empty() {
        form = form.text("language", config.language.clone());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let mut request = client.post(config.endpoint()).multipart(form);
    if !config.api_key.is_empty() {
        request = request.bearer_auth(&config.api_key);
    }
    let response = request.send().await.map_err(|err| {
        if err.is_timeout() {
            AppError::from("语音识别超时（120 秒），请重试或使用更短的音频")
        } else {
            AppError::from("无法连接 ASR 服务，请检查服务地址和网络")
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        // Do not expose an untrusted response body which may echo audio or credentials.
        let hint = match status.as_u16() {
            401 | 403 => "请检查 API Key 和模型访问权限",
            404 => "请检查接口地址和模型名称",
            413 => "服务商的音频大小限制更小，请缩短音频",
            429 => "请求过多或额度不足，请稍后重试或检查账户额度",
            _ => "请检查模型名称、音频格式及服务状态",
        };
        return Err(format!("语音识别失败（HTTP {}）：{hint}", status.as_u16()).into());
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| AppError::from("ASR 服务未返回有效 JSON，请检查接口协议"))?;
    let text = value
        .get("text")
        .and_then(|text| text.as_str())
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return Err("未识别到文字，请检查音频内容或服务返回格式".into());
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn config(base_url: &str) -> AsrConfig {
        AsrConfig {
            base_url: base_url.into(),
            model: "Qwen/Qwen3-ASR-1.7B".into(),
            api_key: "test-key".into(),
            language: "".into(),
        }
    }

    #[test]
    fn accepts_root_and_full_endpoint_without_duplicate_path() {
        assert_eq!(
            config("https://api.siliconflow.cn/v1/")
                .normalized()
                .unwrap()
                .endpoint(),
            "https://api.siliconflow.cn/v1/audio/transcriptions"
        );
        assert_eq!(
            config("https://api.siliconflow.cn/v1/audio/transcriptions/")
                .normalized()
                .unwrap()
                .endpoint(),
            "https://api.siliconflow.cn/v1/audio/transcriptions"
        );
        assert!(config("file:///tmp").normalized().is_err());
        assert!(config("https://asr.example/v1?key=secret")
            .normalized()
            .is_err());
        assert!(AsrConfig {
            model: "".into(),
            ..config("https://asr.example/v1")
        }
        .normalized()
        .is_err());
        assert!(AsrConfig {
            language: "Chinese".into(),
            ..config("https://asr.example/v1")
        }
        .normalized()
        .is_err());
    }

    #[tokio::test]
    async fn rejects_invalid_audio_before_network() {
        let cfg = config("http://127.0.0.1:1/v1");
        assert!(request_transcription(&cfg, "voice.wav", vec![])
            .await
            .unwrap_err()
            .to_string()
            .contains("为空"));
        assert!(request_transcription(&cfg, "voice.txt", vec![1])
            .await
            .unwrap_err()
            .to_string()
            .contains("格式"));
        assert!(
            request_transcription(&cfg, "voice.wav", vec![0; MAX_AUDIO_BYTES + 1])
                .await
                .unwrap_err()
                .to_string()
                .contains("25 MB")
        );
    }

    fn mock_server(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = socket.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            String::from_utf8_lossy(&bytes).into_owned()
        });
        (base, handle)
    }

    #[tokio::test]
    async fn sends_multipart_audio_and_reads_text() {
        let (base, server) = mock_server(200, r#"{"text":"  测试识别文字  "}"#);
        let text =
            request_transcription(&config(&base), "recording.wav", b"RIFF-test-audio".to_vec())
                .await
                .unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
        assert!(request
            .to_lowercase()
            .contains("authorization: bearer test-key"));
        assert!(request
            .to_lowercase()
            .contains("content-type: multipart/form-data; boundary="));
        assert!(request.contains("name=\"file\"; filename=\"recording.wav\""));
        assert!(request.contains("RIFF-test-audio"));
        assert!(request.contains("name=\"model\""));
        assert!(request.contains("Qwen/Qwen3-ASR-1.7B"));
        assert!(!request.contains("name=\"language\""));
        assert_eq!(text, "测试识别文字");
    }

    #[tokio::test]
    async fn supports_full_url_language_and_anonymous_local_service() {
        let (base, server) = mock_server(200, r#"{"text":"hello"}"#);
        let cfg = AsrConfig {
            base_url: format!("{base}/audio/transcriptions"),
            api_key: "".into(),
            language: "en".into(),
            ..config(&base)
        };
        assert_eq!(
            request_transcription(&cfg, "voice.mp3", vec![1, 2, 3])
                .await
                .unwrap(),
            "hello"
        );
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
        assert!(!request.to_lowercase().contains("authorization:"));
        assert!(request.contains("name=\"language\"\r\n\r\nen"));
    }

    #[tokio::test]
    async fn reports_auth_failure_without_echoing_credentials() {
        let (base, server) = mock_server(401, r#"{"message":"test-key"}"#);
        let error = request_transcription(&config(&base), "voice.wav", vec![1])
            .await
            .unwrap_err()
            .to_string();
        server.join().unwrap();
        assert!(error.contains("401"));
        assert!(error.contains("API Key"));
        assert!(!error.contains("test-key"));
    }

    #[tokio::test]
    async fn rejects_empty_or_incompatible_response() {
        for body in [
            r#"{"text":"  "}"#,
            r#"{"result":"wrong protocol"}"#,
            "not-json",
        ] {
            let (base, server) = mock_server(200, body);
            assert!(request_transcription(&config(&base), "voice.wav", vec![1])
                .await
                .is_err());
            server.join().unwrap();
        }
    }
}
