#!/usr/bin/env bash
# bootstrap-cosign-key.sh
#
# Generate a cosign keypair for signing launchapp-dev animus plugins.
# Run ONCE per launchapp-dev install. Private key NEVER committed, NEVER touched by Claude.
#
# What this script does:
#   1. Verify cosign is installed
#   2. Generate password-protected keypair into ~/.animus/keys/launchapp-dev.{key,pub}
#   3. Print the public key for embedding into animus source
#   4. Print next-step instructions for GitHub Actions secret upload
#
# What this script does NOT do:
#   - Touch your password (you type it into cosign prompt)
#   - Upload anything to GitHub
#   - Commit anything to git
#   - Make the private key readable by anything but your user

set -euo pipefail

if ! command -v cosign >/dev/null 2>&1; then
    echo "ERROR: cosign not installed."
    echo "Install with: brew install cosign  (macOS)"
    echo "         or: https://docs.sigstore.dev/cosign/installation/"
    exit 1
fi

COSIGN_VERSION=$(cosign version 2>&1 | grep -i 'gitversion' | head -1 | awk '{print $2}' || cosign version 2>&1 | head -1)
echo "Using cosign: $COSIGN_VERSION"
echo ""

KEY_DIR="${HOME}/.animus/keys"
KEY_PREFIX="launchapp-dev"
KEY_PATH="${KEY_DIR}/${KEY_PREFIX}.key"
PUB_PATH="${KEY_DIR}/${KEY_PREFIX}.pub"

mkdir -p "$KEY_DIR"
chmod 700 "$KEY_DIR"

if [[ -f "$KEY_PATH" ]]; then
    echo "ERROR: key already exists at $KEY_PATH"
    echo ""
    echo "Regenerating invalidates every signed artifact across all 18 plugin repos."
    echo "If you genuinely want to rotate keys:"
    echo "  1. Manually delete $KEY_PATH (and remove from GitHub Actions secrets)"
    echo "  2. Re-run this script"
    echo "  3. Re-sign + re-release every plugin"
    echo "  4. Bump animus-cli + bake the new public key"
    echo ""
    echo "If you just want to view the public key, run:"
    echo "  cat $PUB_PATH"
    exit 1
fi

echo "============================================================"
echo "  Generating cosign keypair for launchapp-dev"
echo "============================================================"
echo ""
echo "cosign will prompt for a password to encrypt the private key."
echo ""
echo "Pick a strong, memorable password. You'll need it to:"
echo "  - Sign plugin releases in CI (uploaded as GitHub Actions secret)"
echo "  - Sign anything locally for testing"
echo ""
echo "LOSE THIS PASSWORD = your signing identity is gone. Save it to a"
echo "password manager (1Password / Bitwarden / etc.) BEFORE proceeding."
echo ""
read -p "Ready to generate? Press Enter to continue, Ctrl-C to abort."

cd "$KEY_DIR"
cosign generate-key-pair --output-key-prefix "$KEY_PREFIX"

# Tighten perms on the private key
chmod 600 "$KEY_PATH"
chmod 644 "$PUB_PATH"

echo ""
echo "============================================================"
echo "  Done. Files created:"
echo "============================================================"
echo "  Private key (KEEP SECRET): $KEY_PATH"
echo "  Public key  (safe to share): $PUB_PATH"
echo ""

echo "============================================================"
echo "  Public key contents (bake into source):"
echo "============================================================"
cat "$PUB_PATH"
echo ""

echo "============================================================"
echo "  Next steps for YOU (manual, one-time):"
echo "============================================================"
echo ""
echo "  1. Back up the password and private key to your password manager:"
echo "     - Save '${KEY_PATH}' file contents to a secure note"
echo "     - Save the password you just chose to the same secure note"
echo ""
echo "  2. Upload to GitHub Actions secrets in the launchapp-dev org:"
echo "     https://github.com/organizations/launchapp-dev/settings/secrets/actions"
echo ""
echo "     Add two organization secrets (visible to all repos):"
echo "       COSIGN_PRIVATE_KEY = (paste contents of ${KEY_PATH})"
echo "       COSIGN_PASSWORD    = (paste the password you just chose)"
echo ""
echo "  3. Tell Claude: 'public key is at ${PUB_PATH}, bake it into source'"
echo "     (Claude will read the .pub file — never the .key file or password)"
echo ""
echo "  4. After Claude bakes the public key + ships the CI signing"
echo "     workflows, re-release every plugin so artifacts get signed."
echo ""

echo "============================================================"
echo "  Security reminders:"
echo "============================================================"
echo "  - The private key is encrypted with your password (cosign default)"
echo "  - Filesystem perms set to 0700 on dir, 0600 on key"
echo "  - Claude will never request the private key file or password"
echo "  - Signing happens in CI via GitHub Actions secrets, not on this machine"
echo "  - If your machine is compromised, you must rotate the key (see top of script)"
echo ""
