## REQ-479 Refinement Summary

### Status: DUPLICATE → REQ-352

**Reason**: REQ-352 "orchestrator-cli over-coupling with 10 internal dependencies" already covers this architectural concern.

**Action Taken**: REQ-479 marked as done (duplicate). Technical details merged into REQ-352 description.

### Current State (from Cargo.toml lines 17-26)

CLI has **10** internal workspace dependencies:
1. `orchestrator-core`
2. `orchestrator-web-api`
3. `orchestrator-web-server`
4. `orchestrator-daemon-runtime`
5. `orchestrator-notifications`
6. `orchestrator-config`
7. `orchestrator-logging`
8. `protocol`
9. `llm-cli-wrapper` (as `cli-wrapper`)
10. `workflow-runner-v2`

### Technical Assessment

**Confirmed Issue**: The CLI accesses `orchestrator-daemon-runtime` via **three independent import paths**:
- Direct import (Cargo.toml line 23)
- Via `orchestrator-web-api` (line 19) which re-exports daemon types
- Via `orchestrator-web-server` (line 20) → `orchestrator-web-api`

**Root Cause**: The CLI uses IPC types from `workflow-runner-v2` and CLI wrapper types from `llm-cli-wrapper` directly, even though `orchestrator-core` should encapsulate these.

### Refined Acceptance Criteria
- Reduce orchestrator-cli internal dependencies from 10 to ≤5
- CLI only imports web components through orchestrator-web-api abstraction
- CLI does not directly depend on orchestrator-daemon-runtime or workflow-runner-v2
- Architectural layers preserved: CLI → Core → Domain crates
- No runtime concern accessed via more than one direct import path