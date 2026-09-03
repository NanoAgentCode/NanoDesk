use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::brand;
use crate::core::plugin::{AgentToolDefinition, AppPlugin, PluginManifest, PluginRegistry};
use crate::error::{AppError, AppResult};
use crate::tool_policy;

pub fn built_in_registry() -> AppResult<PluginRegistry> {
    PluginRegistry::new(vec![
        Box::new(CoreAgentToolsPlugin),
        Box::new(McpToolsPlugin),
    ])
}

struct CoreAgentToolsPlugin;

impl AppPlugin for CoreAgentToolsPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: format!("{}.core-tools", brand::PLUGIN_NAMESPACE),
            name: "Core Agent Tools".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec!["agent-tools".to_string()],
        }
    }

    fn agent_tool_definitions(&self) -> Vec<AgentToolDefinition> {
        tool_policy::built_in_tool_policies()
            .into_iter()
            .map(|policy| AgentToolDefinition {
                name: policy.name.to_string(),
                description: policy.description.to_string(),
                risk: policy.risk.to_string(),
                requires_approval: policy.requires_approval,
                parameters_json: tool_parameters_json(policy.name).to_string(),
            })
            .collect()
    }

    fn owns_agent_tool(&self, name: &str) -> bool {
        matches!(
            name,
            "read_file" | "write_file" | "execute_command" | "ocr_image"
        )
    }

    fn validate_agent_tool_args(
        &self,
        name: &str,
        args: &BTreeMap<String, String>,
    ) -> AppResult<()> {
        match name {
            "read_file" => require_arg(args, "path").map(|_| ()),
            "write_file" => {
                require_arg(args, "path")?;
                require_arg(args, "content")?;
                Ok(())
            }
            "execute_command" => require_arg(args, "command").map(|_| ()),
            "ocr_image" => {
                require_arg(args, "path")?;
                if let Some(output_format) = args.get("output_format") {
                    let output_format = output_format.trim();
                    if !output_format.is_empty()
                        && output_format != "text"
                        && output_format != "raw"
                    {
                        return Err(AppError::Message(
                            "ocr_image output_format must be text or raw".to_string(),
                        ));
                    }
                }
                Ok(())
            }
            _ => Err(AppError::Message(format!("unknown tool: {name}"))),
        }
    }
}

struct McpToolsPlugin;

impl AppPlugin for McpToolsPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: format!("{}.mcp-tools", brand::PLUGIN_NAMESPACE),
            name: "MCP Tool Adapter".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec!["agent-tools".to_string(), "mcp".to_string()],
        }
    }

    fn owns_agent_tool(&self, name: &str) -> bool {
        name.starts_with("mcp__")
    }

    fn validate_agent_tool_args(
        &self,
        name: &str,
        _args: &BTreeMap<String, String>,
    ) -> AppResult<()> {
        if self.owns_agent_tool(name) {
            Ok(())
        } else {
            Err(AppError::Message(format!("unknown tool: {name}")))
        }
    }
}

fn tool_parameters_json(name: &str) -> Value {
    match name {
        "read_file" => json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "Project-relative file path." }
            }
        }),
        "write_file" => json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string", "description": "Project-relative file path." },
                "content": { "type": "string", "description": "Complete file contents to write." }
            }
        }),
        "execute_command" => json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command line to execute in the active project directory."
                }
            }
        }),
        "ocr_image" => json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "Project-relative image path." },
                "output_format": {
                    "type": "string",
                    "enum": ["text", "raw"],
                    "description": "Return compact recognized text or raw PaddleOCR output. Defaults to text."
                }
            }
        }),
        _ => json!({ "type": "object" }),
    }
}

fn require_arg<'a>(args: &'a BTreeMap<String, String>, name: &str) -> AppResult<&'a str> {
    args.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Message(format!("missing tool argument: {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_registry_exposes_core_tool_definitions() {
        let registry = built_in_registry().expect("built-in plugins should register");
        let names = registry
            .agent_tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["read_file", "write_file", "execute_command", "ocr_image"]
        );
    }

    #[test]
    fn mcp_plugin_owns_only_mcp_namespace() {
        let registry = built_in_registry().expect("built-in plugins should register");
        assert!(registry.owns_agent_tool("mcp__server__tool"));
        assert!(!registry.owns_agent_tool("unknown_tool"));
    }
}
