# AO CLI Workspace (Non-Tauri)

This repository contains the Rust-only AO orchestration CLI workspace.

- Main CLI crate: `crates/orchestrator-cli`
- Main CLI binary: `ao`
- Supporting crates:
  - `crates/agent-runner`
  - `crates/llm-cli-wrapper`
  - `crates/llm-mcp-server`
  - `crates/orchestrator-core`
  - `crates/orchestrator-web-api`
  - `crates/orchestrator-web-server`
  - `crates/orchestrator-web-contracts`
  - `crates/protocol`

## Workspace Layout

```text
.
├── Cargo.toml
├── Cargo.lock
├── .cargo/
└── crates/
    ├── agent-runner/
    ├── llm-cli-wrapper/
    ├── llm-mcp-server/
    ├── orchestrator-cli/
    ├── orchestrator-core/
    ├── orchestrator-web-api/
    ├── orchestrator-web-contracts/
    ├── orchestrator-web-server/
    └── protocol/
```

## Build and Run

```bash
# Build the main CLI
cargo build -p orchestrator-cli

# Show CLI help
cargo run -p orchestrator-cli -- --help

# Wrapper CLI help
cargo run -p llm-cli-wrapper -- --help
```

## Agent Runner

```bash
cargo build -p agent-runner
```

The runner binary is discovered by orchestrator components from the workspace
`target/` and `crates/agent-runner/target/` locations.

## MCP Server

```bash
cargo build -p llm-mcp-server --release
```

`llm-cli-wrapper` can build and launch this crate via:

- manifest path: `crates/llm-mcp-server/Cargo.toml`
- binary path: `crates/llm-mcp-server/target/release/llm-mcp-server`

## Notes

- This workspace intentionally excludes Tauri crates and frontend app code.
- Runtime/project state for AO commands is stored under `.ao/` within target
  project roots.
