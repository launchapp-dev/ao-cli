#!/usr/bin/env bash
#
# packs/organization-meetings/scripts/setup.sh
#
# Idempotent setup for the Organization Meetings reference pack.
# Installs required plugins and seeds the local meeting inbox.
#
# Usage (from the root of your project):
#   bash packs/organization-meetings/scripts/setup.sh
#
# Re-runnable: skips installs that are already present and never
# clobbers per-meeting markdown files that already exist on disk.

set -euo pipefail

PACK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_ROOT="$(pwd)"

INBOX_DIR="${PROJECT_ROOT}/meetings/inbox"
WORKFLOW_OVERLAY_DIR="${PROJECT_ROOT}/.animus/workflows"

echo "==> Organization Meetings pack setup"
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
# acknowledge the shadow-builtin override.
install_plugin launchapp-dev/animus-provider-claude --allow-shadow-builtin

# 2. Seed the workflow overlays
echo
echo "==> Seeding workflow overlay at ${WORKFLOW_OVERLAY_DIR}"
mkdir -p "${WORKFLOW_OVERLAY_DIR}"
for wf in prep-and-followup.yaml weekly-status.yaml; do
  if [ ! -f "${WORKFLOW_OVERLAY_DIR}/${wf}" ]; then
    cp "${PACK_DIR}/workflows/${wf}" "${WORKFLOW_OVERLAY_DIR}/"
    echo "    Copied ${wf}"
  else
    echo "    ${wf} already present — leaving it untouched"
  fi
done

# 3. Seed the meeting inbox. Split the bundled sample-meetings.md into
#    one file per meeting so each can be dispatched independently.
#    Idempotent: skips meetings that already exist on disk.
echo
echo "==> Seeding meeting inbox at ${INBOX_DIR}"
mkdir -p "${INBOX_DIR}"

SAMPLE_BUNDLE="${PACK_DIR}/subjects/sample-meetings.md"
if [ ! -f "${SAMPLE_BUNDLE}" ]; then
  echo "WARN: ${SAMPLE_BUNDLE} missing — skipping meeting seeding" >&2
else
  # Idempotent: skip any MTG-* file that already exists on disk so a
  # rerun never clobbers a user's local edits or status updates.
  # File existence is checked via awk getline (no shell — avoids any
  # injection from project paths containing $(...) or quotes).
  awk -v outdir="${INBOX_DIR}" '
    function file_exists(path,   line, status) {
      status = (getline line < path)
      close(path)
      return status >= 0
    }
    /^## MTG-[0-9]+:/ {
      if (out) close(out)
      out = ""
      # Extract meeting id (MTG-2001) from "## MTG-2001: ..."
      match($0, /MTG-[0-9]+/)
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
      # Section separators in the bundle — do not write to per-meeting files
      next
    }
    out { print >> out }
  ' "${SAMPLE_BUNDLE}"
  echo "    Per-meeting files in ${INBOX_DIR}/ (existing files left untouched):"
  ls "${INBOX_DIR}"/MTG-*.md 2>/dev/null | sed 's/^/      /'
fi

# 4. Friendly next-steps
cat <<EOF

==> Setup complete.

Next steps (all commands include --project-root so they target the
directory we just seeded, regardless of cwd):

  1. Make sure your LLM API key is exported in the daemon's environment:
       export ANTHROPIC_API_KEY=...   # or OPENAI_API_KEY / GEMINI_API_KEY

  2. Start the daemon (preflight will check plugin coverage):
       animus daemon start --auto-install --project-root "${PROJECT_ROOT}"

  3. Smoke-test the subject backend is wired up:
       animus subject list --kind meeting --project-root "${PROJECT_ROOT}"

  4. Run the per-meeting workflow synchronously against a single meeting.
     The workflow run CLI currently requires --task-id, --requirement-id,
     or --title (no first-class --subject-id for arbitrary kinds yet),
     so we pass the title AND the meeting body via --description so the
     prep + extract phases actually see the meeting content:
       MEETING_FILE="${PROJECT_ROOT}/meetings/inbox/MTG-2001.md"
       animus workflow run animus.organization-meetings/prep-and-followup \\
         --title "\$(head -n 1 "\$MEETING_FILE" | sed 's/^## //')" \\
         --description "\$(tail -n +2 "\$MEETING_FILE")" \\
         --sync \\
         --project-root "${PROJECT_ROOT}"

  5. End-of-week, collect this project's per-meeting weekly entries
     and run the rollup. The aggregator reads its input from
     --description because nothing persists a weekly_entry field on the
     meeting subject yet. Both Animus calls are project-scoped via
     --project-root, and we filter by completed_at >= now-7d so
     historical runs from prior weeks don't leak into this week's
     report. See packs/organization-meetings/README.md for the full
     end-to-end snippet.

  6. Inspect a run. Animus mints a fresh UUID per dispatch, so list
     recent runs and use that id:
       animus workflow list --project-root "${PROJECT_ROOT}"
       animus output phase-outputs --workflow-id <id-from-list> \\
         --project-root "${PROJECT_ROOT}"

To customize the brief template, swap the LLM, wire to a calendar
trigger, or send the followup automatically, see
packs/organization-meetings/docs/customizing.md.
EOF
