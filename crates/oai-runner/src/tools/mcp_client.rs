use anyhow::{Context, Result};
use rmcp::model::{CallToolRequestParams, RawContent};
use rmcp::service::RunningService;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::streamable_http_client::{StreamableHttpClientTransport, StreamableHttpClientTransportConfig};
use rmcp::{RoleClient, ServiceExt};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::process::Command;

use crate::api::types::{FunctionSchema, ToolDefinition};

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// HTTP endpoint URL. When set, uses HTTP/SSE transport instead of stdio.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub auth_token_env: Option<String>,
    /// Transport type hint ("stdio" or "http"). Presence of `url` takes precedence.
    #[serde(default)]
    pub transport: Option<String>,
}

pub struct McpClient {
    service: RunningService<RoleClient, ()>,
    tool_names: Vec<String>,
}

pub async fn connect(config: &McpServerConfig) -> Result<McpClient> {
    // Use HTTP transport when a URL is provided or transport is explicitly "http".
    let use_http = config.url.is_some() || config.transport.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("http"));

    if use_http {
        let url = config
            .url
            .as_deref()
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("HTTP MCP server config is missing 'url'"))?;
        let client = {
            let mut builder = reqwest::Client::builder();
            if let Some(auth_token) = resolve_http_auth_token(config)? {
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&auth_token)
                        .map_err(|e| anyhow::anyhow!("invalid MCP auth token header: {}", e))?,
                );
                builder = builder.default_headers(headers);
            }
            builder.build().map_err(|e| anyhow::anyhow!("failed to build MCP HTTP client: {}", e))?
        };
        let transport =
            StreamableHttpClientTransport::with_client(client, StreamableHttpClientTransportConfig::with_uri(url));
        let service: RunningService<RoleClient, ()> =
            ().serve(transport).await.map_err(|e| anyhow::anyhow!("failed to initialize HTTP MCP session: {}", e))?;
        return Ok(McpClient { service, tool_names: Vec::new() });
    }

    let mut cmd = Command::new(&config.command);
    for arg in &config.args {
        cmd.arg(arg);
    }
    if !config.env.is_empty() {
        cmd.envs(config.env.iter());
    }

    let transport = TokioChildProcess::new(cmd).context("failed to spawn MCP server process")?;

    let service: RunningService<RoleClient, ()> =
        ().serve(transport).await.map_err(|e| anyhow::anyhow!("failed to initialize MCP session: {}", e))?;

    Ok(McpClient { service, tool_names: Vec::new() })
}

fn resolve_http_auth_token(config: &McpServerConfig) -> Result<Option<String>> {
    if let Some(auth_token) = config.auth_token.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(Some(auth_token.to_string()));
    }

    let Some(env_key) = config.auth_token_env.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let auth_token =
        std::env::var(env_key).map_err(|_| anyhow::anyhow!("MCP auth token env var '{}' is not set", env_key))?;
    let auth_token = auth_token.trim();
    if auth_token.is_empty() {
        anyhow::bail!("MCP auth token env var '{}' is empty", env_key);
    }
    Ok(Some(auth_token.to_string()))
}

pub async fn connect_all(configs: &[McpServerConfig]) -> Result<Vec<McpClient>> {
    let mut clients = Vec::with_capacity(configs.len());
    for config in configs {
        clients.push(connect(config).await?);
    }
    Ok(clients)
}

pub async fn fetch_tool_definitions(client: &mut McpClient) -> Result<Vec<ToolDefinition>> {
    let tools =
        client.service.peer().list_all_tools().await.map_err(|e| anyhow::anyhow!("failed to list MCP tools: {}", e))?;

    let mut defs = Vec::new();
    for tool in &tools {
        client.tool_names.push(tool.name.to_string());
        defs.push(mcp_tool_to_openai(tool));
    }
    Ok(defs)
}

pub async fn fetch_all_tool_definitions(clients: &mut [McpClient]) -> Result<Vec<ToolDefinition>> {
    let mut all_defs = Vec::new();
    for client in clients.iter_mut() {
        all_defs.extend(fetch_tool_definitions(client).await?);
    }
    Ok(all_defs)
}

fn mcp_tool_to_openai(tool: &rmcp::model::Tool) -> ToolDefinition {
    let input_schema: &Arc<serde_json::Map<String, serde_json::Value>> = &tool.input_schema;
    let parameters = serde_json::Value::Object((**input_schema).clone());

    ToolDefinition {
        type_: "function".to_string(),
        function: FunctionSchema {
            name: tool.name.to_string(),
            description: tool.description.as_deref().unwrap_or("").to_string(),
            parameters,
        },
    }
}

pub fn find_client_for_tool<'a>(clients: &'a [McpClient], name: &str) -> Option<&'a McpClient> {
    clients.iter().find(|c| c.tool_names.iter().any(|n| n == name))
}

pub async fn call_tool(client: &McpClient, name: &str, args_json: &str) -> Result<String> {
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Object(Default::default()));

    let arguments = match args {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    };

    let mut params = CallToolRequestParams::new(Cow::Owned(name.to_string()));
    if let Some(args) = arguments {
        params = params.with_arguments(args);
    }

    let result = client
        .service
        .peer()
        .call_tool(params)
        .await
        .map_err(|e| anyhow::anyhow!("MCP tool call failed for {}: {}", name, e))?;

    let text_parts: Vec<String> = result
        .content
        .iter()
        .filter_map(|content| match &content.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();

    if result.is_error.unwrap_or(false) {
        anyhow::bail!("MCP tool error: {}", text_parts.join("\n"));
    }

    Ok(text_parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_tool_to_openai_converts_basic_tool() {
        let schema = Arc::new(serde_json::Map::from_iter([
            ("type".to_string(), serde_json::json!("object")),
            (
                "properties".to_string(),
                serde_json::json!({
                    "query": { "type": "string", "description": "Search query" }
                }),
            ),
            ("required".to_string(), serde_json::json!(["query"])),
        ]));

        let tool = rmcp::model::Tool::new("search", "Search for files", schema);

        let def = mcp_tool_to_openai(&tool);
        assert_eq!(def.type_, "function");
        assert_eq!(def.function.name, "search");
        assert_eq!(def.function.description, "Search for files");
        assert_eq!(def.function.parameters["type"], "object");
        assert!(def.function.parameters["properties"]["query"].is_object());
    }

    #[test]
    fn mcp_tool_to_openai_handles_empty_description() {
        let schema = Arc::new(serde_json::Map::from_iter([
            ("type".to_string(), serde_json::json!("object")),
            ("properties".to_string(), serde_json::json!({})),
        ]));

        let tool = rmcp::model::Tool::new_with_raw("noop", None, schema);

        let def = mcp_tool_to_openai(&tool);
        assert_eq!(def.function.description, "");
    }

    #[test]
    fn resolve_http_auth_token_prefers_env_reference_without_serialized_secret() {
        let config = McpServerConfig {
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            url: Some("https://primary.example/mcp".to_string()),
            auth_token: None,
            auth_token_env: Some("AO_OAI_RUNNER_MCP_AUTH_TOKEN_TEST".to_string()),
            transport: Some("http".to_string()),
        };

        unsafe {
            std::env::set_var("AO_OAI_RUNNER_MCP_AUTH_TOKEN_TEST", "Bearer primary");
        }
        let token = resolve_http_auth_token(&config).expect("env token should resolve");
        assert_eq!(token.as_deref(), Some("Bearer primary"));
        unsafe {
            std::env::remove_var("AO_OAI_RUNNER_MCP_AUTH_TOKEN_TEST");
        }
    }
}
