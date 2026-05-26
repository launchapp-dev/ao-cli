//! Cosign signature verification policy for plugin installs.
//!
//! This module owns the *policy* surface (`SignaturePolicy`, `PolicyMode`,
//! `VerificationResult`) so it can be reused by the CLI install pipeline
//! and any future install entry point (MCP wire, daemon-side installs).
//!
//! Verification itself currently shells out to the `cosign` binary. The
//! intent (see `docs/architecture/plugin-signing.md`) is to migrate to
//! the `sigstore` Rust crate in v0.5+ without changing this module's
//! public surface.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default OIDC issuer for GitHub Actions keyless signatures.
pub const GITHUB_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

/// Built-in cosign public key for the `launchapp-dev` org.
///
/// Used to seed `~/.animus/trusted-keys/launchapp-dev.pem` on the first
/// install attempt against `launchapp-dev/*`. This is *not* the trust anchor
/// for cosign keyless verification (which uses Sigstore's Fulcio CA); it is
/// the legacy public-key path for repos that publish a `.sig` + `.pub` pair
/// instead of a keyless `.bundle`.
///
/// Loaded at compile time from `trusted-keys/launchapp-dev.pub`. Rotation:
/// update the .pub file (e.g. via `scripts/bootstrap-cosign-key.sh --force`),
/// rebuild the binary, ship a new release. Procedure documented in
/// `docs/reference/security.md`.
pub const LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM: &str = include_str!("../trusted-keys/launchapp-dev.pub");

/// Filename written under `~/.animus/trusted-keys/` when seeding the
/// built-in launchapp-dev key.
pub const LAUNCHAPP_DEV_TRUSTED_KEY_FILENAME: &str = "launchapp-dev.pem";

/// Signature enforcement mode selected by the operator.
///
/// The default for `animus plugin install` is `Warn` in v0.4.12 because
/// `LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM` is still the placeholder; a
/// `Strict` default would reject every real release signature. v0.4.13
/// flips this back to `Strict` once release-eng bakes in the real key.
/// Operators who manage their own trusted keys can opt back in today via
/// `--signature-policy strict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// Fail-closed. Refuse install if signature is missing, invalid, or
    /// from an untrusted signer.
    Strict,
    /// Verify when possible; log a warning and proceed on any failure.
    Warn,
    /// Skip verification entirely. Intended for air-gapped or local-build
    /// flows; never the default.
    Disabled,
}

impl PolicyMode {
    /// Default policy for `animus plugin install`.
    ///
    /// Temporarily `Warn` for v0.4.12 because the built-in launchapp-dev
    /// cosign key is a placeholder; flipping back to `Strict` is tracked
    /// for v0.4.13 once release-eng ships the real key. See
    /// `docs/reference/security.md` for the rationale and opt-in path.
    #[must_use]
    pub fn default_for_install() -> Self {
        PolicyMode::Warn
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PolicyMode::Strict => "strict",
            PolicyMode::Warn => "warn",
            PolicyMode::Disabled => "disabled",
        }
    }
}

impl fmt::Display for PolicyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for PolicyMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Ok(PolicyMode::Strict),
            "warn" | "warning" => Ok(PolicyMode::Warn),
            "disabled" | "off" | "none" => Ok(PolicyMode::Disabled),
            other => Err(format!("unknown signature policy '{other}' (expected: strict|warn|disabled)")),
        }
    }
}

/// Full signature policy: enforcement mode plus the trust material the
/// install pipeline consults when picking a key for a given repo.
#[derive(Debug, Clone)]
pub struct SignaturePolicy {
    pub mode: PolicyMode,
    /// Directory of PEM-encoded public keys, one file per identity. The
    /// filename (without `.pem`) is the key identifier.
    pub trusted_keys_dir: PathBuf,
    /// Optional one-off trust-key path; overrides `trusted_keys_dir` lookup
    /// when set. Wired from `animus plugin install --trust-key <PATH>`.
    pub trust_key_override: Option<PathBuf>,
    /// Repos that are exempt from signature checks even under `Strict`.
    /// MUST be empty by default — populating this list is a security
    /// regression and should only happen when signing is genuinely
    /// infeasible for that repo.
    pub allow_unsigned_for: Vec<String>,
}

impl SignaturePolicy {
    /// Build a default install-time policy: `Strict`, trusted-keys at
    /// `~/.animus/trusted-keys/`, no overrides, no exemptions.
    #[must_use]
    pub fn default_install() -> Self {
        Self {
            mode: PolicyMode::default_for_install(),
            trusted_keys_dir: default_trusted_keys_dir(),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        }
    }

    /// Returns `true` when the policy allows the install to proceed without
    /// signature verification for the supplied `owner/repo`.
    #[must_use]
    pub fn allows_unsigned_repo(&self, owner_repo: &str) -> bool {
        self.allow_unsigned_for.iter().any(|r| r == owner_repo)
    }

    /// Resolve the public-key file path for a given `<owner>/<repo>`. Returns
    /// `Some(path)` when either an override is set or a matching file exists
    /// in `trusted_keys_dir`. Lookup order:
    ///
    /// 1. `trust_key_override` (CLI `--trust-key`).
    /// 2. `<trusted_keys_dir>/<owner>.pem`.
    /// 3. `<trusted_keys_dir>/<owner>-<repo>.pem`.
    #[must_use]
    pub fn resolve_trusted_key_for(&self, owner: &str, repo: &str) -> Option<PathBuf> {
        if let Some(p) = &self.trust_key_override {
            return Some(p.clone());
        }
        let by_owner = self.trusted_keys_dir.join(format!("{owner}.pem"));
        if by_owner.exists() {
            return Some(by_owner);
        }
        let by_owner_repo = self.trusted_keys_dir.join(format!("{owner}-{repo}.pem"));
        if by_owner_repo.exists() {
            return Some(by_owner_repo);
        }
        None
    }
}

/// Default trusted-keys directory: `~/.animus/trusted-keys/`.
#[must_use]
pub fn default_trusted_keys_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".animus").join("trusted-keys")
}

/// Seed the built-in launchapp-dev public key into the trusted-keys dir,
/// if one isn't already present. Returns the path of the written file
/// (when a write happened) so callers can surface a "wrote trust anchor
/// for launchapp-dev" hint.
///
/// This is the trust-on-first-use seed for users who never configured
/// their own `~/.animus/trusted-keys/`. It is a no-op when the file
/// already exists, so users who pinned their own key are not stomped.
pub fn seed_launchapp_dev_trusted_key(trusted_keys_dir: &Path) -> Result<Option<PathBuf>> {
    let target = trusted_keys_dir.join(LAUNCHAPP_DEV_TRUSTED_KEY_FILENAME);
    if target.exists() {
        return Ok(None);
    }
    if !trusted_keys_dir.exists() {
        std::fs::create_dir_all(trusted_keys_dir)
            .with_context(|| format!("failed to create {}", trusted_keys_dir.display()))?;
    }
    std::fs::write(&target, LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM)
        .with_context(|| format!("failed to write {}", target.display()))?;
    Ok(Some(target))
}

/// Outcome of a signature verification attempt for a single asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VerificationResult {
    /// Cosign verified the binary against the trusted key/identity.
    Verified { identity: String, bundle_path: String },
    /// No signature bundle was published or available for this install.
    Unsigned { reason: String },
    /// A signature bundle exists but cosign rejected it.
    Invalid { identity_pattern: String, message: String },
    /// Cosign accepted the bundle but the signer identity is not on the
    /// trusted list.
    UntrustedSigner { identity_pattern: String },
    /// Verification was skipped (policy = `Disabled` or `--skip-signature`).
    Skipped,
}

impl VerificationResult {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Verified { .. } => "verified",
            Self::Unsigned { .. } => "unsigned",
            Self::Invalid { .. } => "invalid",
            Self::UntrustedSigner { .. } => "untrusted_signer",
            Self::Skipped => "skipped",
        }
    }

    /// Returns `true` when the result represents a hard failure under
    /// `Strict` mode. `Unsigned` is included because Strict requires a
    /// valid signature; under `Warn` callers should treat this as
    /// best-effort instead.
    #[must_use]
    pub fn is_strict_failure(&self) -> bool {
        matches!(self, Self::Unsigned { .. } | Self::Invalid { .. } | Self::UntrustedSigner { .. })
    }
}

/// Returns `true` when the `cosign` binary is available on `$PATH`.
#[must_use]
pub fn cosign_available() -> bool {
    Command::new("cosign")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Verify a single binary against its cosign bundle using a specific
/// public key.
///
/// Shells out to `cosign verify-blob --key <pem> --bundle <bundle> <binary>`.
/// Returns `Verified` on success or `Invalid` with cosign's stderr on
/// failure. Returns `Err` when cosign itself could not be spawned.
pub fn verify_plugin_binary(
    binary_path: &Path,
    signature_path: &Path,
    public_key_path: &Path,
) -> Result<VerificationResult> {
    let mut cmd = Command::new("cosign");
    cmd.arg("verify-blob");
    cmd.arg("--key").arg(public_key_path);
    cmd.arg("--bundle").arg(signature_path);
    cmd.arg(binary_path);

    let output = cmd.output().context("failed to spawn cosign verify-blob")?;
    if output.status.success() {
        Ok(VerificationResult::Verified {
            identity: public_key_path.display().to_string(),
            bundle_path: signature_path.display().to_string(),
        })
    } else {
        Ok(VerificationResult::Invalid {
            identity_pattern: public_key_path.display().to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// Apply the supplied [`SignaturePolicy`] to a downloaded plugin binary.
///
/// `repo_spec` should be `<owner>/<repo>` (e.g. `launchapp-dev/animus-provider-claude`).
/// `binary_path` is the asset that was signed (typically the `.tar.gz`,
/// not the extracted binary — that's a CLI-side detail).
/// `signature_path` is the cosign `.bundle` next to the asset, or `None`
/// when no bundle was published.
///
/// Returns:
/// - `VerificationResult::Skipped` when `policy.mode == Disabled` or
///   `repo_spec` is in `policy.allow_unsigned_for`.
/// - `VerificationResult::Unsigned` when `signature_path` is `None` or
///   no trusted key can be resolved.
/// - Whatever [`verify_plugin_binary`] returns otherwise.
///
/// This function never **enforces** the policy — it only reports. Callers
/// must check `VerificationResult::is_strict_failure()` against
/// `policy.mode` and turn `Strict` failures into install errors.
pub fn verify_plugin_install(
    repo_spec: &str,
    binary_path: &Path,
    signature_path: Option<&Path>,
    policy: &SignaturePolicy,
) -> Result<VerificationResult> {
    if matches!(policy.mode, PolicyMode::Disabled) {
        return Ok(VerificationResult::Skipped);
    }
    if policy.allows_unsigned_repo(repo_spec) {
        return Ok(VerificationResult::Skipped);
    }

    let Some(sig_path) = signature_path else {
        return Ok(VerificationResult::Unsigned {
            reason: "no cosign signature bundle published alongside this asset".to_string(),
        });
    };

    let (owner, repo) = split_owner_repo(repo_spec)
        .ok_or_else(|| anyhow::anyhow!("repo_spec must be '<owner>/<repo>', got '{repo_spec}'"))?;

    let Some(key_path) = policy.resolve_trusted_key_for(owner, repo) else {
        return Ok(VerificationResult::Unsigned {
            reason: format!(
                "no trusted key found for {repo_spec} in {} (add a PEM file or pass --trust-key)",
                policy.trusted_keys_dir.display()
            ),
        });
    };

    if !cosign_available() {
        return Ok(VerificationResult::Unsigned {
            reason: "cosign binary not found on PATH; install cosign from https://github.com/sigstore/cosign to enable signature verification".to_string(),
        });
    }

    verify_plugin_binary(binary_path, sig_path, &key_path)
}

fn split_owner_repo(spec: &str) -> Option<(&str, &str)> {
    let (owner, repo) = spec.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn policy_mode_defaults_to_warn_for_v0_4_12() {
        // v0.4.12 temporary default while LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM
        // is still a placeholder. v0.4.13 flips this back to Strict.
        assert_eq!(PolicyMode::default_for_install(), PolicyMode::Warn);
    }

    #[test]
    fn policy_mode_parses_each_variant() {
        assert_eq!(PolicyMode::from_str("strict").unwrap(), PolicyMode::Strict);
        assert_eq!(PolicyMode::from_str("STRICT").unwrap(), PolicyMode::Strict);
        assert_eq!(PolicyMode::from_str("warn").unwrap(), PolicyMode::Warn);
        assert_eq!(PolicyMode::from_str("warning").unwrap(), PolicyMode::Warn);
        assert_eq!(PolicyMode::from_str("disabled").unwrap(), PolicyMode::Disabled);
        assert_eq!(PolicyMode::from_str("off").unwrap(), PolicyMode::Disabled);
        assert!(PolicyMode::from_str("bogus").is_err());
    }

    #[test]
    fn policy_mode_display_is_stable() {
        assert_eq!(PolicyMode::Strict.to_string(), "strict");
        assert_eq!(PolicyMode::Warn.to_string(), "warn");
        assert_eq!(PolicyMode::Disabled.to_string(), "disabled");
    }

    #[test]
    fn allow_unsigned_for_is_empty_by_default() {
        let p = SignaturePolicy::default_install();
        assert!(p.allow_unsigned_for.is_empty(), "default policy must NOT exempt any repos");
        assert_eq!(p.mode, PolicyMode::default_for_install());
    }

    #[test]
    fn allow_unsigned_for_matches_exact_repo() {
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: PathBuf::from("/tmp/keys"),
            trust_key_override: None,
            allow_unsigned_for: vec!["launchapp-dev/animus-provider-mock".to_string()],
        };
        assert!(p.allows_unsigned_repo("launchapp-dev/animus-provider-mock"));
        assert!(!p.allows_unsigned_repo("launchapp-dev/animus-provider-claude"));
    }

    #[test]
    fn resolve_trusted_key_prefers_override() {
        let tmp = tempfile::tempdir().unwrap();
        let override_key = tmp.path().join("custom.pem");
        std::fs::write(&override_key, "pem").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: tmp.path().to_path_buf(),
            trust_key_override: Some(override_key.clone()),
            allow_unsigned_for: Vec::new(),
        };
        let resolved = p.resolve_trusted_key_for("anyone", "anything").expect("override resolves");
        assert_eq!(resolved, override_key);
    }

    #[test]
    fn resolve_trusted_key_prefers_owner_pem() {
        let tmp = tempfile::tempdir().unwrap();
        let owner_key = tmp.path().join("launchapp-dev.pem");
        std::fs::write(&owner_key, "pem").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: tmp.path().to_path_buf(),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        };
        let resolved =
            p.resolve_trusted_key_for("launchapp-dev", "animus-provider-claude").expect("owner-scoped key resolves");
        assert_eq!(resolved, owner_key);
    }

    #[test]
    fn resolve_trusted_key_falls_back_to_owner_repo_pem() {
        let tmp = tempfile::tempdir().unwrap();
        let scoped_key = tmp.path().join("alice-myplugin.pem");
        std::fs::write(&scoped_key, "pem").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: tmp.path().to_path_buf(),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        };
        let resolved = p.resolve_trusted_key_for("alice", "myplugin").expect("scoped key resolves");
        assert_eq!(resolved, scoped_key);
    }

    #[test]
    fn resolve_trusted_key_returns_none_when_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: tmp.path().to_path_buf(),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        };
        assert!(p.resolve_trusted_key_for("nobody", "nothing").is_none());
    }

    #[test]
    fn verify_install_returns_skipped_when_disabled() {
        let p = SignaturePolicy {
            mode: PolicyMode::Disabled,
            trusted_keys_dir: PathBuf::from("/nope"),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        };
        let result = verify_plugin_install("any/thing", Path::new("/tmp/x"), None, &p).unwrap();
        assert_eq!(result, VerificationResult::Skipped);
    }

    #[test]
    fn verify_install_returns_skipped_when_repo_is_exempt() {
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: PathBuf::from("/nope"),
            trust_key_override: None,
            allow_unsigned_for: vec!["test/fixture".to_string()],
        };
        let result = verify_plugin_install("test/fixture", Path::new("/tmp/x"), None, &p).unwrap();
        assert_eq!(result, VerificationResult::Skipped);
    }

    #[test]
    fn verify_install_returns_unsigned_when_no_bundle() {
        let p = SignaturePolicy::default_install();
        let result =
            verify_plugin_install("launchapp-dev/animus-provider-claude", Path::new("/tmp/x"), None, &p).unwrap();
        assert!(matches!(&result, VerificationResult::Unsigned { .. }));
        assert!(result.is_strict_failure());
    }

    #[test]
    fn verify_install_returns_unsigned_when_no_trusted_key() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("x.bundle");
        std::fs::write(&bundle, b"bundle").unwrap();
        let binary = tmp.path().join("x.tar.gz");
        std::fs::write(&binary, b"bin").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: tmp.path().join("empty-keys"),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        };
        let result = verify_plugin_install("launchapp-dev/animus-provider-claude", &binary, Some(&bundle), &p).unwrap();
        match result {
            VerificationResult::Unsigned { reason } => {
                assert!(reason.contains("no trusted key found"), "unexpected reason: {reason}");
            }
            other => panic!("expected Unsigned, got {other:?}"),
        }
    }

    #[test]
    fn verify_install_rejects_malformed_repo_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("x.bundle");
        std::fs::write(&bundle, b"bundle").unwrap();
        let key = tmp.path().join("launchapp-dev.pem");
        std::fs::write(&key, "pem").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_keys_dir: tmp.path().to_path_buf(),
            trust_key_override: None,
            allow_unsigned_for: Vec::new(),
        };
        let err = verify_plugin_install("not-a-spec", &bundle, Some(&bundle), &p).unwrap_err();
        assert!(format!("{err}").contains("repo_spec must be"));
    }

    #[test]
    fn verification_result_strict_failure_classification() {
        assert!(VerificationResult::Unsigned { reason: "x".into() }.is_strict_failure());
        assert!(VerificationResult::Invalid { identity_pattern: "x".into(), message: "y".into() }.is_strict_failure());
        assert!(VerificationResult::UntrustedSigner { identity_pattern: "x".into() }.is_strict_failure());
        assert!(!VerificationResult::Verified { identity: "x".into(), bundle_path: "y".into() }.is_strict_failure());
        assert!(!VerificationResult::Skipped.is_strict_failure());
    }

    #[test]
    fn seed_writes_launchapp_key_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("trusted-keys");
        let written = seed_launchapp_dev_trusted_key(&dir).unwrap();
        let path = written.expect("first seed must write");
        assert!(path.ends_with(LAUNCHAPP_DEV_TRUSTED_KEY_FILENAME));
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("BEGIN PUBLIC KEY"), "seed must contain a PEM header");
    }

    #[test]
    fn seed_is_noop_when_key_already_present() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("trusted-keys");
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join(LAUNCHAPP_DEV_TRUSTED_KEY_FILENAME);
        std::fs::write(&existing, "user-pinned-key").unwrap();
        let written = seed_launchapp_dev_trusted_key(&dir).unwrap();
        assert!(written.is_none(), "seed must not stomp a user-pinned key");
        let contents = std::fs::read_to_string(&existing).unwrap();
        assert_eq!(contents, "user-pinned-key");
    }
}
