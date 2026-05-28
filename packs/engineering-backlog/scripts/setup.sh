#!/usr/bin/env bash
#
# packs/engineering-backlog/scripts/setup.sh
#
# Idempotent setup for the Engineering Backlog reference pack.
# Installs required plugins and seeds the local task backlog.
#
# Usage (from the root of your project):
#   bash packs/engineering-backlog/scripts/setup.sh
#
# Re-runnable: skips installs that are already present.

set -euo pipefail

PACK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_ROOT="$(pwd)"

BACKLOG_DIR="${PROJECT_ROOT}/tasks/backlog"
WORKFLOW_OVERLAY_DIR="${PROJECT_ROOT}/.animus/workflows"

echo "==> Engineering Backlog pack setup"
echo "    Pack directory : ${PACK_DIR}"
echo "    Project root   : ${PROJECT_ROOT}"
echo

# 0. Sanity check: animus CLI must be on PATH
if ! command -v animus >/dev/null 2>&1; then
  echo "ERROR: 'animus' CLI not found on PATH." >&2
  echo "  Install: curl -fsSL https://animus.sh/install | bash" >&2
  echo "  or see docs/getting-started/installation.md" >&2
  exit 1
fi

# Helper: run an `animus plugin install` and decide whether to abort
# setup. Treats "already installed" as success; any other non-zero exit
# is a hard failure so users don't end up with a half-configured pack.
#
# Output is NOT redirected: the installer's trust-on-first-use prompt
# (for publishers like launchapp-dev that have not been trusted yet)
# needs to reach the user's terminal. We tee stdout to a temp file so
# the success/failure classification can grep for "already installed"
# without hiding the prompt.
install_plugin() {
  local plugin_id="$1"; shift
  # NOTE: macOS ships Bash 3.2 where expanding an empty array under
  # `set -u` errors with "unbound variable". Use "$@" (which is always
  # defined, possibly empty) instead of a local array copy.
  local tmp
  tmp="$(mktemp)"
  set +e
  animus plugin install "${plugin_id}" "$@" 2>&1 | tee "${tmp}"
  local status="${PIPESTATUS[0]}"
  set -e
  if [ "${status}" -eq 0 ]; then
    rm -f "${tmp}"
    return 0
  fi
  if grep -qiE "already installed|already present" "${tmp}"; then
    echo "    (already installed — continuing)"
    rm -f "${tmp}"
    return 0
  fi
  rm -f "${tmp}"
  echo >&2
  echo "ERROR: failed to install ${plugin_id}. Setup aborted." >&2
  echo "       If the failure was a trust-on-first-use prompt and you are" >&2
  echo "       running non-interactively, add the publisher to your" >&2
  echo "       trusted-signers list and rerun." >&2
  exit 1
}

# 1. Install required plugins
echo "==> Installing required plugins"

# Subject backend for `task` — file-backed markdown/JSON storage.
install_plugin launchapp-dev/animus-subject-default

# Implementer + planner + researcher + finalizer all use Claude.
# Provider plugins claim reserved tool names (claude, codex, oai, etc.),
# so installation needs --allow-shadow-builtin to acknowledge the
# shadow-builtin override.
install_plugin launchapp-dev/animus-provider-claude --allow-shadow-builtin

# Cross-model reviewer uses Codex.
install_plugin launchapp-dev/animus-provider-codex --allow-shadow-builtin

# 2. Seed the workflow overlay
echo
echo "==> Seeding workflow overlay at ${WORKFLOW_OVERLAY_DIR}"
mkdir -p "${WORKFLOW_OVERLAY_DIR}"
if [ ! -f "${WORKFLOW_OVERLAY_DIR}/implement-task.yaml" ]; then
  cp "${PACK_DIR}/workflows/implement-task.yaml" "${WORKFLOW_OVERLAY_DIR}/"
  echo "    Copied implement-task.yaml"
else
  echo "    implement-task.yaml already present — leaving it untouched"
fi

# 3. Seed the task backlog. Split the bundled sample-tasks.md into one
#    file per task so each can be dispatched independently. Idempotent:
#    skips tasks that already exist on disk.
echo
echo "==> Seeding task backlog at ${BACKLOG_DIR}"
mkdir -p "${BACKLOG_DIR}"

SAMPLE_BUNDLE="${PACK_DIR}/subjects/sample-tasks.md"
if [ ! -f "${SAMPLE_BUNDLE}" ]; then
  echo "WARN: ${SAMPLE_BUNDLE} missing — skipping task seeding" >&2
else
  # Idempotent: skip any TASK-* file that already exists on disk so a
  # rerun never clobbers a user's local edits or status updates.
  # File existence is checked via awk getline (no shell — avoids any
  # injection from project paths containing $(...) or quotes).
  awk -v outdir="${BACKLOG_DIR}" '
    function file_exists(path,   line, status) {
      status = (getline line < path)
      close(path)
      return status >= 0
    }
    /^## TASK-[0-9]+:/ {
      if (out) close(out)
      out = ""
      # Extract task id (TASK-1001) from "## TASK-1001: ..."
      match($0, /TASK-[0-9]+/)
      id = substr($0, RSTART, RLENGTH)
      target = outdir "/" id ".md"
      if (file_exists(target)) {
        # File already exists — leave it alone.
        next
      }
      out = target
      print $0 > out
      next
    }
    /^---$/ {
      # Section separators in the bundle — do not write to per-task files
      next
    }
    out { print >> out }
  ' "${SAMPLE_BUNDLE}"
  echo "    Per-task files in ${BACKLOG_DIR}/ (existing files left untouched):"
  ls "${BACKLOG_DIR}"/TASK-*.md 2>/dev/null | sed 's/^/      /'
fi

# 4. Friendly next-steps
# Pass the same PROJECT_ROOT we just seeded into the printed commands,
# so the user's worktree/cwd doesn't accidentally resolve a different
# git common root.
cat <<EOF

==> Setup complete.

Next steps (all commands include --project-root so they target the
directory we just seeded, regardless of cwd):

  1. Make sure your LLM API keys are exported in the daemon's environment:
       export ANTHROPIC_API_KEY=...        # for the planner/implementer
       export OPENAI_API_KEY=...           # for the codex reviewer

  2. Start the daemon (preflight will check plugin coverage):
       animus daemon start --auto-install --project-root "${PROJECT_ROOT}"

  3. Smoke-test the subject backend is wired up:
       animus subject list --kind task --project-root "${PROJECT_ROOT}"

  4. Run the workflow synchronously against the first sample task. The
     workflow run CLI currently requires --task-id, --requirement-id,
     or --title. --task-id resolves through the built-in task adapter,
     which does not see plugin-backed markdown tasks in a fresh
     project — so the verified-working dispatch path is --title +
     --description (same approach as the customer-support pack). The
     LLM phases get the task title + body inline; the finalize phase
     opens a PR but skips the subject status mutation (it checks the
     id shape and bails out — see workflows/implement-task.yaml).

     IMPORTANT: title-dispatch runs in YOUR checkout, not a managed
     worktree. Switch to a feature branch BEFORE dispatching so the
     implement phase commits don't land on main and the finalize push
     doesn't update your default branch. The finalize agent has a
     safety gate that refuses to push when HEAD is on main/master/
     develop, but creating a branch up-front is the clean path:
       git switch -c ao/smoke-engineering-backlog-001
       TASK_FILE="${PROJECT_ROOT}/tasks/backlog/TASK-1005.md"
       animus workflow run animus.engineering-backlog/implement-task \\
         --title "\$(head -n 1 "\$TASK_FILE" | sed 's/^## //')" \\
         --description "\$(tail -n +2 "\$TASK_FILE")" \\
         --sync \\
         --project-root "${PROJECT_ROOT}"

     End-to-end --task-id dispatch (so finalize can mutate the task
     status via animus.subject.status) is NOT available with this
     plugin-backed setup today: `animus subject create --kind task`
     routes through the animus-subject-default plugin, but
     `animus workflow run --task-id` resolves through the built-in
     task adapter, so plugin-created ids return "task not found".
     The end-to-end path needs either:
       (a) the missing `animus workflow run --subject-id <id>
           --subject-kind task` primitive (tracked upstream), OR
       (b) a built-in task adapter that mirrors plugin task records.
     Until either lands, stick with the --title path above. The
     finalize phase will skip the subject status mutation (and emit
     "skipped subject update" in its verdict) but the PR will still
     be opened on the success path.

  5. Inspect the run. Animus mints a fresh UUID per dispatch, so list
     recent runs and use that id:
       animus workflow list --project-root "${PROJECT_ROOT}"
       animus output phase-outputs --workflow-id <id-from-list> \\
         --project-root "${PROJECT_ROOT}"

     Or capture the id via --json at dispatch time (the CLI envelope is
     animus.cli.v1 — payload under data):
       WF_ID=\$(animus workflow run ... --sync --json --project-root "${PROJECT_ROOT}" \\
         | jq -r '.data.workflow_id')
       animus output phase-outputs --workflow-id "\$WF_ID" \\
         --project-root "${PROJECT_ROOT}"

To customize the workflow (swap models, skip review, change branch /
PR target, wire Linear), see
packs/engineering-backlog/docs/customizing.md.
EOF
