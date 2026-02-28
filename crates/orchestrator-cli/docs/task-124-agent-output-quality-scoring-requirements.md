# TASK-124: Agent Output Quality Scoring with Model Routing Feedback

## Problem Statement

The Agent Orchestrator currently selects models for workflow phases using static routing rules but lacks post-hoc quality assessment. When an agent completes a phase, the system doesn't verify whether the output actually:
1. Modified files as intended
2. Produced compilable code
3. Passed tests

This prevents evidence-based model selection and allows consistently-failing models to continue being routed.

## Requirements

### Core Requirements

| ID | Requirement | Description |
|----|-------------|-------------|
| RQ-01 | Quality Score Recording | After each agent phase execution, record a quality score based on post-hoc verification: files modified, compilation status, test results |
| RQ-02 | Model Success Rate Tracking | Track per-model success rates across all phases and runs, storing metrics in `.ao/state/model-performance.json` |
| RQ-03 | Compilation Check | After implementation phases, run `cargo check` or equivalent to verify code compiles |
| RQ-04 | Test Verification | After testing phases (or when tests exist), run test suites to verify pass/fail status |
| RQ-05 | File Modification Detection | Detect and record whether agent actually modified repository files during the phase |
| RQ-06 | Model Routing Feedback | Feed quality scores back into model selection; exclude models with >3 consecutive compilation failures |
| RQ-07 | Phase Decision Integration | Integrate quality scoring with phase decision contracts; emit evidence based on scoring results |

### Evidence Types

| Evidence Kind | Source | When Emitted |
|--------------|--------|--------------|
| `files_modified` | Git diff detection | Post-execution for agent modes |
| `compilation_passed` | `cargo check` result | Post-implementation phase |
| `compilation_failed` | `cargo check` result | Post-implementation phase |
| `tests_passed` | Test suite exit code | Post-testing phase |
| `tests_failed` | Test suite exit code | Post-testing phase |
| `quality_score` | Aggregated score 0-1.0 | Post any agent execution |

### Configuration

| Config Location | Purpose |
|-----------------|---------|
| `.ao/state/model-performance.json` | Per-model metrics: attempts, successes, failures, consecutive_failures |
| `.ao/config.json` (new field) | Quality scoring thresholds and routing preferences |

### Acceptance Criteria

- [ ] AC-01: Quality scores are recorded after each agent phase execution
- [ ] AC-02: Model success rates are tracked and persisted across daemon restarts
- [ ] AC-03: Compilation checks run automatically after implementation phases
- [ ] AC-04: Test verification runs when test files exist in the project
- [ ] AC-05: Models with >3 consecutive compilation failures are excluded from routing
- [ ] AC-06: Phase decisions include quality evidence from scoring
- [ ] AC-07: `ao model status` displays current model performance metrics
- [ ] AC-08: Routing logic considers historical model performance when selecting models

### Implementation Notes

1. **State Storage**: Add `model-performance.json` in `.ao/state/` following existing JSON state patterns
2. **Scoring Pipeline**: Extend `process_phase_execution_completion` in `daemon_scheduler_project_tick.rs` to run quality checks
3. **Model Exclusion**: Modify `default_primary_model_for_phase` and fallback selection in `protocol/src/model_routing.rs` to accept performance context
4. **CLI Integration**: Add `ao model stats` or extend `ao model status` to show performance data

### Risk Assessment

- **Low Risk**: Additive feature; doesn't modify existing core routing logic
- **Medium Risk**: Requires running external commands (`cargo check`, tests) which may have side effects; must sandbox appropriately
- **Mitigation**: Run checks in read-only mode first; only record results without modifying state on failure
