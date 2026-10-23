use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const MANIFEST_URL: &str = "https://github.com/NanoAgentCode/NanoDesk/releases/latest/download/latest.json";
const RELEASE_API_URL: &str = "https://api.github.com/repos/NanoAgentCode/NanoDesk/releases/latest";

// Match the frontend release contract and reject malformed manifests as errors.
#[derive(Deserialize, Serialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize, Serialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

async fn fetch_release(client: &reqwest::Client, manifest_url: &str, api_url: &str) -> AppResult<Option<Release>> {
    let response = client.get(manifest_url).send().await?;
    // Older releases have no manifest. Only absence permits the legacy API path;
    // network, rate-limit and invalid JSON failures must remain visible errors.
    let response = if response.status() == reqwest::StatusCode::NOT_FOUND {
        let legacy = client.get(api_url)
            .header("Accept", "application/vnd.github+json").send().await?;
        if legacy.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        legacy
    } else {
        response
    };
    Ok(Some(response.error_for_status()?.json::<Release>().await?))
}

/// Query only the official stable release; never accepts a caller-supplied URL.
#[tauri::command]
pub async fn check_app_update() -> AppResult<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("NanoDesk/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let release = fetch_release(&client, MANIFEST_URL, RELEASE_API_URL).await?;
    let arch = match std::env::consts::ARCH { "x86_64" => "x64", "aarch64" => "arm64", other => other };
    Ok(json!({ "currentVersion": env!("CARGO_PKG_VERSION"), "arch": arch, "release": release }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpListener, thread, time::Instant};

    const RELEASE: &str = r#"{"tag_name":"v0.2.8","body":"Fix updates","assets":[{"name":"NanoDesk_0.2.8_x64-setup.exe","browser_download_url":"https://github.com/NanoAgentCode/NanoDesk/releases/download/v0.2.8/NanoDesk_0.2.8_x64-setup.exe"}]}"#;

    fn server(responses: Vec<(u16, &'static str)>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(5);
            for (status, body) in responses {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                        Err(e) => panic!("Missing update request: {e}"),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                }
                requests.push(String::from_utf8(request).unwrap());
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (url, handle)
    }

    async fn fetch(url: &str) -> AppResult<Option<Release>> {
        let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().unwrap();
        fetch_release(&client, &format!("{url}/latest.json"), &format!("{url}/api")).await
    }

    #[tokio::test]
    async fn manifest_works_without_calling_rate_limited_api() {
        let (url, server) = server(vec![(200, RELEASE)]);
        assert_eq!(fetch(&url).await.unwrap().unwrap().tag_name, "v0.2.8");
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /latest.json "));
    }

    #[tokio::test]
    async fn missing_manifest_supports_old_releases() {
        let (url, server) = server(vec![(404, ""), (200, RELEASE)]);
        assert_eq!(fetch(&url).await.unwrap().unwrap().tag_name, "v0.2.8");
        assert!(server.join().unwrap()[1].starts_with("GET /api "));
    }

    #[tokio::test]
    async fn absent_release_returns_none() {
        let (url, server) = server(vec![(404, ""), (404, "")]);
        assert!(fetch(&url).await.unwrap().is_none());
        server.join().unwrap();
    }

    #[tokio::test]
    async fn failures_are_not_reported_as_no_update() {
        for responses in [vec![(403, "rate limited")], vec![(200, "{}")], vec![(200, "invalid JSON")], vec![(404, ""), (403, "rate limited")]] {
            let (url, server) = server(responses);
            assert!(fetch(&url).await.is_err());
            server.join().unwrap();
        }
    }
}
