use super::*;
use serde_json::json;

#[tokio::test]
async fn polling_stops_at_its_budget() {
    let (url, server) = mock_http(vec![
        (202, r#"{"task_id":"job-1"}"#),
        (202, r#"{"task_id":"job-1"}"#),
    ]);
    let config = OcrConfig {
        provider: OcrProvider::Mineru,
        mineru_url: format!("{url}/v1/file_parse"),
        ..Default::default()
    };
    let error = request_remote_with_polling(
        &config,
        "sample.png",
        "image/png",
        b"image",
        "text",
        1,
        Duration::from_millis(1),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("等待超时"));
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn defaults_and_validation() {
    let config: OcrConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(config.provider, OcrProvider::Local);
    assert!(config.validate().is_ok());
    let mut config = OcrConfig {
        provider: OcrProvider::External,
        ..Default::default()
    };
    assert!(config.validate().is_err());
    config.external_url = "http://localhost:8000/v1/chat/completions".into();
    config.model = "vision".into();
    assert!(config.validate().is_ok());
    config.external_url = "file:///tmp/ocr".into();
    assert!(config.validate().is_err());
}

#[test]
fn extracts_both_mineru_protocols_and_rejects_empty_or_failed() {
    assert_eq!(
        extract_text(
            &json!({"results":{"image":{"md_content":"文字"}}}),
            OcrProvider::Mineru
        )
        .unwrap(),
        "文字"
    );
    assert_eq!(extract_text(&json!({"status":"completed","files":[{"status":"completed","content":{"markdown":"# 标题"}}]}), OcrProvider::Mineru).unwrap(), "# 标题");
    assert!(extract_text(&json!({"status":"failed","files":[]}), OcrProvider::Mineru).is_err());
    assert!(extract_text(&json!({"results":{}}), OcrProvider::Mineru).is_err());
    assert_eq!(
        extract_text(
            &json!({"choices":[{"message":{"content":"识别文本"}}]}),
            OcrProvider::External
        )
        .unwrap(),
        "识别文本"
    );
}

fn mock_http(replies: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (code, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let count = stream.read(&mut buf).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buf[..count]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let len: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8_lossy(&bytes).into_owned());
            write!(stream, "HTTP/1.1 {code} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (url, handle)
}

#[tokio::test]
async fn external_sends_image_model_and_auth() {
    let (url, server) = mock_http(vec![(
        200,
        r#"{"choices":[{"message":{"content":"hello"}}]}"#,
    )]);
    let config = OcrConfig {
        provider: OcrProvider::External,
        external_url: format!("{url}/v1/chat/completions"),
        external_key: "secret".into(),
        model: "vision".into(),
        ..Default::default()
    };
    assert_eq!(
        request_remote(&config, "sample.png", "image/png", b"image", "text")
            .await
            .unwrap(),
        "hello"
    );
    let requests = server.join().unwrap();
    assert!(requests[0].contains("Bearer secret"));
    assert!(requests[0].contains("data:image/png;base64,aW1hZ2U="));
    assert!(requests[0].contains("\"model\":\"vision\""));
}

#[tokio::test]
async fn mineru_uploads_file_and_polls_accepted_task() {
    let (url, server) = mock_http(vec![
        (202, r#"{"task_id":"job-1"}"#),
        (
            200,
            r#"{"status":"completed","files":[{"content":{"markdown":"done"}}]}"#,
        ),
    ]);
    let config = OcrConfig {
        provider: OcrProvider::Mineru,
        mineru_url: format!("{url}/v1/file_parse"),
        ..Default::default()
    };
    assert_eq!(
        request_remote(&config, "sample.png", "image/png", b"image", "text")
            .await
            .unwrap(),
        "done"
    );
    let requests = server.join().unwrap();
    assert!(requests[0].contains("multipart/form-data"));
    assert!(requests[0].contains("name=\"files\"; filename=\"sample.png\""));
    assert!(requests[1].starts_with("GET /v1/tasks/job-1/result?response_format=json"));
}

#[tokio::test]
async fn http_failure_is_not_success_or_local_fallback() {
    let (url, server) = mock_http(vec![(401, r#"{"error":"unauthorized"}"#)]);
    let config = OcrConfig {
        provider: OcrProvider::External,
        external_url: url,
        model: "vision".into(),
        ..Default::default()
    };
    assert!(
        request_remote(&config, "sample.png", "image/png", b"image", "raw")
            .await
            .unwrap_err()
            .to_string()
            .contains("401")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn legacy_mineru_sends_options_and_supports_raw_output() {
    let (url, server) = mock_http(vec![(
        200,
        r#"{"results":{"sample":{"md_content":"legacy text"}}}"#,
    )]);
    let config = OcrConfig {
        provider: OcrProvider::Mineru,
        mineru_url: format!("{url}/file_parse"),
        mineru_key: "token".into(),
        ..Default::default()
    };
    let raw = request_remote(&config, "sample.png", "image/png", b"image", "raw")
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&raw).unwrap()["results"]["sample"]["md_content"],
        "legacy text"
    );
    let requests = server.join().unwrap();
    assert!(requests[0].contains("Bearer token"));
    assert!(requests[0].contains("name=\"return_md\"\r\n\r\ntrue"));
    assert!(requests[0].contains("name=\"response_format_zip\"\r\n\r\nfalse"));
}

#[tokio::test]
async fn rejects_malformed_pending_task_instead_of_polling_forever() {
    let (url, server) = mock_http(vec![(202, r#"{"status":"running"}"#)]);
    let config = OcrConfig {
        provider: OcrProvider::Mineru,
        mineru_url: format!("{url}/v1/file_parse"),
        ..Default::default()
    };
    assert!(
        request_remote(&config, "sample.png", "image/png", b"image", "text")
            .await
            .unwrap_err()
            .to_string()
            .contains("task_id")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn invalid_inputs_fail_before_any_remote_request() {
    let root = std::env::temp_dir().join(format!("ocr-inputs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let config = OcrConfig {
        provider: OcrProvider::External,
        external_url: "http://127.0.0.1:1/v1/chat/completions".into(),
        model: "vision".into(),
        ..Default::default()
    };
    std::fs::write(root.join("bad.txt"), "text").unwrap();
    assert!(
        run_ocr(config.clone(), root.to_str().unwrap(), "bad.txt", "text")
            .await
            .unwrap_err()
            .to_string()
            .contains("图片")
    );
    let path = root.join("large.png");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        run_ocr(config.clone(), root.to_str().unwrap(), "large.png", "text")
            .await
            .unwrap_err()
            .to_string()
            .contains("8MB")
    );
    assert!(
        run_ocr(config, root.to_str().unwrap(), "../outside.png", "text")
            .await
            .is_err()
    );
    std::fs::remove_file(root.join("bad.txt")).unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
}
