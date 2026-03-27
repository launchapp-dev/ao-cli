use std::process::Command;

#[test]
fn quickstart_commands_parse_successfully() -> Result<(), Box<dyn std::error::Error>> {
    let binary = assert_cmd::cargo::cargo_bin!("ao");

    // Read the quick-start documentation
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let quickstart_path = std::path::PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("docs/getting-started/quick-start.md"))
        .ok_or("failed to resolve quick-start.md path")?;

    let quickstart_content = std::fs::read_to_string(&quickstart_path)
        .map_err(|e| format!("failed to read quick-start.md: {}", e))?;

    // Extract all ao commands from the markdown
    let commands = extract_commands_from_markdown(&quickstart_content)?;

    if commands.is_empty() {
        return Err("no ao commands found in quick-start.md".into());
    }

    // Verify each command can be run with --help
    for (line_num, cmd) in commands {
        let mut args = shell_words::split(&cmd)
            .map_err(|e| format!("line {}: failed to parse command args: {}", line_num, e))?;

        // Remove 'ao' if it's the first argument
        if args.first().map_or(false, |s| s == "ao") {
            args.remove(0);
        }

        // Add --help to verify command parsing
        args.push("--help".to_string());

        let output = Command::new(&binary)
            .args(&args)
            .output()
            .map_err(|e| format!("line {}: failed to execute command {:?}: {}", line_num, cmd, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "line {}: command failed with exit code {:?}\nCommand: ao {}\nStderr: {}",
                line_num,
                output.status.code(),
                args.join(" "),
                stderr
            ).into());
        }
    }

    Ok(())
}

fn extract_commands_from_markdown(content: &str) -> Result<Vec<(usize, String)>, Box<dyn std::error::Error>> {
    let mut commands = Vec::new();

    // Split by triple backticks and find bash sections
    let parts: Vec<&str> = content.split("```").collect();

    for i in (0..parts.len()).step_by(2) {
        if i + 1 >= parts.len() {
            break;
        }

        let block_marker_and_content = parts[i + 1];
        let lines: Vec<&str> = block_marker_and_content.split('\n').collect();

        // Check if this is a bash block (first line should be "bash")
        if lines.is_empty() || !lines[0].trim().eq("bash") {
            continue;
        }

        // Find the starting line number of this block in the original content
        let block_start = content.find(parts[i + 1]).unwrap_or(0);
        let start_line = content[..block_start].lines().count();

        // Extract commands from this bash block
        let mut current_command = String::new();
        let mut command_start_line = 0;

        for (idx, line) in lines.iter().enumerate().skip(1) {
            if idx == lines.len() - 1 {
                // This is an empty final line after the last newline
                break;
            }

            let trimmed = line.trim();
            let current_line = start_line + idx;

            // Skip comments and empty lines unless we're in a command continuation
            if trimmed.is_empty() || trimmed.starts_with('#') {
                if !current_command.is_empty() {
                    commands.push((command_start_line, current_command.clone()));
                    current_command.clear();
                }
                continue;
            }

            // Check if this line starts with 'ao'
            if trimmed.starts_with("ao ") {
                // Save previous command if any
                if !current_command.is_empty() {
                    commands.push((command_start_line, current_command.clone()));
                }

                current_command = trimmed.to_string();
                command_start_line = current_line;
            } else if !current_command.is_empty() && line.starts_with("  ") {
                // This is a continuation line (must be indented)
                if current_command.ends_with('\\') {
                    current_command.pop(); // remove the backslash
                }
                current_command.push(' ');
                current_command.push_str(trimmed);
            }
        }

        // Save the last command
        if !current_command.is_empty() {
            commands.push((command_start_line, current_command));
        }
    }

    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_commands_parses_markdown_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let markdown = r#"
# Quick Start

## Section 1

```bash
ao doctor
ao setup
```

Some text in between.

```bash
cd /path/to/project
ao task create \
  --title "My Task"
```
        "#;

        let commands = extract_commands_from_markdown(markdown)?;
        assert!(!commands.is_empty(), "should extract commands");

        let command_texts: Vec<&str> = commands.iter().map(|(_, cmd)| cmd.as_str()).collect();
        assert!(command_texts.iter().any(|c| c.contains("ao doctor")), "should find ao doctor");
        assert!(command_texts.iter().any(|c| c.contains("ao setup")), "should find ao setup");
        assert!(command_texts.iter().any(|c| c.contains("ao task create")), "should find ao task create");

        Ok(())
    }
}
