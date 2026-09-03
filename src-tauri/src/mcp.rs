use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE, ORIGIN, USER_AGENT,
};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{timeout, Duration};

use crate::brand;
use crate::error::{AppError, AppResult};
use crate::models::McpServerConfig;
use crate::tool_policy::McpToolScope;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2024-11-05"];
const STDERR_LINE_LIMIT: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server_id: String,
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub server_id: String,
    pub connected: bool,
    pub tool_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerView {
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    pub tools: Vec<McpToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallRequest {
    pub server_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallResult {
    pub server_id: String,
    pub tool_name: String,
    pub content_json: String,
    pub is_error: bool,
}

#[derive(Default)]
pub struct McpClientManager {
    sessions: HashMap<String, McpSession>,
    last_errors: HashMap<String, String>,
}

impl McpClientManager {
    pub fn list_views(&self, configs: Vec<McpServerConfig>) -> Vec<McpServerView> {
        configs
            .into_iter()
            .map(|config| {
                let tools = self
                    .sessions
                    .get(&config.id)
                    .map(McpSession::tools)
                    .unwrap_or_default();
                let status = McpServerStatus {
                    server_id: config.id.clone(),
                    connected: self.sessions.contains_key(&config.id),
                    tool_count: tools.len(),
                    error: self.last_errors.get(&config.id).cloned(),
                };
                McpServerView {
                    config,
                    status,
                    tools,
                }
            })
            .collect()
    }

    pub async fn connect(&mut self, config: McpServerConfig) -> AppResult<McpServerView> {
        self.disconnect(&config.id).await?;
        match McpSession::start(&config).await {
            Ok(mut session) => {
                let tools = match session.list_tools().await {
                    Ok(tools) => tools,
                    Err(err) => {
                        let message = format!("mcp startup tool discovery failed: {err}");
                        self.last_errors.insert(config.id.clone(), message.clone());
                        session.shutdown().await;
                        return Err(AppError::Message(message));
                    }
                };
                session.set_tools(tools);
                self.last_errors.remove(&config.id);
                let view = McpServerView {
                    status: McpServerStatus {
                        server_id: config.id.clone(),
                        connected: true,
                        tool_count: session.tools().len(),
                        error: None,
                    },
                    tools: session.tools(),
                    config: config.clone(),
                };
                self.sessions.insert(config.id.clone(), session);
                Ok(view)
            }
            Err(err) => {
                self.last_errors.insert(config.id.clone(), err.to_string());
                Err(err)
            }
        }
    }

    pub async fn disconnect(&mut self, server_id: &str) -> AppResult<()> {
        if let Some(session) = self.sessions.remove(server_id) {
            session.shutdown().await;
        }
        Ok(())
    }

    pub async fn refresh_tools(&mut self, server_id: &str) -> AppResult<Vec<McpToolInfo>> {
        let session = self
            .sessions
            .get_mut(server_id)
            .ok_or_else(|| AppError::Message("mcp server is not connected".to_string()))?;
        let tools = session.list_tools().await?;
        session.set_tools(tools.clone());
        self.last_errors.remove(server_id);
        Ok(tools)
    }

    pub async fn call_tool(&mut self, request: McpToolCallRequest) -> AppResult<McpToolCallResult> {
        let session = self
            .sessions
            .get_mut(&request.server_id)
            .ok_or_else(|| AppError::Message("mcp server is not connected".to_string()))?;
        session.call_tool(request).await
    }

    pub fn allowed_tool_scopes(&self) -> BTreeSet<McpToolScope> {
        self.sessions
            .iter()
            .flat_map(|(server_id, session)| {
                session
                    .tools()
                    .into_iter()
                    .map(|tool| McpToolScope {
                        server_id: server_id.clone(),
                        tool_name: tool.name,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

enum McpSession {
    Stdio(StdioSession),
    StreamableHttp(HttpSession),
    Sse(SseSession),
}

impl McpSession {
    async fn start(config: &McpServerConfig) -> AppResult<Self> {
        match config.transport.as_str() {
            "stdio" => Ok(Self::Stdio(StdioSession::start(config).await?)),
            "sse" => Ok(Self::Sse(SseSession::start(config).await?)),
            "streamable_http" => Ok(Self::StreamableHttp(HttpSession::start(config).await?)),
            other => Err(AppError::Message(format!(
                "unsupported mcp transport: {other}"
            ))),
        }
    }

    fn tools(&self) -> Vec<McpToolInfo> {
        match self {
            Self::Stdio(session) => session.tools.clone(),
            Self::StreamableHttp(session) => session.tools.clone(),
            Self::Sse(session) => session.tools.clone(),
        }
    }

    fn set_tools(&mut self, tools: Vec<McpToolInfo>) {
        match self {
            Self::Stdio(session) => session.tools = tools,
            Self::StreamableHttp(session) => session.tools = tools,
            Self::Sse(session) => session.tools = tools,
        }
    }

    async fn shutdown(self) {
        match self {
            Self::Stdio(mut session) => {
                let _ = session.child.kill().await;
            }
            Self::StreamableHttp(session) => {
                let _ = session.shutdown().await;
            }
            Self::Sse(_) => {}
        }
    }

    async fn list_tools(&mut self) -> AppResult<Vec<McpToolInfo>> {
        let server_id = self.server_id().to_string();
        let result = self.request("tools/list", json!({})).await?;
        Ok(parse_tools(&server_id, result))
    }

    async fn call_tool(&mut self, request: McpToolCallRequest) -> AppResult<McpToolCallResult> {
        let tool_name = request.tool_name.clone();
        let arguments = parse_arguments(&request.arguments_json)?;
        let result = self
            .request(
                "tools/call",
                json!({
                    "name": tool_name,
                    "arguments": arguments,
                }),
            )
            .await?;
        Ok(McpToolCallResult {
            server_id: request.server_id,
            tool_name,
            content_json: result
                .get("content")
                .cloned()
                .unwrap_or_else(|| json!([]))
                .to_string(),
            is_error: result
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        match self {
            Self::Stdio(session) => session.request(method, params).await,
            Self::StreamableHttp(session) => session.request(method, params).await,
            Self::Sse(session) => session.request(method, params).await,
        }
    }

    fn server_id(&self) -> &str {
        match self {
            Self::Stdio(session) => &session.server_id,
            Self::StreamableHttp(session) => &session.server_id,
            Self::Sse(session) => &session.server_id,
        }
    }
}

struct StdioSession {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    stderr: StdioStderrCapture,
    request_id: u64,
    server_id: String,
    tools: Vec<McpToolInfo>,
}

#[derive(Clone, Default)]
struct StdioStderrCapture {
    lines: Arc<AsyncMutex<VecDeque<String>>>,
}

impl StdioStderrCapture {
    fn spawn<R>(&self, stderr: R)
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let lines = self.lines.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            loop {
                match reader.next_line().await {
                    Ok(Some(line)) => {
                        let mut captured = lines.lock().await;
                        if captured.len() >= STDERR_LINE_LIMIT {
                            captured.pop_front();
                        }
                        captured.push_back(line);
                    }
                    Ok(None) => break,
                    Err(err) => {
                        let mut captured = lines.lock().await;
                        if captured.len() >= STDERR_LINE_LIMIT {
                            captured.pop_front();
                        }
                        captured.push_back(format!("failed to read stderr: {err}"));
                        break;
                    }
                }
            }
        });
    }

    async fn snapshot(&self) -> String {
        self.lines
            .lock()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl StdioSession {
    async fn start(config: &McpServerConfig) -> AppResult<Self> {
        let args = parse_args(&config.args_json)?;
        let env = parse_env(&config.env_json)?;
        let mut command = if cfg!(windows) {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C");
            cmd.arg(&config.command);
            cmd
        } else {
            Command::new(&config.command)
        };
        command.args(args);
        if !config.working_dir.trim().is_empty() {
            command.current_dir(config.working_dir.trim());
        }
        for (key, value) in env {
            command.env(key, value);
        }
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = command.spawn()?;
        let stderr = StdioStderrCapture::default();
        if let Some(child_stderr) = child.stderr.take() {
            stderr.spawn(child_stderr);
        }
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Message("failed to open mcp stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Message("failed to open mcp stdout".to_string()))?;
        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            stderr,
            request_id: 0,
            server_id: config.id.clone(),
            tools: Vec::new(),
        };
        session.initialize_with_fallback().await?;
        session
            .notify("notifications/initialized", json!({}))
            .await?;
        Ok(session)
    }

    async fn initialize_with_fallback(&mut self) -> AppResult<String> {
        let mut failures = Vec::new();
        for version in MCP_PROTOCOL_VERSIONS {
            match self.request("initialize", initialize_params(version)).await {
                Ok(result) => return Ok(negotiated_protocol_version(&result, version)),
                Err(err) => {
                    failures.push(format!("{version}: {err}"));
                }
            }
        }
        let stderr = self.stderr.snapshot().await;
        let detail = format_handshake_failures("stdio", &failures, Some(&stderr));
        Err(AppError::Message(detail))
    }

    async fn notify(&mut self, method: &str, params: Value) -> AppResult<()> {
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(message).await
    }

    async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        self.request_id += 1;
        let id = self.request_id;
        let message = json_rpc_request(id, method, params);
        self.write_message(message).await.map_err(|err| {
            AppError::Message(format!("mcp stdio write failed for {method}: {err}"))
        })?;

        loop {
            let line = match timeout(REQUEST_TIMEOUT, self.stdout.next_line()).await {
                Ok(Ok(line)) => line,
                Ok(Err(err)) => {
                    let stderr = self.stderr.snapshot().await;
                    return Err(AppError::Message(append_detail(
                        &format!("mcp stdio read failed for {method}: {err}"),
                        "stderr",
                        &stderr,
                    )));
                }
                Err(_) => {
                    let stderr = self.stderr.snapshot().await;
                    return Err(AppError::Message(append_detail(
                        &format!("mcp request timed out: {method}"),
                        "stderr",
                        &stderr,
                    )));
                }
            };
            let Some(line) = line else {
                let stderr = self.stderr.snapshot().await;
                return Err(AppError::Message(append_detail(
                    "mcp server closed stdout",
                    "stderr",
                    &stderr,
                )));
            };
            if let Some(result) = parse_json_rpc_line(&line, id)? {
                return result;
            }
        }
    }

    async fn write_message(&mut self, message: Value) -> AppResult<()> {
        self.stdin.write_all(message.to_string().as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

struct HttpSession {
    client: Client,
    server_id: String,
    url: String,
    headers: HeaderMap,
    session_id: Option<String>,
    protocol_version: String,
    request_id: u64,
    tools: Vec<McpToolInfo>,
}

impl HttpSession {
    async fn start(config: &McpServerConfig) -> AppResult<Self> {
        let mut session = Self {
            client: Client::new(),
            server_id: config.id.clone(),
            url: config.url.clone(),
            headers: parse_headers(&config.headers_json)?,
            session_id: None,
            protocol_version: MCP_PROTOCOL_VERSIONS[0].to_string(),
            request_id: 0,
            tools: Vec::new(),
        };
        session.initialize_with_fallback().await?;
        session
            .notification("notifications/initialized", json!({}))
            .await?;
        Ok(session)
    }

    async fn initialize_with_fallback(&mut self) -> AppResult<String> {
        let mut failures = Vec::new();
        for version in MCP_PROTOCOL_VERSIONS {
            self.protocol_version = (*version).to_string();
            match self.request("initialize", initialize_params(version)).await {
                Ok(result) => {
                    self.protocol_version = negotiated_protocol_version(&result, version);
                    return Ok(self.protocol_version.clone());
                }
                Err(err) => {
                    self.session_id = None;
                    failures.push(format!("{version}: {err}"));
                }
            }
        }
        Err(AppError::Message(format_handshake_failures(
            "streamable http",
            &failures,
            None,
        )))
    }

    async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        self.request_id += 1;
        let id = self.request_id;
        let response = self
            .post_json(json_rpc_request(id, method, params), true)
            .await?;
        self.capture_session_id(&response);
        response_to_json_rpc_result(response, id, method).await
    }

    async fn notification(&mut self, method: &str, params: Value) -> AppResult<()> {
        let response = self
            .post_json(
                json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": params,
                }),
                false,
            )
            .await?;
        if response.status() == StatusCode::ACCEPTED || response.status().is_success() {
            Ok(())
        } else {
            Err(AppError::Message(format!(
                "mcp notification failed: HTTP {}",
                response.status()
            )))
        }
    }

    async fn post_json(&self, body: Value, is_request: bool) -> AppResult<Response> {
        let mut request = self
            .client
            .post(&self.url)
            .headers(self.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .header("MCP-Protocol-Version", self.protocol_version.as_str());
        if is_request {
            request = request.header(ACCEPT, "application/json, text/event-stream");
        } else {
            request = request.header(ACCEPT, "application/json");
        }
        if let Some(session_id) = &self.session_id {
            request = request.header("Mcp-Session-Id", session_id);
        }
        let response = timeout(REQUEST_TIMEOUT, request.json(&body).send())
            .await
            .map_err(|_| AppError::Message("mcp http request timed out".to_string()))??;
        if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Message(format!(
                "mcp http request failed: HTTP {}{}",
                status,
                format_response_body(&body)
            )));
        }
        Ok(response)
    }

    fn capture_session_id(&mut self, response: &Response) {
        if let Some(value) = response.headers().get("Mcp-Session-Id") {
            if let Ok(value) = value.to_str() {
                self.session_id = Some(value.to_string());
            }
        }
    }

    async fn shutdown(self) -> AppResult<()> {
        let Some(session_id) = self.session_id else {
            return Ok(());
        };
        let _ = self
            .client
            .delete(&self.url)
            .headers(self.headers)
            .header("MCP-Protocol-Version", self.protocol_version)
            .header("Mcp-Session-Id", session_id)
            .send()
            .await;
        Ok(())
    }
}

struct SseSession {
    client: Client,
    server_id: String,
    post_url: String,
    headers: HeaderMap,
    stream: Response,
    stream_buffer: String,
    protocol_version: String,
    request_id: u64,
    tools: Vec<McpToolInfo>,
}

impl SseSession {
    async fn start(config: &McpServerConfig) -> AppResult<Self> {
        let client = Client::new();
        let headers = parse_headers(&config.headers_json)?;
        let mut response = client
            .get(&config.url)
            .headers(headers.clone())
            .header(ACCEPT, "text/event-stream")
            .header(ORIGIN, "tauri://localhost")
            .header(USER_AGENT, brand::DISPLAY_NAME)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AppError::Message(format!(
                "mcp sse connect failed: HTTP {}",
                response.status()
            )));
        }
        let mut buffer = String::new();
        let endpoint = loop {
            let event = timeout(REQUEST_TIMEOUT, read_sse_event(&mut response, &mut buffer))
                .await
                .map_err(|_| AppError::Message("mcp sse endpoint timed out".to_string()))??;
            if event.event.as_deref() == Some("endpoint") {
                let endpoint = event.data.trim();
                if endpoint.is_empty() {
                    return Err(AppError::Message(
                        "mcp sse endpoint event is empty".to_string(),
                    ));
                }
                break resolve_endpoint_url(&config.url, endpoint)?;
            }
        };
        let mut session = Self {
            client,
            server_id: config.id.clone(),
            post_url: endpoint,
            headers,
            stream: response,
            stream_buffer: buffer,
            protocol_version: MCP_PROTOCOL_VERSIONS[0].to_string(),
            request_id: 0,
            tools: Vec::new(),
        };
        session.initialize_with_fallback().await?;
        session
            .notification("notifications/initialized", json!({}))
            .await?;
        Ok(session)
    }

    async fn initialize_with_fallback(&mut self) -> AppResult<String> {
        let mut failures = Vec::new();
        for version in MCP_PROTOCOL_VERSIONS {
            self.protocol_version = (*version).to_string();
            match self.request("initialize", initialize_params(version)).await {
                Ok(result) => {
                    self.protocol_version = negotiated_protocol_version(&result, version);
                    return Ok(self.protocol_version.clone());
                }
                Err(err) => failures.push(format!("{version}: {err}")),
            }
        }
        Err(AppError::Message(format_handshake_failures(
            "sse", &failures, None,
        )))
    }

    async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        self.request_id += 1;
        let id = self.request_id;
        self.post_message(json_rpc_request(id, method, params))
            .await?;
        loop {
            let event = timeout(
                REQUEST_TIMEOUT,
                read_sse_event(&mut self.stream, &mut self.stream_buffer),
            )
            .await
            .map_err(|_| AppError::Message(format!("mcp sse request timed out: {method}")))??;
            if event
                .event
                .as_deref()
                .is_some_and(|event| event != "message")
            {
                continue;
            }
            if let Some(result) = parse_json_rpc_line(&event.data, id)? {
                return result;
            }
        }
    }

    async fn notification(&self, method: &str, params: Value) -> AppResult<()> {
        self.post_message(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    async fn post_message(&self, body: Value) -> AppResult<()> {
        let response = self
            .client
            .post(&self.post_url)
            .headers(self.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header("MCP-Protocol-Version", self.protocol_version.as_str())
            .json(&body)
            .send()
            .await?;
        if response.status().is_success() || response.status() == StatusCode::ACCEPTED {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(AppError::Message(format!(
                "mcp sse post failed: HTTP {}{}",
                status,
                format_response_body(&body)
            )))
        }
    }
}

#[derive(Default)]
struct SseEvent {
    event: Option<String>,
    data: String,
}

async fn read_sse_event(response: &mut Response, buffer: &mut String) -> AppResult<SseEvent> {
    loop {
        if let Some((event_text, rest)) = take_sse_event(buffer) {
            *buffer = rest;
            let event = parse_sse_event(&event_text);
            if !event.data.trim().is_empty() {
                return Ok(event);
            }
        }
        let chunk = response
            .chunk()
            .await?
            .ok_or_else(|| AppError::Message("mcp sse stream closed".to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
    }
}

fn take_sse_event(buffer: &str) -> Option<(String, String)> {
    let normalized = buffer.replace("\r\n", "\n");
    let index = normalized.find("\n\n")?;
    let event = normalized[..index].to_string();
    let rest = normalized[index + 2..].to_string();
    Some((event, rest))
}

fn parse_sse_event(text: &str) -> SseEvent {
    let mut event = SseEvent::default();
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event.event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string());
        }
    }
    event.data = data.join("\n");
    event
}

async fn response_to_json_rpc_result(
    response: Response,
    id: u64,
    method: &str,
) -> AppResult<Value> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    if content_type.contains("text/event-stream") {
        let text = response.text().await?;
        let event = sse_text_events(&text)
            .into_iter()
            .find(|event| {
                parse_json_rpc_line(&event.data, id)
                    .ok()
                    .flatten()
                    .is_some()
            })
            .ok_or_else(|| {
                AppError::Message(format!(
                    "mcp streamable http response missing id for {method}"
                ))
            })?;
        parse_json_rpc_line(&event.data, id)?.ok_or_else(|| {
            AppError::Message(format!(
                "mcp streamable http response missing id for {method}"
            ))
        })?
    } else {
        let text = response.text().await?;
        parse_json_rpc_line(&text, id)?.ok_or_else(|| {
            AppError::Message(format!("mcp http response missing id for {method}"))
        })?
    }
}

fn sse_text_events(text: &str) -> Vec<SseEvent> {
    let mut buffer = text.replace("\r\n", "\n");
    let mut events = Vec::new();
    while let Some((event_text, rest)) = take_sse_event(&buffer) {
        buffer = rest;
        let event = parse_sse_event(&event_text);
        if !event.data.trim().is_empty() {
            events.push(event);
        }
    }
    events
}

fn parse_json_rpc_line(line: &str, id: u64) -> AppResult<Option<AppResult<Value>>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let response: Value = serde_json::from_str(trimmed)?;
    if response.get("id").and_then(Value::as_u64) != Some(id) {
        return Ok(None);
    }
    if let Some(error) = response.get("error") {
        return Ok(Some(Err(AppError::Message(format!("mcp error: {error}")))));
    }
    Ok(Some(Ok(response
        .get("result")
        .cloned()
        .unwrap_or_else(|| json!({})))))
}

fn json_rpc_request(id: u64, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

fn initialize_params(protocol_version: &str) -> Value {
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {},
        "clientInfo": {
            "name": brand::DISPLAY_NAME,
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn negotiated_protocol_version(result: &Value, fallback: &str) -> String {
    result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn format_handshake_failures(transport: &str, failures: &[String], stderr: Option<&str>) -> String {
    let detail = if failures.is_empty() {
        "no protocol attempts were made".to_string()
    } else {
        failures.join("\n")
    };
    let message = format!(
        "mcp {transport} handshake failed after trying protocol versions {}:\n{detail}",
        MCP_PROTOCOL_VERSIONS.join(", ")
    );
    append_detail(&message, "stderr", stderr.unwrap_or(""))
}

fn append_detail(message: &str, label: &str, detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        message.to_string()
    } else {
        format!("{message}\n\n{label}:\n{}", truncate_detail(detail, 4000))
    }
}

fn format_response_body(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        String::new()
    } else {
        format!("\nresponse body:\n{}", truncate_detail(body, 2000))
    }
}

fn truncate_detail(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}\n... truncated ...")
    } else {
        truncated
    }
}

fn parse_tools(server_id: &str, result: Value) -> Vec<McpToolInfo> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.to_string();
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let input_schema_json = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object" }))
                .to_string();
            Some(McpToolInfo {
                server_id: server_id.to_string(),
                name,
                description,
                input_schema_json,
            })
        })
        .collect()
}

fn parse_args(args_json: &str) -> AppResult<Vec<String>> {
    let trimmed = args_json.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_str(trimmed)?;
    let args = value
        .as_array()
        .ok_or_else(|| AppError::Message("mcp args_json must be an array".to_string()))?;
    Ok(args
        .iter()
        .map(|arg| match arg {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        })
        .collect())
}

fn parse_env(env_json: &str) -> AppResult<HashMap<String, String>> {
    let trimmed = env_json.trim();
    if trimmed.is_empty() {
        return Ok(HashMap::new());
    }
    let value: Value = serde_json::from_str(trimmed)?;
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Message("mcp env_json must be an object".to_string()))?;
    Ok(object
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value.clone(),
                other => other.to_string(),
            };
            (key.clone(), value)
        })
        .collect())
}

fn parse_headers(headers_json: &str) -> AppResult<HeaderMap> {
    let trimmed = headers_json.trim();
    let mut headers = HeaderMap::new();
    if trimmed.is_empty() {
        return Ok(headers);
    }
    let value: Value = serde_json::from_str(trimmed)?;
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Message("mcp headers_json must be an object".to_string()))?;
    for (key, value) in object {
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|err| AppError::Message(format!("invalid header name {key}: {err}")))?;
        let value = match value {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        };
        let value = HeaderValue::from_str(&value)
            .map_err(|err| AppError::Message(format!("invalid header value for {key}: {err}")))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn parse_arguments(arguments_json: &str) -> AppResult<Value> {
    let trimmed = arguments_json.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(trimmed)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(AppError::Message(
            "mcp tool arguments must be a JSON object".to_string(),
        ))
    }
}

fn resolve_endpoint_url(base_url: &str, endpoint: &str) -> AppResult<String> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return Ok(endpoint.to_string());
    }
    let base = reqwest::Url::parse(base_url)
        .map_err(|err| AppError::Message(format!("invalid mcp sse url: {err}")))?;
    base.join(endpoint)
        .map(|url| url.to_string())
        .map_err(|err| AppError::Message(format!("invalid mcp sse endpoint: {err}")))
}
