use super::*;
use serde_json::json;

#[test]
fn mcp_tool_enforcement_defaults_to_animus_prefix_when_endpoint_is_set() {
    let contract = json!({
        "cli": { "capabilities": { "supports_mcp": true } },
        "mcp": { "endpoint": "http://127.0.0.1:3101/mcp/animus" }
    });
    let enforcement = resolve_mcp_tool_enforcement(Some(&contract));
    assert!(enforcement.enabled);
    assert_eq!(enforcement.endpoint.as_deref(), Some("http://127.0.0.1:3101/mcp/animus"));
    assert_eq!(enforcement.agent_id, "animus");
    assert!(enforcement.allowed_prefixes.iter().any(|prefix| prefix == "animus."));
    assert!(enforcement.allowed_prefixes.iter().any(|prefix| prefix == "mcp__animus__"));
}

#[test]
fn mcp_tool_enforcement_rejects_non_matching_tool_calls() {
    let contract = json!({
        "cli": { "capabilities": { "supports_mcp": true } },
        "mcp": {
            "endpoint": "http://127.0.0.1:3101/mcp/animus",
            "enforce_only": true,
            "allowed_tool_prefixes": ["animus."]
        }
    });
    let enforcement = resolve_mcp_tool_enforcement(Some(&contract));
    assert!(is_tool_call_allowed("animus.task.list", &json!({}), &enforcement));
    assert!(is_tool_call_allowed("phase_transition", &json!({}), &enforcement));
    assert!(!is_tool_call_allowed("Bash", &json!({}), &enforcement));
    assert!(!is_tool_call_allowed("stories-search", &json!({ "server": "shortcut" }), &enforcement));
    assert!(is_tool_call_allowed("requirements-get", &json!({ "server": "animus" }), &enforcement));
    assert!(is_tool_call_allowed("list_mcp_resources", &json!({}), &enforcement));
    assert!(is_tool_call_allowed("list_mcp_resources", &json!({ "server": "codex" }), &enforcement));
}

#[test]
fn native_mcp_policy_rejects_unknown_cli_when_enforced() {
    let mut invocation = LaunchInvocation {
        command: "unknown-cli".to_string(),
        args: vec!["hello".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: None,
        stdio: Some(McpStdioConfig {
            command: "/path/to/animus/target/debug/animus".to_string(),
            args: vec![
                "--project-root".to_string(),
                "/path/to/project".to_string(),
                "mcp".to_string(),
                "serve".to_string(),
            ],
        }),
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-1".to_string());

    let err = apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect_err("unknown provider should fail closed");

    assert!(err.to_string().contains("no native enforcement adapter"));
}

#[test]
fn native_mcp_policy_requires_transport_when_enforced() {
    let mut invocation = LaunchInvocation {
        command: "claude".to_string(),
        args: vec!["--print".to_string(), "hello".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: None,
        stdio: None,
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-1b".to_string());

    let err = apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect_err("missing transport should fail closed");

    assert!(err.to_string().contains("neither mcp.endpoint nor mcp.stdio.command"));
}

#[test]
fn native_mcp_policy_adds_codex_mcp_server_override() {
    let mut invocation = LaunchInvocation {
        command: "codex".to_string(),
        args: vec!["exec".to_string(), "--json".to_string(), "hello".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: Some("http://127.0.0.1:3101/mcp/animus".to_string()),
        stdio: None,
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-2".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("codex policy should apply");

    let joined = invocation.args.join(" ");
    assert!(joined.contains("mcp_servers.animus.url=\"http://127.0.0.1:3101/mcp/animus\""));
}

#[test]
fn native_mcp_policy_configures_claude_permission_mode() {
    let mut invocation = LaunchInvocation {
        command: "claude".to_string(),
        args: vec!["--print".to_string(), "hello".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: Some("http://127.0.0.1:3101/mcp/animus".to_string()),
        stdio: None,
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-claude".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("claude policy should apply");

    assert!(invocation
        .args
        .windows(2)
        .any(|pair| { pair[0] == "--permission-mode" && pair[1] == "bypassPermissions" }));
    assert!(invocation.args.iter().any(|arg| arg == "--strict-mcp-config"));
    assert!(!invocation.args.iter().any(|arg| arg == "--tools"));
}

#[test]
fn native_mcp_policy_preserves_primary_server_when_additional_server_name_collides() {
    let mut invocation = LaunchInvocation {
        command: "claude".to_string(),
        args: vec!["--print".to_string(), "hello".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: None,
        stdio: Some(McpStdioConfig {
            command: "/path/to/animus/target/debug/animus".to_string(),
            args: vec![
                "--project-root".to_string(),
                "/path/to/project".to_string(),
                "mcp".to_string(),
                "serve".to_string(),
            ],
        }),
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: vec![AdditionalMcpServer {
            name: "animus".to_string(),
            command: "animus".to_string(),
            args: vec!["mcp".to_string(), "serve".to_string()],
            env: HashMap::new(),
            url: None,
        }],
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-claude-collision".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("claude policy should preserve the primary MCP server");

    let mcp_config = invocation
        .args
        .windows(2)
        .find_map(|pair| (pair[0] == "--mcp-config").then(|| pair[1].clone()))
        .expect("strict claude config should be present");
    let parsed: serde_json::Value = serde_json::from_str(&mcp_config).expect("claude mcp config should parse");

    assert_eq!(
        parsed.pointer("/mcpServers/animus/command").and_then(serde_json::Value::as_str),
        Some("/path/to/animus/target/debug/animus")
    );
    assert_eq!(
        parsed.pointer("/mcpServers/animus/args").and_then(serde_json::Value::as_array).cloned(),
        Some(vec![
            serde_json::Value::String("--project-root".to_string()),
            serde_json::Value::String("/path/to/project".to_string()),
            serde_json::Value::String("mcp".to_string()),
            serde_json::Value::String("serve".to_string()),
        ])
    );
}

#[test]
fn parse_codex_mcp_server_names_extracts_safe_names() {
    let payload = r#"
            [
              {"name":"animus"},
              {"name":"shortcut"},
              {"name":"bad.name"},
              {"name":"with space"}
            ]
        "#;
    assert_eq!(parse_codex_mcp_server_names(payload), vec!["animus".to_string(), "shortcut".to_string()]);
}

#[test]
fn codex_native_lockdown_disables_non_target_servers() {
    let mut args = vec!["exec".to_string(), "--json".to_string(), "hello".to_string()];
    let configured_servers = vec!["shortcut".to_string(), "animus".to_string()];

    apply_codex_native_mcp_lockdown(
        &mut args,
        McpServerTransport::Http("http://127.0.0.1:3101/mcp/animus"),
        "animus",
        &configured_servers,
        &[],
    );

    let joined = args.join(" ");
    assert!(joined.contains("mcp_servers.shortcut.enabled=false"));
    assert!(joined.contains("mcp_servers.animus.url=\"http://127.0.0.1:3101/mcp/animus\""));
    assert!(!joined.contains("mcp_servers.animus.enabled=false"));
}

#[test]
fn codex_native_lockdown_sets_stdio_transport_when_configured() {
    let mut args = vec!["exec".to_string(), "--json".to_string(), "hello".to_string()];

    apply_codex_native_mcp_lockdown(
        &mut args,
        McpServerTransport::Stdio {
            command: "/path/to/animus/target/debug/animus",
            args: &[
                "--project-root".to_string(),
                "/path/to/project".to_string(),
                "mcp".to_string(),
                "serve".to_string(),
            ],
        },
        "animus",
        &[],
        &[],
    );

    let joined = args.join(" ");
    assert!(joined.contains("mcp_servers.animus.command=\"/path/to/animus/target/debug/animus\""));
    assert!(joined.contains("mcp_servers.animus.args=[\"--project-root\", \"/path/to/project\", \"mcp\", \"serve\"]"));
    assert!(joined.contains("mcp_servers.animus.enabled=true"));
}

#[test]
fn native_mcp_policy_sets_gemini_system_settings_path_for_stdio_transport() {
    let mut invocation = LaunchInvocation {
        command: "gemini".to_string(),
        args: vec!["--output-format".to_string(), "json".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: None,
        stdio: Some(McpStdioConfig {
            command: "/path/to/animus/target/debug/animus".to_string(),
            args: vec![
                "--project-root".to_string(),
                "/path/to/project".to_string(),
                "mcp".to_string(),
                "serve".to_string(),
            ],
        }),
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-3".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("gemini policy should apply");

    let settings_path =
        env.get("GEMINI_CLI_SYSTEM_SETTINGS_PATH").expect("gemini settings path should be set").to_string();
    assert!(invocation.args.windows(2).any(|pair| pair[0] == "--allowed-mcp-server-names" && pair[1] == "animus"));
    let settings = std::fs::read_to_string(&settings_path).expect("read gemini settings");
    assert!(
        settings.contains("\"ANIMUS_MCP_SCHEMA_DRAFT\":\"draft07\""),
        "expected draft07 env in gemini settings, got: {settings}"
    );
    assert!(settings.contains("\"type\":\"stdio\""), "expected stdio transport in gemini settings, got: {settings}");
}

#[test]
fn native_mcp_policy_sets_gemini_http_settings_without_schema_override() {
    let mut invocation = LaunchInvocation {
        command: "gemini".to_string(),
        args: vec!["--output-format".to_string(), "json".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: Some("http://127.0.0.1:3101/mcp/animus".to_string()),
        stdio: None,
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-3-http".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("gemini policy should apply");

    let settings_path =
        env.get("GEMINI_CLI_SYSTEM_SETTINGS_PATH").expect("gemini settings path should be set").to_string();
    let settings = std::fs::read_to_string(&settings_path).expect("read gemini settings");
    assert!(settings.contains("\"type\":\"http\""), "expected http transport in gemini settings, got: {settings}");
    assert!(
        settings.contains("\"url\":\"http://127.0.0.1:3101/mcp/animus\""),
        "expected ao endpoint in gemini settings, got: {settings}"
    );
    assert!(
        !settings.contains("\"ANIMUS_MCP_SCHEMA_DRAFT\""),
        "did not expect schema override env for gemini http transport, got: {settings}"
    );
}

#[test]
fn native_mcp_policy_sets_opencode_local_mcp_command_array() {
    let mut invocation = LaunchInvocation {
        command: "opencode".to_string(),
        args: vec!["run".to_string(), "--format".to_string(), "json".to_string()],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: None,
        stdio: Some(McpStdioConfig {
            command: "/path/to/animus/target/debug/animus".to_string(),
            args: vec![
                "--project-root".to_string(),
                "/path/to/project".to_string(),
                "mcp".to_string(),
                "serve".to_string(),
            ],
        }),
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-opencode".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("opencode policy should apply");

    let config_raw = env.get("OPENCODE_CONFIG_CONTENT").expect("opencode config should be provided");
    let parsed: serde_json::Value = serde_json::from_str(config_raw).expect("opencode config should be valid JSON");
    assert_eq!(parsed.pointer("/mcp/animus/type").and_then(serde_json::Value::as_str), Some("local"));
    assert_eq!(
        parsed.pointer("/mcp/animus/command/0").and_then(serde_json::Value::as_str),
        Some("/path/to/animus/target/debug/animus")
    );
    assert_eq!(parsed.pointer("/mcp/animus/command/4").and_then(serde_json::Value::as_str), Some("serve"));
    assert!(parsed.pointer("/mcp/animus/args").is_none());
}

#[test]
fn native_mcp_policy_inserts_oai_runner_mcp_config_after_run_subcommand() {
    let mut invocation = LaunchInvocation {
        command: "animus-oai-runner".to_string(),
        args: vec![
            "run".to_string(),
            "-m".to_string(),
            "minimax/MiniMax-M2.5".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "hello".to_string(),
        ],
        env: Default::default(),
        prompt_via_stdin: false,
    };
    let enforcement = McpToolEnforcement {
        enabled: true,
        endpoint: None,
        stdio: Some(McpStdioConfig {
            command: "/path/to/animus/target/debug/animus".to_string(),
            args: vec![
                "mcp".to_string(),
                "serve".to_string(),
                "--project-root".to_string(),
                "/path/to/project".to_string(),
            ],
        }),
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string()],
        tool_policy_allow: Vec::new(),
        tool_policy_deny: Vec::new(),
        additional_servers: Vec::new(),
    };
    let mut env = HashMap::new();
    let mut cleanup = TempPathCleanup::default();
    let run_id = RunId("run-oai-runner".to_string());

    apply_native_mcp_policy(&mut invocation, &enforcement, &mut env, &run_id, &mut cleanup)
        .expect("oai-runner policy should apply");

    let mcp_idx =
        invocation.args.iter().position(|arg| arg == "--mcp-config").expect("mcp config flag should be present");
    assert_eq!(invocation.args.first().map(String::as_str), Some("run"));
    assert_eq!(mcp_idx, 1, "mcp config should follow the run subcommand");
}

fn enforcement_with_tool_policy(allow: Vec<&str>, deny: Vec<&str>) -> McpToolEnforcement {
    McpToolEnforcement {
        enabled: true,
        endpoint: Some("http://127.0.0.1:3101/mcp/animus".to_string()),
        stdio: None,
        agent_id: "animus".to_string(),
        allowed_prefixes: vec!["animus.".to_string(), "mcp__animus__".to_string()],
        tool_policy_allow: allow.into_iter().map(ToString::to_string).collect(),
        tool_policy_deny: deny.into_iter().map(ToString::to_string).collect(),
        additional_servers: Vec::new(),
    }
}

#[test]
fn tool_policy_empty_permits_all_prefixed_tools() {
    let enforcement = enforcement_with_tool_policy(vec![], vec![]);
    assert!(is_tool_call_allowed("animus.task.list", &serde_json::json!({}), &enforcement));
    assert!(is_tool_call_allowed("animus.daemon.start", &serde_json::json!({}), &enforcement));
}

#[test]
fn tool_policy_allowlist_restricts_to_matching() {
    let enforcement = enforcement_with_tool_policy(vec!["animus.task.*"], vec![]);
    assert!(is_tool_call_allowed("animus.task.list", &serde_json::json!({}), &enforcement));
    assert!(is_tool_call_allowed("animus.task.get", &serde_json::json!({}), &enforcement));
    assert!(!is_tool_call_allowed("animus.daemon.start", &serde_json::json!({}), &enforcement));
}

#[test]
fn tool_policy_denylist_blocks_matching() {
    let enforcement = enforcement_with_tool_policy(vec![], vec!["animus.daemon.*"]);
    assert!(is_tool_call_allowed("animus.task.list", &serde_json::json!({}), &enforcement));
    assert!(!is_tool_call_allowed("animus.daemon.start", &serde_json::json!({}), &enforcement));
    assert!(!is_tool_call_allowed("animus.daemon.stop", &serde_json::json!({}), &enforcement));
}

#[test]
fn tool_policy_deny_overrides_allow() {
    let enforcement = enforcement_with_tool_policy(vec!["animus.*"], vec!["animus.daemon.*"]);
    assert!(is_tool_call_allowed("animus.task.list", &serde_json::json!({}), &enforcement));
    assert!(!is_tool_call_allowed("animus.daemon.start", &serde_json::json!({}), &enforcement));
}

#[test]
fn tool_policy_does_not_affect_phase_transition() {
    let enforcement = enforcement_with_tool_policy(vec!["animus.task.*"], vec![]);
    assert!(is_tool_call_allowed("phase_transition", &serde_json::json!({}), &enforcement));
}

#[test]
fn tool_policy_glob_match_basics() {
    assert!(tool_policy_glob_match("animus.*", "animus.task"));
    assert!(tool_policy_glob_match("animus.task.*", "animus.task.list"));
    assert!(tool_policy_glob_match("*", "anything"));
    assert!(!tool_policy_glob_match("animus.task.*", "animus.daemon.start"));
    assert!(tool_policy_glob_match("animus.task.list", "animus.task.list"));
    assert!(!tool_policy_glob_match("animus.task.list", "animus.task.get"));
}

#[test]
fn resolve_enforcement_parses_tool_policy_from_contract() {
    let contract = serde_json::json!({
        "cli": {
            "name": "claude",
            "capabilities": { "supports_mcp": true, "supports_tool_use": true },
            "launch": { "args": ["--print", "hello"] }
        },
        "mcp": {
            "endpoint": "http://127.0.0.1:3101/mcp/animus",
            "agent_id": "animus",
            "tool_policy": {
                "allow": ["animus.task.*", "animus.workflow.*"],
                "deny": ["animus.task.delete"]
            }
        }
    });
    let enforcement = resolve_mcp_tool_enforcement(Some(&contract));
    assert_eq!(enforcement.tool_policy_allow, vec!["animus.task.*", "animus.workflow.*"]);
    assert_eq!(enforcement.tool_policy_deny, vec!["animus.task.delete"]);
    assert!(is_tool_call_allowed("animus.task.list", &serde_json::json!({}), &enforcement));
    assert!(!is_tool_call_allowed("animus.task.delete", &serde_json::json!({}), &enforcement));
    assert!(!is_tool_call_allowed("animus.daemon.start", &serde_json::json!({}), &enforcement));
}

#[test]
fn resolve_enforcement_parses_additional_servers() {
    let contract = serde_json::json!({
        "cli": {
            "name": "claude",
            "capabilities": { "supports_mcp": true, "supports_tool_use": true },
            "launch": { "args": ["--print", "hello"] }
        },
        "mcp": {
            "endpoint": "http://127.0.0.1:3101/mcp/animus",
            "agent_id": "animus",
            "additional_servers": {
                "my-db": {
                    "command": "/usr/local/bin/db-mcp",
                    "args": ["--port", "5432"],
                    "env": { "DB_HOST": "localhost" }
                }
            }
        }
    });
    let enforcement = resolve_mcp_tool_enforcement(Some(&contract));
    assert_eq!(enforcement.additional_servers.len(), 1);
    assert_eq!(enforcement.additional_servers[0].name, "my-db");
    assert_eq!(enforcement.additional_servers[0].command, "/usr/local/bin/db-mcp");
    assert_eq!(enforcement.additional_servers[0].args, vec!["--port", "5432"]);
    assert_eq!(enforcement.additional_servers[0].env.get("DB_HOST").map(String::as_str), Some("localhost"));
}

#[test]
fn claude_lockdown_includes_additional_servers() {
    let mut args = vec!["--print".to_string(), "hello".to_string()];
    let additional = vec![AdditionalMcpServer {
        name: "my-db".to_string(),
        command: "/usr/local/bin/db-mcp".to_string(),
        args: vec!["--port".to_string(), "5432".to_string()],
        env: HashMap::from([("DB_HOST".to_string(), "localhost".to_string())]),
        url: None,
    }];
    apply_claude_native_mcp_lockdown(
        &mut args,
        McpServerTransport::Stdio { command: "/usr/local/bin/animus", args: &["mcp".to_string(), "serve".to_string()] },
        "animus",
        &additional,
    );
    let joined = args.join(" ");
    assert!(joined.contains("mcpServers"));
    let mcp_config_idx = args.iter().position(|a| a == "--mcp-config").unwrap();
    let config_json: serde_json::Value = serde_json::from_str(&args[mcp_config_idx + 1]).unwrap();
    assert!(config_json.pointer("/mcpServers/animus").is_some());
    assert!(config_json.pointer("/mcpServers/my-db").is_some());
    assert_eq!(
        config_json.pointer("/mcpServers/my-db/command").and_then(serde_json::Value::as_str),
        Some("/usr/local/bin/db-mcp")
    );
    assert_eq!(
        config_json.pointer("/mcpServers/my-db/env/DB_HOST").and_then(serde_json::Value::as_str),
        Some("localhost")
    );
}
