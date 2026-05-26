# Quick Start

This guide takes you from a fresh repository to autonomous AI workflows. Animus is built to run continuously with a background daemon that executes work automatically.

## 0. Five-Minute Walkthrough (v0.4.13+)

The fastest way to verify a fresh install end-to-end is the `animus init`
walkthrough. From your project root:

```bash
cd /path/to/your/project
animus init --walkthrough
```

The walkthrough:

1. Detects which provider CLIs (`claude`, `codex`, `gemini`, `opencode`) are
   on `$PATH` and whether their API keys are configured via env vars or local
   dotfiles.
2. Prompts you to install the default provider plugins via
   `animus plugin install-defaults` (skip with `--no-install` if already
   installed).
3. Copies the bundled `hello-world` workflow template into
   `.animus/workflows/hello-world.yaml` (skip with `--no-template`).
4. Optionally starts the autonomous daemon (`--auto-start` or answer the
   prompt).
5. Prints next steps so you can immediately run:

   ```bash
   animus workflow run hello-world --sync
   ```

Non-interactive form for scripts / CI:

```bash
animus init --walkthrough --non-interactive --auto-start
```

If you want the registry-driven, pattern-based init instead, skip ahead to
step 2 below.

## 1. Install Plugins (one-time, v0.4.12+)

If you've already installed the default plugin set on this machine for another
project, skip to step 2 — plugins live in `~/.animus/plugins/` and are
shared across projects.

```bash
animus plugin install-defaults --include-subjects --include-transports
animus daemon preflight     # verify everything is in place
```

This installs the 5 standard providers, the 5 subject backends, and the
transport + web UI plugins. The daemon will refuse to start until at least
one provider and the required subject backends are present.

## 2. Prepare the Repository

```bash
cd /path/to/your/project
animus doctor
animus init --template task-queue --non-interactive
```

`animus init` is the primary first-run flow. It bootstraps the project-local `.animus/` config, copies the selected template workflow wrappers into the repo, and provisions repo-scoped runtime state under `~/.animus/<repo-scope>/`.

If you are running in a real terminal and want the guided picker instead of an explicit template id, run `animus init`. The default LaunchApp template registry currently provides:

- `task-queue` for queue-driven delivery with aggressive daemon defaults
- `conductor` for planning-heavy requirement intake and queue execution
- `direct-workflow` for human-driven workflow runs with conservative automation

## 3. Create Your First Task

```bash
animus subject create --kind task \
  --title "Add rate limiting" \
  --description "Throttle API requests before they hit the upstream provider" \
  --task-type feature \
  --priority high
```

The first task in a repository is typically `TASK-001`. This routes through
the `animus-subject-default` plugin you installed in step 1.

## 4. Mark the Task Ready and Start the Daemon

```bash
animus subject status --kind task --id task:TASK-001 --status ready
animus daemon start --autonomous
```

The daemon runs `animus daemon preflight` before booting. If preflight
already passed in step 1, no plugins will be downloaded here — it just
starts.

If you want the daemon to install missing plugins on its own (useful for
one-shot dev boxes):

```bash
animus daemon start --autonomous --auto-install
```

Escape hatch for dev iteration when knowingly running without the required
plugins:

```bash
animus daemon start --autonomous --skip-preflight
```

The daemon now polls for ready tasks and starts workflows automatically. You can let it run in the background.

## 5. Inspect Progress

```bash
animus subject list --kind task
animus workflow list
animus daemon status
animus output tail
animus status
```

## Testing a Workflow Before Daemon

If you want to test a workflow definition before running the daemon, use the `--sync` flag to run it synchronously in your terminal:

```bash
animus workflow run --task-id TASK-001 --sync
```

This is useful for debugging workflow definitions, agent prompts, or MCP tools. Once you're satisfied, follow steps 4–5 above to enable autonomous execution.

## Requirement-First Flow

If you want to start from product requirements instead of a direct task:

```bash
animus subject create --kind requirement \
  --title "Rate limiting" \
  --priority must \
  --acceptance-criterion "Requests above the threshold are delayed or rejected"
```

This routes through the `animus-subject-requirements` plugin and materializes
implementation tasks for the daemon to execute.

## Next Steps

- [Project Setup](project-setup.md)
- [A Typical Day](typical-day.md)
- [Upgrading Animus](../guides/upgrading.md)
- [Workflows](../concepts/workflows.md)
