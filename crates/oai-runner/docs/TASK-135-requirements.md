# TASK-135: Multi-turn Agent Loop and Output Formatting - Requirements

## Task Overview
- **Task ID**: TASK-135
- **Title**: Implement multi-turn agent loop and output formatting
- **Status**: Requirements Phase
- **Dependencies**: TASK-134 (native tool definitions and executor)

## Implementation Scope

### Module Structure
The `crates/oai-runner/src/runner/` module contains:
- `mod.rs` - Module declarations
- `agent_loop.rs` - Multi-turn agent loop with streaming and tool execution
- `output.rs` - Output formatting for text, tool calls, and metadata

### Agent Loop (`agent_loop.rs`)

#### Core Functionality
| Feature | Status | Implementation |
|---------|--------|----------------|
| Send messages to API with tools | ✓ | ChatRequest includes tools vec |
| Stream response (stream:true) | ✓ | client.stream_chat() with callback |
| Print text chunks to stdout | ✓ | output.text_chunk() callback |
| Execute tool_calls | ✓ | executor::execute_tool() for each |
| Append results to conversation | ✓ | Add tool message to messages vec |
| Continue loop on tool_calls | ✓ | Loop continues if has_tool_calls |
| Exit on content-only response | ✓ | Returns Ok(()) when no tool_calls |
| max_turns limit | ✓ | for loop with max_turns iterations |

#### Implementation Details
- Uses OpenAI-compatible chat completions API
- Accumulates tool_calls across streaming chunks
- Supports response schema validation for structured output
- Retry logic for schema validation failures (3 retries)

### Output Formatting (`output.rs`)

#### Output Types
| Output Type | Format | Status |
|-------------|--------|--------|
| Text chunks | Printed directly to stdout | ✓ |
| Tool calls | JSON with `type: tool_call` | ✓ |
| Tool results | JSON with `type: tool_result` | ✓ |
| Tool errors | JSON with `type: tool_error` | ✓ |
| Thinking | XML tags `<thinking>...</thinking>` | ✓ (pass-through) |
| Metadata | JSON with `type: metadata` + tokens | ✓ (tokens only) |

#### JSON Mode vs Text Mode
- **JSON mode** (`--format json`): All output is JSON lines
- **Text mode** (default): Human-readable formatting with JSON metadata at end

## Constraints

### API Compatibility
- Works with OpenAI-compatible APIs (OpenAI, MiniMax, DeepSeek, ZAI, etc.)
- Requires `stream: true` for streaming responses
- Tool definitions must follow OpenAI function calling schema

### Resource Limits
- Default max_turns: 50 (configurable via CLI)
- Each tool has its own timeout (default 120s for execute_command)
- Output truncation at 50,000 characters for command execution

### Path Security
- All file operations restricted to working_dir
- Path traversal prevention via canonicalize + prefix check

## Acceptance Criteria

### Functional Requirements
- [x] Agent loop sends messages with tools and streams response
- [x] Text chunks printed to stdout in real-time
- [x] Tool calls executed and results appended to conversation
- [x] Loop continues until content-only response or max_turns
- [x] Output formatted as specified (text/chunks/JSON lines)
- [x] Metadata includes token counts

### Integration Requirements
- [x] Phase decision text emitted naturally for orchestrator extraction
- [x] Thinking tags (`<thinking>...</thinking>`) pass through for parser extraction
- [x] Tool call JSON format compatible with agent-runner parser

### Quality Requirements
- [x] All 34 unit tests pass
- [x] Code compiles without errors
- [x] Schema validation works correctly (bonus feature)

## Notes on Missing Features

### Cost in Metadata
The task mentions "metadata JSON at end with tokens/cost". The API provides token counts but not cost. To add cost calculation would require:
- A pricing model/configuration
- Per-model pricing lookup
- This is deemed out of scope for this implementation as it would require external configuration

### Thinking Detection
The current implementation passes through all text content as-is. Models that output thinking in `<thinking>...</thinking>` tags will have those tags in the output. The agent-runner's parser extracts thinking from these tags. No explicit thinking detection is needed in oai-runner.

## Validation Evidence

Test run results:
```
running 34 tests
test config::tests::infer_api_base_for_glm ... ok
test config::tests::infer_api_base_for_minimax ... ok
test config::tests::infer_api_base_for_deepseek ... ok
test config::tests::infer_api_base_fails_for_unknown_model ... ok
test api::types::tests::chat_message_skips_none_fields ... ok
test runner::agent_loop::tests::extracts_json_from_inline_line ... ok
test runner::agent_loop::tests::validates_valid_json_against_schema ... ok
... (34 total)

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Conclusion

The implementation satisfies all core requirements specified in TASK-135:
- Multi-turn agent loop with streaming and tool execution ✓
- Output formatting with text chunks, tool calls, and metadata ✓
- Phase decision text emitted naturally for orchestrator extraction ✓
- Thinking passes through in XML tags for parser extraction ✓

**Verdict**: Ready for advancement to implementation phase if additional features are needed.
