use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use futures_util::{pin_mut, stream, StreamExt};
use tokio_util::sync::CancellationToken;

use crate::api::types::ToolCall;
use crate::tools::executor;

const DEFAULT_READ_ONLY_TOOL_CONCURRENCY: usize = 4;
const INTERRUPTED_RESULT: &str = "[result unavailable — session was interrupted]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadOnlyToolCallOutcome {
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
    pub(crate) result: String,
    pub(crate) error: Option<String>,
    pub(crate) emit_result: bool,
}

impl ReadOnlyToolCallOutcome {
    fn success(tool_call_id: String, tool_name: String, result: String) -> Self {
        Self { tool_call_id, tool_name, result, error: None, emit_result: true }
    }

    fn error(tool_call_id: String, tool_name: String, error: String) -> Self {
        Self { tool_call_id, tool_name, result: format!("Error: {}", error), error: Some(error), emit_result: true }
    }

    fn interrupted(tool_call_id: String, tool_name: String) -> Self {
        Self { tool_call_id, tool_name, result: INTERRUPTED_RESULT.to_string(), error: None, emit_result: false }
    }
}

fn read_only_tool_concurrency_limit() -> usize {
    static LIMIT: OnceLock<usize> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("AO_OAI_RUNNER_READ_ONLY_TOOL_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_READ_ONLY_TOOL_CONCURRENCY)
    })
}

pub(crate) fn is_parallel_read_only_tool(name: &str) -> bool {
    matches!(name, "read_file" | "list_files" | "search_files")
}

pub(crate) fn parallel_read_only_batch_len(tool_calls: &[ToolCall], start_idx: usize) -> usize {
    let mut len = 0;
    while let Some(tool_call) = tool_calls.get(start_idx + len) {
        if !is_parallel_read_only_tool(&tool_call.function.name) {
            break;
        }
        len += 1;
    }
    len
}

pub(crate) async fn execute_parallel_read_only_tools(
    tool_calls: &[ToolCall],
    working_dir: &Path,
    cancel_token: CancellationToken,
) -> Result<Vec<ReadOnlyToolCallOutcome>> {
    if tool_calls.is_empty() {
        return Ok(Vec::new());
    }

    let concurrency_limit = read_only_tool_concurrency_limit().min(tool_calls.len()).max(1);
    let mut completed: Vec<Option<ReadOnlyToolCallOutcome>> = vec![None; tool_calls.len()];
    let pending = stream::iter(tool_calls.iter().enumerate().map(|(idx, tool_call)| {
        let tool_call_id = tool_call.id.clone();
        let tool_name = tool_call.function.name.clone();
        let args = tool_call.function.arguments.clone();
        let cancel_token = cancel_token.clone();
        async move {
            let outcome = if cancel_token.is_cancelled() {
                ReadOnlyToolCallOutcome::interrupted(tool_call_id, tool_name)
            } else {
                match executor::execute_tool(&tool_name, &args, working_dir).await {
                    Ok(result) => ReadOnlyToolCallOutcome::success(tool_call_id, tool_name, result),
                    Err(err) => ReadOnlyToolCallOutcome::error(tool_call_id, tool_name, err.to_string()),
                }
            };
            (idx, outcome)
        }
    }))
    .buffer_unordered(concurrency_limit);
    pin_mut!(pending);

    let mut cancelled = false;
    loop {
        let next = tokio::select! {
            maybe = pending.next() => maybe,
            _ = cancel_token.cancelled(), if !cancelled => {
                cancelled = true;
                None
            }
        };

        let Some((idx, outcome)) = next else {
            break;
        };
        completed[idx] = Some(outcome);
    }

    if cancelled {
        for (idx, tool_call) in tool_calls.iter().enumerate() {
            if completed[idx].is_none() {
                completed[idx] =
                    Some(ReadOnlyToolCallOutcome::interrupted(tool_call.id.clone(), tool_call.function.name.clone()));
            }
        }
    }

    Ok(completed.into_iter().map(|item| item.expect("all outcomes must be populated")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::api::types::{FunctionCall, ToolCall};

    fn tool_call(id: &str, name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            type_: "function".to_string(),
            function: FunctionCall { name: name.to_string(), arguments: arguments.to_string() },
        }
    }

    #[test]
    fn recognizes_parallel_read_only_tools() {
        assert!(is_parallel_read_only_tool("read_file"));
        assert!(is_parallel_read_only_tool("list_files"));
        assert!(is_parallel_read_only_tool("search_files"));
        assert!(!is_parallel_read_only_tool("write_file"));
        assert!(!is_parallel_read_only_tool("execute_command"));
    }

    #[tokio::test]
    async fn executes_parallel_read_only_tools_in_input_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("alpha.txt"), "alpha").unwrap();
        std::fs::write(dir.path().join("beta.txt"), "beta").unwrap();

        let token = CancellationToken::new();
        let calls = vec![
            tool_call("call_1", "read_file", r#"{"path":"alpha.txt"}"#),
            tool_call("call_2", "read_file", r#"{"path":"beta.txt"}"#),
        ];

        let outcomes = execute_parallel_read_only_tools(&calls, dir.path(), token).await.unwrap();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].tool_call_id, "call_1");
        assert_eq!(outcomes[1].tool_call_id, "call_2");
        assert!(outcomes[0].result.contains("alpha"));
        assert!(outcomes[1].result.contains("beta"));
    }

    #[test]
    fn parallel_read_only_batch_len_stops_at_first_non_read_only_tool() {
        let calls = vec![
            tool_call("call_1", "read_file", r#"{"path":"alpha.txt"}"#),
            tool_call("call_2", "list_files", r#"{"pattern":"**/*.rs"}"#),
            tool_call("call_3", "execute_command", r#"{"command":"echo hi"}"#),
            tool_call("call_4", "search_files", r#"{"pattern":"fn main"}"#),
        ];

        assert_eq!(parallel_read_only_batch_len(&calls, 0), 2);
        assert_eq!(parallel_read_only_batch_len(&calls, 2), 0);
        assert_eq!(parallel_read_only_batch_len(&calls, 3), 1);
    }

    #[test]
    fn interrupted_outcomes_preserve_order_and_emit_flags() {
        let success =
            ReadOnlyToolCallOutcome::success("call_1".to_string(), "read_file".to_string(), "alpha".to_string());
        let interrupted = ReadOnlyToolCallOutcome::interrupted("call_2".to_string(), "read_file".to_string());

        assert_eq!(success.tool_call_id, "call_1");
        assert!(success.emit_result);
        assert!(!interrupted.emit_result);
        assert_eq!(interrupted.result, INTERRUPTED_RESULT);
        assert_eq!(interrupted.tool_name, "read_file");
        assert_eq!(interrupted.error, None);
    }

    #[tokio::test]
    async fn cancelled_batch_marks_every_call_as_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("alpha.txt"), "alpha").unwrap();

        let token = CancellationToken::new();
        token.cancel();

        let calls = vec![
            tool_call("call_1", "read_file", r#"{"path":"alpha.txt"}"#),
            tool_call("call_2", "read_file", r#"{"path":"alpha.txt"}"#),
        ];

        let outcomes = execute_parallel_read_only_tools(&calls, dir.path(), token).await.unwrap();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(|outcome| !outcome.emit_result));
        assert!(outcomes.iter().all(|outcome| outcome.result == INTERRUPTED_RESULT));
    }
}
