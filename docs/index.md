# AO Documentation

**AO is a Rust-based agent orchestrator CLI that dispatches YAML-defined workflows to AI agents, turning ideas into shipped code through automated planning, implementation, review, and testing pipelines.**

---

## Documentation

- **[Getting Started](getting-started/index.md)** -- Install AO, configure your first project, and run your first autonomous workflow.

- **[Concepts](concepts/)** -- Core mental models: SubjectDispatch, workflows, phases, agents, projectors, and the three-layer architecture.

- **[Guides](guides/)** -- Task-oriented walkthroughs for common operations like writing custom workflows, configuring MCP servers, and tuning daemon capacity.

- **[Reference](reference/)** -- Complete CLI command surface, YAML workflow schema, JSON output contracts, and exit codes.

- **[Architecture](architecture/)** -- Design documents covering the subject dispatch model, tool-driven mutation surfaces, and workflow-first CLI design.

- **[Internals](internals/)** -- Crate-level deep dives into orchestrator-core, agent-runner, workflow-runner, and the daemon scheduler.

- **[Contributing](contributing/)** -- Development setup, testing conventions, CI pipelines, and coding standards.

---

## Quick Links

```bash
ao setup                              # Configure project
ao vision draft                       # Generate vision document
ao requirements draft                 # Generate requirements
ao requirements execute               # Create tasks from requirements
ao daemon start --autonomous          # Start autonomous execution
ao task stats                         # Check progress
```

See the [CLI Command Surface](cli-command-surface.md) for the full command tree.
