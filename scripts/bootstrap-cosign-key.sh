#!/usr/bin/env bash
# bootstrap-cosign-key.sh
#
# Generate a cosign keypair for signing launchapp-dev animus plugins.
# Run ONCE per launchapp-dev install. Private key NEVER committed, NEVER touched by Claude.
#
# Default mode: auto-generates a 48-char alphanumeric password, saves it to:
#   - ~/.animus/keys/launchapp-dev.password (0600, plaintext)
#   - macOS Keychain (service 'animus-cosign-launchapp-dev', account 'launchapp-dev')
#
# Flags:
#   --interactive   Prompt for password (you type it) instead of auto-generating
#   --force         Overwrite existing key (DESTRUCTIVE — invalidates every signed artifact)
#   --help          Show this help

set -euo pipefail

# -----------------------------------------------------------------------------
# Flags
# -----------------------------------------------------------------------------
INTERACTIVE=0
FORCE=0
for arg in "$@"; do
    case "$arg" in
        --interactive) INTERACTIVE=1 ;;
        --force)       FORCE=1 ;;
        --help|-h)
            sed -n '2,18p' "$0"
            exit 0
            ;;
        *)
            echo "ERROR: unknown flag: $arg"
            echo "Try --help"
            exit 2
            ;;
    esac
done

# -----------------------------------------------------------------------------
# Pre-flight
# -----------------------------------------------------------------------------
if ! command -v cosign >/dev/null 2>&1; then
    echo "cosign not installed; attempting brew install..."
    if command -v brew >/dev/null 2>&1; then
        brew install cosign
    else
        echo "ERROR: neither cosign nor brew found. Install cosign:"
        echo "  https://docs.sigstore.dev/cosign/installation/"
        exit 1
    fi
fi

COSIGN_VERSION=$(cosign version 2>&1 | grep -i 'gitversion' | head -1 | awk '{print $2}' || cosign version 2>&1 | head -1)
echo "Using cosign: $COSIGN_VERSION"
echo ""

KEY_DIR="${HOME}/.animus/keys"
KEY_PREFIX="launchapp-dev"
KEY_PATH="${KEY_DIR}/${KEY_PREFIX}.key"
PUB_PATH="${KEY_DIR}/${KEY_PREFIX}.pub"
PASSWORD_PATH="${KEY_DIR}/${KEY_PREFIX}.password"
KEYCHAIN_SERVICE="animus-cosign-${KEY_PREFIX}"
KEYCHAIN_ACCOUNT="${KEY_PREFIX}"

mkdir -p "$KEY_DIR"
chmod 700 "$KEY_DIR"

# -----------------------------------------------------------------------------
# Existing-key guard
# -----------------------------------------------------------------------------
if [[ -f "$KEY_PATH" ]]; then
    if [[ $FORCE -ne 1 ]]; then
        echo "ERROR: key already exists at $KEY_PATH"
        echo ""
        echo "Regenerating invalidates every signed artifact across all 18 plugin repos."
        echo "If you genuinely want to rotate keys:"
        echo "  1. Re-run with --force to overwrite"
        echo "  2. Re-sign + re-release every plugin"
        echo "  3. Bump animus-cli + bake the new public key"
        echo ""
        echo "If you just want to view the public key, run:"
        echo "  cat $PUB_PATH"
        echo ""
        echo "If you just want to view the password, run:"
        echo "  cat $PASSWORD_PATH"
        echo "  # or"
        echo "  security find-generic-password -s '$KEYCHAIN_SERVICE' -w"
        exit 1
    fi
    echo "WARN: --force given; backing up existing key..."
    BACKUP_TS=$(date +%Y%m%d-%H%M%S)
    mv "$KEY_PATH" "${KEY_PATH}.bak.${BACKUP_TS}"
    mv "$PUB_PATH" "${PUB_PATH}.bak.${BACKUP_TS}" 2>/dev/null || true
    mv "$PASSWORD_PATH" "${PASSWORD_PATH}.bak.${BACKUP_TS}" 2>/dev/null || true
    echo "  Backed up to ${KEY_PATH}.bak.${BACKUP_TS}"
fi

# -----------------------------------------------------------------------------
# Password: auto-generated (default) or interactive
# -----------------------------------------------------------------------------
if [[ $INTERACTIVE -eq 1 ]]; then
    echo "============================================================"
    echo "  Interactive mode: cosign will prompt for password twice"
    echo "============================================================"
    echo ""
    echo "Pick a strong, memorable password. You'll need it to:"
    echo "  - Sign plugin releases in CI (uploaded as GitHub Actions secret)"
    echo "  - Sign anything locally for testing"
    echo ""
    echo "LOSE THIS PASSWORD = your signing identity is gone. Save it to a"
    echo "password manager (1Password / Bitwarden / etc.) BEFORE proceeding."
    echo ""
    read -p "Ready to generate? Press Enter to continue, Ctrl-C to abort."
    unset COSIGN_PASSWORD  # let cosign prompt
else
    echo "============================================================"
    echo "  Auto-generating 48-char alphanumeric password"
    echo "============================================================"
    export COSIGN_PASSWORD="$(openssl rand -base64 64 | tr -dc 'A-Za-z0-9' | head -c 48)"
    if [[ ${#COSIGN_PASSWORD} -ne 48 ]]; then
        echo "ERROR: password generation produced unexpected length ${#COSIGN_PASSWORD}"
        exit 1
    fi
fi

# -----------------------------------------------------------------------------
# Generate keypair
# -----------------------------------------------------------------------------
(
    cd "$KEY_DIR"
    cosign generate-key-pair --output-key-prefix "$KEY_PREFIX"
)

chmod 600 "$KEY_PATH"
chmod 644 "$PUB_PATH"

# -----------------------------------------------------------------------------
# Persist password (auto-generated mode only)
# -----------------------------------------------------------------------------
if [[ $INTERACTIVE -ne 1 ]]; then
    # Plaintext file with 0600 perms
    printf '%s' "$COSIGN_PASSWORD" > "$PASSWORD_PATH"
    chmod 600 "$PASSWORD_PATH"

    # macOS Keychain (only if `security` is available)
    if command -v security >/dev/null 2>&1; then
        security add-generic-password \
            -U \
            -s "$KEYCHAIN_SERVICE" \
            -a "$KEYCHAIN_ACCOUNT" \
            -w "$COSIGN_PASSWORD" 2>/dev/null \
            && echo "  Saved password to macOS Keychain (service: $KEYCHAIN_SERVICE)" \
            || echo "  WARN: failed to save to Keychain (file backup at $PASSWORD_PATH is still good)"
    else
        echo "  (macOS Keychain not available — password saved to file only)"
    fi
fi

# -----------------------------------------------------------------------------
# Output
# -----------------------------------------------------------------------------
echo ""
echo "============================================================"
echo "  Done. Files created:"
echo "============================================================"
echo "  Private key (KEEP SECRET): $KEY_PATH"
echo "  Public key  (safe to share): $PUB_PATH"
if [[ $INTERACTIVE -ne 1 ]]; then
    echo "  Password    (KEEP SECRET): $PASSWORD_PATH"
fi
echo ""

echo "============================================================"
echo "  Public key contents (bake into source):"
echo "============================================================"
cat "$PUB_PATH"
echo ""

if [[ $INTERACTIVE -ne 1 ]]; then
    echo "============================================================"
    echo "  Generated password (copy for GitHub Actions secret upload):"
    echo "============================================================"
    echo "$COSIGN_PASSWORD"
    echo ""
fi

echo "============================================================"
echo "  Next steps for YOU (manual, one-time):"
echo "============================================================"
echo ""
echo "  1. Optionally back up to a password manager (1Password / Bitwarden):"
echo "     - The private key (${KEY_PATH})"
if [[ $INTERACTIVE -ne 1 ]]; then
    echo "     - The password (also in ${PASSWORD_PATH} + macOS Keychain)"
else
    echo "     - The password you just chose"
fi
echo ""
echo "  2. Upload to GitHub Actions secrets in the launchapp-dev org:"
echo "     https://github.com/organizations/launchapp-dev/settings/secrets/actions"
echo ""
echo "     Add two organization secrets (visible to all repos):"
echo "       COSIGN_PRIVATE_KEY = (paste contents of ${KEY_PATH})"
if [[ $INTERACTIVE -ne 1 ]]; then
    echo "       COSIGN_PASSWORD    = (see ${PASSWORD_PATH} or run:"
    echo "                              security find-generic-password -s '$KEYCHAIN_SERVICE' -w)"
else
    echo "       COSIGN_PASSWORD    = (the password you just chose)"
fi
echo ""
echo "  3. Tell Claude: 'public key is at ${PUB_PATH}, bake it into source'"
echo "     (Claude reads only the .pub file — never the .key file or password)"
echo ""
echo "  4. After Claude bakes the public key + ships the CI signing"
echo "     workflows, re-release every plugin so artifacts get signed."
echo ""

echo "============================================================"
echo "  Security reminders:"
echo "============================================================"
echo "  - The private key is encrypted with the password (cosign default)"
echo "  - Filesystem perms set to 0700 on dir, 0600 on key + password files"
echo "  - Claude will never request the private key file or password"
echo "  - Signing happens in CI via GitHub Actions secrets, not on this machine"
echo "  - If your machine is compromised, you must rotate the key (re-run with --force)"
if [[ $INTERACTIVE -ne 1 ]]; then
    echo ""
    echo "  ⚠ Password is stored PLAINTEXT in ${PASSWORD_PATH}"
    echo "    (0600 perms — readable only by your user)"
    echo "    For higher security, copy it to a password manager + delete the file."
fi
echo ""

# Clean up env var so it doesn't leak into the parent shell
unset COSIGN_PASSWORD || true
