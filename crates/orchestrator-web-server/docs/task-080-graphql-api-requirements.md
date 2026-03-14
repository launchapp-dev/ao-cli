# TASK-080: GraphQL API Requirements

## Overview

Add a GraphQL API endpoint (`/graphql`) to the web server using `async-graphql`, providing flexible queries and mutations for AO domain entities alongside the existing REST API.

## Current State

The web server crate (`crates/orchestrator-web-server`) has:
- A `graphql` feature flag in `Cargo.toml` (currently optional)
- Basic GraphQL types and queries in `services/graphql.rs`:
  - `GqlTask`, `GqlRequirement`, `GqlWorkflow`, `GqlDaemonStatus`
  - Query fields: `tasks`, `task`, `requirements`, `requirement`, `workflows`, `workflow`, `daemon_status`
  - Mutation fields: `create_task`
- Route registration at `/graphql` with playground support
- Integration with `ServiceHub` for data access

## Requirements

### 1. GraphQL Schema Coverage

The GraphQL API must expose the following domain types with full query and mutation support:

#### Tasks
- **Query**: `tasks(filter: TaskFilterInput)`, `task(id: ID!)`
- **Mutation**: `createTask(input: CreateTaskInput!)`, `updateTask(id: ID!, input: UpdateTaskInput!)`, `deleteTask(id: ID!)`
- **Relationships**:
  - `Task.linked_requirements` → list of `Requirement`
  - `Task.checklist` → list of `ChecklistItem`

#### Requirements
- **Query**: `requirements`, `requirement(id: ID!)`
- **Mutation**: `createRequirement(input: CreateRequirementInput!)`, `updateRequirement(id: ID!, input: UpdateRequirementInput!)`, `deleteRequirement(id: ID!)`
- **Relationships**:
  - `Requirement.tasks` → list of `Task`

#### Workflows
- **Query**: `workflows`, `workflow(id: ID!)`
- **Mutation**: `runWorkflow(input: RunWorkflowInput!)`, `resumeWorkflow(id: ID!)`, `pauseWorkflow(id: ID!)`, `cancelWorkflow(id: ID!)`
- **Relationships**:
  - `Workflow.phases` → list of `WorkflowPhase`
  - `Workflow.decisions` → list of `WorkflowDecision`

#### Daemon Status
- **Query**: `daemonStatus`, `daemonHealth`
- **Agent Runs** (if available):
  - **Query**: `agentRuns`, `agentRun(id: ID!)`

### 2. GraphQL Types Specification

```graphql
# Input Types
input TaskFilterInput {
  status: String
  priority: String
  task_type: String
}

input CreateTaskInput {
  title: String!
  description: String
  priority: String
  task_type: String
}

input UpdateTaskInput {
  title: String
  description: String
  status: String
  priority: String
  task_type: String
}

input CreateRequirementInput {
  title: String!
  description: String
  priority: String
  requirement_type: String
}

input UpdateRequirementInput {
  title: String
  description: String
  status: String
  priority: String
}

input RunWorkflowInput {
  task_id: String
  phase: String
}

# Object Types
type Task {
  id: ID!
  title: String!
  description: String
  status: String!
  priority: String!
  task_type: String!
  risk: String
  complexity: String
  scope: String
  checklist: [ChecklistItem!]
  requirements: [Requirement!]
}

type Requirement {
  id: ID!
  title: String!
  description: String
  priority: String!
  status: String!
  requirement_type: String
  tasks: [Task!]
}

type Workflow {
  id: ID!
  status: String!
  current_phase: String
  started_at: String
  completed_at: String
  phases: [WorkflowPhase!]
  decisions: [WorkflowDecision!]
}

type WorkflowPhase {
  name: String!
  status: String!
  started_at: String
  completed_at: String
}

type WorkflowDecision {
  id: ID!
  phase: String!
  decision: String!
  made_by: String
  made_at: String
}

type ChecklistItem {
  id: ID!
  content: String!
  completed: Boolean!
}

type DaemonStatus {
  healthy: Boolean!
  status: String!
  runner_connected: Boolean!
  active_agents: Int!
  max_agents: Int
  project_root: String
}

type DaemonHealth {
  healthy: Boolean!
  status: String!
  runner_connected: Boolean!
  active_agents: Int!
  max_agents: Int
  project_root: String
}

# Agent Run (if available)
type AgentRun {
  id: ID!
  task_id: String
  model: String
  status: String!
  started_at: String
  completed_at: String
}

# Root Types
type Query {
  tasks(filter: TaskFilterInput): [Task!]!
  task(id: ID!): Task
  requirements: [Requirement!]!
  requirement(id: ID!): Requirement
  workflows: [Workflow!]!
  workflow(id: ID!): Workflow
  daemonStatus: DaemonStatus!
  daemonHealth: DaemonHealth!
  agentRuns: [AgentRun!]
  agentRun(id: ID!): AgentRun
}

type Mutation {
  createTask(input: CreateTaskInput!): Task!
  updateTask(id: ID!, input: UpdateTaskInput!): Task
  deleteTask(id: ID!): Boolean!
  createRequirement(input: CreateRequirementInput!): Requirement!
  updateRequirement(id: ID!, input: UpdateRequirementInput!): Requirement
  deleteRequirement(id: ID!): Boolean!
  runWorkflow(input: RunWorkflowInput!): Workflow!
  resumeWorkflow(id: ID!): Workflow
  pauseWorkflow(id: ID!): Workflow
  cancelWorkflow(id: ID!): Workflow
}
```

### 3. Implementation Constraints

1. **Feature Flag**: GraphQL remains opt-in via `graphql` feature in `Cargo.toml`
2. **ServiceHub Integration**: Use existing `ServiceHub` trait methods for data access
3. **Error Handling**: Map service errors to GraphQL errors with meaningful messages
4. **ID Format**: Use string IDs for all entity types (matching AO's `REQ-XXX`, `TASK-XXX`, `WF-XXX` format)
5. **Subscription**: Not required for this phase; noted as follow-up

### 4. Acceptance Criteria

- [ ] GraphQL endpoint available at `/graphql` when `graphql` feature enabled
- [ ] GraphQL Playground available at `/graphql` (GET) for development
- [ ] All specified queries return correct data from ServiceHub
- [ ] All specified mutations create/update/delete entities correctly
- [ ] Task ↔ Requirement relationship queries work in both directions
- [ ] Workflow phases and decisions are queryable
- [ ] Daemon status and health are queryable
- [ ] Errors from ServiceHub are properly mapped to GraphQL errors
- [ ] Web server builds and runs with `graphql` feature enabled
- [ ] Basic smoke test confirms endpoint responds

### 5. Out of Scope (Follow-up)

- GraphQL Subscriptions for real-time updates
- Complex filtering beyond status/priority/type for tasks
- Authentication/authorization at GraphQL level
- Full CRUD for all entity types beyond core operations
