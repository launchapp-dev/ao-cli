#!/usr/bin/env bash
# bootstrap-cosign-key.sh
#
# Generate a cosign keypair for signing launchapp-dev animus plugins.
# Run ONCE per launchapp-dev install (or with --force --upload-secrets to rotate).
# Private key NEVER committed, NEVER touched by Claude.
#
# Default mode: auto-generates a 48-char alphanumeric password, saves to:
#   - ~/.animus/keys/launchapp-dev.password (0600, plaintext)
#   - macOS Keychain (service 'animus-cosign-launchapp-dev')
#
# Flags:
#   --upload-secrets    Push COSIGN_PRIVATE_KEY + COSIGN_PASSWORD to GitHub
#                       Actions org-level secrets via gh CLI. Org defaults to
#                       'launchapp-dev'; override with --gh-org.
#   --upload-only       Skip key generation; just upload existing key+password
#                       to GitHub. Useful for re-uploading after rotating
#                       GitHub-side secrets without rotating local keys.
#   --gh-org <name>     GitHub org for secret upload (default: launchapp-dev)
#   --gh-visibility <v> Secret visibility: 'all' (default) | 'private' | 'selected'
#   --interactive       Prompt for password (you type it) instead of auto-generating
#   --force             Overwrite existing key (DESTRUCTIVE — invalidates every
#                       signed artifact across all plugins)
#   --help              Show this help
#
# Rotation in one command:
#   ./scripts/bootstrap-cosign-key.sh --force --upload-secrets
#
# Initial setup in one command:
#   ./scripts/bootstrap-cosign-key.sh --upload-secrets

set -euo pipefail

# -----------------------------------------------------------------------------
# Flags
# -----------------------------------------------------------------------------
INTERACTIVE=0
FORCE=0
UPLOAD_SECRETS=0
UPLOAD_ONLY=0
GH_ORG="launchapp-dev"
GH_VISIBILITY="all"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --interactive)     INTERACTIVE=1; shift ;;
        --force)           FORCE=1; shift ;;
        --upload-secrets)  UPLOAD_SECRETS=1; shift ;;
        --upload-only)     UPLOAD_ONLY=1; UPLOAD_SECRETS=1; shift ;;
        --gh-org)          GH_ORG="$2"; shift 2 ;;
        --gh-visibility)   GH_VISIBILITY="$2"; shift 2 ;;
        --help|-h)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *)
            echo "ERROR: unknown flag: $1"
            echo "Try --help"
            exit 2
            ;;
    esac
done

# -----------------------------------------------------------------------------
# Pre-flight
# -----------------------------------------------------------------------------
if [[ $UPLOAD_ONLY -eq 0 ]]; then
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
fi

if [[ $UPLOAD_SECRETS -eq 1 ]]; then
    if ! command -v gh >/dev/null 2>&1; then
        echo "ERROR: gh CLI not installed (required for --upload-secrets)"
        echo "  Install: brew install gh"
        exit 1
    fi
    if ! gh auth status >/dev/null 2>&1; then
        echo "ERROR: gh not authenticated"
        echo "  Run: gh auth login"
        echo "  You must be an admin of the '$GH_ORG' org to upload org-level secrets."
        exit 1
    fi
fi
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
# Existing-key guard (skip in --upload-only mode)
# -----------------------------------------------------------------------------
if [[ $UPLOAD_ONLY -eq 0 ]]; then
    if [[ -f "$KEY_PATH" ]]; then
        if [[ $FORCE -ne 1 ]]; then
            echo "ERROR: key already exists at $KEY_PATH"
            echo ""
            echo "Regenerating invalidates every signed artifact across all plugin repos."
            echo "Options:"
            echo "  --force --upload-secrets    Rotate + push new key to GitHub in one go"
            echo "  --upload-only               Re-upload existing key to GitHub without regen"
            echo "  --help                      Show all flags"
            echo ""
            echo "To inspect current key:"
            echo "  cat $PUB_PATH                    # public key"
            echo "  cat $PASSWORD_PATH               # password (plaintext file)"
            echo "  security find-generic-password -s '$KEYCHAIN_SERVICE' -w  # password (keychain)"
            exit 1
        fi
        echo "WARN: --force given; backing up existing key..."
        BACKUP_TS=$(date +%Y%m%d-%H%M%S)
        mv "$KEY_PATH" "${KEY_PATH}.bak.${BACKUP_TS}"
        mv "$PUB_PATH" "${PUB_PATH}.bak.${BACKUP_TS}" 2>/dev/null || true
        mv "$PASSWORD_PATH" "${PASSWORD_PATH}.bak.${BACKUP_TS}" 2>/dev/null || true
        echo "  Backed up to ${KEY_PATH}.bak.${BACKUP_TS}"
    fi
fi

# -----------------------------------------------------------------------------
# Password: auto-generated (default) or interactive
# -----------------------------------------------------------------------------
if [[ $UPLOAD_ONLY -eq 1 ]]; then
    # Re-read existing password for upload
    if [[ ! -f "$PASSWORD_PATH" ]]; then
        # Try Keychain
        if command -v security >/dev/null 2>&1; then
            COSIGN_PASSWORD=$(security find-generic-password -s "$KEYCHAIN_SERVICE" -w 2>/dev/null || true)
        fi
        if [[ -z "${COSIGN_PASSWORD:-}" ]]; then
            echo "ERROR: --upload-only requires existing password at $PASSWORD_PATH"
            echo "  or in macOS Keychain (service '$KEYCHAIN_SERVICE')"
            exit 1
        fi
    else
        COSIGN_PASSWORD=$(cat "$PASSWORD_PATH")
    fi
    if [[ ! -f "$KEY_PATH" ]]; then
        echo "ERROR: --upload-only requires existing key at $KEY_PATH"
        exit 1
    fi
    export COSIGN_PASSWORD
    echo "Re-uploading existing key from $KEY_PATH"
elif [[ $INTERACTIVE -eq 1 ]]; then
    echo "============================================================"
    echo "  Interactive mode: cosign will prompt for password twice"
    echo "============================================================"
    echo ""
    echo "Pick a strong, memorable password. Save it to a password manager"
    echo "BEFORE proceeding. LOSE THIS PASSWORD = signing identity gone."
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
# Generate keypair (skip in --upload-only mode)
# -----------------------------------------------------------------------------
if [[ $UPLOAD_ONLY -eq 0 ]]; then
    (
        cd "$KEY_DIR"
        cosign generate-key-pair --output-key-prefix "$KEY_PREFIX"
    )
    chmod 600 "$KEY_PATH"
    chmod 644 "$PUB_PATH"
fi

# -----------------------------------------------------------------------------
# Persist password locally (auto-generated mode only, not --upload-only)
# -----------------------------------------------------------------------------
if [[ $UPLOAD_ONLY -eq 0 && $INTERACTIVE -ne 1 ]]; then
    printf '%s' "$COSIGN_PASSWORD" > "$PASSWORD_PATH"
    chmod 600 "$PASSWORD_PATH"

    if command -v security >/dev/null 2>&1; then
        security add-generic-password \
            -U \
            -s "$KEYCHAIN_SERVICE" \
            -a "$KEYCHAIN_ACCOUNT" \
            -w "$COSIGN_PASSWORD" 2>/dev/null \
            && echo "  Saved password to macOS Keychain (service: $KEYCHAIN_SERVICE)" \
            || echo "  WARN: failed to save to Keychain (file backup at $PASSWORD_PATH is still good)"
    fi
fi

# -----------------------------------------------------------------------------
# Upload to GitHub Actions org-level secrets (--upload-secrets)
# -----------------------------------------------------------------------------
UPLOAD_REPORT=""
if [[ $UPLOAD_SECRETS -eq 1 ]]; then
    echo ""
    echo "============================================================"
    echo "  Uploading secrets to GitHub org '$GH_ORG' (visibility: $GH_VISIBILITY)"
    echo "============================================================"

    # Get the password we need to upload
    if [[ -z "${COSIGN_PASSWORD:-}" ]]; then
        if [[ -f "$PASSWORD_PATH" ]]; then
            COSIGN_PASSWORD=$(cat "$PASSWORD_PATH")
        elif command -v security >/dev/null 2>&1; then
            COSIGN_PASSWORD=$(security find-generic-password -s "$KEYCHAIN_SERVICE" -w 2>/dev/null || true)
        fi
        if [[ -z "${COSIGN_PASSWORD:-}" ]]; then
            echo "ERROR: cannot determine password to upload (file + Keychain both empty)"
            exit 1
        fi
    fi

    # Upload COSIGN_PRIVATE_KEY (the encrypted key file contents)
    if gh secret set COSIGN_PRIVATE_KEY \
            --org "$GH_ORG" \
            --visibility "$GH_VISIBILITY" \
            --body "$(cat "$KEY_PATH")"; then
        UPLOAD_REPORT="${UPLOAD_REPORT}  ✓ COSIGN_PRIVATE_KEY uploaded (visibility: $GH_VISIBILITY)\n"
    else
        UPLOAD_REPORT="${UPLOAD_REPORT}  ✗ COSIGN_PRIVATE_KEY upload FAILED\n"
    fi

    # Upload COSIGN_PASSWORD
    if gh secret set COSIGN_PASSWORD \
            --org "$GH_ORG" \
            --visibility "$GH_VISIBILITY" \
            --body "$COSIGN_PASSWORD"; then
        UPLOAD_REPORT="${UPLOAD_REPORT}  ✓ COSIGN_PASSWORD uploaded (visibility: $GH_VISIBILITY)\n"
    else
        UPLOAD_REPORT="${UPLOAD_REPORT}  ✗ COSIGN_PASSWORD upload FAILED\n"
    fi

    # Verify the secrets are visible
    echo ""
    echo "Verifying via 'gh secret list --org $GH_ORG':"
    gh secret list --org "$GH_ORG" 2>/dev/null | grep -E '^COSIGN_(PRIVATE_KEY|PASSWORD)' || echo "  (none found — check 'gh auth status' + org admin permissions)"
fi

# -----------------------------------------------------------------------------
# Output
# -----------------------------------------------------------------------------
echo ""
echo "============================================================"
echo "  Done."
echo "============================================================"
echo "  Private key (KEEP SECRET): $KEY_PATH"
echo "  Public key  (safe to share): $PUB_PATH"
if [[ $INTERACTIVE -ne 1 && $UPLOAD_ONLY -eq 0 ]]; then
    echo "  Password    (KEEP SECRET): $PASSWORD_PATH"
fi
if [[ -n "$UPLOAD_REPORT" ]]; then
    echo ""
    echo "GitHub org secrets ('$GH_ORG'):"
    echo -e "$UPLOAD_REPORT"
fi
echo ""

echo "============================================================"
echo "  Public key contents (bake into source):"
echo "============================================================"
cat "$PUB_PATH"
echo ""

if [[ $UPLOAD_SECRETS -eq 0 ]]; then
    echo "============================================================"
    echo "  Next: upload secrets to GitHub (manual OR re-run with --upload-secrets)"
    echo "============================================================"
    echo ""
    echo "  Manual upload:"
    echo "    1. Go to https://github.com/organizations/$GH_ORG/settings/secrets/actions"
    echo "    2. Add two org-level secrets (visibility: all repositories):"
    echo "         COSIGN_PRIVATE_KEY = (paste contents of $KEY_PATH)"
    echo "         COSIGN_PASSWORD    = (see $PASSWORD_PATH)"
    echo ""
    echo "  Automated upload (recommended for rotation):"
    echo "    ./scripts/bootstrap-cosign-key.sh --upload-only"
    echo ""
fi

echo "============================================================"
echo "  Security reminders:"
echo "============================================================"
echo "  - The private key is encrypted with the password (cosign default)"
echo "  - Filesystem perms set to 0700 on dir, 0600 on key + password files"
echo "  - Claude never reads the .key or .password files"
echo "  - Signing happens in CI via GitHub Actions secrets — not on this machine"
echo "  - Rotation: ./scripts/bootstrap-cosign-key.sh --force --upload-secrets"
if [[ $INTERACTIVE -ne 1 && $UPLOAD_ONLY -eq 0 ]]; then
    echo ""
    echo "  ⚠ Password is stored PLAINTEXT in ${PASSWORD_PATH}"
    echo "    (0600 perms — readable only by your user)"
    echo "    For higher security, copy it to a password manager + delete the file."
fi
echo ""

# Clean up env var so it doesn't leak into the parent shell
unset COSIGN_PASSWORD || true
