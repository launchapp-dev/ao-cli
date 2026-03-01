# TASK-133: Implement OpenAI-compatible streaming API client

## Implementation Status: COMPLETE ✓

### Overview
This document details the implementation of the OpenAI-compatible streaming API client in the `oai-runner` crate.

## Implementation Scope

### Files Created
- `crates/oai-runner/src/api/mod.rs` - Module declaration
- `crates/oai-runner/src/api/types.rs` - Type definitions
- `crates/oai-runner/src/api/client.rs` - Streaming HTTP client

### Type Definitions (types.rs)

| Type | Description | Status |
|------|-------------|--------|
| `ChatMessage` | Chat message with role, content, tool_calls, tool_call_id | ✓ |
| `ToolCall` | Tool call with id, type, function | ✓ |
| `ToolDefinition` | Tool definition for function tools | ✓ |
| `FunctionCall` | Function call with name and arguments | ✓ |
| `FunctionSchema` | Function schema with name, description, parameters | ✓ |
| `StreamDelta` | Streaming delta with content and tool_calls | ✓ |

### Supporting Types
- `StreamChunk` - SSE chunk wrapper
- `StreamChoice` - Choice container for deltas
- `StreamToolCall` - Streaming tool call delta
- `StreamFunctionCall` - Streaming function call delta
- `UsageInfo` - Token usage information
- `ChatRequest` - Full chat request structure
- `ResponseFormat` - Response format specification

### Client Functionality (client.rs)

| Feature | Implementation | Status |
|---------|----------------|--------|
| POST to api_base/chat/completions | Creates URL from api_base | ✓ |
| stream:true parameter | Set in ChatRequest | ✓ |
| SSE data line parsing | Line-by-line parsing of "data: " prefix | ✓ |
| delta.content accumulation | String concatenation in loop | ✓ |
| delta.tool_calls accumulation | Vec<ToolCall> with index-based merging | ✓ |
| DONE sentinel handling | Checks for "[DONE]" string | ✓ |
| Retry on 429/5xx | Exponential backoff (500ms * 2^attempt) | ✓ |
| Return complete ChatMessage | Returns (ChatMessage, Option<UsageInfo>) | ✓ |

### API Design

```rust
pub struct ApiClient {
    http: reqwest::Client,
    api_base: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(api_base: String, api_key: String, timeout_secs: u64) -> Self;
    
    pub async fn stream_chat(
        &self,
        request: &ChatRequest,
        on_text_chunk: &mut dyn FnMut(&str),
    ) -> Result<(ChatMessage, Option<UsageInfo>)>;
}
```

### Retry Logic
- Maximum 3 attempts
- Exponential backoff: 500ms, 1000ms, 2000ms
- Retries on: HTTP 429 (rate limit) and 5xx errors
- Non-retryable errors return immediately

### SSE Parsing
1. Reads bytes stream from response
2. Accumulates into buffer, splits on newlines
3. Skips empty lines and comment lines (starting with ':')
4. Parses "data: " prefix to extract JSON
5. Handles "[DONE]" sentinel to complete stream
6. Accumulates content and tool_calls across chunks

## Acceptance Criteria

| Criterion | Validation |
|-----------|------------|
| Types serialize/deserialize correctly | Unit tests pass |
| Client connects to OpenAI-compatible endpoints | Manual testing |
| Streaming works correctly | Unit tests for SSE parsing |
| Tool calls accumulate properly | Unit tests for delta merging |
| Retry logic handles errors | Tested via error injection |
| Return value is complete ChatMessage | Code review |

## Validation

```bash
$ cargo test --package oai-runner
running 26 tests
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All unit tests pass including:
- `chat_message_skips_none_fields`
- `stream_chunk_deserializes_content_delta`
- `stream_chunk_deserializes_tool_call_delta`
- `tool_definition_serializes_to_openai_shape`
- `chat_request_serializes_with_response_format`

## Dependencies

- `reqwest` 0.12 with `json` and `stream` features
- `serde` with derive feature
- `futures-util` for stream handling
- `tokio` for async runtime

## Constraints

- Uses reqwest for HTTP (not ureq)
- Synchronous stdout flush in streaming callback
- API key passed as Bearer token in Authorization header
- Requires `stream: true` in request for SSE

## Notes

- The implementation handles partial tool call arguments (streaming JSON)
- Tool calls are accumulated by index, allowing multiple concurrent calls
- The client returns the complete accumulated ChatMessage when stream ends
- Usage information is captured when present in final chunk
