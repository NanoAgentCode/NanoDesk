use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};

use crate::error::AppResult;
use crate::models::{
    ChatMessage, ChatRequest, ChatResponse, OpsAiRequest, OpsServer, OpsServerDraft,
    OpsUploadRequest,
};
use crate::{
    finish_observation, send_chat_completion, start_observation, AppState, ObservationStart,
};

pub(crate) struct OpsSshSessionHandle {
    pub(crate) server_id: String,
    pub(crate) input: mpsc::Sender<OpsSshControl>,
}

pub(crate) enum OpsSshControl {
    Input(String),
    Resize { cols: u32, rows: u32 },
    Close,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpsSshEvent {
    session_id: String,
    kind: String,
    data: String,
}

#[tauri::command]
pub(crate) async fn list_ops_servers(state: State<'_, AppState>) -> AppResult<Vec<OpsServer>> {
    state.db.lock().await.list_ops_servers()
}

#[tauri::command]
pub(crate) async fn save_ops_server(
    state: State<'_, AppState>,
    draft: OpsServerDraft,
) -> AppResult<OpsServer> {
    state.db.lock().await.save_ops_server(draft)
}

#[tauri::command]
pub(crate) async fn delete_ops_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_ops_server(&id)
}

fn ops_ssh_target(server: &OpsServer) -> String {
    format!("{}@{}", server.username, server.host)
}

fn add_ops_ssh_args(command: &mut std::process::Command, server: &OpsServer) -> AppResult<()> {
    command
        .arg("-p")
        .arg(server.port.to_string())
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new");

    match server.auth_method.as_str() {
        "key" => {
            if server.key_path.trim().is_empty() {
                return Err(crate::error::AppError::Message(
                    "密钥认证需要填写本地私钥路径".to_string(),
                ));
            }
            command.arg("-i").arg(server.key_path.trim());
        }
        "agent" => {
            command.arg("-o").arg("BatchMode=yes");
        }
        "password" => {
            return Err(crate::error::AppError::Message(
                "当前版本不保存或注入明文密码。请改用 SSH Agent、密钥路径，或在本机 ~/.ssh/config 中配置该主机。".to_string(),
            ));
        }
        _ => {}
    }

    Ok(())
}

fn run_ops_command(mut command: std::process::Command) -> AppResult<String> {
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "命令已完成，无输出。".to_string(),
    };

    if output.status.success() {
        Ok(combined)
    } else {
        Err(crate::error::AppError::Message(format!(
            "命令执行失败，退出码 {:?}\n{}",
            output.status.code(),
            combined
        )))
    }
}

fn ssh2_error(err: ssh2::Error) -> crate::error::AppError {
    crate::error::AppError::Message(err.to_string())
}

fn connect_ops_password_session(server: &OpsServer) -> AppResult<ssh2::Session> {
    if server.password.is_empty() {
        return Err(crate::error::AppError::Message(
            "密码认证需要填写服务器登录密码".to_string(),
        ));
    }

    let addr = format!("{}:{}", server.host, server.port);
    let socket_addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| crate::error::AppError::Message("无法解析服务器地址".to_string()))?;
    let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(8))?;
    tcp.set_read_timeout(Some(Duration::from_secs(20)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(20)))?;

    let mut session = ssh2::Session::new().map_err(ssh2_error)?;
    session.set_tcp_stream(tcp);
    session.handshake().map_err(ssh2_error)?;
    session
        .userauth_password(&server.username, &server.password)
        .map_err(ssh2_error)?;
    if !session.authenticated() {
        return Err(crate::error::AppError::Message(
            "用户名或密码认证失败".to_string(),
        ));
    }

    Ok(session)
}

fn connect_ops_ssh2_session(server: &OpsServer) -> AppResult<ssh2::Session> {
    let addr = format!("{}:{}", server.host, server.port);
    let socket_addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| crate::error::AppError::Message("无法解析服务器地址".to_string()))?;
    let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(8))?;
    tcp.set_read_timeout(Some(Duration::from_millis(250)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(20)))?;

    let mut session = ssh2::Session::new().map_err(ssh2_error)?;
    session.set_tcp_stream(tcp);
    session.handshake().map_err(ssh2_error)?;

    match server.auth_method.as_str() {
        "password" => {
            if server.password.is_empty() {
                return Err(crate::error::AppError::Message(
                    "密码认证需要填写服务器登录密码".to_string(),
                ));
            }
            session
                .userauth_password(&server.username, &server.password)
                .map_err(ssh2_error)?;
        }
        "key" => {
            if server.key_path.trim().is_empty() {
                return Err(crate::error::AppError::Message(
                    "密钥认证需要填写本地私钥路径".to_string(),
                ));
            }
            session
                .userauth_pubkey_file(
                    &server.username,
                    None,
                    std::path::Path::new(server.key_path.trim()),
                    None,
                )
                .map_err(ssh2_error)?;
        }
        "agent" => {
            let mut agent = session.agent().map_err(ssh2_error)?;
            agent.connect().map_err(ssh2_error)?;
            agent.list_identities().map_err(ssh2_error)?;
            let mut authenticated = false;
            for identity in agent.identities().map_err(ssh2_error)? {
                if agent.userauth(&server.username, &identity).is_ok() {
                    authenticated = true;
                    break;
                }
            }
            if !authenticated {
                return Err(crate::error::AppError::Message(
                    "SSH Agent 认证失败，未找到可用身份".to_string(),
                ));
            }
        }
        _ => {
            return Err(crate::error::AppError::Message(
                "不支持的 SSH 认证方式".to_string(),
            ));
        }
    }

    if !session.authenticated() {
        return Err(crate::error::AppError::Message("SSH 认证失败".to_string()));
    }

    Ok(session)
}

fn emit_ops_ssh_event(app: &AppHandle, session_id: &str, kind: &str, data: impl Into<String>) {
    let _ = app.emit(
        "ops-ssh",
        OpsSshEvent {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            data: data.into(),
        },
    );
}

fn normalize_ops_pty_size(cols: Option<u32>, rows: Option<u32>) -> (u32, u32) {
    (
        cols.unwrap_or(120).clamp(20, 500),
        rows.unwrap_or(32).clamp(6, 200),
    )
}

fn spawn_ops_ssh_shell(
    app: AppHandle,
    server: OpsServer,
    session_id: String,
    initial_size: (u32, u32),
    rx: mpsc::Receiver<OpsSshControl>,
) {
    thread::spawn(move || {
        let result = (|| -> AppResult<()> {
            let session = connect_ops_ssh2_session(&server)?;
            let mut channel = session.channel_session().map_err(ssh2_error)?;
            channel
                .request_pty(
                    "xterm-256color",
                    None,
                    Some((initial_size.0, initial_size.1, 0, 0)),
                )
                .map_err(ssh2_error)?;
            channel.shell().map_err(ssh2_error)?;
            session.set_blocking(false);
            emit_ops_ssh_event(
                &app,
                &session_id,
                "ready",
                format!(
                    "已连接 {}@{}:{}\r\n",
                    server.username, server.host, server.port
                ),
            );

            let mut buffer = [0_u8; 4096];
            loop {
                match channel.read(&mut buffer) {
                    Ok(0) => {
                        if channel.eof() {
                            break;
                        }
                    }
                    Ok(size) => {
                        let data = String::from_utf8_lossy(&buffer[..size]).to_string();
                        emit_ops_ssh_event(&app, &session_id, "data", data);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(err) => return Err(crate::error::AppError::Io(err)),
                }

                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(OpsSshControl::Input(input)) => {
                        channel.write_all(input.as_bytes())?;
                        channel.flush()?;
                    }
                    Ok(OpsSshControl::Resize { cols, rows }) => {
                        channel
                            .request_pty_size(cols.clamp(20, 500), rows.clamp(6, 200), None, None)
                            .map_err(ssh2_error)?;
                    }
                    Ok(OpsSshControl::Close) => {
                        let _ = channel.close();
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = channel.close();
                        break;
                    }
                }

                if channel.eof() {
                    break;
                }
            }

            let _ = channel.wait_close();
            Ok(())
        })();

        if let Err(err) = result {
            emit_ops_ssh_event(&app, &session_id, "error", err.to_string());
        }
        emit_ops_ssh_event(&app, &session_id, "closed", "");
    });
}

fn run_ops_password_command(server: &OpsServer, remote_command: &str) -> AppResult<String> {
    let session = connect_ops_password_session(server)?;
    let mut channel = session.channel_session().map_err(ssh2_error)?;
    channel.exec(remote_command).map_err(ssh2_error)?;

    let mut stdout = String::new();
    channel.read_to_string(&mut stdout)?;
    let mut stderr = String::new();
    channel.stderr().read_to_string(&mut stderr)?;
    channel.wait_close().map_err(ssh2_error)?;
    let exit_status = channel.exit_status().map_err(ssh2_error)?;
    let stdout = stdout.trim().to_string();
    let stderr = stderr.trim().to_string();
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "命令已完成，无输出。".to_string(),
    };

    if exit_status == 0 {
        Ok(combined)
    } else {
        Err(crate::error::AppError::Message(format!(
            "远程命令执行失败，退出码 {exit_status}\n{combined}"
        )))
    }
}

fn resolve_ops_remote_upload_path(
    server: &OpsServer,
    requested: &str,
    local_path: &std::path::Path,
) -> String {
    let file_name = local_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload.bin");
    let base = if requested.trim().is_empty() {
        server.remote_dir.trim()
    } else {
        requested.trim()
    };

    if base.is_empty() || base == "." || base == "./" {
        return format!("./{file_name}");
    }
    if base.ends_with('/') {
        return format!("{base}{file_name}");
    }
    base.to_string()
}

fn upload_ops_password_file(
    server: &OpsServer,
    local_path: &std::path::Path,
    remote_path: &str,
) -> AppResult<String> {
    let session = connect_ops_password_session(server)?;
    let sftp = session.sftp().map_err(ssh2_error)?;
    let resolved_remote_path = resolve_ops_remote_upload_path(server, remote_path, local_path);
    let mut local_file = std::fs::File::open(local_path)?;
    let mut remote_file = sftp
        .create(std::path::Path::new(&resolved_remote_path))
        .map_err(ssh2_error)?;
    let bytes = std::io::copy(&mut local_file, &mut remote_file)?;
    remote_file.flush()?;
    Ok(format!(
        "上传完成：{} 字节 -> {}",
        bytes, resolved_remote_path
    ))
}

#[tauri::command]
pub(crate) async fn test_ops_ssh_connection(
    state: State<'_, AppState>,
    server_id: String,
) -> AppResult<String> {
    let server = state.db.lock().await.get_ops_server(&server_id)?;
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "ops.ssh.test",
            category: "tool",
            entity_type: Some("ops_server"),
            entity_id: Some(server.id.clone()),
            input_summary: Some(format!(
                "{}@{}:{}",
                server.username, server.host, server.port
            )),
            metadata: serde_json::json!({ "auth_method": server.auth_method }),
            trace_id: Some(server.id.clone()),
        },
    )
    .await;
    let result = (|| -> AppResult<String> {
        let remote_command = "printf 'connected: '; hostname; printf 'kernel: '; uname -a";
        if server.auth_method == "password" {
            return run_ops_password_command(&server, remote_command);
        }

        let mut command = std::process::Command::new("ssh");
        add_ops_ssh_args(&mut command, &server)?;
        command.arg(ops_ssh_target(&server)).arg(remote_command);
        run_ops_command(command)
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|output| format!("output_chars={}", output.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
pub(crate) async fn upload_ops_file(
    state: State<'_, AppState>,
    request: OpsUploadRequest,
) -> AppResult<String> {
    let server = state.db.lock().await.get_ops_server(&request.server_id)?;
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "ops.file.upload",
            category: "tool",
            entity_type: Some("ops_server"),
            entity_id: Some(server.id.clone()),
            input_summary: Some(format!(
                "local_path_chars={}",
                request.local_path.chars().count()
            )),
            metadata: serde_json::json!({ "remote_path": request.remote_path.clone() }),
            trace_id: Some(server.id.clone()),
        },
    )
    .await;
    let result = (|| -> AppResult<String> {
        let local_path = std::path::Path::new(&request.local_path);
        if !local_path.is_file() {
            return Err(crate::error::AppError::Message(
                "只能上传本地普通文件".to_string(),
            ));
        }
        if server.auth_method == "password" {
            return upload_ops_password_file(&server, local_path, request.remote_path.trim());
        }

        let remote_path = if request.remote_path.trim().is_empty() {
            if server.remote_dir.trim().is_empty() {
                "./".to_string()
            } else {
                server.remote_dir.trim().to_string()
            }
        } else {
            request.remote_path.trim().to_string()
        };

        let mut command = std::process::Command::new("scp");
        command
            .arg("-P")
            .arg(server.port.to_string())
            .arg("-o")
            .arg("ConnectTimeout=8")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new");
        match server.auth_method.as_str() {
            "key" => {
                if server.key_path.trim().is_empty() {
                    return Err(crate::error::AppError::Message(
                        "密钥认证需要填写本地私钥路径".to_string(),
                    ));
                }
                command.arg("-i").arg(server.key_path.trim());
            }
            "agent" => {
                command.arg("-o").arg("BatchMode=yes");
            }
            "password" => {
                return Err(crate::error::AppError::Message(
                    "当前版本不保存或注入明文密码。请改用 SSH Agent、密钥路径，或本机 SSH 配置。"
                        .to_string(),
                ));
            }
            _ => {}
        }
        command
            .arg(local_path)
            .arg(format!("{}:{}", ops_ssh_target(&server), remote_path));
        run_ops_command(command).map(|output| {
            if output.trim().is_empty() {
                "上传完成。".to_string()
            } else {
                output
            }
        })
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|output| format!("output_chars={}", output.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
pub(crate) async fn start_ops_ssh_session(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    cols: Option<u32>,
    rows: Option<u32>,
) -> AppResult<String> {
    {
        let mut sessions = state.ops_ssh_sessions.lock().await;
        let existing_ids = sessions
            .iter()
            .filter_map(|(session_id, handle)| {
                if handle.server_id == server_id {
                    Some(session_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for session_id in existing_ids {
            if let Some(handle) = sessions.remove(&session_id) {
                let _ = handle.input.send(OpsSshControl::Close);
            }
        }
    }

    let server = state.db.lock().await.get_ops_server(&server_id)?;
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "ops.ssh.session.start",
            category: "tool",
            entity_type: Some("ops_server"),
            entity_id: Some(server.id.clone()),
            input_summary: Some(format!(
                "{}@{}:{}",
                server.username, server.host, server.port
            )),
            metadata: serde_json::json!({ "auth_method": server.auth_method }),
            trace_id: Some(server.id.clone()),
        },
    )
    .await;
    let result: AppResult<(String, mpsc::Sender<OpsSshControl>)> = {
        let session_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel();
        spawn_ops_ssh_shell(
            app,
            server.clone(),
            session_id.clone(),
            normalize_ops_pty_size(cols, rows),
            rx,
        );
        Ok((session_id, tx))
    };
    let result = match result {
        Ok((session_id, tx)) => {
            state.ops_ssh_sessions.lock().await.insert(
                session_id.clone(),
                OpsSshSessionHandle {
                    server_id: server.id.clone(),
                    input: tx,
                },
            );
            Ok(session_id)
        }
        Err(err) => Err(err),
    };
    let summary = result
        .as_ref()
        .ok()
        .map(|session_id| format!("session_id={session_id}"));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
pub(crate) async fn send_ops_ssh_input(
    state: State<'_, AppState>,
    session_id: String,
    input: String,
) -> AppResult<()> {
    let sessions = state.ops_ssh_sessions.lock().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| crate::error::AppError::Message("SSH 会话不存在或已关闭".to_string()))?;
    handle
        .input
        .send(OpsSshControl::Input(input))
        .map_err(|_| crate::error::AppError::Message("SSH 会话已关闭".to_string()))
}

#[tauri::command]
pub(crate) async fn resize_ops_ssh_session(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> AppResult<()> {
    let sessions = state.ops_ssh_sessions.lock().await;
    let handle = sessions.get(&session_id).ok_or_else(|| {
        crate::error::AppError::Message("SSH session does not exist or is closed".to_string())
    })?;
    let (cols, rows) = normalize_ops_pty_size(Some(cols), Some(rows));
    handle
        .input
        .send(OpsSshControl::Resize { cols, rows })
        .map_err(|_| crate::error::AppError::Message("SSH session is closed".to_string()))
}

#[tauri::command]
pub(crate) async fn stop_ops_ssh_session(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<()> {
    if let Some(handle) = state.ops_ssh_sessions.lock().await.remove(&session_id) {
        let _ = handle.input.send(OpsSshControl::Close);
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn ask_ops_ai(
    state: State<'_, AppState>,
    request: OpsAiRequest,
) -> AppResult<ChatResponse> {
    let server = state.db.lock().await.get_ops_server(&request.server_id)?;
    let config = state
        .db
        .lock()
        .await
        .get_model_config(&request.model_config_id)?;
    let prompt = request.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(crate::error::AppError::Message(
            "请输入运维问题".to_string(),
        ));
    }

    let chat_request = ChatRequest {
        model_config_id: config.id.clone(),
        temperature: Some(0.2),
        trace_id: Some(server.id.clone()),
        max_tokens: None,
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "你是 NanoAgent 的本地运维助手。基于用户保存的服务器上下文提供谨慎、可执行的建议。涉及危险命令、删除、重启、权限变更、网络暴露时必须明确风险和确认步骤。不要编造服务器状态。".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "服务器上下文：\n名称：{}\n地址：{}@{}:{}\n认证：{}\n默认目录：{}\n最近 SSH 输出：{}\n\n用户问题：{}",
                    server.name,
                    server.username,
                    server.host,
                    server.port,
                    server.auth_method,
                    if server.remote_dir.trim().is_empty() { "(未设置)" } else { &server.remote_dir },
                    request.last_ssh_output.as_deref().unwrap_or("(无)"),
                    prompt
                ),
            },
        ],
    };
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "ops.ai.ask",
            category: "llm",
            entity_type: Some("ops_server"),
            entity_id: Some(server.id.clone()),
            input_summary: Some(format!("prompt_chars={}", prompt.chars().count())),
            metadata: serde_json::json!({
                "model_config_id": config.id,
                "server_id": server.id,
                "last_ssh_output_chars": request
                    .last_ssh_output
                    .as_ref()
                    .map(|output| output.chars().count())
                    .unwrap_or(0),
            }),
            trace_id: chat_request.trace_id.clone(),
        },
    )
    .await;
    let result = send_chat_completion(config, chat_request).await;
    let output = result
        .as_ref()
        .ok()
        .map(|response| format!("content_chars={}", response.content.chars().count()));
    finish_observation(&state, span, &result, output).await;
    result
}
