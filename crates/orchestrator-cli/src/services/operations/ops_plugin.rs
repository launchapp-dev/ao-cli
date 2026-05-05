use std::path::Path;

use anyhow::{anyhow, Context, Result};
use orchestrator_plugin_host::{
    discover_plugins, DiscoveredPlugin, DiscoverySource, PluginDiscovery, PluginHost,
};
use orchestrator_plugin_protocol::PluginManifest;
use serde::Serialize;
use serde_json::Value;

use crate::{
    invalid_input_error, not_found_error, print_value, PluginCallArgs, PluginCommand, PluginInfoArgs, PluginListArgs,
    PluginPingArgs,
};

#[derive(Debug, Serialize)]
struct DiscoveredPluginRow {
    name: String,
    version: String,
    plugin_kind: String,
    description: String,
    protocol_version: String,
    capabilities: Vec<String>,
    source: &'static str,
    path: String,
}

#[derive(Debug, Serialize)]
struct PluginInfoOutput {
    name: String,
    source: &'static str,
    path: String,
    manifest: PluginManifest,
    initialize: Value,
}

#[derive(Debug, Serialize)]
struct PluginCallOutput {
    name: String,
    method: String,
    result: Value,
}

#[derive(Debug, Serialize)]
struct PluginPingOutput {
    name: String,
    ok: bool,
    plugin_info: Value,
}

pub(crate) async fn handle_plugin(command: PluginCommand, project_root: &str, json: bool) -> Result<()> {
    match command {
        PluginCommand::List(args) => handle_plugin_list(args, project_root, json),
        PluginCommand::Info(args) => handle_plugin_info(args, project_root, json).await,
        PluginCommand::Call(args) => handle_plugin_call(args, project_root, json).await,
        PluginCommand::Ping(args) => handle_plugin_ping(args, project_root, json).await,
    }
}

fn discover(project_root: &str, include_system_path: bool) -> Result<Vec<DiscoveredPlugin>> {
    PluginDiscovery::new()
        .with_project_root(Path::new(project_root))
        .include_system_path(include_system_path)
        .discover()
        .context("plugin discovery failed")
}

fn source_label(source: DiscoverySource) -> &'static str {
    match source {
        DiscoverySource::ExplicitConfig => "explicit_config",
        DiscoverySource::ProjectLocal => "project_local",
        DiscoverySource::PluginPath => "plugin_path",
        DiscoverySource::SystemPath => "system_path",
    }
}

fn handle_plugin_list(args: PluginListArgs, project_root: &str, json: bool) -> Result<()> {
    let discovered = discover(project_root, args.include_system_path)?;
    let rows: Vec<DiscoveredPluginRow> = discovered
        .into_iter()
        .map(|plugin| DiscoveredPluginRow {
            name: plugin.name,
            version: plugin.manifest.version,
            plugin_kind: plugin.manifest.plugin_kind,
            description: plugin.manifest.description,
            protocol_version: plugin.manifest.protocol_version,
            capabilities: plugin.manifest.capabilities,
            source: source_label(plugin.source),
            path: plugin.path.display().to_string(),
        })
        .collect();
    print_value(rows, json)
}

fn find_plugin(project_root: &str, name: &str, include_system_path: bool) -> Result<DiscoveredPlugin> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid_input_error("--name must not be empty"));
    }
    let mut matches = if include_system_path {
        discover(project_root, true)?
    } else {
        discover_plugins(Path::new(project_root))?
    };
    matches.retain(|plugin| plugin.name == trimmed);
    matches.pop().ok_or_else(|| not_found_error(format!("plugin not found: {trimmed}")))
}

async fn handle_plugin_info(args: PluginInfoArgs, project_root: &str, json: bool) -> Result<()> {
    let discovered = find_plugin(project_root, &args.name, args.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    let _ = host.shutdown().await;

    let output = PluginInfoOutput {
        name: discovered.name,
        source: source_label(discovered.source),
        path: discovered.path.display().to_string(),
        manifest: discovered.manifest,
        initialize: serde_json::to_value(initialize)?,
    };
    print_value(output, json)
}

async fn handle_plugin_call(args: PluginCallArgs, project_root: &str, json: bool) -> Result<()> {
    let method = args.method.trim().to_string();
    if method.is_empty() {
        return Err(invalid_input_error("--method must not be empty"));
    }
    let params = match args.params {
        Some(raw) => Some(serde_json::from_str::<Value>(&raw).context("--params must be valid JSON")?),
        None => None,
    };

    let discovered = find_plugin(project_root, &args.name, args.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let _ = host.handshake().await.context("plugin initialize failed")?;
    let result = host
        .request(method.clone(), params)
        .await
        .map_err(|err| anyhow!("plugin call failed ({}): {}", err.code, err.message))?;
    let _ = host.shutdown().await;

    print_value(PluginCallOutput { name: discovered.name, method, result }, json)
}

async fn handle_plugin_ping(args: PluginPingArgs, project_root: &str, json: bool) -> Result<()> {
    let discovered = find_plugin(project_root, &args.name, args.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    host.ping().await.context("plugin ping failed")?;
    let _ = host.shutdown().await;

    print_value(
        PluginPingOutput { name: discovered.name, ok: true, plugin_info: serde_json::to_value(initialize.plugin_info)? },
        json,
    )
}
