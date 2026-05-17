# MCP Integration

## What MCP Is

MCP is Animus's tool boundary. Agents and workflows use MCP to read and mutate
state, and packs can contribute additional MCP server descriptors without
teaching the daemon new behavior.

## Animus's Core MCP Surface

Animus ships an MCP server:

```bash
animus mcp serve
```

It exposes Animus mutation and query tools such as:

- `animus.task.*`
- `animus.requirements.*`
- `animus.workflow.*`
- `animus.daemon.*`

Many of those tools are now conceptually owned by bundled first-party packs
such as `animus.task` and `animus.requirement`, even though they are exposed through
the Animus MCP server.

## Pack-Owned MCP Descriptors

Packs can also ship MCP descriptors under pack assets. Animus loads those
descriptors, namespaces the resulting server ids by pack id, and makes them
available to workflows and phases.

Examples:

- `animus.requirement/github-sync`
- `vendor.crm/runtime`

Pack-owned MCP behavior stays outside the daemon. The daemon only supervises the
processes and records execution facts.

## Workflow-Level Usage

Project YAML can reference MCP servers directly, and pack overlays can inject
phase bindings and default server sets.

Key rules:

- project YAML defines repo-specific MCP servers
- pack overlays can contribute namespaced MCP servers
- agents and phases only see explicitly allowed tools
- Animus state mutations should go through MCP or CLI mutation surfaces, not direct
  file edits

## Why This Boundary Exists

Tool-driven mutation keeps Animus auditable and composable:

- state changes flow through validated surfaces
- external integrations remain process-based
- packs can add behavior without changing daemon-core

See [Workflows](./workflows.md) and [How Animus Works](./how-ao-works.md) for how
MCP fits into execution.
