# Changelog - v0.1.1

## Release Date
2026-03-21

## Overview
This patch release includes 2 new features, 4 bug fixes, and quality improvements.

---

## ✨ Features

### CLI & Developer Experience
- **CI failure and PR conflict monitor script**: New monitoring script to track CI failures and PR conflicts proactively.
- **Cost optimization for research phases**: Research phases now route to `gemini-2.5-flash-lite` for reduced API costs.

---

## 🐛 Fixes

### Reliability & Correctness
- **[CRITICAL] Pool semaphore enforcement regression**: Fixed broken utilization enforcement that was causing 233% utilization despite previous fix (TASK-1212).
- **CLI tracker schema alignment**: Fixed schema inconsistency between `agent-runner` and `orchestrator-cli`.
- **Phase output contract validation**: Expanded JSON Schema validation coverage for phase output contracts.
- **Workspace test failures**: Resolved 10 failing workspace tests (`daemon_run`, `runtime_project_task`, `shared::parsing`).

---

## 🔧 Improvements

### Quality & Process
- **Stronger code review directive**: Implemented 7-point checklist for Opus quality gate.
- **Enhanced PR reviews**: Using Claude Opus for PR review and code review for stronger reasoning.
- **Task acceptance criteria**: Code review now verifies task requirements and acceptance criteria before approving.

---

## Dependencies
- Updated `Cargo.lock` for v0.1.0 compatibility.

---

## Previous Release
- [v0.1.0](./CHANGELOG-v0.1.0.md)
