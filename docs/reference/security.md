# Security Reference

This page documents the security controls Animus applies at the install
boundary and the trust model behind plugin signature verification. For the
background design and threat model, see
[`docs/architecture/plugin-signing.md`](../architecture/plugin-signing.md).

## Plugin signature policy

`animus plugin install` verifies that every downloaded plugin binary was
signed by a trusted publisher before it is copied into
`~/.animus/plugins/`. Verification happens against a cosign signature
bundle (`<asset>.bundle`) published next to the asset on the GitHub
Release.

### Policy modes

`animus plugin install` exposes three enforcement modes via the
`--signature-policy <MODE>` flag. The default is `strict`.

| Mode       | Behavior                                                                                          | When to use                                                                |
| ---------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `strict`   | Refuse install if signature is missing, invalid, or signed by an untrusted key. **DEFAULT.**      | All production environments.                                               |
| `warn`     | Verify when possible; log a warning to stderr and proceed on any failure.                         | Temporary: integrating a publisher that hasn't enabled cosign signing yet. |
| `disabled` | Skip verification entirely.                                                                       | Air-gapped or local-build flows where signing is not feasible.             |

The legacy `--require-signature` and `--skip-signature` flags are
retained for backward compatibility. They map to `--signature-policy
strict` and `--signature-policy disabled` respectively.

`--allow-unsigned` is a convenience alias for `--signature-policy warn`.

#### Strict mode (default)

```bash
animus plugin install launchapp-dev/animus-provider-claude
# equivalent to:
animus plugin install --signature-policy strict launchapp-dev/animus-provider-claude
```

When strict mode rejects an install you'll see one of:

- `signature policy is strict but no cosign signature could be verified: <reason>` — no `.bundle` was published, or cosign isn't on `$PATH`.
- `cosign signature verification FAILED; refusing install: <message>` — the bundle exists but cosign rejected it.
- `signature is valid but the signer is not in trusted-signers.yaml` — the cert identity isn't on your allowlist.

Each error suggests the appropriate override flag (`--allow-unsigned`,
`--signature-policy disabled`, or adding the signer to
`~/.animus/trusted-signers.yaml`).

#### Warn mode

```bash
animus plugin install --signature-policy warn launchapp-dev/animus-subject-experimental
# or equivalently
animus plugin install --allow-unsigned launchapp-dev/animus-subject-experimental
```

Verification still runs and the result is recorded in
`~/.animus/plugins.yaml` under `signature_status`, but the install
proceeds even on failure. Use only when migrating publishers onto
cosign signing.

#### Disabled mode

```bash
animus plugin install --signature-policy disabled ./my-local-build.bin --path ./my-local-build.bin
```

Skips verification entirely. Required for `--path` and `--url` installs
of locally-built binaries that have no upstream signature bundle.

### Trusted keys directory

PEM-encoded cosign public keys live under `~/.animus/trusted-keys/`,
one file per identity:

```
~/.animus/trusted-keys/
├── launchapp-dev.pem
├── my-org.pem
└── alice-myplugin.pem        # scoped to one specific repo
```

Lookup order for a given `<owner>/<repo>` install:

1. `--trust-key <PATH>` if passed on the command line.
2. `~/.animus/trusted-keys/<owner>.pem` — covers every repo from that owner.
3. `~/.animus/trusted-keys/<owner>-<repo>.pem` — repo-scoped key.

The first match wins.

### Adding a trusted key

For a new publisher:

```bash
mkdir -p ~/.animus/trusted-keys
cp /path/to/publisher.pem ~/.animus/trusted-keys/<owner>.pem
chmod 644 ~/.animus/trusted-keys/<owner>.pem
```

For a one-off install with an ad-hoc key (does not persist):

```bash
animus plugin install --trust-key /tmp/publisher.pem <owner>/<repo>
```

### Built-in launchapp-dev key

The Animus binary ships with the canonical `launchapp-dev` cosign public
key embedded as a constant. The first time you run `animus plugin
install` under the default `strict` policy, the binary writes that key
to `~/.animus/trusted-keys/launchapp-dev.pem` if no file is already
there. This makes the out-of-the-box install of `launchapp-dev/animus-*`
plugins work without any manual key setup.

The seed step is a strict no-op if you have already created
`launchapp-dev.pem` yourself — your pinned key is never stomped.

To rotate the embedded key, see
`crates/orchestrator-plugin-host/src/signature_verifier.rs` —
`LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM`.

### Cosign binary dependency

Verification currently shells out to the `cosign` binary. If `cosign` is
not on `$PATH`:

- Under `strict`: the install fails with an actionable
  `install cosign from https://github.com/sigstore/cosign` error.
- Under `warn`: the install proceeds and `signature_status` is recorded
  as `unsigned` with the same reason.
- Under `disabled`: cosign is never consulted.

A migration to the in-process `sigstore` Rust crate is planned for
v0.5+. The CLI flag surface will not change.

### Signature status field

Every successful install records one of these stable strings under
`signature_status` in `~/.animus/plugins.yaml`:

| Value              | Meaning                                                                       |
| ------------------ | ----------------------------------------------------------------------------- |
| `verified`         | cosign accepted the bundle against a trusted key.                             |
| `unsigned`         | No bundle was published, no trusted key matched, or cosign wasn't installed.  |
| `invalid`          | A bundle was published but cosign rejected it.                                |
| `untrusted_signer` | A bundle verified, but the cert identity isn't on the trusted-signers list.   |
| `skipped`          | Verification was bypassed (`--signature-policy disabled` / `--skip-signature`). |

`animus plugin list` surfaces this in the `SIG` column.

## Trusted orgs (TOFU)

Independent of cosign verification, every `animus plugin install` from a
release source also consults a separate allowlist at
`~/.animus/trusted-orgs.yaml`. Installing from an org not in this list
prompts the operator at the TTY (non-suppressible) or fails non-interactively.

Built-in trusted orgs: `launchapp-dev`.

See [`docs/architecture/plugin-signing.md`](../architecture/plugin-signing.md#trusted-orgs-tofu)
for full details.

## Manifest name vs repo basename

Install refuses any plugin whose `manifest.name` does not equal the repo
basename. This blocks typosquat attacks where an attacker publishes a
near-name release that shadows a legitimate plugin. `--force` overrides.

## Reserved provider tools

Provider plugins that claim one of the in-tree provider tools
(`claude`, `codex`, `gemini`, `opencode`, `oai-runner`) are refused by
default. `--allow-shadow-builtin` is required to deliberately shadow an
in-tree backend.

## Configuration files

| File                                  | Purpose                                                                          |
| ------------------------------------- | -------------------------------------------------------------------------------- |
| `~/.animus/trusted-keys/`             | Per-publisher cosign public keys (PEM).                                          |
| `~/.animus/trusted-signers.yaml`      | Optional glob allowlist for cosign cert identities (keyless-signing trust list). |
| `~/.animus/trusted-orgs.yaml`         | TOFU allowlist of GitHub orgs the operator has accepted.                         |
| `~/.animus/plugins.yaml`              | Installed-plugin registry. Records `signature_status` per entry.                 |

## Recommended posture for production

1. Leave the default `--signature-policy strict` in place.
2. Pre-populate `~/.animus/trusted-keys/` with the cosign public keys of
   every publisher you plan to install from.
3. Pre-populate `~/.animus/trusted-orgs.yaml` so non-interactive installs
   never block on a TOFU prompt.
4. Install `cosign` on every machine that runs `animus plugin install`.
   Strict mode fails closed without it.
5. Audit `signature_status` in `~/.animus/plugins.yaml` periodically.
   Anything other than `verified` or `skipped` for a `--path`/`--url`
   install is a policy violation worth investigating.

## Kill switches

Two environment variables disable plugin-system features at daemon start
for emergency rollback:

- `ANIMUS_DAEMON_DISABLE_TRIGGERS=1` — skip the trigger plugin supervisor.
- `ANIMUS_PROVIDER_DISABLE_PLUGIN=1` — force `SessionBackendResolver` to
  bypass installed provider plugins and use in-tree backends only.

Both require a daemon restart to take effect and to re-enable. See
[`docs/reference/configuration.md`](configuration.md#plugin-kill-switches).
