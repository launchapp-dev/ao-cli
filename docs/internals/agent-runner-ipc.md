# Agent Runner IPC

The agent runner (`agent-runner`) is a standalone daemon that manages LLM CLI tool processes. It communicates with workflow runners over an IPC protocol.

## Transport

- **Unix domain socket** on Unix platforms. The server binds to
  `<ANIMUS_CONFIG_DIR>/agent-runner.sock`.
  In normal Animus flows, the runner is launched with `ANIMUS_CONFIG_DIR`
  pointing at the repo-scoped runner directory
  (`~/.animus/<repo-scope>/runner/`), so clients connect there.
  If that Unix socket path would exceed the platform limit, Animus shortens it
  into `/tmp/ao-runner/<hash>/agent-runner.sock` and writes `origin-path.txt`
  with the canonical path.
- **TCP** on Windows as a fallback

The IPC server (`crates/agent-runner/src/ipc/server.rs`) listens for incoming connections and routes them through the request handler pipeline. Each connection is assigned a monotonically increasing connection ID for tracing.

## Auth-First Connection

Every new connection must authenticate before sending any operational requests:

1. Client sends an `IpcAuthRequest` JSON message as the first payload: `{"kind": "ipc_auth", "token": "<token>"}`
2. Server validates the token against `agent_runner_token` loaded from
   `<ANIMUS_CONFIG_DIR>/config.json`
3. Server responds with `IpcAuthResult`: either `ok: true` or `ok: false` with a failure code
4. If rejected, the connection is closed immediately

Failure codes:
- `MalformedAuthPayload` -- The first message was not a valid auth request
- `InvalidToken` -- Token did not match
- `ServerTokenUnavailable` -- Server could not load its own token (fails closed)

## Request Routing

After authentication, the IPC router (`crates/agent-runner/src/ipc/router.rs`) dispatches requests to handlers:

- **Run handler** (`handlers/run.rs`) -- Start a new agent run with a runtime contract
- **Status handler** (`handlers/status.rs`) -- Query the status of running agents
- **Control handler** (`handlers/control.rs`) -- Stop or manage running agents

## Event Streaming

During an agent run, the server streams `AgentRunEvent` messages back to the client over the socket, one JSON object per line. Events include:

- Agent process started/stopped
- Stdout/stderr output lines
- Parsed tool calls and their results
- Thinking blocks
- Artifacts
- Phase decision (the final structured output)

The stream bridge (`crates/agent-runner/src/runner/stream_bridge.rs`) connects the agent process output to the IPC event stream, translating raw process output into structured events.

## Output Parsing

The output parser (`crates/agent-runner/src/output/parser/`) processes raw agent output into structured events:

- **Tool calls** (`tool_calls.rs`) -- Detects JSON and XML tool call patterns in agent output
- **Artifacts** (`artifacts.rs`) -- Extracts file artifacts and structured outputs
- **Events** (`events.rs`) -- Converts parsed output into `AgentRunEvent` messages
- **State** (`state.rs`) -- Maintains parser state across incremental output chunks

The parser handles multiple output formats since different CLI tools (claude, codex, gemini) produce output in different structures.

## Sandbox

The agent runner sanitizes the environment before spawning CLI tool processes:

### Environment Sanitization

`crates/agent-runner/src/sandbox/env_sanitizer.rs` implements an allowlist-based approach:

Allowed variables:
- System: `PATH`, `HOME`, `USER`, `SHELL`, `LANG`, `LC_ALL`, `TMPDIR`
- Terminal/session plumbing: `TERM`, `COLORTERM`, `SSH_AUTH_SOCK`
- Claude config: `CLAUDE_CODE_SETTINGS_PATH`, `CLAUDE_API_KEY`, `CLAUDE_CODE_DIR`
- Prefixes: `ANIMUS_*`, `XDG_*`

API keys are not forwarded by the sanitizer. The runner also explicitly strips
Claude/Codex session markers such as `CLAUDECODE`,
`CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_SESSION_ACCESS_TOKEN`,
`CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_EXECPATH`, `CLAUDE_EFFORT`, and
`AI_AGENT` before spawning the child process so nested agent CLIs do not inherit
host session state.

### Workspace Validation

`crates/agent-runner/src/sandbox/workspace_guard.rs` validates that the requested workspace directory is safe to use, preventing path traversal attacks.

## Provider Abstraction

The runner module (`crates/agent-runner/src/providers/`) provides a unified interface for spawning different LLM CLI tools. The process builder (`crates/agent-runner/src/runner/process_builder.rs`) constructs the appropriate command line for each supported tool:

- **claude** -- Anthropic's Claude Code CLI
- **codex** -- OpenAI's Codex CLI
- **gemini** -- Google's Gemini CLI
- **opencode** -- Open-source alternative

The supervisor (`crates/agent-runner/src/runner/supervisor.rs`) manages the lifecycle of spawned processes, handling graceful shutdown and cleanup.
