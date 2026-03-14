# TASK-134: Implementation Notes

## Overview
Native tool definitions and executor for OpenAI function calling in the oai-runner crate.

## Architecture

### Module Hierarchy
```
tools/
├── mod.rs        - Module exports
├── definitions.rs - JSON Schema for all 6 tools
├── executor.rs   - Dispatch by name, returns string
├── file_ops.rs   - File I/O + path validation
├── search.rs     - Regex search via grep
└── bash.rs       - Command execution with timeout
```

### Data Flow
1. **Tool Call Received** → `executor::execute_tool(name, args_json, working_dir)`
2. **Argument Parsing** → JSON args parsed to extract parameters
3. **Path Resolution** (for file ops) → `resolve_path()` validates against working_dir
4. **Tool Execution** → Delegates to appropriate module (file_ops/search/bash)
5. **Result Returned** → String result returned to caller

## Key Functions

### executor.rs
```rust
pub fn execute_tool(name: &str, args_json: &str, working_dir: &Path) -> Result<String>
```
Dispatches to:
- `read_file` → file_ops::read_file
- `write_file` → file_ops::write_file
- `edit_file` → file_ops::edit_file
- `list_files` → file_ops::list_files
- `search_files` → search::search_files
- `execute_command` → bash::execute_command

### file_ops.rs
```rust
fn resolve_path(working_dir: &Path, path: &str) -> Result<PathBuf>
```
Path traversal prevention:
1. Resolves relative paths relative to working_dir
2. Canonicalizes both paths
3. Checks if resolved path starts with working_dir
4. Explicitly checks for `..` components as secondary check

## Security Considerations

### Path Traversal
The `resolve_path()` function prevents directory escape:
- Canonicalizes working directory and target path
- Validates target starts with working directory prefix
- Rejects paths containing `..` that escape bounds

### Command Injection
- Commands execute via `sh -c` in working directory context
- No shell injection protection beyond working directory isolation
- Consider adding command allowlist for production use

### Output Limits
- `execute_command`: 50,000 character truncation
- `search_files`: 200 match limit
- `list_files`: 500 file limit

## Testing Strategy

### Unit Tests
Located in each module with `#[cfg(test)]` blocks:
- `definitions.rs`: Validates schema structure
- `executor.rs`: Integration tests for all tools
- `file_ops.rs`: Path resolution and file operations
- `search.rs`: Regex matching behavior
- `bash.rs`: Command execution and timeout

### Test Fixtures
- Uses `tempfile` crate for isolated test directories
- Creates test files with known content
- Validates outputs match expected patterns

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| read_file | O(n) | Line-by-line processing |
| write_file | O(n) | Direct file write |
| edit_file | O(n) | Full file read/modify/write |
| list_files | O(m) | Glob traversal |
| search_files | O(f) | grep subprocess |
| execute_command | O(1) | Subprocess overhead |

## Future Enhancements

Potential improvements:
1. Async tool execution for long-running commands
2. Command allowlist for execute_command
3. Streaming output for large file reads
4. File watching for edit_file conflict detection
5. Support for binary files in read_file

## Dependencies

```toml
[dependencies]
glob = "0.3"
serde_json = "1"
anyhow = "1"

[dev-dependencies]
tempfile = "3"
```
