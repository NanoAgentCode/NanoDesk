use futures_util::StreamExt;
use serde::Serialize;
use std::{io::Write, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{ipc::Channel, AppHandle, Manager};

static READY: Mutex<Option<PathBuf>> = Mutex::new(None);
static DOWNLOADING: Mutex<bool> = Mutex::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadProgress { downloaded: u64, total: Option<u64> }

fn installer_name(url: &str) -> Result<&str, String> {
    let path = url.strip_prefix("https://github.com/NanoAgentCode/NanoDesk/releases/download/")
        .ok_or("仅支持官方 GitHub Release 安装包")?;
    let (tag, name) = path.split_once('/').ok_or("安装包链接无效")?;
    let suffix = match std::env::consts::ARCH {
        "x86_64" => "_x64-setup.exe", "aarch64" => "_arm64-setup.exe", _ => return Err("不支持当前架构".into()),
    };
    if tag.is_empty() || !name.ends_with(suffix) || !name.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)) {
        return Err("安装包名称或架构无效".into());
    }
    Ok(name)
}

#[tauri::command]
pub(crate) async fn download_app_update(app: AppHandle, url: String, on_progress: Channel<DownloadProgress>) -> Result<(), String> {
    let name = installer_name(&url)?.to_owned();
    {
        let mut busy = DOWNLOADING.lock().map_err(|e| e.to_string())?;
        if *busy { return Err("正在下载更新".into()); }
        *busy = true;
    }
    let result = async {
        if let Some(old) = READY.lock().map_err(|e| e.to_string())?.take() { let _ = std::fs::remove_file(old); }
        let directory = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let target = directory.join(format!("{}-{name}", uuid::Uuid::new_v4()));
        let partial = target.with_extension("part");
        let transfer = async {
            let response = reqwest::Client::builder().connect_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(3600)).build().map_err(|e| e.to_string())?
                .get(&url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
            let total = response.content_length();
            let mut stream = response.bytes_stream();
            let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
            let mut downloaded = 0;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| e.to_string())?;
                file.write_all(&chunk).map_err(|e| e.to_string())?;
                downloaded += chunk.len() as u64;
                let _ = on_progress.send(DownloadProgress { downloaded, total });
            }
            if downloaded == 0 || total.is_some_and(|size| size != downloaded) { return Err("安装包下载不完整".to_string()); }
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            std::fs::rename(&partial, &target).map_err(|e| e.to_string())?;
            *READY.lock().map_err(|e| e.to_string())? = Some(target);
            Ok(())
        }.await;
        if transfer.is_err() { let _ = std::fs::remove_file(partial); }
        transfer
    }.await;
    *DOWNLOADING.lock().map_err(|e| e.to_string())? = false;
    result
}

#[tauri::command]
pub(crate) fn install_app_update(app: AppHandle) -> Result<(), String> {
    let ready = READY.lock().map_err(|e| e.to_string())?;
    let path = ready.as_ref().ok_or("请先完成下载")?;
    std::process::Command::new(path).spawn().map_err(|e| format!("无法启动安装程序：{e}"))?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_official_installer_for_this_architecture() {
        let arch = if std::env::consts::ARCH == "aarch64" { "arm64" } else { "x64" };
        let url = format!("https://github.com/NanoAgentCode/NanoDesk/releases/download/v0.3.0/NanoDesk_0.3.0_{arch}-setup.exe");
        assert!(installer_name(&url).is_ok());
        for bad in [url.replace("github.com", "evil.example"), format!("{url}?x=1"), url.replace("NanoDesk_0.3.0", "../NanoDesk"), url.replace("-setup.exe", ".msi")] {
            assert!(installer_name(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn rejects_wrong_architecture() {
        let arch = if std::env::consts::ARCH == "aarch64" { "x64" } else { "arm64" };
        assert!(installer_name(&format!("https://github.com/NanoAgentCode/NanoDesk/releases/download/v1/NanoDesk_{arch}-setup.exe")).is_err());
    }
}
