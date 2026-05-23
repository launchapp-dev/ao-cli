use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cli_wrapper::error::Result;
use cli_wrapper::session::{
    claude::ClaudeSessionBackend, codex::CodexSessionBackend, gemini::GeminiSessionBackend,
    oai_runner::OaiRunnerSessionBackend, opencode::OpenCodeSessionBackend, session_backend::SessionBackend,
    session_request::SessionRequest, session_run::SessionRun, subprocess_session_backend::SubprocessSessionBackend,
};

use crate::plugin_backend::{discover_provider_plugins, PluginSessionBackend};

/// Provider tool names that map to in-tree (built-in) backends. A
/// third-party plugin whose `provider_tool` matches one of these silently
/// hijacks the entire dispatch path for that tool, which is a supply-chain
/// risk; the install pipeline refuses such installs unless the operator
/// passes `--allow-shadow-builtin`. See
/// [`SessionBackendResolver::resolve`] for the runtime warning emitted when
/// a non-reserved plugin nevertheless overrides a built-in.
pub const RESERVED_PROVIDER_TOOLS: &[&str] = &["claude", "codex", "gemini", "opencode", "oai-runner"];

/// Returns `true` when the supplied provider tool name collides with an
/// in-tree built-in backend (case-insensitive).
pub fn is_reserved_provider_tool(tool: &str) -> bool {
    let lower = tool.trim().to_ascii_lowercase();
    RESERVED_PROVIDER_TOOLS.iter().any(|reserved| reserved.eq_ignore_ascii_case(&lower))
}

pub struct SessionBackendResolver {
    claude: Arc<ClaudeSessionBackend>,
    codex: Arc<CodexSessionBackend>,
    gemini: Arc<GeminiSessionBackend>,
    opencode: Arc<OpenCodeSessionBackend>,
    oai_runner: Arc<OaiRunnerSessionBackend>,
    subprocess: Arc<SubprocessSessionBackend>,
    plugin_providers: HashMap<String, Arc<PluginSessionBackend>>,
}

impl SessionBackendResolver {
    pub fn new() -> Self {
        Self {
            claude: Arc::new(ClaudeSessionBackend::new()),
            codex: Arc::new(CodexSessionBackend::new()),
            gemini: Arc::new(GeminiSessionBackend::new()),
            opencode: Arc::new(OpenCodeSessionBackend::new()),
            oai_runner: Arc::new(OaiRunnerSessionBackend::new()),
            subprocess: Arc::new(SubprocessSessionBackend::new()),
            plugin_providers: HashMap::new(),
        }
    }

    /// Construct a resolver that prefers discovered AO STDIO provider plugins for any tool
    /// whose name matches a discovered plugin's `provider_tool` (default: plugin name minus
    /// the `animus-provider-` prefix). In-tree backends remain as fallback for tools without
    /// a discovered plugin.
    pub fn with_plugin_discovery(project_root: &Path) -> Self {
        let mut resolver = Self::new();
        resolver.refresh_plugin_providers(project_root);
        resolver
    }

    /// Re-scan plugin discovery sources and replace the cached provider map.
    ///
    /// When `ANIMUS_PROVIDER_DISABLE_PLUGIN` is set to a truthy value (`1`, `true`,
    /// `yes`, case-insensitive) the cached provider map is cleared and discovery is
    /// skipped entirely. This is the documented escape hatch for forcing all
    /// dispatch through the in-tree backends — useful when an installed plugin is
    /// misbehaving and the daemon needs to keep running.
    pub fn refresh_plugin_providers(&mut self, project_root: &Path) {
        if plugin_discovery_disabled() {
            self.plugin_providers.clear();
            return;
        }
        self.plugin_providers = discover_provider_plugins(project_root)
            .into_iter()
            .map(|plugin| (plugin.provider_tool.to_ascii_lowercase(), plugin.into_backend()))
            .collect();
    }

    pub fn fallback_reason(&self, request: &SessionRequest) -> Option<String> {
        if self.plugin_providers.contains_key(&request.tool.to_ascii_lowercase()) {
            return None;
        }
        if request.tool.eq_ignore_ascii_case("claude")
            || request.tool.eq_ignore_ascii_case("codex")
            || request.tool.eq_ignore_ascii_case("gemini")
            || request.tool.eq_ignore_ascii_case("opencode")
            || request.tool.eq_ignore_ascii_case("oai-runner")
            || request.tool.eq_ignore_ascii_case("animus-oai-runner")
        {
            return None;
        }

        Some(format!("native backend not implemented for tool '{}'; using subprocess backend", request.tool))
    }

    pub fn resolve(&self, request: &SessionRequest) -> Arc<dyn SessionBackend> {
        let tool_lower = request.tool.to_ascii_lowercase();
        if let Some(plugin) = self.plugin_providers.get(&tool_lower) {
            if is_reserved_provider_tool(&tool_lower) {
                tracing::warn!(
                    plugin = %plugin.plugin_name,
                    tool = %tool_lower,
                    plugin_path = %plugin.binary_path.display(),
                    "plugin '{}' shadowing built-in {} backend at {}",
                    plugin.plugin_name,
                    tool_lower,
                    plugin.binary_path.display(),
                );
            }
            return plugin.clone();
        }
        if request.tool.eq_ignore_ascii_case("claude") {
            return self.claude.clone();
        }
        if request.tool.eq_ignore_ascii_case("codex") {
            return self.codex.clone();
        }
        if request.tool.eq_ignore_ascii_case("gemini") {
            return self.gemini.clone();
        }
        if request.tool.eq_ignore_ascii_case("opencode") {
            return self.opencode.clone();
        }
        if request.tool.eq_ignore_ascii_case("oai-runner") || request.tool.eq_ignore_ascii_case("animus-oai-runner") {
            return self.oai_runner.clone();
        }

        self.subprocess.clone()
    }

    pub async fn start_session(&self, mut request: SessionRequest) -> Result<SessionRun> {
        if let Some(reason) = self.fallback_reason(&request) {
            if let Some(extras) = request.extras.as_object_mut() {
                extras.insert("fallback_reason".to_string(), serde_json::Value::String(reason));
            }
        }

        self.resolve(&request).start_session(request).await
    }
}

impl Default for SessionBackendResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns `true` when the operator has opted out of installed-plugin dispatch
/// via `ANIMUS_PROVIDER_DISABLE_PLUGIN`.
///
/// Honors `1`, `true`, `yes`, `on` (case-insensitive) as the disable signal.
/// Anything else — including unset — leaves plugin discovery enabled.
fn plugin_discovery_disabled() -> bool {
    match std::env::var("ANIMUS_PROVIDER_DISABLE_PLUGIN") {
        Ok(raw) => matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::path::PathBuf;

    use cli_wrapper::session::{SessionEvent, SessionRequest};

    use super::SessionBackendResolver;

    #[test]
    fn resolver_reports_subprocess_fallback_reason() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "sh".to_string(),
            model: String::new(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        let reason = resolver.fallback_reason(&request).expect("fallback reason should exist");
        assert!(reason.contains("using subprocess backend"));
    }

    #[test]
    fn resolver_selects_claude_backend_without_fallback() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "claude".to_string(),
            model: "claude-sonnet".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        assert!(resolver.fallback_reason(&request).is_none());
        assert_eq!(resolver.resolve(&request).info().provider_tool, "claude");
    }

    #[test]
    fn resolver_selects_codex_backend_without_fallback() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "codex".to_string(),
            model: "gpt-5".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        assert!(resolver.fallback_reason(&request).is_none());
        assert_eq!(resolver.resolve(&request).info().provider_tool, "codex");
    }

    #[test]
    fn resolver_selects_gemini_backend_without_fallback() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "gemini".to_string(),
            model: "gemini-2.5-pro".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        assert!(resolver.fallback_reason(&request).is_none());
        assert_eq!(resolver.resolve(&request).info().provider_tool, "gemini");
    }

    #[test]
    fn resolver_selects_opencode_backend_without_fallback() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "opencode".to_string(),
            model: "glm-5".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        assert!(resolver.fallback_reason(&request).is_none());
        assert_eq!(resolver.resolve(&request).info().provider_tool, "opencode");
    }

    #[test]
    fn resolver_selects_oai_runner_backend_without_fallback() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "oai-runner".to_string(),
            model: "deepseek/deepseek-chat".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        assert!(resolver.fallback_reason(&request).is_none());
        assert_eq!(resolver.resolve(&request).info().provider_tool, "oai-runner");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn resolver_starts_session_with_fallback_reason() {
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "sh".to_string(),
            model: String::new(),
            prompt: String::new(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({
                "runtime_contract": {
                    "cli": {
                        "launch": {
                            "command": "sh",
                            "args": ["-c", "printf 'resolver\\n'"],
                            "prompt_via_stdin": false
                        }
                    }
                }
            }),
        };

        let mut run = resolver.start_session(request).await.expect("session should start");

        assert_eq!(run.selected_backend, "subprocess");
        assert!(run.fallback_reason.as_deref().is_some_and(|reason| reason.contains("using subprocess backend")));

        let _ = run.events.recv().await.expect("started event");
        let text = run.events.recv().await.expect("text event");
        assert_eq!(text, SessionEvent::TextDelta { text: "resolver".to_string() });
    }

    #[test]
    fn resolver_falls_back_to_in_tree_when_no_plugin_for_claude() {
        // No discovery surface configured — claude tool should land on the in-tree backend.
        let resolver = SessionBackendResolver::new();
        let request = SessionRequest {
            tool: "claude".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        let backend = resolver.resolve(&request);
        let info = backend.info();
        assert_eq!(info.provider_tool, "claude");
        assert!(
            !info.display_name.to_ascii_lowercase().contains("plugin"),
            "expected in-tree backend, got display_name={}",
            info.display_name
        );
    }

    #[test]
    fn reserved_provider_tools_includes_all_in_tree_backends() {
        for tool in ["claude", "codex", "gemini", "opencode", "oai-runner"] {
            assert!(super::is_reserved_provider_tool(tool), "{tool} should be considered reserved (built-in)");
            assert!(super::is_reserved_provider_tool(&tool.to_ascii_uppercase()));
        }
        assert!(!super::is_reserved_provider_tool("linear"));
        assert!(!super::is_reserved_provider_tool("custom-backend"));
    }

    #[test]
    fn resolver_emits_warning_when_plugin_shadows_builtin() {
        // The actual warn! is emitted through tracing; we verify the code path runs
        // (resolve picks the plugin) and the plugin path differs from the in-tree
        // backend's display name.
        let mut resolver = SessionBackendResolver::new();
        resolver.plugin_providers.insert(
            "claude".to_string(),
            std::sync::Arc::new(crate::plugin_backend::PluginSessionBackend::new(
                "animus-provider-claude-shadow",
                PathBuf::from("/tmp/animus-provider-claude-shadow"),
                "claude",
            )),
        );
        let request = SessionRequest {
            tool: "claude".to_string(),
            model: "claude-sonnet".to_string(),
            prompt: "hi".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };
        let backend = resolver.resolve(&request);
        let info = backend.info();
        // The plugin we registered claimed provider_tool="claude" and its display
        // name comes from the plugin path, not the in-tree label.
        assert_eq!(info.provider_tool, "claude");
        assert!(
            info.display_name.to_ascii_lowercase().contains("plugin"),
            "expected plugin backend to win over built-in; got display={}",
            info.display_name
        );
    }

    #[test]
    fn refresh_plugin_providers_skips_discovery_when_disabled() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Sanity: with the env unset, the discovery hook can populate (or leave
        // empty if no plugins are installed). We're not asserting on the populated
        // state — only that the disable knob is honored when set.
        std::env::set_var("ANIMUS_PROVIDER_DISABLE_PLUGIN", "1");
        let mut resolver = SessionBackendResolver::new();
        // Seed a fake provider so we can confirm refresh clears it under disable.
        resolver.plugin_providers.insert(
            "phantom".to_string(),
            std::sync::Arc::new(crate::plugin_backend::PluginSessionBackend::new(
                "phantom-plugin",
                PathBuf::from("/does/not/exist"),
                "phantom",
            )),
        );
        resolver.refresh_plugin_providers(&project_root);
        assert!(resolver.plugin_providers.is_empty(), "disable knob must clear plugin providers");
        std::env::remove_var("ANIMUS_PROVIDER_DISABLE_PLUGIN");
    }
}
