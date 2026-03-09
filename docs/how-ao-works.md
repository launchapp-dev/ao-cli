# How AO Works: End-to-End Guide

> From idea to shipped code — how a SaaS company uses AO to orchestrate AI agents across the entire software delivery lifecycle.

## The Big Picture

```mermaid
flowchart TB
    subgraph YOU["👤 You (Founder / PM)"]
        vision["ao vision draft<br/>Define what you're building"]
        reqs["ao requirements draft<br/>Break vision into requirements"]
        execute["ao requirements execute<br/>Convert to tasks + start workflows"]
    end

    subgraph DAEMON["🔄 AO Daemon (Autonomous Background Process)"]
        tick["Project Tick Loop<br/>Every 5 seconds"]
        queue["Dispatch Queue<br/>Priority-ordered task queue"]
        spawn["Spawn Workflow<br/>Create worktree + assign agents"]
        reconcile["Reconcile Results<br/>Update task status, cleanup"]
    end

    subgraph WORKFLOW["⚙️ Workflow Pipeline (Per Task)"]
        direction LR
        triage["Triage"] --> research["Research"]
        research --> plan["Plan"]
        plan --> impl["Implement"]
        impl --> review["Code Review"]
        review -->|rework| impl
        review --> security["Security Review"]
        security --> test["Testing"]
        test --> po["PO Review"]
    end

    subgraph AGENTS["🤖 AI Agents (Each Phase)"]
        agent["Agent gets:<br/>• System prompt for role<br/>• Task description + ACs<br/>• MCP tools (AO, GitHub, Slack...)<br/>• Isolated git worktree"]
        decision["Returns PhaseDecision:<br/>advance / rework / fail / skip"]
    end

    subgraph OUTPUT["📦 Deliverables"]
        pr["Pull Request on GitHub"]
        branch["Feature branch with commits"]
        artifacts["Test results, review notes"]
        dashboard["Web Dashboard metrics"]
    end

    vision --> reqs --> execute
    execute --> queue
    tick --> queue
    queue --> spawn
    spawn --> WORKFLOW
    WORKFLOW --> AGENTS
    AGENTS --> reconcile
    reconcile --> tick
    reconcile --> OUTPUT
```

---

## Step-by-Step: Building Your SaaS

### Phase 1: Define What You're Building

```mermaid
flowchart LR
    A["ao setup<br/>Configure project,<br/>tech stack, MCP servers"]
    --> B["ao vision draft<br/>AI generates vision doc<br/>from your description"]
    --> C["ao requirements draft<br/>--include-codebase-scan<br/>Scans code + generates<br/>8-16 requirements"]
    --> D["ao requirements refine<br/>AI sharpens acceptance<br/>criteria per requirement"]

    style A fill:#1a1a2e,stroke:#e94560,color:#fff
    style B fill:#1a1a2e,stroke:#e94560,color:#fff
    style C fill:#1a1a2e,stroke:#e94560,color:#fff
    style D fill:#1a1a2e,stroke:#e94560,color:#fff
```

You describe your SaaS idea. AO generates a **vision document**, breaks it into **requirements** (REQ-001, REQ-002...) with priorities (Must/Should/Could), and refines each with acceptance criteria.

**Commands:**

```bash
ao setup                                          # Interactive project setup
ao vision draft                                   # Generate vision from description
ao requirements draft --include-codebase-scan     # Scan code + generate requirements
ao requirements refine --requirement-ids REQ-001  # Sharpen acceptance criteria
```

### Phase 2: Requirements Become Tasks

```mermaid
flowchart TB
    REQ["REQ-001: User Authentication<br/>Priority: Must | Status: Planned"]

    REQ --> T1["TASK-001: OAuth2 Google login<br/>Type: feature | Priority: high"]
    REQ --> T2["TASK-002: JWT session management<br/>Type: feature | Priority: high"]
    REQ --> T3["TASK-003: Rate limiting middleware<br/>Type: feature | Priority: medium"]

    T1 --> W1["Workflow spawned →"]
    T2 --> W2["Workflow spawned →"]
    T3 -->|blocked by T1, T2| QUEUE["Waiting in queue"]

    style REQ fill:#0f3460,stroke:#e94560,color:#fff
    style T1 fill:#16213e,stroke:#0f3460,color:#fff
    style T2 fill:#16213e,stroke:#0f3460,color:#fff
    style T3 fill:#16213e,stroke:#0f3460,color:#fff
```

`ao requirements execute` creates tasks and immediately starts the daemon working on them. Tasks respect **dependency ordering** — TASK-003 waits until TASK-001 and TASK-002 are done.

**The hierarchy:**

| Level | Entity | Example |
|-------|--------|---------|
| Vision | Single document | "Build a project management SaaS" |
| Requirements | REQ-001..REQ-016 | "User authentication with OAuth2" |
| Tasks | TASK-001..TASK-040 | "Implement Google OAuth2 login flow" |

**Task metadata includes:** type (feature/bugfix/refactor), priority (critical/high/medium/low), risk level, impact areas (frontend/backend/API/infra), assignee, dependencies, acceptance criteria, and worktree path.

### Phase 3: The Daemon Orchestrates Everything

```mermaid
flowchart TB
    subgraph TICK["Daemon Tick (every 5s)"]
        direction TB
        load["Load tasks + queue state<br/>from .ao/state/*.json"]
        check["Check: any tasks Ready?<br/>Check: concurrency < limit?"]
        dispatch["Dequeue highest priority<br/>Create SubjectDispatch"]
        worktree["Create git worktree<br/>~/.ao/project/worktrees/TASK-001/"]
        run["Spawn: ao workflow execute<br/>as subprocess"]
        poll["Poll running workflows<br/>for completion"]
        update["Update task status<br/>Clean up worktree<br/>Emit events"]
    end

    load --> check --> dispatch --> worktree --> run --> poll --> update
    update -->|next tick| load

    subgraph LIMITS["Concurrency Controls"]
        max_wf["Max 3 workflows running"]
        max_agents["Max 10 agents total"]
        priority["Higher priority = dispatched first"]
    end

    check -.-> LIMITS
```

The daemon is the brain. It runs autonomously in the background, picking up ready tasks, spawning workflows in isolated git worktrees, and reconciling results.

**Commands:**

```bash
ao daemon start --autonomous  # Fork background daemon
ao daemon status              # Check if running
ao daemon pause               # Pause scheduling (finish in-flight work)
ao daemon resume              # Resume scheduling
ao daemon stop                # Graceful shutdown
```

**What happens each tick:**

1. **Load state** — reads tasks, workflows, dispatch queue from `.ao/state/`
2. **Queue candidates** — identifies tasks in `Ready` status
3. **Apply limits** — respects max concurrent workflows and agents
4. **Dispatch** — dequeues highest priority task, creates `SubjectDispatch` envelope
5. **Spawn** — creates git worktree, launches `ao workflow execute` subprocess
6. **Poll** — checks running workflows for completion
7. **Reconcile** — updates task status, cleans up worktrees, emits events

### Phase 4: Each Task Runs Through a Workflow Pipeline

```mermaid
sequenceDiagram
    participant D as Daemon
    participant W as Worktree
    participant T as Triager Agent
    participant R as Researcher Agent
    participant E as Engineer Agent
    participant CR as Code Reviewer
    participant TE as Tester Agent
    participant PO as PO Reviewer

    D->>W: Create worktree + branch task/TASK-001
    D->>T: Phase 1: Triage
    T->>T: Validate task, check for duplicates
    T-->>D: verdict: advance ✓

    D->>R: Phase 2: Research
    R->>R: Explore codebase, find patterns
    R-->>D: verdict: advance ✓

    D->>E: Phase 3: Implement
    E->>W: Write code in worktree
    E->>W: Run tests locally
    E->>W: git commit + push
    E-->>D: verdict: advance ✓

    D->>CR: Phase 4: Code Review
    CR->>W: Review git diff
    CR-->>D: verdict: rework ✗ (missing error handling)

    D->>E: Phase 3 again: Rework (attempt 2/3)
    E->>W: Fix issues, recommit
    E-->>D: verdict: advance ✓

    D->>CR: Phase 4 again: Code Review
    CR-->>D: verdict: advance ✓

    D->>TE: Phase 5: Testing
    TE->>W: cargo test --workspace
    TE-->>D: verdict: advance ✓

    D->>PO: Phase 6: PO Review
    PO->>PO: Verify all acceptance criteria
    PO-->>D: verdict: advance ✓

    D->>W: Auto-create PR + merge
    D->>D: Task status → Done
```

Each agent is a **specialized AI persona** with its own system prompt, model, and MCP tool access. If code review fails, the engineer gets sent back to rework — up to a configurable maximum before escalating.

**Workflow definitions** live in `.ao/workflows/` as YAML:

```yaml
pipelines:
  standard-workflow:
    phases:
      - id: triage
        agent: triager
      - id: research
        agent: researcher
      - id: implementation
        agent: senior-engineer
        max_reworks: 3
      - id: code-review
        agent: code-reviewer
      - id: testing
        agent: integration-tester
      - id: po-review
        agent: po-reviewer
    post_success:
      auto_merge: true
      auto_pr: true
```

**Phase decisions** returned by agents:

| Verdict | Meaning |
|---------|---------|
| `advance` | Phase passed, move to next |
| `rework` | Send back to previous phase for fixes |
| `skip` | Phase not applicable, skip it |
| `fail` | Unrecoverable failure, stop workflow |

### Phase 5: Agents Have Superpowers via MCP

```mermaid
flowchart LR
    subgraph AGENT["🤖 Senior Engineer Agent"]
        prompt["System prompt:<br/>'You are a senior engineer...'"]
    end

    subgraph MCP["MCP Tool Servers Available"]
        ao["ao tools<br/>task.get, task.update<br/>workflow.phases, git.commit"]
        gh["GitHub<br/>create PR, review comments<br/>check CI status"]
        slack["Slack<br/>notify team channel<br/>request human input"]
        db["PostgreSQL<br/>query schema, test data"]
        search["Web Search<br/>look up docs, APIs"]
        notion["Notion<br/>update project wiki"]
    end

    AGENT --> ao
    AGENT --> gh
    AGENT --> slack
    AGENT --> db
    AGENT --> search
    AGENT --> notion
```

Agents aren't just coding — they can interact with your entire tool stack. A researcher agent can search the web, an engineer can query your database schema, and a PO reviewer can update Notion.

**MCP servers are configured per workflow:**

```yaml
mcp_servers:
  ao:
    command: ao
    args: [mcp, serve]
  github:
    command: npx
    args: [-y, @modelcontextprotocol/server-github]
    env:
      GITHUB_PERSONAL_ACCESS_TOKEN: ${GITHUB_TOKEN}
  slack:
    command: npx
    args: [-y, @anthropic/mcp-server-slack]
    env:
      SLACK_BOT_TOKEN: ${SLACK_BOT_TOKEN}
```

**Agent profiles define specialized personas:**

| Agent | Role | Tools |
|-------|------|-------|
| `triager` | Validates tasks, detects duplicates | ao |
| `researcher` | Gathers evidence, explores patterns | ao, web-search |
| `senior-engineer` | Writes production code | ao, github |
| `code-reviewer` | Reviews diffs for bugs and edge cases | ao, github |
| `security-reviewer` | Validates against OWASP, secrets exposure | ao |
| `integration-tester` | Runs test suites, checks coverage | ao |
| `po-reviewer` | Verifies acceptance criteria are met | ao, notion |

### Phase 6: Monitor Everything

```mermaid
flowchart TB
    subgraph MONITOR["How You Monitor"]
        cli["CLI<br/>ao task stats<br/>ao daemon status<br/>ao workflow list"]
        web["Web Dashboard<br/>ao web serve<br/>Real-time Kanban board"]
        tui["Terminal UI<br/>ao tui<br/>Ratatui dashboard"]
        mcp_monitor["MCP in Claude Code<br/>ao.task.list<br/>ao.daemon.health"]
    end

    subgraph METRICS["What You See"]
        tasks["5/10 tasks done<br/>3 in-progress, 2 blocked"]
        workflows["9/10 workflows passed<br/>1 needed rework"]
        cost["Total cost: $12.50<br/>Avg $1.25/task"]
        time["Avg completion: 8 min/task"]
    end

    MONITOR --> METRICS
```

**Monitoring commands:**

```bash
ao task stats                    # Task breakdown by status/priority/type
ao task prioritized              # View priority-ordered task list
ao daemon status                 # Daemon health + active workflows
ao daemon logs                   # Review daemon log output
ao workflow list                 # All workflows with phase progress
ao workflow get <id>             # Full workflow with decision history
ao web serve                     # Launch web dashboard
ao tui                           # Terminal UI dashboard
```

---

## The Full Lifecycle

```mermaid
flowchart TB
    IDEA["💡 Your SaaS Idea"]
    --> VISION["ao vision draft"]
    --> REQS["ao requirements draft<br/>REQ-001..REQ-016"]
    --> EXECUTE["ao requirements execute<br/>Creates TASK-001..TASK-040"]
    --> DAEMON["ao daemon start --autonomous"]

    DAEMON --> LOOP{"Daemon Tick Loop"}

    LOOP -->|"Ready task found"| WORKTREE["Create isolated worktree"]
    WORKTREE --> PIPELINE["Run workflow pipeline<br/>triage → research → plan →<br/>implement → review → test → accept"]
    PIPELINE -->|"All phases pass"| PR["Auto-create PR + merge"]
    PIPELINE -->|"Phase fails"| REWORK["Rework or escalate"]
    REWORK --> PIPELINE
    PR --> DONE["Task → Done ✓"]
    DONE --> LOOP

    LOOP -->|"All tasks done"| SHIPPED["🚀 Feature Shipped"]

    SHIPPED -->|"Next sprint"| REQS

    style IDEA fill:#e94560,stroke:#fff,color:#fff
    style SHIPPED fill:#0f3460,stroke:#fff,color:#fff
    style DAEMON fill:#533483,stroke:#fff,color:#fff
```

**The short version:** You describe what to build. AO breaks it into tasks, assigns AI agents to each one, runs them through a quality pipeline (triage → research → code → review → test → accept), and delivers PRs. You review and merge.

---

## Key Architecture Patterns

### Isolated Worktrees

Every task executes in its own git worktree — an isolated copy of the repository at `~/.ao/<repo-scope>/worktrees/<task-id>/`. Agents can write code, run tests, and commit without interfering with each other or your working directory.

### Subject Dispatch

All work flows through a unified `SubjectDispatch` envelope:

```
SubjectDispatch {
    subject: Task { id: "TASK-001" } | Requirement { id: "REQ-001" } | Custom { ... },
    workflow_ref: "standard-workflow",
    trigger: Schedule | Manual | Queue,
}
```

This means the same dispatch pipeline handles scheduled cron jobs, manual fires, and priority-queue picks.

### Atomic Persistence

All state is persisted via atomic writes (write to temp file → sync → rename). This prevents corruption if the daemon crashes mid-write. State lives in `.ao/state/*.json`.

### Self-Correcting Pipelines

The rework loop is the quality guarantee. When a code reviewer finds issues, it sends work back to the engineer with failure context. The engineer sees exactly what went wrong and fixes it. Up to 3 rework cycles before escalating to a human.

### Failure Recovery

- **Phase fails** → retried up to configured max
- **All retries exhausted** → workflow fails, task marked blocked
- **Daemon crashes** → orphan recovery on next startup
- **Merge conflicts** → AI-powered conflict resolution attempts before escalating

---

## Example: A Typical Day

```
Morning:
  $ ao requirements execute --requirement-ids REQ-005..REQ-008 --start-workflows
  → 12 tasks created, daemon picks them up

  $ ao daemon status
  → 3 workflows running, 9 queued

Afternoon:
  $ ao task stats
  → 7 done, 3 in-progress, 2 blocked (waiting on dependency)

  $ ao workflow list --status failed
  → 1 workflow failed at security-review (hardcoded API key detected)
  → Agent auto-reworked, now passing

Evening:
  $ ao task stats
  → 11 done, 1 in-progress

  $ gh pr list
  → 11 PRs ready for review
  → Review, approve, merge
```
