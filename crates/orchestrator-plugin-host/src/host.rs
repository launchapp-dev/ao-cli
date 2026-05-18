use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use animus_plugin_protocol::{
    error_codes, EnvRequirement, HealthCheckResult, HostCapabilities, HostInfo, InitializeParams, InitializeResult,
    RpcError, RpcNotification, RpcRequest, RpcResponse, PROTOCOL_VERSION,
};
use anyhow::{anyhow, Result};
use semver::Version;
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Universal shell environment variables that every plugin gets regardless of
/// its declared `env_required` manifest. These are the locale + shell + Rust
/// telemetry vars that practically every CLI tool expects; withholding them
/// breaks even well-behaved plugins for no security gain (none of them carry
/// secrets).
///
/// Anything **not** in this list and **not** explicitly declared by the
/// plugin's manifest is scrubbed from the spawn environment via
/// [`std::process::Command::env_clear`].
pub const PLUGIN_BASE_ENV_ALLOWLIST: &[&str] =
    &["PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "RUST_LOG", "RUST_BACKTRACE", "TZ"];

use crate::StdioTransport;

/// Structured plugin-host errors that benefit from being matched on by
/// callers. Most internal failures still flow through `anyhow::Error`; this
/// enum is reserved for conditions the daemon needs to react to (e.g. log a
/// plugin as incompatible and skip it instead of crashing the supervisor).
#[derive(Debug, Error)]
pub enum HostError {
    /// The plugin advertised a `protocol_version` that the host cannot speak.
    ///
    /// Major-version mismatch (or non-semver gibberish) trips this. The host
    /// should quarantine the plugin and surface the message so users can see
    /// which plugin is wedged.
    #[error("incompatible plugin protocol: {0}")]
    IncompatibleProtocol(String),
}

/// Validate that a plugin's advertised `protocol_version` is wire-compatible
/// with the host's [`PROTOCOL_VERSION`].
///
/// Compatibility is gated by the semver major component. Plugins reporting a
/// matching major are accepted (minor/patch drift is treated as additive and
/// backwards-compatible). Plugins reporting a different major — or a
/// non-semver string — are rejected with [`HostError::IncompatibleProtocol`].
pub fn check_protocol_compat(plugin_version: &str) -> Result<(), HostError> {
    let host: Version = PROTOCOL_VERSION
        .parse()
        .map_err(|err| HostError::IncompatibleProtocol(format!("host protocol version is not valid semver: {err}")))?;
    let plugin: Version = plugin_version.parse().map_err(|_| {
        HostError::IncompatibleProtocol(format!(
            "plugin advertised non-semver protocol_version '{plugin_version}' (host speaks {PROTOCOL_VERSION})"
        ))
    })?;
    if plugin.major != host.major {
        return Err(HostError::IncompatibleProtocol(format!(
            "plugin protocol_version {plugin_version} incompatible with host {PROTOCOL_VERSION} (major version mismatch)"
        )));
    }
    Ok(())
}

/// Sink for plugin stderr lines. Receives `(plugin_name, line)` on each stderr line.
pub type PluginStderrSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Caller-supplied options that drive how the plugin host spawns a plugin
/// process.
///
/// Use [`PluginSpawnOptions::for_manifest`] to derive an environment allowlist
/// from a plugin's [`PluginManifest::env_required`](animus_plugin_protocol::PluginManifest::env_required)
/// list. See [`PLUGIN_BASE_ENV_ALLOWLIST`] for the universally-forwarded vars.
#[derive(Default, Clone)]
pub struct PluginSpawnOptions {
    /// Routes every stderr line through this sink in addition to the standard
    /// `tracing::warn!` log. Useful for surfacing plugin diagnostics into a
    /// project's structured events log.
    pub stderr_sink: Option<PluginStderrSink>,
    /// Names of environment variables the plugin is allowed to see. The host
    /// always forwards [`PLUGIN_BASE_ENV_ALLOWLIST`] on top of this list.
    /// Anything else is scrubbed.
    pub env_allowlist: Vec<String>,
    /// Plugin-name label used in any spawn-time warnings (e.g. missing
    /// required env). When empty, the host falls back to the binary file name.
    pub plugin_label: Option<String>,
    /// Required-but-missing env variable names. The host emits a `warn!` for
    /// each at spawn time so operators can see why the plugin will likely
    /// fail.
    pub missing_required_env: Vec<String>,
}

impl PluginSpawnOptions {
    /// Build options for a plugin whose manifest declares the supplied env
    /// requirements. Returns the assembled options and a list of declared-as
    /// `required = true` vars that are not currently set in the host process.
    ///
    /// The returned options force the spawn to scrub the daemon's environment
    /// to [`PLUGIN_BASE_ENV_ALLOWLIST`] plus the manifest's declared variables
    /// plus any explicit `extra` names supplied by the caller (e.g. one-off
    /// runtime overrides).
    pub fn for_manifest(
        plugin_label: impl Into<String>,
        env_required: &[EnvRequirement],
        extra_env_vars: impl IntoIterator<Item = String>,
        stderr_sink: Option<PluginStderrSink>,
    ) -> Self {
        let plugin_label = plugin_label.into();
        let mut allow: BTreeSet<String> = env_required.iter().map(|requirement| requirement.name.clone()).collect();
        allow.extend(extra_env_vars);
        let missing_required: Vec<String> = env_required
            .iter()
            .filter(|requirement| requirement.required)
            .filter(|requirement| std::env::var_os(&requirement.name).is_none())
            .map(|requirement| requirement.name.clone())
            .collect();
        Self {
            stderr_sink,
            env_allowlist: allow.into_iter().collect(),
            plugin_label: if plugin_label.is_empty() { None } else { Some(plugin_label) },
            missing_required_env: missing_required,
        }
    }
}

/// Channel that receives plugin-emitted JSON-RPC notifications (no `id`).
pub type PluginNotificationRx = mpsc::Receiver<RpcNotification>;
type PluginNotificationTx = mpsc::Sender<RpcNotification>;

pub struct PluginHost<R = ChildStdout, W = ChildStdin> {
    pub name: String,
    child: Option<Child>,
    transport: StdioTransport<R, W>,
    next_id: u64,
    notification_tx: Option<PluginNotificationTx>,
}

impl PluginHost<ChildStdout, ChildStdin> {
    /// Spawn a plugin without forwarding any environment beyond
    /// [`PLUGIN_BASE_ENV_ALLOWLIST`]. Most production callers should use
    /// [`PluginHost::spawn_with_options`] instead so the plugin sees the
    /// env it declared in its manifest.
    pub async fn spawn(binary_path: &Path, args: &[&str]) -> Result<Self> {
        Self::spawn_with_options(binary_path, args, PluginSpawnOptions::default()).await
    }

    /// Spawn a plugin and route every stderr line through the supplied sink in addition
    /// to the standard `tracing::warn!` log. Use this from the host runtime so plugin
    /// diagnostics land in the project's structured `events.jsonl`.
    ///
    /// Note: this convenience does not forward any plugin-specific env vars.
    /// Prefer [`PluginHost::spawn_with_options`] (with options built via
    /// [`PluginSpawnOptions::for_manifest`]) for production spawns so the
    /// plugin's manifest-declared environment is honored.
    pub async fn spawn_with_stderr(
        binary_path: &Path,
        args: &[&str],
        stderr_sink: Option<PluginStderrSink>,
    ) -> Result<Self> {
        let options = PluginSpawnOptions { stderr_sink, ..PluginSpawnOptions::default() };
        Self::spawn_with_options(binary_path, args, options).await
    }

    /// Spawn a plugin under the supplied [`PluginSpawnOptions`].
    ///
    /// The host always calls `env_clear()` on the child process and forwards
    /// only the union of [`PLUGIN_BASE_ENV_ALLOWLIST`] and
    /// `options.env_allowlist`. This is the v0.4.x trust boundary: plugins
    /// only see secrets they explicitly declared in their manifest.
    pub async fn spawn_with_options(binary_path: &Path, args: &[&str], options: PluginSpawnOptions) -> Result<Self> {
        let binary_name = binary_path.file_name().and_then(|value| value.to_str()).unwrap_or("plugin").to_string();
        let name = options.plugin_label.clone().unwrap_or_else(|| binary_name.clone());

        let mut command = tokio::process::Command::new(binary_path);
        command
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Build the allowlist: universal base + caller-declared. Deduplicate
        // case-sensitively (env var names are case-sensitive on POSIX).
        let mut allow: BTreeSet<&str> = PLUGIN_BASE_ENV_ALLOWLIST.iter().copied().collect();
        for var in &options.env_allowlist {
            allow.insert(var.as_str());
        }

        command.env_clear();
        for var in &allow {
            if let Some(value) = std::env::var_os(var) {
                command.env(var, value);
            }
        }

        for missing in &options.missing_required_env {
            warn!(
                plugin = %name,
                env_var = %missing,
                "plugin declared env_required={{name={missing}, required=true}} but the host environment does not have it set; the plugin will likely fail to start"
            );
        }

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("failed to take plugin stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("failed to take plugin stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("failed to take plugin stderr"))?;

        let stderr_plugin_name = name.clone();
        let stderr_sink = options.stderr_sink.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!(plugin = %stderr_plugin_name, "{}", line);
                if let Some(sink) = stderr_sink.as_ref() {
                    sink(&stderr_plugin_name, &line);
                }
            }
        });

        Ok(Self {
            name,
            child: Some(child),
            transport: StdioTransport::new(stdout, stdin),
            next_id: 1,
            notification_tx: None,
        })
    }
}

impl<R, W> PluginHost<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn from_streams(name: impl Into<String>, reader: R, writer: W) -> Self {
        Self {
            name: name.into(),
            child: None,
            transport: StdioTransport::new(reader, writer),
            next_id: 1,
            notification_tx: None,
        }
    }

    /// Subscribe to JSON-RPC notifications (frames with no `id`) emitted by the
    /// plugin. The returned receiver is fed by `send_and_receive` whenever it
    /// observes a notification on the way to a request response.
    ///
    /// If you don't subscribe, notifications are silently dropped — same as
    /// before this method existed.
    pub fn subscribe_notifications(&mut self, capacity: usize) -> PluginNotificationRx {
        let (tx, rx) = mpsc::channel(capacity);
        self.notification_tx = Some(tx);
        rx
    }

    pub fn next_request_id(&self) -> u64 {
        self.next_id
    }

    async fn send_and_receive(&mut self, id: u64, method: &str, params: Option<Value>) -> Result<RpcResponse> {
        self.transport.write_message(&RpcRequest::new(id, method, params)).await?;
        let expected_id = serde_json::json!(id);

        loop {
            let frame = self
                .transport
                .read_message::<Value>()
                .await?
                .ok_or_else(|| anyhow!("plugin closed while waiting for response to '{method}'"))?;

            // Notifications carry no `id` field; forward (or drop) and keep waiting.
            if frame.get("id").is_none() {
                if let Some(tx) = self.notification_tx.clone() {
                    if let Ok(notification) = serde_json::from_value::<RpcNotification>(frame) {
                        let _ = tx.try_send(notification);
                    }
                } else {
                    debug!(plugin = %self.name, "dropped plugin notification (no subscriber)");
                }
                continue;
            }

            let response: RpcResponse = serde_json::from_value(frame)
                .map_err(|error| anyhow!("plugin '{}' sent malformed response: {error}", self.name))?;
            if response.id.as_ref() == Some(&expected_id) {
                return Ok(response);
            }
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub async fn handshake(&mut self) -> Result<InitializeResult> {
        const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            host_info: HostInfo { name: "animus".to_string(), version: env!("CARGO_PKG_VERSION").to_string() },
            capabilities: HostCapabilities { streaming: true, progress: true, cancellation: true },
        };

        let id = self.take_id();
        let response = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            self.send_and_receive(id, "initialize", Some(serde_json::to_value(params)?)),
        )
        .await
        .map_err(|_| {
            anyhow!("plugin '{}' did not respond to initialize within {}s", self.name, HANDSHAKE_TIMEOUT.as_secs())
        })??;
        if let Some(error) = response.error {
            return Err(anyhow!("plugin initialize failed ({}): {}", error.code, error.message));
        }

        let result: InitializeResult =
            serde_json::from_value(response.result.ok_or_else(|| anyhow!("plugin initialize returned no result"))?)?;

        if let Err(host_error) = check_protocol_compat(&result.protocol_version) {
            return Err(anyhow!("plugin '{}' rejected at handshake: {host_error}", self.name));
        }

        self.notify("initialized", None).await?;
        debug!(plugin = %self.name, plugin_name = %result.plugin_info.name, "stdio plugin initialized");
        Ok(result)
    }

    pub async fn request(&mut self, method: impl Into<String>, params: Option<Value>) -> Result<Value, RpcError> {
        let method = method.into();
        let id = self.take_id();
        let response = self.send_and_receive(id, &method, params).await.map_err(|error| RpcError {
            code: error_codes::INTERNAL_ERROR,
            message: error.to_string(),
            data: None,
        })?;

        if let Some(error) = response.error {
            return Err(error);
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    pub async fn notify(&mut self, method: impl Into<String>, params: Option<Value>) -> Result<()> {
        self.transport.write_message(&RpcNotification::new(method, params)).await
    }

    pub async fn ping(&mut self) -> Result<()> {
        let id = self.take_id();
        let response = tokio::time::timeout(Duration::from_secs(2), self.send_and_receive(id, "$/ping", None))
            .await
            .map_err(|_| anyhow!("plugin ping timed out"))??;
        if let Some(error) = response.error {
            return Err(anyhow!("plugin ping failed ({}): {}", error.code, error.message));
        }
        Ok(())
    }

    pub async fn health_check(&mut self) -> Result<HealthCheckResult> {
        let result = tokio::time::timeout(Duration::from_secs(2), self.request("health/check", None))
            .await
            .map_err(|_| anyhow!("plugin health/check timed out"))?
            .map_err(|error| anyhow!("plugin health/check failed ({}): {}", error.code, error.message))?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.request("shutdown", None).await;
        let _ = self.notify("exit", None).await;
        if let Some(mut child) = self.child.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use animus_plugin_protocol::{PluginCapabilities, PluginInfo, RpcRequest, RpcResponse};
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    fn ok_initialize_response(id: Option<Value>, protocol_version: &str) -> RpcResponse {
        RpcResponse::ok(
            id,
            serde_json::json!(InitializeResult {
                protocol_version: protocol_version.to_string(),
                plugin_info: PluginInfo {
                    name: "test".to_string(),
                    version: "0.1.0".to_string(),
                    plugin_kind: "custom".to_string(),
                    description: None,
                },
                capabilities: PluginCapabilities::default(),
            }),
        )
    }

    async fn drive_handshake(plugin_protocol_version: &'static str) -> Result<InitializeResult> {
        let (host_reader, mut plugin_writer) = duplex(8192);
        let (plugin_reader, host_writer) = duplex(8192);

        tokio::spawn(async move {
            let mut reader = BufReader::new(plugin_reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("read initialize");
            let request: RpcRequest = serde_json::from_str(line.trim()).expect("parse initialize");

            let response = ok_initialize_response(request.id, plugin_protocol_version);
            let mut encoded = serde_json::to_string(&response).expect("encode response");
            encoded.push('\n');
            plugin_writer.write_all(encoded.as_bytes()).await.expect("write response");

            // The host only sends `initialized` after compat check passes; reading
            // here is best-effort so rejected handshakes don't deadlock the test.
            let _ = reader.read_line(&mut line).await;
        });

        let mut host = PluginHost::from_streams("test", host_reader, host_writer);
        host.handshake().await
    }

    #[tokio::test]
    async fn handshake_sends_initialize_and_initialized() {
        let (host_reader, mut plugin_writer) = duplex(8192);
        let (plugin_reader, host_writer) = duplex(8192);

        tokio::spawn(async move {
            let mut reader = BufReader::new(plugin_reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("read initialize");
            let request: RpcRequest = serde_json::from_str(line.trim()).expect("parse initialize");
            assert_eq!(request.method, "initialize");

            let response = ok_initialize_response(request.id, PROTOCOL_VERSION);
            let mut encoded = serde_json::to_string(&response).expect("encode response");
            encoded.push('\n');
            plugin_writer.write_all(encoded.as_bytes()).await.expect("write response");

            line.clear();
            reader.read_line(&mut line).await.expect("read initialized");
            let notification: serde_json::Value = serde_json::from_str(line.trim()).expect("parse initialized");
            assert_eq!(notification["method"], "initialized");
        });

        let mut host = PluginHost::from_streams("test", host_reader, host_writer);
        let result = host.handshake().await.expect("handshake should succeed");

        assert_eq!(result.plugin_info.name, "test");
    }

    #[test]
    fn check_protocol_compat_accepts_matching_major() {
        // PROTOCOL_VERSION = "1.0.0"; same major => OK.
        assert!(check_protocol_compat(PROTOCOL_VERSION).is_ok());
        assert!(check_protocol_compat("1.0.0").is_ok());
    }

    #[test]
    fn check_protocol_compat_accepts_minor_patch_drift_within_major() {
        // Host 1.0.0 + plugin 1.2.5 => OK (additive minor/patch is backwards-compatible).
        assert!(check_protocol_compat("1.2.5").is_ok());
        assert!(check_protocol_compat("1.0.99").is_ok());
        assert!(check_protocol_compat("1.999.0").is_ok());
    }

    #[test]
    fn check_protocol_compat_rejects_major_mismatch() {
        // Host 1.0.0 + plugin 2.0.0 => error.
        let err = check_protocol_compat("2.0.0").expect_err("major mismatch must fail");
        let HostError::IncompatibleProtocol(message) = err;
        assert!(message.contains("major version mismatch"), "unexpected message: {message}");
    }

    #[test]
    fn check_protocol_compat_rejects_non_semver() {
        // Host 1.0.0 + plugin "garbage" => error.
        let err = check_protocol_compat("garbage").expect_err("non-semver must fail");
        let HostError::IncompatibleProtocol(message) = err;
        assert!(message.contains("non-semver"), "unexpected message: {message}");
    }

    #[tokio::test]
    async fn handshake_rejects_plugin_with_major_mismatch() {
        let err = drive_handshake("2.0.0").await.expect_err("major mismatch must abort handshake");
        let message = format!("{err}");
        assert!(
            message.contains("incompatible plugin protocol") && message.contains("major version mismatch"),
            "unexpected error: {message}"
        );
    }

    // ===== Env scrubbing tests =====
    //
    // These exercise the v0.4.x trust-boundary promise: a spawned plugin must
    // not inherit any env var that's not in PLUGIN_BASE_ENV_ALLOWLIST and not
    // declared in its manifest. We build a tiny shell-script "plugin" that
    // serializes its env to a file, spawn it via spawn_with_options, and
    // inspect the file.

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::sync::Mutex;
    #[cfg(unix)]
    static ENV_SCRUB_GUARD: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
    fn write_env_dump_plugin(dir: &std::path::Path) -> std::path::PathBuf {
        let plugin = dir.join("env-dump-plugin");
        // Dump every env var as KEY=VALUE\n into ./env.out next to argv[1].
        std::fs::write(&plugin, "#!/bin/sh\nout=\"$1\"\nenv > \"$out\"\n").expect("write env-dump plugin");
        let mut perms = std::fs::metadata(&plugin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&plugin, perms).unwrap();
        plugin
    }

    #[cfg(unix)]
    fn read_env_dump(path: &std::path::Path) -> std::collections::HashMap<String, String> {
        let body = std::fs::read_to_string(path).expect("env dump should be written");
        let mut env = std::collections::HashMap::new();
        for line in body.lines() {
            if let Some((k, v)) = line.split_once('=') {
                env.insert(k.to_string(), v.to_string());
            }
        }
        env
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // intentional: guards std::env mutation across spawn await
    async fn env_scrubbing_strips_unrelated_vars() {
        let _guard = ENV_SCRUB_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let plugin = write_env_dump_plugin(dir.path());
        let env_out = dir.path().join("env.out");

        std::env::set_var("ANIMUS_TEST_SECRET", "should-not-leak");
        let result =
            PluginHost::spawn_with_options(&plugin, &[env_out.to_str().unwrap()], PluginSpawnOptions::default()).await;
        // The plugin runs and exits immediately; just wait for the child to flush.
        let mut host = result.expect("spawn should succeed");
        if let Some(mut child) = host.child.take() {
            let _ = child.wait().await;
        }
        std::env::remove_var("ANIMUS_TEST_SECRET");

        let env = read_env_dump(&env_out);
        assert!(!env.contains_key("ANIMUS_TEST_SECRET"), "env_clear() must strip ANIMUS_TEST_SECRET; saw env={env:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // intentional: guards std::env mutation across spawn await
    async fn env_scrubbing_keeps_declared_vars() {
        let _guard = ENV_SCRUB_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let plugin = write_env_dump_plugin(dir.path());
        let env_out = dir.path().join("env.out");

        std::env::set_var("ANIMUS_TEST_OPENAI_KEY", "sk-test-value");
        let manifest_env = vec![EnvRequirement {
            name: "ANIMUS_TEST_OPENAI_KEY".to_string(),
            description: None,
            sensitive: true,
            required: true,
        }];
        let opts =
            PluginSpawnOptions::for_manifest("env-dump-plugin", &manifest_env, std::iter::empty::<String>(), None);

        let mut host =
            PluginHost::spawn_with_options(&plugin, &[env_out.to_str().unwrap()], opts).await.expect("spawn");
        if let Some(mut child) = host.child.take() {
            let _ = child.wait().await;
        }
        std::env::remove_var("ANIMUS_TEST_OPENAI_KEY");

        let env = read_env_dump(&env_out);
        assert_eq!(
            env.get("ANIMUS_TEST_OPENAI_KEY").map(String::as_str),
            Some("sk-test-value"),
            "declared env var must be forwarded; saw env={env:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // intentional: guards std::env mutation across spawn await
    async fn env_scrubbing_always_includes_path_and_home() {
        let _guard = ENV_SCRUB_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let plugin = write_env_dump_plugin(dir.path());
        let env_out = dir.path().join("env.out");

        // PATH and HOME are always set on a unix dev/CI machine.
        let mut host =
            PluginHost::spawn_with_options(&plugin, &[env_out.to_str().unwrap()], PluginSpawnOptions::default())
                .await
                .expect("spawn");
        if let Some(mut child) = host.child.take() {
            let _ = child.wait().await;
        }

        let env = read_env_dump(&env_out);
        assert!(env.contains_key("PATH"), "PATH must be in the base allowlist; saw env={env:?}");
        assert!(env.contains_key("HOME"), "HOME must be in the base allowlist; saw env={env:?}");
    }

    #[test]
    fn for_manifest_reports_missing_required_vars() {
        let unique = format!("ANIMUS_TEST_REQUIRED_MISSING_{}", std::process::id());
        // Ensure unset
        std::env::remove_var(&unique);
        let manifest_env = vec![
            EnvRequirement { name: unique.clone(), description: None, sensitive: false, required: true },
            EnvRequirement { name: format!("{unique}_OPTIONAL"), description: None, sensitive: false, required: false },
        ];
        let opts = PluginSpawnOptions::for_manifest("plugin-name", &manifest_env, std::iter::empty::<String>(), None);
        assert!(opts.missing_required_env.contains(&unique));
        assert!(!opts.missing_required_env.iter().any(|v| v.ends_with("_OPTIONAL")));
        // Both names should be in the allowlist regardless of "required".
        assert!(opts.env_allowlist.contains(&unique));
        assert!(opts.env_allowlist.contains(&format!("{unique}_OPTIONAL")));
    }
}
