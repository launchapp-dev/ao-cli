#!/usr/bin/env bash
# Dev install: copy the launcher + bundled JS into ~/.animus/plugins/.
#
# Animus's `plugin install` flow expects a signed binary downloaded from a
# GitHub release. For local TypeScript-plugin iteration that's overkill —
# this script does the manual equivalent so you can edit, rebuild, and
# re-test without round-tripping a release.
#
# Cosign signing for non-Rust plugins is an open question (to be answered
# in a future SDK wave); for now the dev path is "drop in the install
# dir, restart the daemon."

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
launcher="${here}/scripts/animus-plugin-hello-ts"
bundle="${here}/dist/index.js"
install_root="${ANIMUS_PLUGIN_DIR:-${HOME}/.animus/plugins}"
target_launcher="${install_root}/animus-plugin-hello-ts"
target_bundle_dir="${install_root}/animus-plugin-hello-ts.d"

if [[ ! -f "${bundle}" ]]; then
  echo "error: bundled JS not found at ${bundle}" >&2
  echo "Run: pnpm install && pnpm run build" >&2
  exit 1
fi

mkdir -p "${install_root}" "${target_bundle_dir}"
cp -f "${launcher}" "${target_launcher}"
cp -f "${bundle}"   "${target_bundle_dir}/index.js"
chmod +x "${target_launcher}"

echo "Installed:"
echo "  ${target_launcher}"
echo "  ${target_bundle_dir}/index.js"
echo
echo "Discovery note:"
echo "  Animus only auto-scans ${install_root} when ANIMUS_PLUGIN_DIR is set."
echo "  Export it in your shell (and in the daemon's env) before restarting:"
echo "    export ANIMUS_PLUGIN_DIR='${install_root}'"
echo
echo "Next:"
echo "  animus daemon stop && animus daemon start    # daemon picks up new plugins on (re)start"
echo "  animus plugin list                           # confirm 'animus-plugin-hello-ts' is shown"
echo "  animus subject list --kind hello_world_demo"
