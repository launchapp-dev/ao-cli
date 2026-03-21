use crate::api::types::{FunctionSchema, ToolDefinition};
use crate::config::ExecPolicy;
use serde_json::json;

pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            type_: "function".to_string(),
            function: FunctionSchema {
                name: "read_file".to_string(),
                description: "Read the contents of a file. Returns the file content as a string.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read (relative to working directory)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line number to start reading from (1-indexed). Optional."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of lines to read. Optional."
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".to_string(),
            function: FunctionSchema {
                name: "write_file".to_string(),
                description: "Create or overwrite a file with the given content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write (relative to working directory)"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".to_string(),
            function: FunctionSchema {
                name: "edit_file".to_string(),
                description: "Replace exact text in a file. The old_text must match exactly (including whitespace and indentation).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to edit (relative to working directory)"
                        },
                        "old_text": {
                            "type": "string",
                            "description": "The exact text to find and replace"
                        },
                        "new_text": {
                            "type": "string",
                            "description": "The text to replace it with"
                        }
                    },
                    "required": ["path", "old_text", "new_text"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".to_string(),
            function: FunctionSchema {
                name: "list_files".to_string(),
                description: "List files matching a glob pattern.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern to match (e.g. '**/*.rs', 'src/*.ts')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Base directory for the glob (relative to working directory). Defaults to '.'."
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".to_string(),
            function: FunctionSchema {
                name: "search_files".to_string(),
                description: "Search for a regex pattern in files. Returns matching lines with file paths and line numbers.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in (relative to working directory). Defaults to '.'."
                        },
                        "include": {
                            "type": "string",
                            "description": "Glob pattern to filter files (e.g. '*.rs'). Optional."
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".to_string(),
            function: FunctionSchema {
                name: "execute_command".to_string(),
                description: "Execute a shell command and return its output. Use for running tests, builds, git commands, etc.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds. Defaults to 120."
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
    ]
}

/// Legacy read-only tool names (kept for documentation; prefer `tools_for_policy(ExecPolicy::ReadOnly)`).
#[allow(dead_code)]
const READ_ONLY_TOOLS: &[&str] = &["read_file", "list_files", "search_files"];

/// Returns built-in tool definitions filtered by the given execution policy.
pub fn tools_for_policy(policy: ExecPolicy) -> Vec<ToolDefinition> {
    let allowed = policy.allowed_tools();
    all_tool_definitions()
        .into_iter()
        .filter(|t| allowed.contains(&t.function.name.as_str()))
        .collect()
}

/// Returns built-in tool definitions filtered to read-only tools.
/// Prefer `tools_for_policy(ExecPolicy::ReadOnly)` for new code.
#[allow(dead_code)]
pub fn read_only_tool_definitions() -> Vec<ToolDefinition> {
    tools_for_policy(ExecPolicy::ReadOnly)
}

pub fn merge_tools(native: Vec<ToolDefinition>, mcp: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    let native_names: std::collections::HashSet<String> = native.iter().map(|t| t.function.name.clone()).collect();
    let mut merged = native;
    for tool in mcp {
        if !native_names.contains(&tool.function.name) {
            merged.push(tool);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_definitions_exclude_write_tools() {
        let tools = read_only_tool_definitions();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"search_files"));
        assert!(!names.contains(&"write_file"));
        assert!(!names.contains(&"edit_file"));
        assert!(!names.contains(&"execute_command"));
    }

    #[test]
    fn all_definitions_have_required_fields() {
        let tools = all_tool_definitions();
        assert_eq!(tools.len(), 6);

        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"search_files"));
        assert!(names.contains(&"execute_command"));

        for tool in &tools {
            assert_eq!(tool.type_, "function");
            assert!(!tool.function.name.is_empty());
            assert!(!tool.function.description.is_empty());
            assert!(tool.function.parameters.get("type").is_some());
            assert!(tool.function.parameters.get("required").is_some());
        }
    }

    #[test]
    fn tool_definitions_serialize_to_valid_openai_json() {
        let tools = all_tool_definitions();
        let json = serde_json::to_value(&tools).unwrap();
        let arr = json.as_array().unwrap();
        for item in arr {
            assert_eq!(item["type"], "function");
            assert!(item["function"]["name"].is_string());
            assert!(item["function"]["parameters"]["properties"].is_object());
        }
    }

    #[test]
    fn tools_for_policy_full_includes_all() {
        let tools = tools_for_policy(ExecPolicy::Full);
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(names.contains(&"execute_command"));
    }

    #[test]
    fn tools_for_policy_no_shell_excludes_execute_command() {
        let tools = tools_for_policy(ExecPolicy::NoShell);
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(!names.contains(&"execute_command"));
        assert!(names.contains(&"write_file"));
    }

    #[test]
    fn tools_for_policy_read_only_matches_read_only_tool_definitions() {
        let by_policy = tools_for_policy(ExecPolicy::ReadOnly);
        let by_fn = read_only_tool_definitions();
        assert_eq!(by_policy.len(), by_fn.len());
        let mut names_a: Vec<&str> = by_policy.iter().map(|t| t.function.name.as_str()).collect();
        let mut names_b: Vec<&str> = by_fn.iter().map(|t| t.function.name.as_str()).collect();
        names_a.sort();
        names_b.sort();
        assert_eq!(names_a, names_b);
    }
}
