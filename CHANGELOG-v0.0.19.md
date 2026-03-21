# Changelog v0.0.19

Release date: 2026-03-21

## Features

- **Daemon Reliability Improvements**
  - Phase failure diagnostics with improved error classification
  - Retry logic with exponential backoff for transient failures
  - Session resume for reliability and error recovery
  - Process lifecycle management with zombie reaping and signal handling
  - Graceful pool drain on daemon stop/pause

- **oai-runner Enhancements**
  - Production-grade structured output (json_schema support)
  - Enhanced API client with tool executor improvements
  - Token tracking and cost reporting
  - Circuit breaker with jitter and typed error classification
  - Fallback model chains for rate limit failover
  - Increased max_turns (50 → 200)
  - Groq provider support added

- **Multi-Model Routing**
  - Cost-aware task routing across multiple providers
  - Native session backends (Claude, Gemini, OpenCode)
  - Auto-detect provider structured output support (json_schema vs json_object)
  - Model balancing: features→Sonnet, bugfix/refactor→Codex, UI→Gemini

- **Workflow Improvements**
  - Per-phase retry config to replace global MAX_PHASE_REWORKS constant
  - Per-phase verdict-to-target routing configuration
  - Cross-phase data passing for workflow context continuity
  - HumanEscalated state for rework budget exhaustion
  - Skip_if guard conditions to pipeline phase execution
  - Two-stage dispatch: work-planner → triage → implementation

- **Web UI & API**
  - Visual workflow builder with phase pipeline editor
  - Agent profiles management with MCP server configs
  - Requirements lifecycle workflow UI with review gates
  - Workflow dispatch wizard and execution controls
  - Pagination, sorting, and bulk operations

- **CLI & MCP Tools**
  - Add `ao workflow execute` command
  - Add `ao schedule list/fire/history` CLI commands
  - Add `ao queue drop` command/MCP tool
  - Add `ao.runner.orphans-cleanup` MCP tool
  - Add `ao.daemon.config` MCP tools
  - Add `ao daemon logs` CLI command and MCP tool
  - Add `ao output phase-outputs` MCP tool
  - Add `ao task bulk-status` and `ao task bulk-update` tools
  - Add `ao workflow run-multiple` batch tool

- **Documentation**
  - Track GPT-4.1 and GPT-4.1 Nano as cost-efficient model alternatives
  - Add missing `queue` and `pack` commands to CLI docs
  - Fix tool group counts in agents.md and mcp-tools.md
  - Comprehensive README overhaul for all workspace crates
  - VitePress docs site with Mermaid support

## Fixes

- **Critical Process Leak Fixes** (11 occurrences)
  - Agent-runner process leak detection and reaping
  - Track native session backend PIDs in orphan tracker
  - Fix zombie processes in runner lock acquisition

- **Daemon Reliability**
  - Fix work-planner MCP crash (set mcp_servers to ao-only)
  - Fix task-reconciler auto-unblock for transient failures
  - Fix failing daemon_run integration tests
  - Fix authentication_error handling as ProviderExhaustion
  - Add dispatch retry budget with auto-block

- **Install Script**
  - Fix macOS Sequoia ad-hoc codesign
  - Cross-platform version detection with awk
  - Trap cleanup on exit

- **Codebase**
  - Fix Codex model ID: gpt-5.4 not gpt-5.4-codex
  - Fix rustfmt violations
  - Fix .mcp.json to use release binary path
  - Fix 3 regression tests (HOME isolation, duplicate workflow)

## Improvements

- **Performance**
  - Optimize release binary: 52MB → 16MB (strip, LTO, codegen-units=1, opt-level=z)
  - Clean stale web UI builds: 18MB → 1.7MB embedded

- **Architecture**
  - Refactor daemon runtime into modular crate structure
  - Extract workflow runner to workflow-runner library crate
  - Extract orchestrator-config, orchestrator-store, orchestrator-providers crates
  - Subject dispatch envelope for workflow routing
  - Scoped state directory (~/.ao/<scope>/)

- **Release Pipeline**
  - AI-driven release decisions with commit significance evaluation
  - Work-planner triggers releases when 10+ PRs merged
  - Embed bundled packs in binary

## Breaking Changes

None

## Deprecations

- Removed legacy daemon task-scanning behavior (3,278 lines removed)
- Removed workflow-runner v1 and llm-cli-wrapper dependencies
- Removed deprecated CLI commands: task-control, execute, planning, schedule, workflow-monitor
- Removed unused env vars and dead code across 3 crates
