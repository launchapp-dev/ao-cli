use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use orchestrator_core::ServiceHub;
use orchestrator_plugin_host::{discover_plugins, DiscoveredPlugin, PluginHost};
use serde_json::{json, Value};

use crate::{print_ok, print_value, WebCommand};

const TRANSPORT_PLUGIN_KIND: &str = "transport_backend";
const WEB_UI_PLUGIN_KIND: &str = "web_ui";
const DEFAULT_TRANSPORT_KIND_PREFERENCE: &[&str] = &["transport-http", "transport-graphql"];
const PLUGIN_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) async fn handle_web(
    command: WebCommand,
    _hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    match command {
        WebCommand::Serve(args) => handle_serve(args, project_root, json).await,
        WebCommand::Open(args) => handle_open(args, project_root, json).await,
    }
}

async fn handle_serve(args: crate::WebServeArgs, project_root: &str, json: bool) -> Result<()> {
    let (transports, web_ui_plugins) = collect_transport_plugins(project_root)?;
    if transports.is_empty() && web_ui_plugins.is_empty() {
        bail_with_install_help();
    }

    let mut spawned: Vec<SpawnedTransport> = Vec::new();
    for plugin in transports.iter().chain(web_ui_plugins.iter()) {
        let info = spawn_and_describe(plugin).await?;
        spawned.push(info);
    }

    let primary_url =
        spawned.iter().find(|s| s.kind == WEB_UI_PLUGIN_KIND).or_else(|| spawned.first()).and_then(|s| s.url.clone());

    if args.open {
        if let Some(url) = primary_url.as_deref() {
            open_in_browser(url)?;
        }
    }

    let payload = json!({
        "message": "transport plugins ready",
        "primary_url": primary_url,
        "transports": spawned.iter().map(|s| json!({
            "name": s.name,
            "kind": s.kind,
            "url": s.url,
            "info": s.info,
        })).collect::<Vec<_>>(),
    });
    print_value(payload, json)?;

    if !json {
        if let Some(url) = primary_url {
            print_ok(&format!("web UI available at {url}"), false);
        }
        eprintln!(
            "[note] animus web now delegates to installed transport plugins. Daemon supervision \
             of plugin lifetime arrives in a follow-up — for long-running serving, run the plugin \
             binaries directly or rely on the daemon plugin host (animus daemon start)."
        );
    }
    Ok(())
}

async fn handle_open(args: crate::WebOpenArgs, project_root: &str, json: bool) -> Result<()> {
    let (transports, web_ui_plugins) = collect_transport_plugins(project_root)?;
    if transports.is_empty() && web_ui_plugins.is_empty() {
        bail_with_install_help();
    }

    let mut url = args.url.clone();
    if url.is_none() {
        for plugin in web_ui_plugins.iter().chain(transports.iter()) {
            if let Ok(info) = spawn_and_describe(plugin).await {
                if let Some(resolved) = info.url {
                    url = Some(append_path(&resolved, &args.path));
                    break;
                }
            }
        }
    }

    let url = url.ok_or_else(|| {
        anyhow!(
            "no transport plugin advertised a URL. Pass --url explicitly or install \
             launchapp-dev/animus-web-ui via `animus plugin install-defaults --include-transports`."
        )
    })?;

    open_in_browser(&url)?;
    if json {
        print_value(json!({"message": "browser opened", "url": url}), true)
    } else {
        print_ok(&format!("opened {url}"), false);
        Ok(())
    }
}

struct SpawnedTransport {
    name: String,
    kind: String,
    url: Option<String>,
    info: Value,
}

async fn spawn_and_describe(plugin: &DiscoveredPlugin) -> Result<SpawnedTransport> {
    let host = PluginHost::spawn(&plugin.path, &[])
        .await
        .map_err(|err| anyhow!("failed to spawn transport plugin {}: {err}", plugin.name))?;
    let init = tokio::time::timeout(PLUGIN_HANDSHAKE_TIMEOUT, host.handshake())
        .await
        .map_err(|_| anyhow!("transport plugin {} handshake timed out", plugin.name))?
        .map_err(|err| anyhow!("transport plugin {} handshake failed: {err}", plugin.name))?;
    let init_value = serde_json::to_value(&init).unwrap_or(Value::Null);

    let mut url = extract_url(&init_value);
    if url.is_none() {
        if let Ok(resp) =
            tokio::time::timeout(PLUGIN_HANDSHAKE_TIMEOUT, host.request_typed("transport/info", None)).await
        {
            if let Ok(value) = resp {
                url = extract_url(&value);
            }
        }
    }
    let _ = host.shutdown().await;

    Ok(SpawnedTransport { name: plugin.name.clone(), kind: plugin.manifest.plugin_kind.clone(), url, info: init_value })
}

fn extract_url(value: &Value) -> Option<String> {
    if let Some(direct) = value.get("url").and_then(Value::as_str) {
        return Some(direct.to_string());
    }
    if let Some(transport) = value.get("transport") {
        if let Some(url) = transport.get("url").and_then(Value::as_str) {
            return Some(url.to_string());
        }
        let host = transport.get("host").and_then(Value::as_str);
        let port = transport.get("port").and_then(Value::as_u64);
        let scheme = transport.get("scheme").and_then(Value::as_str).unwrap_or("http");
        if let (Some(host), Some(port)) = (host, port) {
            return Some(format!("{scheme}://{host}:{port}"));
        }
    }
    value
        .get("capabilities")
        .and_then(Value::as_array)
        .and_then(|caps| caps.iter().find_map(|cap| cap.get("url").and_then(Value::as_str).map(ToString::to_string)))
}

fn collect_transport_plugins(project_root: &str) -> Result<(Vec<DiscoveredPlugin>, Vec<DiscoveredPlugin>)> {
    let discovered = discover_plugins(project_root)?;
    let mut transports: Vec<DiscoveredPlugin> = Vec::new();
    let mut web_ui: Vec<DiscoveredPlugin> = Vec::new();
    for plugin in discovered {
        match plugin.manifest.plugin_kind.as_str() {
            TRANSPORT_PLUGIN_KIND => transports.push(plugin),
            WEB_UI_PLUGIN_KIND => web_ui.push(plugin),
            _ => {}
        }
    }
    transports.sort_by_key(|p| {
        DEFAULT_TRANSPORT_KIND_PREFERENCE.iter().position(|name| p.name.contains(name)).unwrap_or(usize::MAX)
    });
    Ok((transports, web_ui))
}

fn bail_with_install_help() -> ! {
    let lines = [
        "No transport_backend or web_ui plugins are installed.".to_string(),
        "".to_string(),
        "Animus delegates `animus web` to standalone transport + UI plugins.".to_string(),
        "Install the defaults with:".to_string(),
        "".to_string(),
        "  animus plugin install-defaults --include-transports".to_string(),
        "".to_string(),
        "Or install them individually:".to_string(),
        "  animus plugin install launchapp-dev/animus-transport-http@v0.2.0".to_string(),
        "  animus plugin install launchapp-dev/animus-transport-graphql@v0.2.3".to_string(),
        "  animus plugin install launchapp-dev/animus-web-ui@v0.1.0".to_string(),
    ];
    eprintln!("{}", lines.join("\n"));
    std::process::exit(2);
}

fn open_in_browser(url: &str) -> Result<()> {
    webbrowser::open(url).map(|_| ()).map_err(|error| anyhow!("failed to open browser: {error}"))
}

fn append_path(base: &str, path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return base.to_string();
    }
    let suffix = if trimmed.starts_with('/') { trimmed.to_string() } else { format!("/{trimmed}") };
    if let Some(stripped) = base.strip_suffix('/') {
        format!("{stripped}{suffix}")
    } else {
        format!("{base}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::{append_path, extract_url};
    use serde_json::json;

    #[test]
    fn extract_url_from_direct_field() {
        let v = json!({"url": "http://127.0.0.1:4173"});
        assert_eq!(extract_url(&v).as_deref(), Some("http://127.0.0.1:4173"));
    }

    #[test]
    fn extract_url_from_transport_object() {
        let v = json!({"transport": {"host": "127.0.0.1", "port": 4173, "scheme": "https"}});
        assert_eq!(extract_url(&v).as_deref(), Some("https://127.0.0.1:4173"));
    }

    #[test]
    fn extract_url_returns_none_for_empty_value() {
        assert!(extract_url(&json!({})).is_none());
    }

    #[test]
    fn append_path_handles_trailing_slash() {
        assert_eq!(append_path("http://h/", "/runs"), "http://h/runs");
        assert_eq!(append_path("http://h", "runs"), "http://h/runs");
        assert_eq!(append_path("http://h", "/"), "http://h");
    }
}
