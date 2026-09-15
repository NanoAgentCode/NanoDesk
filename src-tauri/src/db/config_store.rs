use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{
    clean_mcp_transport, clean_ops_auth_method, clean_optional_string, clean_or_default,
    ensure_affected, parse_time, validate_json_array_or_empty, validate_json_object_or_empty,
    Database,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    McpServerConfig, McpServerDraft, ModelConfig, ModelConfigDraft, OpsServer, OpsServerDraft,
};

impl Database {
    pub fn list_model_configs(&self) -> AppResult<Vec<ModelConfig>> {
        let mut stmt = self.config_conn.prepare(
            "
            SELECT id, name, provider, base_url, model, api_key,
                   temperature, max_tokens, context_window, top_p, reasoning_effort,
                   routing_group, routing_enabled, routing_cost, routing_quality, routing_speed, routing_tasks_json,
                   embedding_provider, embedding_base_url, embedding_model, embedding_api_key,
                   created_at, updated_at
            FROM model_configs
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_model_config)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_model_config(&self, id: &str) -> AppResult<ModelConfig> {
        self.config_conn
            .query_row(
                "
                SELECT id, name, provider, base_url, model, api_key,
                       temperature, max_tokens, context_window, top_p, reasoning_effort,
                       routing_group, routing_enabled, routing_cost, routing_quality, routing_speed, routing_tasks_json,
                       embedding_provider, embedding_base_url, embedding_model, embedding_api_key,
                       created_at, updated_at
                FROM model_configs WHERE id = ?1
                ",
                params![id],
                Self::row_to_model_config,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("model config not found".to_string()))
    }

    pub fn save_model_config(&self, draft: ModelConfigDraft) -> AppResult<ModelConfig> {
        let now = Utc::now();
        let id = draft.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = self
            .config_conn
            .query_row(
                "SELECT created_at FROM model_configs WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_time(&value))
            .transpose()?
            .unwrap_or(now);

        let config = ModelConfig {
            id,
            name: clean_or_default(draft.name, "默认模型"),
            provider: clean_or_default(draft.provider, "openai-compatible"),
            base_url: clean_or_default(draft.base_url, "https://api.openai.com/v1"),
            model: clean_or_default(draft.model, "gpt-4o-mini"),
            api_key: draft.api_key,
            temperature: validate_temperature(draft.temperature)?,
            max_tokens: validate_max_tokens(draft.max_tokens)?,
            context_window: validate_context_window(draft.context_window, draft.max_tokens)?,
            top_p: validate_top_p(draft.top_p)?,
            reasoning_effort: validate_reasoning_effort(draft.reasoning_effort)?,
            routing_group: clean_or_default(draft.routing_group, "默认组"),
            routing_enabled: draft.routing_enabled,
            routing_cost: validate_routing_score(draft.routing_cost)?,
            routing_quality: validate_routing_score(draft.routing_quality)?,
            routing_speed: validate_routing_score(draft.routing_speed)?,
            routing_tasks: validate_routing_tasks(draft.routing_tasks)?,
            embedding_provider: clean_or_default(draft.embedding_provider, "openai-compatible"),
            embedding_base_url: clean_optional_string(draft.embedding_base_url),
            embedding_model: clean_or_default(draft.embedding_model, "text-embedding-3-small"),
            embedding_api_key: clean_optional_string(draft.embedding_api_key),
            created_at,
            updated_at: now,
        };

        self.config_conn.execute(
            "
            INSERT INTO model_configs
                (id, name, provider, base_url, model, api_key,
                 temperature, max_tokens, context_window, top_p, reasoning_effort,
                 routing_group, routing_enabled, routing_cost, routing_quality, routing_speed, routing_tasks_json,
                 embedding_provider, embedding_base_url, embedding_model, embedding_api_key,
                 created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                base_url = excluded.base_url,
                model = excluded.model,
                api_key = excluded.api_key,
                temperature = excluded.temperature,
                max_tokens = excluded.max_tokens,
                context_window = excluded.context_window,
                top_p = excluded.top_p,
                reasoning_effort = excluded.reasoning_effort,
                routing_group = excluded.routing_group,
                routing_enabled = excluded.routing_enabled,
                routing_cost = excluded.routing_cost,
                routing_quality = excluded.routing_quality,
                routing_speed = excluded.routing_speed,
                routing_tasks_json = excluded.routing_tasks_json,
                embedding_provider = excluded.embedding_provider,
                embedding_base_url = excluded.embedding_base_url,
                embedding_model = excluded.embedding_model,
                embedding_api_key = excluded.embedding_api_key,
                updated_at = excluded.updated_at
            ",
            params![
                config.id,
                config.name,
                config.provider,
                config.base_url,
                config.model,
                config.api_key,
                config.temperature,
                config.max_tokens,
                config.context_window,
                config.top_p,
                config.reasoning_effort,
                config.routing_group,
                if config.routing_enabled { 1 } else { 0 },
                config.routing_cost,
                config.routing_quality,
                config.routing_speed,
                serde_json::to_string(&config.routing_tasks)?,
                config.embedding_provider,
                config.embedding_base_url,
                config.embedding_model,
                config.embedding_api_key,
                config.created_at.to_rfc3339(),
                config.updated_at.to_rfc3339()
            ],
        )?;

        self.sync_model_config_references()?;
        Ok(config)
    }

    pub fn delete_model_config(&self, id: &str) -> AppResult<()> {
        let affected = self
            .config_conn
            .execute("DELETE FROM model_configs WHERE id = ?1", params![id])?;
        ensure_affected(affected, "model config not found")?;
        self.sync_model_config_references()?;
        Ok(())
    }

    pub fn list_mcp_servers(&self) -> AppResult<Vec<McpServerConfig>> {
        let mut stmt = self.config_conn.prepare(
            "
            SELECT id, name, transport, command, args_json, env_json, url, headers_json,
                   working_dir, enabled, created_at, updated_at
            FROM mcp_servers
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_mcp_server)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_mcp_server(&self, id: &str) -> AppResult<McpServerConfig> {
        self.config_conn
            .query_row(
                "
                SELECT id, name, transport, command, args_json, env_json, url, headers_json,
                       working_dir, enabled, created_at, updated_at
                FROM mcp_servers WHERE id = ?1
                ",
                params![id],
                Self::row_to_mcp_server,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("mcp server not found".to_string()))
    }

    pub fn save_mcp_server(&self, draft: McpServerDraft) -> AppResult<McpServerConfig> {
        let now = Utc::now();
        let id = draft.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = self
            .config_conn
            .query_row(
                "SELECT created_at FROM mcp_servers WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_time(&value))
            .transpose()?
            .unwrap_or(now);

        validate_json_array_or_empty(&draft.args_json, "args_json")?;
        validate_json_object_or_empty(&draft.env_json, "env_json")?;
        validate_json_object_or_empty(&draft.headers_json, "headers_json")?;

        let server = McpServerConfig {
            id,
            name: clean_or_default(draft.name, "MCP Server"),
            transport: clean_mcp_transport(draft.transport)?,
            command: clean_optional_string(draft.command),
            args_json: clean_or_default(draft.args_json, "[]"),
            env_json: clean_or_default(draft.env_json, "{}"),
            url: clean_optional_string(draft.url),
            headers_json: clean_or_default(draft.headers_json, "{}"),
            working_dir: clean_optional_string(draft.working_dir),
            enabled: draft.enabled,
            created_at,
            updated_at: now,
        };
        if server.transport == "stdio" && server.command.is_empty() {
            return Err(AppError::Message(
                "mcp server command is required".to_string(),
            ));
        }
        if server.transport != "stdio" && server.url.is_empty() {
            return Err(AppError::Message("mcp server url is required".to_string()));
        }

        self.config_conn.execute(
            "
            INSERT INTO mcp_servers
                (id, name, transport, command, args_json, env_json, url, headers_json,
                 working_dir, enabled, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                transport = excluded.transport,
                command = excluded.command,
                args_json = excluded.args_json,
                env_json = excluded.env_json,
                url = excluded.url,
                headers_json = excluded.headers_json,
                working_dir = excluded.working_dir,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            ",
            params![
                server.id,
                server.name,
                server.transport,
                server.command,
                server.args_json,
                server.env_json,
                server.url,
                server.headers_json,
                server.working_dir,
                if server.enabled { 1 } else { 0 },
                server.created_at.to_rfc3339(),
                server.updated_at.to_rfc3339()
            ],
        )?;

        Ok(server)
    }

    pub fn delete_mcp_server(&self, id: &str) -> AppResult<()> {
        let affected = self
            .config_conn
            .execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        ensure_affected(affected, "mcp server not found")?;
        Ok(())
    }

    pub fn list_ops_servers(&self) -> AppResult<Vec<OpsServer>> {
        let mut stmt = self.config_conn.prepare(
            "
            SELECT id, name, host, port, username, auth_method, key_path, password,
                   remote_dir, created_at, updated_at
            FROM ops_servers
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_ops_server)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_ops_server(&self, id: &str) -> AppResult<OpsServer> {
        self.config_conn
            .query_row(
                "
                SELECT id, name, host, port, username, auth_method, key_path, password,
                       remote_dir, created_at, updated_at
                FROM ops_servers WHERE id = ?1
                ",
                params![id],
                Self::row_to_ops_server,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("server not found".to_string()))
    }

    pub fn save_ops_server(&self, draft: OpsServerDraft) -> AppResult<OpsServer> {
        let now = Utc::now();
        let id = draft.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = self
            .config_conn
            .query_row(
                "SELECT created_at FROM ops_servers WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_time(&value))
            .transpose()?
            .unwrap_or(now);

        let port = draft.port.unwrap_or(22).clamp(1, 65535);
        let server = OpsServer {
            id,
            name: clean_or_default(draft.name, "未命名服务器"),
            host: clean_optional_string(draft.host),
            port,
            username: clean_optional_string(draft.username),
            auth_method: clean_ops_auth_method(draft.auth_method)?,
            key_path: clean_optional_string(draft.key_path),
            password: draft.password,
            remote_dir: clean_optional_string(draft.remote_dir),
            created_at,
            updated_at: now,
        };

        if server.host.is_empty() {
            return Err(AppError::Message("服务器地址不能为空".to_string()));
        }
        if server.username.is_empty() {
            return Err(AppError::Message("用户名不能为空".to_string()));
        }

        self.config_conn.execute(
            "
            INSERT INTO ops_servers
                (id, name, host, port, username, auth_method, key_path, password,
                 remote_dir, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                port = excluded.port,
                username = excluded.username,
                auth_method = excluded.auth_method,
                key_path = excluded.key_path,
                password = excluded.password,
                remote_dir = excluded.remote_dir,
                updated_at = excluded.updated_at
            ",
            params![
                server.id,
                server.name,
                server.host,
                server.port,
                server.username,
                server.auth_method,
                server.key_path,
                server.password,
                server.remote_dir,
                server.created_at.to_rfc3339(),
                server.updated_at.to_rfc3339()
            ],
        )?;

        Ok(server)
    }

    pub fn delete_ops_server(&self, id: &str) -> AppResult<()> {
        let affected = self
            .config_conn
            .execute("DELETE FROM ops_servers WHERE id = ?1", params![id])?;
        ensure_affected(affected, "server not found")?;
        Ok(())
    }
}

fn validate_temperature(value: f32) -> AppResult<f32> {
    if value.is_finite() && (0.0..=2.0).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::Message(
            "Temperature 必须在 0 到 2 之间".to_string(),
        ))
    }
}

fn validate_max_tokens(value: Option<u32>) -> AppResult<Option<u32>> {
    match value {
        Some(0) => Err(AppError::Message("最大输出 Token 必须大于 0".to_string())),
        _ => Ok(value),
    }
}

fn validate_context_window(value: u32, max_tokens: Option<u32>) -> AppResult<u32> {
    if value < 2_048 {
        return Err(AppError::Message(
            "上下文窗口不能小于 2048 Token".to_string(),
        ));
    }
    if max_tokens.is_some_and(|max_tokens| max_tokens >= value) {
        return Err(AppError::Message(
            "最大输出 Token 必须小于上下文窗口".to_string(),
        ));
    }
    Ok(value)
}

fn validate_top_p(value: Option<f32>) -> AppResult<Option<f32>> {
    match value {
        Some(value) if !value.is_finite() || !(0.0..=1.0).contains(&value) => {
            Err(AppError::Message("Top P 必须在 0 到 1 之间".to_string()))
        }
        _ => Ok(value),
    }
}

fn validate_reasoning_effort(value: String) -> AppResult<String> {
    let value = value.trim().to_lowercase();
    if matches!(value.as_str(), "" | "low" | "medium" | "high") {
        Ok(value)
    } else {
        Err(AppError::Message(
            "Reasoning Effort 必须为空、low、medium 或 high".to_string(),
        ))
    }
}

fn validate_routing_score(value: u8) -> AppResult<u8> {
    if (1..=5).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::Message("路由评分必须在 1 到 5 之间".to_string()))
    }
}

fn validate_routing_tasks(values: Vec<String>) -> AppResult<Vec<String>> {
    const ALLOWED: &[&str] = &[
        "general",
        "coding",
        "reasoning",
        "writing",
        "translation",
        "summary",
        "vision",
    ];
    let mut result = Vec::new();
    for value in values {
        let value = value.trim().to_lowercase();
        if !ALLOWED.contains(&value.as_str()) {
            return Err(AppError::Message(format!("不支持的路由任务类型：{value}")));
        }
        if !result.contains(&value) {
            result.push(value);
        }
    }
    Ok(result)
}
