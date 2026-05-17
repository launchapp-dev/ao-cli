use std::path::Path;

use orchestrator_config::skill_definition::SkillDefinition;
use orchestrator_config::skill_resolution::{list_available_skills, resolve_skill};
use orchestrator_config::skill_scoping::{load_skill_sources, AgentHostScope, SkillSourceOrigin};
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::*;

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct SkillListInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Optional source filter. Accepts: "builtin", "installed", "user",
    /// "project", "agent_host" (matches any agent-host source), or an
    /// agent host id like "claude-code", "codex", "cursor".
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct SkillGetInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Skill name (e.g. "code-review", "rust-architect").
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct SkillSearchInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Substring match on name/description/tags. Case-insensitive.
    query: String,
    /// Optional source filter (same vocabulary as `animus.skill.list`).
    #[serde(default)]
    source: Option<String>,
    /// Optional limit on returned matches (default 50).
    #[serde(default)]
    limit: Option<usize>,
}

const DEFAULT_SKILL_SEARCH_LIMIT: usize = 50;

fn normalize_source_filter(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_ascii_lowercase()).filter(|value| !value.is_empty())
}

fn matches_source_filter(origin: &SkillSourceOrigin, filter: &str) -> bool {
    match origin {
        SkillSourceOrigin::Builtin => matches!(filter, "builtin" | "built-in"),
        SkillSourceOrigin::Installed { .. } => filter == "installed",
        SkillSourceOrigin::User => filter == "user",
        SkillSourceOrigin::Project => filter == "project",
        SkillSourceOrigin::AgentHost { host, .. } => {
            filter == "agent_host" || filter == "agent-host" || filter == host.to_ascii_lowercase()
        }
    }
}

fn source_tag(origin: &SkillSourceOrigin) -> &'static str {
    match origin {
        SkillSourceOrigin::Builtin => "builtin",
        SkillSourceOrigin::Installed { .. } => "installed",
        SkillSourceOrigin::User => "user",
        SkillSourceOrigin::Project => "project",
        SkillSourceOrigin::AgentHost { .. } => "agent_host",
    }
}

fn agent_host_scope_str(scope: AgentHostScope) -> &'static str {
    match scope {
        AgentHostScope::Project => "project",
        AgentHostScope::Global => "global",
    }
}

fn source_detail(origin: &SkillSourceOrigin) -> Value {
    match origin {
        SkillSourceOrigin::Builtin => json!({}),
        SkillSourceOrigin::Installed { registry, source, version, integrity, artifact } => json!({
            "registry": registry,
            "source": source,
            "version": version,
            "integrity": integrity,
            "artifact": artifact,
        }),
        SkillSourceOrigin::User => json!({}),
        SkillSourceOrigin::Project => json!({}),
        SkillSourceOrigin::AgentHost { host, scope } => json!({
            "host": host,
            "scope": agent_host_scope_str(*scope),
            // The trust boundary stripped tool_policy / mcp_servers / env / extra_args
            // / capabilities / adapters / codex_config_overrides at parse time.
            // Surface that explicitly so callers know agent-host skills are
            // prompt-text-only.
            "structural_fields_stripped": true,
            "trust_tier": "prompt_text_only",
        }),
    }
}

fn category_label(definition: &SkillDefinition) -> Option<String> {
    definition
        .category
        .as_ref()
        .and_then(|category| serde_json::to_value(category).ok())
        .and_then(|value| value.as_str().map(|s| s.to_string()))
}

fn skill_summary(definition: &SkillDefinition, origin: &SkillSourceOrigin) -> Value {
    let mut payload = json!({
        "name": definition.name,
        "description": definition.description,
        "source": source_tag(origin),
        "source_detail": source_detail(origin),
        "tags": definition.tags,
    });
    if let Some(version) = definition.version.as_deref().filter(|value| !value.trim().is_empty()) {
        payload.as_object_mut().unwrap().insert("version".to_string(), json!(version));
    }
    if let Some(category) = category_label(definition) {
        payload.as_object_mut().unwrap().insert("category".to_string(), json!(category));
    }
    payload
}

fn skill_full(definition: &SkillDefinition, origin: &SkillSourceOrigin) -> Value {
    let mut payload = json!({
        "definition": definition,
        "source": source_tag(origin),
        "source_detail": source_detail(origin),
    });
    if let SkillSourceOrigin::AgentHost { .. } = origin {
        payload
            .as_object_mut()
            .unwrap()
            .insert("notice".to_string(), json!("agent-host skill: structural fields (tool_policy, mcp_servers, env, extra_args, capabilities, adapters, codex_config_overrides) were stripped at parse time. Only prompt text and prompt directives are trusted."));
    }
    payload
}

fn substring_match(haystack: &str, needle_lc: &str) -> bool {
    haystack.to_ascii_lowercase().contains(needle_lc)
}

fn skill_matches_query(definition: &SkillDefinition, query_lc: &str) -> bool {
    if substring_match(&definition.name, query_lc) {
        return true;
    }
    if substring_match(&definition.description, query_lc) {
        return true;
    }
    definition.tags.iter().any(|tag| substring_match(tag, query_lc))
}

fn collect_skills(project_root: &str, source_filter: Option<&str>) -> Result<Vec<Value>, McpError> {
    let sources = load_skill_sources(Path::new(project_root), None)
        .map_err(|err| McpError::internal_error(format!("failed to load skill sources: {err}"), None))?;
    let available = list_available_skills(&sources);
    let rows = available
        .into_iter()
        .filter(|resolved| match source_filter {
            Some(filter) => matches_source_filter(&resolved.source, filter),
            None => true,
        })
        .map(|resolved| skill_summary(&resolved.definition, &resolved.source))
        .collect();
    Ok(rows)
}

impl AoMcpServer {
    fn skill_project_root(&self, override_root: Option<String>) -> String {
        normalize_non_empty(override_root).unwrap_or_else(|| self.default_project_root.clone())
    }
}

#[tool_router(router = skill_tool_router, vis = "pub(super)")]
impl AoMcpServer {
    #[tool(
        name = "animus.skill.list",
        description = "List Animus skills discoverable from this project across every source: bundled built-ins, the `animus.core-skills` pack and other installed packs, registry-tracked installs, user-scoped (~/.ao/skills), project-scoped (.ao/skills), and agent-host probes (~/.claude/skills/, ~/.codex/skills/, etc.). Optional `source` filter accepts \"builtin\", \"installed\", \"user\", \"project\", \"agent_host\", or a host id like \"claude-code\". Each result carries provenance via `source` + `source_detail` so callers can reason about trust tier.",
        input_schema = ao_schema_for_type::<SkillListInput>()
    )]
    async fn ao_skill_list(&self, params: Parameters<SkillListInput>) -> Result<CallToolResult, McpError> {
        let SkillListInput { project_root, source } = params.0;
        let project_root = self.skill_project_root(project_root);
        let source_filter = normalize_source_filter(source);
        let skills = collect_skills(&project_root, source_filter.as_deref())?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.skill.list",
            "result": {
                "count": skills.len(),
                "project_root": project_root,
                "source_filter": source_filter,
                "skills": skills,
            }
        })))
    }

    #[tool(
        name = "animus.skill.get",
        description = "Resolve a skill by name and return its full SkillDefinition plus source provenance. Resolution honors the priority chain: project > user > installed/pack > builtin > agent-host. Returns the parsed definition (prompt, tool_policy, model, mcp_servers, capabilities, adapters, tags, etc.) under `definition`. For agent-host sources, structural fields are stripped at parse time and a `notice` field warns that only prompt text is trusted.",
        input_schema = ao_schema_for_type::<SkillGetInput>()
    )]
    async fn ao_skill_get(&self, params: Parameters<SkillGetInput>) -> Result<CallToolResult, McpError> {
        let SkillGetInput { project_root, name } = params.0;
        let project_root = self.skill_project_root(project_root);
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(McpError::invalid_params("name must not be empty", None));
        }
        let sources = load_skill_sources(Path::new(&project_root), None)
            .map_err(|err| McpError::internal_error(format!("failed to load skill sources: {err}"), None))?;
        let resolved = resolve_skill(&trimmed, &sources)
            .map_err(|err| McpError::invalid_params(format!("skill '{}' not found: {}", trimmed, err), None))?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.skill.get",
            "result": skill_full(&resolved.definition, &resolved.source),
        })))
    }

    #[tool(
        name = "animus.skill.search",
        description = "Case-insensitive substring search over discoverable skills. Matches the query against skill `name`, `description`, and `tags`. Supports the same `source` filter as `animus.skill.list` and a `limit` (default 50). Returns the same row shape as `animus.skill.list` for matched skills.",
        input_schema = ao_schema_for_type::<SkillSearchInput>()
    )]
    async fn ao_skill_search(&self, params: Parameters<SkillSearchInput>) -> Result<CallToolResult, McpError> {
        let SkillSearchInput { project_root, query, source, limit } = params.0;
        let project_root = self.skill_project_root(project_root);
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() {
            return Err(McpError::invalid_params("query must not be empty", None));
        }
        let query_lc = query_trimmed.to_ascii_lowercase();
        let source_filter = normalize_source_filter(source);
        let limit = limit.unwrap_or(DEFAULT_SKILL_SEARCH_LIMIT).max(1);

        let sources = load_skill_sources(Path::new(&project_root), None)
            .map_err(|err| McpError::internal_error(format!("failed to load skill sources: {err}"), None))?;
        let available = list_available_skills(&sources);

        let mut matches: Vec<Value> = Vec::new();
        let mut truncated = false;
        for resolved in available {
            if let Some(filter) = source_filter.as_deref() {
                if !matches_source_filter(&resolved.source, filter) {
                    continue;
                }
            }
            if !skill_matches_query(&resolved.definition, &query_lc) {
                continue;
            }
            if matches.len() >= limit {
                truncated = true;
                break;
            }
            matches.push(skill_summary(&resolved.definition, &resolved.source));
        }

        Ok(CallToolResult::structured(json!({
            "tool": "animus.skill.search",
            "result": {
                "query": query_trimmed,
                "count": matches.len(),
                "limit": limit,
                "truncated": truncated,
                "source_filter": source_filter,
                "skills": matches,
            }
        })))
    }
}

#[cfg(test)]
mod skill_tool_tests {
    use super::super::new_ao_mcp_server;
    use super::*;
    use protocol::test_utils::EnvVarGuard;
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;
    use std::fs;
    use tempfile::TempDir;

    fn structured(result: &rmcp::model::CallToolResult) -> Value {
        result.structured_content.clone().expect("expected structured_content on tool result")
    }

    fn data(result: &rmcp::model::CallToolResult) -> Value {
        let payload = structured(result);
        payload.get("result").cloned().expect("structured result should include `result`")
    }

    /// Set HOME to a fresh tempdir so we don't pick up the contributor's
    /// real ~/.claude/skills, ~/.codex/skills, or installed pack registry.
    fn isolated_home() -> (TempDir, EnvVarGuard) {
        let home = TempDir::new().expect("create HOME tempdir");
        let guard = EnvVarGuard::set("HOME", Some(home.path().to_str().expect("home path utf-8")));
        (home, guard)
    }

    fn project_root_for(tmp: &TempDir) -> String {
        tmp.path().to_string_lossy().to_string()
    }

    fn write_agent_host_claude_skill(home: &TempDir, name: &str, body: &str) {
        let dir = home.path().join(".claude").join("skills").join(name);
        fs::create_dir_all(&dir).expect("create claude skill dir");
        fs::write(dir.join("SKILL.md"), body).expect("write SKILL.md");
    }

    #[tokio::test]
    async fn skill_router_registers_three_tools() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));
        let names: Vec<String> = server.tool_router.list_all().into_iter().map(|tool| tool.name.to_string()).collect();
        assert!(names.contains(&"animus.skill.list".to_string()), "router missing animus.skill.list");
        assert!(names.contains(&"animus.skill.get".to_string()), "router missing animus.skill.get");
        assert!(names.contains(&"animus.skill.search".to_string()), "router missing animus.skill.search");
        assert!(server.tool_router.has_route("animus.skill.list"));
    }

    #[tokio::test]
    async fn skill_list_returns_bundled_catalog() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_skill_list(Parameters(SkillListInput { project_root: None, source: None }))
            .await
            .expect("list should succeed");
        let payload = data(&result);
        let count = payload.pointer("/count").and_then(Value::as_u64).expect("count present");
        // The bundled `animus.core-skills` catalog ships 27 entries (19 unique +
        // alias re-exports). When the pack is installed those surface as
        // `installed`; otherwise the legacy built-in fallback contributes them.
        assert!(count >= 27, "expected >=27 skills in default project (got {count})");
        let skills = payload.pointer("/skills").and_then(Value::as_array).expect("skills array");
        assert!(skills.iter().any(|skill| skill.get("name").and_then(Value::as_str) == Some("code-review")));
        assert!(skills.iter().any(|skill| skill.get("name").and_then(Value::as_str) == Some("implementation")));
    }

    #[tokio::test]
    async fn skill_list_filters_by_source_builtin() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_skill_list(Parameters(SkillListInput { project_root: None, source: Some("builtin".to_string()) }))
            .await
            .expect("list builtin");
        let payload = data(&result);
        let skills = payload.pointer("/skills").and_then(Value::as_array).expect("skills");
        // Every returned row must carry source = builtin (zero rows is also
        // acceptable when the pack supplies these as `installed` instead).
        for skill in skills {
            assert_eq!(skill.get("source").and_then(Value::as_str), Some("builtin"));
        }
    }

    #[tokio::test]
    async fn skill_list_filters_by_agent_host() {
        let (home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        write_agent_host_claude_skill(
            &home,
            "external",
            "---\nname: external\ndescription: External Claude skill\n---\nBody.\n",
        );
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_skill_list(Parameters(SkillListInput { project_root: None, source: Some("agent_host".to_string()) }))
            .await
            .expect("list agent_host");
        let payload = data(&result);
        let skills = payload.pointer("/skills").and_then(Value::as_array).expect("skills");
        assert!(
            skills.iter().any(|skill| skill.get("name").and_then(Value::as_str) == Some("external")),
            "agent-host filter should surface the SKILL.md we wrote under HOME/.claude/skills/"
        );
        let row = skills
            .iter()
            .find(|skill| skill.get("name").and_then(Value::as_str) == Some("external"))
            .expect("external skill row");
        assert_eq!(row.get("source").and_then(Value::as_str), Some("agent_host"));
        let detail = row.get("source_detail").expect("source_detail");
        assert_eq!(detail.get("host").and_then(Value::as_str), Some("claude-code"));
        assert_eq!(detail.get("scope").and_then(Value::as_str), Some("global"));
        assert_eq!(detail.get("structural_fields_stripped").and_then(Value::as_bool), Some(true));
    }

    #[tokio::test]
    async fn skill_list_filters_by_host_id() {
        let (home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        write_agent_host_claude_skill(
            &home,
            "claude-only",
            "---\nname: claude-only\ndescription: claude scoped\n---\nBody.\n",
        );
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_skill_list(Parameters(SkillListInput { project_root: None, source: Some("claude-code".to_string()) }))
            .await
            .expect("filter by host id");
        let payload = data(&result);
        let skills = payload.pointer("/skills").and_then(Value::as_array).expect("skills");
        assert!(skills.iter().any(|skill| skill.get("name").and_then(Value::as_str) == Some("claude-only")));
        for skill in skills {
            let detail = skill.get("source_detail").expect("source_detail");
            assert_eq!(detail.get("host").and_then(Value::as_str), Some("claude-code"));
        }
    }

    #[tokio::test]
    async fn skill_get_returns_full_definition_for_code_review() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_skill_get(Parameters(SkillGetInput { project_root: None, name: "code-review".to_string() }))
            .await
            .expect("get code-review");
        let payload = data(&result);
        let definition = payload.pointer("/definition").expect("definition payload");
        assert_eq!(definition.get("name").and_then(Value::as_str), Some("code-review"));
        // Structural fields from the bundled YAML must flow through on a trusted source.
        assert!(
            definition.get("tool_policy").is_some_and(|policy| !policy.is_null()),
            "code-review tool_policy must be present"
        );
        assert!(
            definition.get("model").is_some_and(|model| !model.is_null()),
            "code-review model preference must be present"
        );
        let source = payload.get("source").and_then(Value::as_str).expect("source tag");
        assert!(source == "installed" || source == "builtin", "expected installed or builtin, got {source}");
    }

    #[tokio::test]
    async fn skill_get_returns_error_for_unknown_skill() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let err = server
            .ao_skill_get(Parameters(SkillGetInput { project_root: None, name: "nonexistent-skill-xyz".to_string() }))
            .await
            .expect_err("unknown skill should be an MCP error");
        let message = err.message.to_string();
        assert!(message.contains("nonexistent-skill-xyz"), "error should mention skill name, got {message}");
    }

    #[tokio::test]
    async fn skill_search_finds_by_substring_in_name() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_skill_search(Parameters(SkillSearchInput {
                project_root: None,
                query: "review".to_string(),
                source: None,
                limit: None,
            }))
            .await
            .expect("search");
        let payload = data(&result);
        let skills = payload.pointer("/skills").and_then(Value::as_array).expect("skills array");
        assert!(
            skills.iter().any(|skill| skill.get("name").and_then(Value::as_str) == Some("code-review")),
            "search for 'review' must include code-review"
        );
        let count = payload.pointer("/count").and_then(Value::as_u64).expect("count");
        assert!(count >= 1);
    }

    #[tokio::test]
    async fn skill_search_respects_limit_and_marks_truncated() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        // Empty single-character query against the catalog matches many skills;
        // use a real character that appears in many skill descriptions.
        let result = server
            .ao_skill_search(Parameters(SkillSearchInput {
                project_root: None,
                query: "a".to_string(),
                source: None,
                limit: Some(2),
            }))
            .await
            .expect("search with small limit");
        let payload = data(&result);
        let skills = payload.pointer("/skills").and_then(Value::as_array).expect("skills");
        assert!(skills.len() <= 2, "limit should cap returned skills");
        assert_eq!(payload.pointer("/limit").and_then(Value::as_u64), Some(2));
        // truncated flag should be true when there are more matches than limit
        assert_eq!(payload.pointer("/truncated").and_then(Value::as_bool), Some(true));
    }

    #[tokio::test]
    async fn skill_search_rejects_empty_query() {
        let (_home, _guard) = isolated_home();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let err = server
            .ao_skill_search(Parameters(SkillSearchInput {
                project_root: None,
                query: "   ".to_string(),
                source: None,
                limit: None,
            }))
            .await
            .expect_err("empty query should be rejected");
        assert!(err.message.contains("query"), "error should mention query: {}", err.message);
    }
}
