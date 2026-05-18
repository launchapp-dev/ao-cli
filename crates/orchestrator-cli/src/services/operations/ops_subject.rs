use std::path::Path;

use anyhow::{anyhow, Result};
use orchestrator_daemon_runtime::{resolve_subject_dispatch, SubjectPluginDispatch};
use serde::Serialize;
use serde_json::{json, Value};

use crate::{print_value, SubjectCommand, SubjectCreateArgs, SubjectGetArgs, SubjectListArgs, SubjectUpdateArgs};

#[derive(Debug, Serialize)]
struct SubjectCallResponse {
    kind: String,
    verb: &'static str,
    method: String,
    plugin_count: usize,
    result: Value,
}

pub(crate) async fn handle_subject(command: SubjectCommand, project_root: &str, json: bool) -> Result<()> {
    match command {
        SubjectCommand::List(args) => handle_subject_list(args, project_root, json).await,
        SubjectCommand::Get(args) => handle_subject_get(args, project_root, json).await,
        SubjectCommand::Create(args) => handle_subject_create(args, project_root, json).await,
        SubjectCommand::Update(args) => handle_subject_update(args, project_root, json).await,
    }
}

async fn handle_subject_list(args: SubjectListArgs, project_root: &str, json: bool) -> Result<()> {
    let kind = require_kind(&args.kind)?;
    let mut filter = serde_json::Map::new();
    filter.insert("kind".to_string(), json!([kind]));
    if let Some(status) = args.status.as_deref() {
        filter.insert("status".to_string(), json!([status]));
    }
    if let Some(limit) = args.limit {
        filter.insert("limit".to_string(), json!(limit));
    }
    let params = Some(Value::Object(filter));
    dispatch(kind, "list", params, project_root, json).await
}

async fn handle_subject_get(args: SubjectGetArgs, project_root: &str, json: bool) -> Result<()> {
    let kind = require_kind(&args.kind)?;
    let id = args.id.trim();
    if id.is_empty() {
        return Err(anyhow!("--id must not be empty"));
    }
    let params = Some(json!({ "id": id }));
    dispatch(kind, "get", params, project_root, json).await
}

async fn handle_subject_create(args: SubjectCreateArgs, project_root: &str, json: bool) -> Result<()> {
    let kind = require_kind(&args.kind)?;
    let title = args.title.trim();
    if title.is_empty() {
        return Err(anyhow!("--title must not be empty"));
    }
    let mut payload = serde_json::Map::new();
    payload.insert("title".to_string(), json!(title));
    if let Some(status) = args.status.as_deref() {
        payload.insert("status".to_string(), json!(status));
    }
    if let Some(priority) = args.priority.as_deref() {
        payload.insert("priority".to_string(), json!(priority));
    }
    if !args.labels.is_empty() {
        payload.insert("labels".to_string(), json!(args.labels));
    }
    if let Some(body) = args.body.as_deref() {
        payload.insert("body".to_string(), json!(body));
    }
    let params = Some(Value::Object(payload));
    dispatch(kind, "create", params, project_root, json).await
}

async fn handle_subject_update(args: SubjectUpdateArgs, project_root: &str, json: bool) -> Result<()> {
    let kind = require_kind(&args.kind)?;
    let id = args.id.trim();
    if id.is_empty() {
        return Err(anyhow!("--id must not be empty"));
    }
    let mut patch = serde_json::Map::new();
    if let Some(status) = args.status.as_deref() {
        patch.insert("status".to_string(), json!(status));
    }
    if let Some(priority) = args.priority.as_deref() {
        patch.insert("priority".to_string(), json!(priority));
    }
    if !args.labels.is_empty() {
        patch.insert("labels".to_string(), json!(args.labels));
    }
    if patch.is_empty() {
        return Err(anyhow!("subject update requires at least one of --status / --priority / --labels"));
    }
    let params = Some(json!({ "id": id, "patch": Value::Object(patch) }));
    dispatch(kind, "update", params, project_root, json).await
}

fn require_kind(raw: &str) -> Result<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("--kind must not be empty"));
    }
    if trimmed.contains('/') {
        return Err(anyhow!("--kind must not contain '/'"));
    }
    Ok(trimmed)
}

/// Build the daemon-side subject dispatch (spawns each installed
/// subject_backend plugin in one-shot mode), route `<kind>/<verb>`
/// through it, and render the response under the `animus.cli.v1`
/// envelope.
///
/// When the daemon is already running we will eventually want the CLI
/// to forward via MCP/IPC to reuse the existing plugin processes; for
/// v0.4.0 the CLI always spawns its own short-lived hosts so the
/// command works whether or not the daemon is up. The plugin host
/// shutdown is implicit (handles dropped at function return), matching
/// `animus plugin call`'s pattern.
async fn dispatch(kind: &str, verb: &'static str, params: Option<Value>, project_root: &str, json: bool) -> Result<()> {
    let resolution = resolve_subject_dispatch(Path::new(project_root)).await?;
    let method = format!("{kind}/{verb}");
    let result = route_or_not_found(&resolution.selected, &method, params).await?;
    print_value(
        SubjectCallResponse {
            kind: kind.to_string(),
            verb,
            method,
            plugin_count: resolution.selected.plugin_count(),
            result,
        },
        json,
    )
}

async fn route_or_not_found(dispatch: &SubjectPluginDispatch, method: &str, params: Option<Value>) -> Result<Value> {
    match dispatch.route_call(method, params).await {
        Ok(value) => Ok(value),
        Err(rpc_error) => Err(anyhow!("subject call '{method}' failed ({}): {}", rpc_error.code, rpc_error.message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_kind_rejects_empty_and_slash() {
        assert!(require_kind("").is_err(), "empty kind rejected");
        assert!(require_kind("   ").is_err(), "whitespace kind rejected");
        assert!(require_kind("task/list").is_err(), "kind containing '/' rejected");
        assert_eq!(require_kind(" task ").expect("trimmed"), "task");
    }

    #[tokio::test]
    async fn route_or_not_found_returns_not_found_for_empty_dispatch() {
        let dispatch = SubjectPluginDispatch::empty();
        let err = route_or_not_found(&dispatch, "task/list", None).await.expect_err("expect NotFound");
        let message = err.to_string();
        assert!(message.contains("task"), "error message names kind: {message}");
        assert!(
            message.contains("subject call") || message.contains("no subject backend"),
            "error includes routing context: {message}"
        );
    }
}
