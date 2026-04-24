use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use orchestrator_plugin_protocol::McpTool;
use serde_json::Value;
use tokio::process::{ChildStdin, ChildStdout};

use crate::{DiscoveredPlugin, PluginDiscovery, PluginHost};

pub struct PluginRegistry {
    discovered: HashMap<String, DiscoveredPlugin>,
    running: HashMap<String, PluginHost<ChildStdout, ChildStdin>>,
    mcp_tools: HashMap<String, (String, McpTool)>,
}

impl PluginRegistry {
    pub fn discover(project_root: impl Into<PathBuf>) -> Result<Self> {
        let discovered = PluginDiscovery::new()
            .with_project_root(project_root)
            .discover()?
            .into_iter()
            .map(|plugin| (plugin.name.clone(), plugin))
            .collect();

        Ok(Self { discovered, running: HashMap::new(), mcp_tools: HashMap::new() })
    }

    pub fn list_plugins(&self) -> impl Iterator<Item = &DiscoveredPlugin> {
        self.discovered.values()
    }

    pub fn is_running(&self, name: &str) -> bool {
        self.running.contains_key(name)
    }

    pub async fn get_plugin(&mut self, name: &str) -> Result<&mut PluginHost<ChildStdout, ChildStdin>> {
        if !self.running.contains_key(name) {
            let path = self.discovered.get(name).ok_or_else(|| anyhow!("unknown plugin '{name}'"))?.path.clone();
            let mut host = PluginHost::spawn(&path, &[]).await?;
            let result = host.handshake().await?;
            self.register_mcp_tools(name, result.capabilities.mcp_tools)?;
            self.running.insert(name.to_string(), host);
        }

        self.running.get_mut(name).ok_or_else(|| anyhow!("plugin '{name}' was not available after startup"))
    }

    pub async fn initialize_all(&mut self) -> Result<()> {
        let names = self.discovered.keys().cloned().collect::<Vec<_>>();
        for name in names {
            self.get_plugin(&name).await?;
        }
        Ok(())
    }

    pub fn mcp_tools(&self) -> impl Iterator<Item = &McpTool> {
        self.mcp_tools.values().map(|(_, tool)| tool)
    }

    pub fn mcp_tool_owner(&self, tool_name: &str) -> Option<&str> {
        self.mcp_tools.get(tool_name).map(|(owner, _)| owner.as_str())
    }

    pub async fn call_mcp_tool(&mut self, tool_name: &str, arguments: Value) -> Result<Value> {
        let owner = self
            .mcp_tools
            .get(tool_name)
            .map(|(owner, _)| owner.clone())
            .ok_or_else(|| anyhow!("no plugin owns MCP tool '{tool_name}'"))?;
        let host = self.get_plugin(&owner).await?;
        host.request(
            "mcp/tool_call",
            Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            })),
        )
        .await
        .map_err(|error| anyhow!("plugin MCP tool call failed ({}): {}", error.code, error.message))
    }

    pub async fn shutdown_all(&mut self) -> Result<()> {
        let running = std::mem::take(&mut self.running);
        for (_, host) in running {
            if let Err(error) = host.shutdown().await {
                tracing::warn!(%error, "failed to shut down plugin");
            }
        }
        self.mcp_tools.clear();
        Ok(())
    }

    fn register_mcp_tools(&mut self, owner: &str, tools: Vec<McpTool>) -> Result<()> {
        for tool in tools {
            if let Some((existing_owner, _)) = self.mcp_tools.get(&tool.name) {
                if existing_owner != owner {
                    return Err(anyhow!(
                        "duplicate MCP tool '{}' registered by '{}' and '{}'",
                        tool.name,
                        existing_owner,
                        owner
                    ));
                }
            }
            self.mcp_tools.insert(tool.name.clone(), (owner.to_string(), tool));
        }
        Ok(())
    }
}
