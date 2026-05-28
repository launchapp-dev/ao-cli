#!/usr/bin/env bash
#
# packs/marketing-outreach/scripts/setup.sh
#
# Idempotent setup for the Marketing Outreach reference pack.
# Installs required plugins and seeds the local prospect inbox.
#
# Usage (from the root of your project):
#   bash packs/marketing-outreach/scripts/setup.sh
#
# Re-runnable: skips installs that are already present.

set -euo pipefail

PACK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_ROOT="$(pwd)"

INBOX_DIR="${PROJECT_ROOT}/prospects/inbox"
WORKFLOW_OVERLAY_DIR="${PROJECT_ROOT}/.animus/workflows"

echo "==> Marketing Outreach pack setup"
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
install_plugin launchapp-dev/animus-subject-markdown

# Provider — default to Claude; users can swap per docs/customizing.md.
# The Claude provider plugin claims the reserved `claude` tool name, so
# the installer requires --allow-shadow-builtin on a clean machine to
# acknowledge the shadow-builtin override. `animus daemon start
# --auto-install` bypasses this guard automatically; we mirror that
# posture here so setup.sh actually completes the install.
install_plugin launchapp-dev/animus-provider-claude --allow-shadow-builtin

# 2. Seed the workflow overlay
echo
echo "==> Seeding workflow overlay at ${WORKFLOW_OVERLAY_DIR}"
mkdir -p "${WORKFLOW_OVERLAY_DIR}"
if [ ! -f "${WORKFLOW_OVERLAY_DIR}/triage-prospect.yaml" ]; then
  cp "${PACK_DIR}/workflows/triage-prospect.yaml" "${WORKFLOW_OVERLAY_DIR}/"
  echo "    Copied triage-prospect.yaml"
else
  echo "    triage-prospect.yaml already present — leaving it untouched"
fi

# 3. Seed the prospect inbox. Split the bundled sample-prospects.md into
#    one file per prospect so each can be dispatched independently.
#    Idempotent: skips prospects that already exist on disk.
echo
echo "==> Seeding prospect inbox at ${INBOX_DIR}"
mkdir -p "${INBOX_DIR}"

SAMPLE_BUNDLE="${PACK_DIR}/subjects/sample-prospects.md"
if [ ! -f "${SAMPLE_BUNDLE}" ]; then
  echo "WARN: ${SAMPLE_BUNDLE} missing — skipping prospect seeding" >&2
else
  # Idempotent: skip any PRS-* file that already exists on disk so a
  # rerun never clobbers a user's local edits or status updates.
  # File existence is checked via awk getline (no shell — avoids any
  # injection from project paths containing $(...) or quotes).
  awk -v outdir="${INBOX_DIR}" '
    function file_exists(path,   line, status) {
      status = (getline line < path)
      close(path)
      return status >= 0
    }
    /^## PRS-[0-9]+:/ {
      if (out) close(out)
      out = ""
      # Extract prospect id (PRS-1001) from "## PRS-1001: ..."
      match($0, /PRS-[0-9]+/)
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
      # Section separators in the bundle — do not write to per-prospect files
      next
    }
    out { print >> out }
  ' "${SAMPLE_BUNDLE}"
  echo "    Per-prospect files in ${INBOX_DIR}/ (existing files left untouched):"
  ls "${INBOX_DIR}"/PRS-*.md 2>/dev/null | sed 's/^/      /'
fi

# 4. Friendly next-steps
# Pass the same PROJECT_ROOT we just seeded into the printed commands,
# so the user's worktree/cwd doesn't accidentally resolve a different
# git common root.
cat <<EOF

==> Setup complete.

Next steps (all commands include --project-root so they target the
directory we just seeded, regardless of cwd):

  1. Make sure your LLM API key is exported in the daemon's environment:
       export ANTHROPIC_API_KEY=...   # or OPENAI_API_KEY / GEMINI_API_KEY

  2. Start the daemon (preflight will check plugin coverage):
       animus daemon start --auto-install --project-root "${PROJECT_ROOT}"

  3. Smoke-test the subject backend is wired up:
       animus subject list --kind prospect --project-root "${PROJECT_ROOT}"

  4. Run the workflow synchronously against a single prospect. The
     workflow run CLI currently requires --task-id, --requirement-id,
     or --title (no first-class --subject-id for arbitrary kinds yet),
     so we pass the title AND the prospect body via --description so
     the enrich/draft/cadence phases actually see the context:
       PROSPECT_FILE="${PROJECT_ROOT}/prospects/inbox/PRS-1001.md"
       animus workflow run animus.marketing-outreach/triage-prospect \\
         --title "\$(head -n 1 "\$PROSPECT_FILE" | sed 's/^## //')" \\
         --description "\$(tail -n +2 "\$PROSPECT_FILE")" \\
         --sync \\
         --project-root "${PROJECT_ROOT}"

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

To customize angles, cadence, models, or wire to a real CRM, see
packs/marketing-outreach/docs/customizing.md.
EOF
