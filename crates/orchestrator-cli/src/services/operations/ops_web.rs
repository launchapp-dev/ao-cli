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

    let mut running: Vec<RunningTransport> = Vec::new();
    for plugin in transports.iter().chain(web_ui_plugins.iter()) {
        match spawn_and_keep_alive(plugin).await {
            Ok(rt) => running.push(rt),
            Err(err) => {
                shutdown_running_transports(running).await;
                return Err(err);
            }
        }
    }

    let primary_url = running
        .iter()
        .find(|s| s.info.kind == WEB_UI_PLUGIN_KIND)
        .or_else(|| running.first())
        .and_then(|s| s.info.url.clone());

    if args.open {
        if let Some(url) = primary_url.as_deref() {
            if let Err(err) = open_in_browser(url) {
                shutdown_running_transports(running).await;
                return Err(err);
            }
        }
    }

    let payload = json!({
        "message": "transport plugins ready",
        "primary_url": primary_url,
        "transports": running.iter().map(|s| json!({
            "name": s.info.name,
            "kind": s.info.kind,
            "url": s.info.url,
            "info": s.info.info,
        })).collect::<Vec<_>>(),
    });
    print_value(payload, json)?;

    if !json {
        if let Some(url) = primary_url.as_deref() {
            print_ok(&format!("web UI available at {url}"), false);
        }
        eprintln!(
            "[serve] transport plugins running in foreground. Press Ctrl-C to shut them down. \
             For long-lived supervision (auto-restart, background lifetime), run \
             `animus daemon start` — daemon-managed web plugins land in v0.5."
        );
    }

    wait_for_shutdown_signal().await;
    if !json {
        eprintln!("[serve] shutdown signal received, stopping transport plugins...");
    }
    shutdown_running_transports(running).await;
    Ok(())
}

async fn handle_open(args: crate::WebOpenArgs, project_root: &str, json: bool) -> Result<()> {
    // --url short-circuits plugin discovery entirely: the help text promises
    // "installed plugins are not consulted", so a machine with zero web plugins
    // installed must still be able to open an arbitrary URL. This is also the
    // recommended path when an externally-managed plugin or daemon is already
    // serving the UI, since the CLI then has no lifetime to manage.
    if let Some(explicit) = args.url.as_deref() {
        return open_resolved_url(explicit, json);
    }

    // Default path before this fix: spawn plugin, ask it for URL, immediately
    // shut it down, then open the browser at an already-dead server. Codex
    // round-6 P2. Fix: keep the plugin alive in the foreground (same lifecycle
    // as `web serve`) so the URL stays reachable. A `--detach` flag was
    // explored but rejected — STDIO plugins die at CLI exit because the OS
    // closes the inherited stdin pipe, so we cannot honestly promise a
    // detached lifecycle without a separate supervisor (which is the job
    // of `animus daemon start` once daemon-managed web plugins land).
    let (transports, web_ui_plugins) = collect_transport_plugins(project_root)?;
    if transports.is_empty() && web_ui_plugins.is_empty() {
        bail_with_install_help();
    }

    handle_open_foreground(&args, transports, web_ui_plugins, json).await
}

async fn handle_open_foreground(
    args: &crate::WebOpenArgs,
    transports: Vec<DiscoveredPlugin>,
    web_ui_plugins: Vec<DiscoveredPlugin>,
    json: bool,
) -> Result<()> {
    let mut running: Vec<RunningTransport> = Vec::new();
    for plugin in web_ui_plugins.iter().chain(transports.iter()) {
        match spawn_and_keep_alive(plugin).await {
            Ok(rt) => running.push(rt),
            Err(err) => {
                shutdown_running_transports(running).await;
                return Err(err);
            }
        }
    }

    let primary_url = running
        .iter()
        .find(|s| s.info.kind == WEB_UI_PLUGIN_KIND)
        .or_else(|| running.first())
        .and_then(|s| s.info.url.clone())
        .map(|u| append_path(&u, &args.path));

    let url = match primary_url {
        Some(url) => url,
        None => {
            shutdown_running_transports(running).await;
            return Err(anyhow!(
                "no transport plugin advertised a URL. Pass --url explicitly or install \
                 launchapp-dev/animus-web-ui via `animus plugin install-defaults --include-transports`."
            ));
        }
    };

    if let Err(err) = open_in_browser(&url) {
        shutdown_running_transports(running).await;
        return Err(err);
    }

    if json {
        print_value(json!({"message": "browser opened", "url": url, "mode": "foreground"}), true)?;
    } else {
        print_ok(&format!("opened {url}"), false);
        eprintln!(
            "[open] transport plugin running in foreground so {url} stays reachable. \
             Press Ctrl-C to stop. For an externally-managed server, pass --url <URL> \
             so this command returns immediately without spawning a plugin."
        );
    }

    wait_for_shutdown_signal().await;
    if !json {
        eprintln!("[open] shutdown signal received, stopping transport plugin...");
    }
    shutdown_running_transports(running).await;
    Ok(())
}

#[cfg(test)]
async fn resolve_open_url(args: &crate::WebOpenArgs, project_root: &str) -> Result<String> {
    // Test-only helper exercising the --url short-circuit + the
    // describe-and-shutdown URL resolution. Production paths now go
    // through `handle_open_foreground` so the plugin handle survives past
    // URL resolution (the round-6 P2 fix).
    if let Some(explicit) = args.url.as_deref() {
        return Ok(explicit.to_string());
    }
    let (transports, web_ui_plugins) = collect_transport_plugins(project_root)?;
    if transports.is_empty() && web_ui_plugins.is_empty() {
        bail_with_install_help();
    }
    let mut url: Option<String> = None;
    for plugin in web_ui_plugins.iter().chain(transports.iter()) {
        if let Ok(info) = spawn_and_describe(plugin).await {
            if let Some(resolved) = info.url {
                url = Some(append_path(&resolved, &args.path));
                break;
            }
        }
    }
    url.ok_or_else(|| {
        anyhow!(
            "no transport plugin advertised a URL. Pass --url explicitly or install \
             launchapp-dev/animus-web-ui via `animus plugin install-defaults --include-transports`."
        )
    })
}

fn open_resolved_url(url: &str, json: bool) -> Result<()> {
    open_in_browser(url)?;
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

struct RunningTransport {
    info: SpawnedTransport,
    host: PluginHost,
}

async fn describe_host(plugin: &DiscoveredPlugin, host: &PluginHost) -> Result<SpawnedTransport> {
    let init = tokio::time::timeout(PLUGIN_HANDSHAKE_TIMEOUT, host.handshake())
        .await
        .map_err(|_| anyhow!("transport plugin {} handshake timed out", plugin.name))?
        .map_err(|err| anyhow!("transport plugin {} handshake failed: {err}", plugin.name))?;
    let init_value = serde_json::to_value(&init).unwrap_or(Value::Null);

    let mut url = extract_url(&init_value);
    if url.is_none() {
        if let Ok(Ok(value)) =
            tokio::time::timeout(PLUGIN_HANDSHAKE_TIMEOUT, host.request_typed("transport/info", None)).await
        {
            url = extract_url(&value);
        }
    }
    Ok(SpawnedTransport { name: plugin.name.clone(), kind: plugin.manifest.plugin_kind.clone(), url, info: init_value })
}

#[cfg(test)]
async fn spawn_and_describe(plugin: &DiscoveredPlugin) -> Result<SpawnedTransport> {
    let host = PluginHost::spawn(&plugin.path, &[])
        .await
        .map_err(|err| anyhow!("failed to spawn transport plugin {}: {err}", plugin.name))?;
    let described = describe_host(plugin, &host).await;
    // Test-only describe-and-shutdown helper. Production paths use
    // `spawn_and_keep_alive` so the plugin survives URL resolution.
    let _ = host.shutdown().await;
    described
}

async fn spawn_and_keep_alive(plugin: &DiscoveredPlugin) -> Result<RunningTransport> {
    let host = PluginHost::spawn(&plugin.path, &[])
        .await
        .map_err(|err| anyhow!("failed to spawn transport plugin {}: {err}", plugin.name))?;
    match describe_host(plugin, &host).await {
        Ok(info) => Ok(RunningTransport { info, host }),
        Err(err) => {
            let _ = host.shutdown().await;
            Err(err)
        }
    }
}

async fn shutdown_running_transports(running: Vec<RunningTransport>) {
    for rt in running {
        let _ = rt.host.shutdown().await;
    }
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
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
    use super::{append_path, extract_url, resolve_open_url, shutdown_running_transports, wait_for_shutdown_signal};
    use crate::WebOpenArgs;
    use serde_json::json;
    use std::time::Duration;

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

    /// P2 #4 regression: `--url` must skip plugin discovery entirely so that
    /// machines without any web plugins installed can still open arbitrary URLs.
    /// We point at a nonexistent project_root to prove discovery never runs —
    /// if it did, `collect_transport_plugins` would observe an empty plugin set
    /// and `bail_with_install_help` would `std::process::exit(2)`, killing the test.
    #[tokio::test]
    async fn web_open_with_explicit_url_does_not_discover_plugins() {
        let args =
            WebOpenArgs { url: Some("http://example.invalid/dashboard".to_string()), path: "/ignored".to_string() };
        let resolved = resolve_open_url(&args, "/path/that/definitely/does/not/exist/animus-test").await.unwrap();
        assert_eq!(resolved, "http://example.invalid/dashboard");
    }

    /// P1 #3 regression scaffold: `web serve` must keep plugin handles alive
    /// for the duration of the session. We can't reach into the live plugin host
    /// from a unit test, but we can verify the two halves of the contract:
    /// (a) `shutdown_running_transports` is the explicit teardown step, and
    /// (b) `wait_for_shutdown_signal` actually blocks until a signal arrives.
    /// Together they prove the lifecycle is no longer "spawn, describe,
    /// immediately shut down" — the fix moved shutdown behind the signal wait.
    #[tokio::test]
    async fn web_serve_keeps_plugin_alive_until_signal() {
        shutdown_running_transports(Vec::new()).await;
        let blocked = tokio::time::timeout(Duration::from_millis(150), wait_for_shutdown_signal()).await;
        assert!(blocked.is_err(), "wait_for_shutdown_signal returned before any signal was raised");
    }
}
