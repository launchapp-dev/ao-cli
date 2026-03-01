# TASK-134: Native Tool Definitions and Executor - Requirements

## Task Overview
- **Task ID**: TASK-134
- **Title**: Implement native tool definitions and executor
- **Status**: Implementation Complete (34 tests passing)
- **Dependencies**: TASK-132 (blocked-by)

## Implementation Scope

### Module Structure
The `crates/oai-runner/src/tools/` module contains:
- `mod.rs` - Module declarations
- `definitions.rs` - OpenAI function calling JSON Schema definitions
- `executor.rs` - Tool dispatch by name, returns string result
- `file_ops.rs` - File read/write/edit/list operations
- `search.rs` - Regex search in files
- `bash.rs` - Shell command execution

### Tools Implemented

| Tool | Parameters | Description |
|------|------------|-------------|
| `read_file` | `path`, `offset?`, `limit?` | Read file contents with optional line offset/limit |
| `write_file` | `path`, `content` | Create or overwrite file with content |
| `edit_file` | `path`, `old_text`, `new_text` | Replace exact text in file |
| `list_files` | `pattern`, `path?` | Glob pattern file listing |
| `search_files` | `pattern`, `path?`, `include?` | Regex search across files |
| `execute_command` | `command`, `timeout_secs?` | Shell command execution with timeout |

## Constraints

### Path Traversal Prevention
- All file operations must stay within the `--working-dir` boundary
- Paths containing `..` that escape the working directory are rejected
- Both absolute and relative paths are supported, but all resolved paths must stay within working directory
- Implementation: `file_ops::resolve_path()` function

### Working Directory
- All file operations are relative to the provided `working_dir` parameter
- Path resolution happens via `resolve_path()` which canonicalizes and validates paths

### Command Execution
- Default timeout: 120 seconds
- Maximum output size: 50,000 characters (truncated beyond this)
- Commands run in `sh -c` context within working directory

## Acceptance Criteria

### Functional Requirements
- [x] All 6 tools have valid JSON Schema definitions for OpenAI function calling
- [x] Executor dispatches by tool name and returns string result
- [x] Path traversal prevention rejects `..` paths escaping working directory
- [x] All file operations are relative to working directory
- [x] Command execution supports configurable timeout

### Security Requirements
- [x] Path traversal attacks are prevented via `resolve_path()` validation
- [x] Commands run within working directory context
- [x] Output truncation prevents memory issues with large outputs

### Quality Requirements
- [x] All 34 unit tests pass
- [x] JSON Schema definitions serialize to valid OpenAI format
- [x] Tool definitions contain required fields (name, description, parameters, required)
- [x] Error handling provides meaningful error messages

### Test Coverage
- read_file: basic read, offset/limit, line numbers
- write_file: create new file, parent directory creation
- edit_file: text replacement, old_text not found error
- list_files: glob pattern matching
- search_files: regex content search
- execute_command: basic execution, exit code capture, timeout
- Error cases: unknown tool, missing parameters

## Implementation Notes

### Key Implementation Details
1. **Path Resolution**: Uses `canonicalize()` to resolve symlinks, checks prefix matches working directory
2. **Line Numbers**: read_file returns content with 1-indexed line numbers
3. **Edit Behavior**: edit_file replaces first occurrence only (consistent with task spec)
4. **Search Limits**: search_files limits to 200 matches, shows truncation notice
5. **Output Truncation**: execute_command truncates at 50,000 characters

### Dependencies
- `glob = "0.3"` - For list_files pattern matching
- `serde_json = "1"` - For JSON Schema definitions
- `anyhow = "1"` - For error handling

## Validation Evidence

Test run results:
```
running 34 tests
test tools::definitions::tests::all_definitions_have_required_fields ... ok
test tools::definitions::tests::tool_definitions_serialize_to_valid_openai_json ... ok
test tools::executor::tests::edit_file_fails_when_old_text_not_found ... ok
test tools::executor::tests::read_file_supports_offset_and_limit ... ok
test tools::executor::tests::missing_required_param_returns_error ... ok
test tools::executor::tests::read_file_returns_content_with_line_numbers ... ok
test tools::executor::tests::edit_file_replaces_text ... ok
test tools::executor::tests::unknown_tool_returns_error ... ok
test tools::executor::tests::list_files_matches_glob_pattern ... ok
test tools::executor::tests::write_file_creates_new_file ... ok
test tools::executor::tests::write_file_creates_parent_directories ... ok
test tools::executor::tests::search_files_finds_matching_content ... ok
test tools::executor::tests::execute_command_captures_exit_code ... ok
test tools::executor::tests::execute_command_runs_shell_commands ... ok

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Conclusion

The implementation satisfies all requirements specified in TASK-134:
- All required files created in correct location
- All 6 tools implemented with correct parameters
- JSON Schema definitions for OpenAI function calling
- Executor dispatches by name, returns string result
- Path traversal prevention working correctly
- All file operations relative to working directory
- 34 tests passing, validating all functionality

** complete and ready forVerdict**: Implementation advancement.
