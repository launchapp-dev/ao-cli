# CLI Wrapper

A standalone Rust tool for testing and managing different AI coding CLIs (Claude Code, Codex, Gemini, etc.).

## Features

- **CLI Discovery**: Automatically detect installed AI coding CLIs
- **Testing Framework**: Verify CLI functionality with customizable test suites
- **Health Checks**: Quick health status for all CLIs
- **Output Validation**: Parse and validate CLI outputs
- **Multi-CLI Support**: Works with Claude, Codex, Gemini, Aider, and more

## Installation

```bash
cd llm-cli-wrapper
cargo build --release
```

The binary will be at `target/release/llm-cli-wrapper`.

## Usage

### Discover Installed CLIs

```bash
llm-cli-wrapper discover
```

### List All CLIs

```bash
llm-cli-wrapper list
```

Output:
```
Installed CLIs:
────────────────────────────────────────────────────────────
Claude Code     ✓ Available
OpenAI Codex    ⚠ Not Authenticated
Google Gemini   ✗ Not Installed
```

### Run Health Checks

Check all CLIs:
```bash
llm-cli-wrapper health
```

Check specific CLI:
```bash
llm-cli-wrapper health claude
```

### Run Tests

Test all CLIs with basic suite:
```bash
llm-cli-wrapper test
```

Test specific CLI:
```bash
llm-cli-wrapper test claude
```

Test with different suite:
```bash
llm-cli-wrapper test --suite file-ops
llm-cli-wrapper test --suite code-gen
```

Available test suites:
- `basic` - Version, auth, simple commands
- `file-ops` - File reading, writing, editing
- `code-gen` - Code generation capabilities

### Show CLI Info

```bash
llm-cli-wrapper info claude
```

Output:
```
Claude Code
────────────────────────────────────────────────────────────
Executable: "/usr/local/bin/claude"
Version: 0.2.0

Capabilities:
  File editing: ✓
  Streaming: ✓
  Tool use: ✓
  Vision: ✓
  Max context: 200000 tokens
```

## Configuration

Create a `config.toml` file:

```toml
# Directory for test workspaces
test_workspace_dir = "/tmp/llm-cli-wrapper-tests"

# Default timeout for CLI operations (seconds)
default_timeout_secs = 300

# Enable verbose logging
verbose = false

# Custom CLI configurations
[[custom_clis]]
name = "my-custom-cli"
executable_path = "/path/to/custom-cli"
auth_command = "custom-cli login"
```

Use with:
```bash
llm-cli-wrapper --config config.toml test
```

## Supported CLIs

| CLI | Support | Auth Method |
|-----|---------|-------------|
| Claude Code | ✓ Full | `ANTHROPIC_API_KEY` env var |
| OpenAI Codex | ✓ Full | `codex login` |
| Google Gemini | ✓ Full | `GEMINI_API_KEY` or `GOOGLE_APPLICATION_CREDENTIALS` |
| Aider | ✓ Basic | API keys in config |
| Cursor | 🚧 Planned | - |
| Cline | 🚧 Planned | - |

## Architecture

```
llm-cli-wrapper/
├── src/
│   ├── cli/           # CLI implementations
│   │   ├── claude.rs  # Claude Code
│   │   ├── codex.rs   # OpenAI Codex
│   │   ├── gemini.rs  # Google Gemini
│   │   └── ...
│   ├── tester/        # Testing framework
│   ├── validator/     # Output validation
│   ├── parser/        # Output parsing
│   └── main.rs        # CLI binary
```

## Integration with Tauri App

This CLI wrapper can be used standalone or integrated with your Tauri agent orchestrator:

```rust
use cli_wrapper::{CliRegistry, CliTester, TestSuite};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut registry = CliRegistry::new();
    registry.discover_clis().await?;

    let tester = CliTester::new();
    let results = tester.test_all_clis(&registry, &TestSuite::basic_verification()).await?;

    for result in results {
        println!("{}: {} ({}ms)",
            result.cli_type.display_name(),
            if result.passed { "✓" } else { "✗" },
            result.duration_ms
        );
    }

    Ok(())
}
```

## Development

Run tests:
```bash
cargo test
```

Run with verbose logging:
```bash
llm-cli-wrapper --verbose test
```

Build documentation:
```bash
cargo doc --open
```

## Examples

### Quick Health Check
```bash
# Check if Claude is working
llm-cli-wrapper health claude

# Check all CLIs
llm-cli-wrapper health
```

### Run Full Test Suite
```bash
# Test basic functionality
llm-cli-wrapper test --suite basic

# Test file operations
llm-cli-wrapper test --suite file-ops

# Test code generation
llm-cli-wrapper test --suite code-gen
```

### Get CLI Information
```bash
# Show Claude capabilities
llm-cli-wrapper info claude

# Show Codex capabilities
llm-cli-wrapper info codex
```

## Troubleshooting

### CLI not found
Make sure the CLI is installed and in your PATH:
```bash
which claude
which codex
which gemini
```

### Authentication errors
Set the required environment variables:
```bash
export ANTHROPIC_API_KEY="your-key"
export OPENAI_API_KEY="your-key"
export GEMINI_API_KEY="your-key"
```

Or run the CLI's login command:
```bash
codex login
gemini login
```

### Test failures
Run with verbose logging to see details:
```bash
llm-cli-wrapper --verbose test
```

## License

MIT
