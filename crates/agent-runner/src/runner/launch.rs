use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchInvocation {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub prompt_via_stdin: bool,
}

pub fn is_ai_cli_tool(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "claude"
            | "codex"
            | "gemini"
            | "opencode"
            | "open-code"
            | "oai-runner"
            | "animus-oai-runner"
            | "aider"
            | "cursor"
            | "cline"
            | "custom"
    )
}

fn canonical_cli_name(command: &str) -> String {
    let trimmed = command.trim();
    let file_name = Path::new(trimmed).file_name().and_then(|value| value.to_str()).unwrap_or(trimmed);
    file_name.to_ascii_lowercase()
}

pub fn ensure_flag(args: &mut Vec<String>, flag: &str, insert_at: usize) {
    if args.iter().any(|value| value == flag) {
        return;
    }
    let insert_at = insert_at.min(args.len());
    args.insert(insert_at, flag.to_string());
}

pub fn ensure_flag_value(args: &mut Vec<String>, flag: &str, value: &str, insert_at: usize) {
    if let Some(index) = args.iter().position(|entry| entry == flag) {
        if index + 1 < args.len() {
            args[index + 1] = value.to_string();
        } else {
            args.push(value.to_string());
        }
        return;
    }

    let insert_at = insert_at.min(args.len());
    args.insert(insert_at, flag.to_string());
    args.insert((insert_at + 1).min(args.len()), value.to_string());
}

pub fn ensure_codex_config_override(args: &mut Vec<String>, key: &str, value_expr: &str) {
    let key_prefix = format!("{key}=");
    let target = format!("{key}={value_expr}");

    let mut index = 0usize;
    while index + 1 < args.len() {
        let flag = args[index].as_str();
        if flag == "-c" || flag == "--config" {
            if args[index + 1].starts_with(&key_prefix) {
                args[index + 1] = target;
                return;
            }
            index += 2;
            continue;
        }
        index += 1;
    }

    let insert_at = args.len().saturating_sub(1);
    args.insert(insert_at, "-c".to_string());
    args.insert(insert_at + 1, target);
}

fn ensure_machine_json_output(invocation: &mut LaunchInvocation) {
    let cli = canonical_cli_name(&invocation.command);

    match cli.as_str() {
        "codex" => {
            let insert_at =
                invocation.args.iter().position(|entry| entry == "exec").map(|index| index + 1).unwrap_or(0);
            ensure_flag(&mut invocation.args, "--json", insert_at);
        }
        "claude" => {
            let insert_at =
                invocation.args.iter().position(|entry| entry == "--print").map(|index| index + 1).unwrap_or(0);
            ensure_flag(&mut invocation.args, "--verbose", insert_at);
            ensure_flag_value(&mut invocation.args, "--output-format", "stream-json", insert_at);
        }
        "gemini" => {
            let insert_at = invocation.args.iter().position(|entry| entry == "-p").unwrap_or(invocation.args.len());
            ensure_flag_value(&mut invocation.args, "--output-format", "json", insert_at);
        }
        "opencode" => {
            let insert_at = invocation.args.iter().position(|entry| entry == "run").map(|index| index + 1).unwrap_or(0);
            ensure_flag_value(&mut invocation.args, "--format", "json", insert_at);
        }
        "animus-oai-runner" | "oai-runner" => {
            let insert_at = invocation.args.iter().position(|entry| entry == "run").map(|index| index + 1).unwrap_or(0);
            ensure_flag_value(&mut invocation.args, "--format", "json", insert_at);
        }
        _ => {}
    }
}

pub fn parse_launch_from_runtime_contract(runtime_contract: Option<&Value>) -> Result<Option<LaunchInvocation>> {
    let Some(contract) = runtime_contract else {
        return Ok(None);
    };

    let Some(launch) = contract.pointer("/cli/launch").or_else(|| contract.get("launch")) else {
        return Ok(None);
    };
    if launch.is_null() {
        return Ok(None);
    }

    let command = launch
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Invalid runtime contract launch command"))?;

    let args = launch
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(|value| value.to_string())
                        .ok_or_else(|| anyhow!("Invalid runtime contract launch arg"))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    let prompt_via_stdin = launch.get("prompt_via_stdin").and_then(Value::as_bool).unwrap_or(false);
    let env = launch
        .get("env")
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string())).ok_or_else(|| {
                        anyhow!("Invalid runtime contract launch env for key '{}': expected string value", key)
                    })
                })
                .collect::<Result<BTreeMap<_, _>>>()
        })
        .transpose()?
        .unwrap_or_default();

    let mut invocation = LaunchInvocation { command: command.to_string(), args, env, prompt_via_stdin };

    ensure_machine_json_output(&mut invocation);
    Ok(Some(invocation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_ai_cli_tool_recognizes_known_aliases() {
        assert!(is_ai_cli_tool("claude"));
        assert!(is_ai_cli_tool("open-code"));
        assert!(!is_ai_cli_tool("unknown"));
    }

    #[test]
    fn parse_launch_enforces_machine_output() {
        let contract = json!({
            "cli": {
                "launch": {
                    "command": "opencode",
                    "args": ["run", "hello"],
                    "prompt_via_stdin": false
                }
            }
        });

        let launch = parse_launch_from_runtime_contract(Some(&contract))
            .expect("launch should parse")
            .expect("launch should be present");
        let idx = launch.args.iter().position(|arg| arg == "--format").expect("opencode format flag should be present");
        assert_eq!(launch.args.get(idx + 1).map(String::as_str), Some("json"));
        assert!(launch.env.is_empty());

        let claude_contract = json!({
            "cli": {
                "launch": {
                    "command": "claude",
                    "args": ["--print", "hello"],
                    "prompt_via_stdin": false
                }
            }
        });
        let claude_launch = parse_launch_from_runtime_contract(Some(&claude_contract))
            .expect("launch should parse")
            .expect("launch should be present");
        assert!(claude_launch.args.contains(&"--verbose".to_string()));
        let output_idx = claude_launch
            .args
            .iter()
            .position(|arg| arg == "--output-format")
            .expect("claude output format flag should be present");
        assert_eq!(claude_launch.args.get(output_idx + 1).map(String::as_str), Some("stream-json"));
    }

    #[test]
    fn parse_launch_preserves_environment_variables() {
        let contract = json!({
            "cli": {
                "launch": {
                    "command": "codex",
                    "args": ["exec", "hello"],
                    "env": {
                        "SKILL_MODE": "review",
                        "ANIMUS_FLAG": "1"
                    },
                    "prompt_via_stdin": false
                }
            }
        });

        let launch = parse_launch_from_runtime_contract(Some(&contract))
            .expect("launch should parse")
            .expect("launch should be present");
        assert_eq!(launch.env.get("SKILL_MODE").map(String::as_str), Some("review"));
        assert_eq!(launch.env.get("ANIMUS_FLAG").map(String::as_str), Some("1"));
    }
}
