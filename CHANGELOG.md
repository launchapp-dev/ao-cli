# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-04-06

### Features
- Cloud status integration: ao cloud status calls /api/cli/status for cloud projects and daemons — show user's cloud projects, daemon states, and active workflows from the cloud API
- Cloud API routing: ao-cli route deploy commands through cloud API instead of direct Fly.io — animus cloud deploy create/destroy/start/stop/status should call /api/cli/daemons/* endpoints instead of requiring Fly.io credentials locally
- Auto-detect project linking from git remote + GitHub App installation
- Config alignment: animus cloud push now sends .ao/ config files
- Device auth flow: implement animus cloud login device auth flow
- ACP evaluation: research ACP spec and design AO as ACP server for IDE integration
- Deploy subcommands: add start/stop/status/create/destroy deployment subcommands
- Feature branch workflow: enable feature branch workflow in standard task pipelines
- Cloud CLI: evolve ao sync into ao cloud CLI subcommands
- Memory MCP: wire ao-memory-mcp into default workflow phases
- Marketplace MVP: Skill marketplace MVP — GitHub registry fetch + pack download
- Event triggers Phase 2: Generic Webhook Support
- Event triggers Phase 1: file watcher support
- Remote MCP servers: HTTP/remote MCP server support

### Fixes
- Cloud linking: rustfmt fix for cloud.rs auto-detect linking
- Config test isolation: improve orchestrator-config test isolation and lock handling
- Model references: replace broken glm-5 model refs with claude-haiku, disable pr-sweep schedule
- Sparkcube removal: remove sparkcube tool_profile refs blocking all daemon workflows
- CLI E2E tests: resolve all 6 storage-mismatch and warning failures
- Lint warnings: resolve all clippy warnings workspace-wide
- Daemon task state: fix last failing unit test — daemon_run task-state-change assertion
- Config test assertion: align test assertion with standard-workflow ID rename
- Env lock handling: recover from poisoned env lock in tests
- Path references: replace hardcoded fixture paths and absolute paths with CARGO_MANIFEST_DIR

### Refactors
- Rebrand to animus: rename primary binary from 'ao' to 'animus', maintain 'ao' as alias
- Help text updates: update all help text and error messages from 'ao' to 'animus'
- Test updates: update test assertions for rebrand to 'animus'
- Unused dependencies: strip unused MCP servers — remove filesystem, sequential-thinking, memory, rust-docs

### Documentation
- Public repo preparation: clean README.md with Animus branding, verify ELv2 LICENSE, remove hardcoded local paths, add CONTRIBUTING.md, ensure .gitignore covers sensitive files
- README updates: fix typo and update GitHub release links

### Style
- Formatting fixes: rustfmt fixes for line length, import ordering, and cloud status command

## [0.2.35] - Previous Release

See git history for earlier releases.
