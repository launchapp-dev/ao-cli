# Requirements Workflow Guide

AO treats requirements as first-class project records. You create them, refine
them, and then execute them into tasks and workflows when they are ready for
implementation.

## Create a Requirement

Start with a requirement record:

```bash
animus requirements create \
  --title "Add rate limiting" \
  --description "Protect the API from burst traffic." \
  --priority high
```

This writes a tracked requirement into AO-managed state. Requirements can also
carry a category, type, source, and acceptance criteria.

## Inspect and Refine

List and inspect existing requirements:

```bash
animus requirements list
animus requirements get --id REQ-001
```

Update a requirement as the scope becomes clearer:

```bash
animus requirements update \
  --id REQ-001 \
  --title "Add request rate limiting" \
  --priority critical
```

Requirements support the lifecycle states used by the CLI and state machine:
`draft`, `refined`, `planned`, `in-progress`, and `done`.

## Execute Into Tasks

When a requirement is ready, execute it into tasks:

```bash
animus requirements execute --id REQ-001
```

Execution turns the requirement into one or more tasks and can optionally start
workflows for the generated tasks. Use `animus task list` and `animus workflow list` to
inspect the results.

## Manage Relationships

Use the requirement graph to understand links between requirements:

```bash
animus requirements graph get
```

Use the recommendations surface when you want AO to scan for improvements or
gaps:

```bash
animus requirements recommendations scan
animus requirements recommendations list
animus requirements recommendations apply --report-id REC-001
```

## Optional Mockups

Requirements can also carry mockup records and linked assets:

```bash
animus requirements mockups list
animus requirements mockups create --name "Rate limit banner" --description "Draft UI for 429 states"
```

## Practical Loop

The normal loop is simple:

1. Create the requirement.
2. Review or update it until the scope is clear.
3. Execute it into tasks.
4. Run the daemon or targeted workflows to do the work.
5. Use `ao task` and `ao workflow` commands to track progress.

See also: [Task Management](task-management.md), [Daemon Operations](daemon-operations.md), and
[Writing Workflows](writing-workflows.md).
