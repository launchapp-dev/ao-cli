//! Cosign **keyless** signature verification policy for plugin installs.
//!
//! This module owns the *policy* surface (`SignaturePolicy`, `PolicyMode`,
//! `TrustedPublisher`, `VerificationResult`) so it can be reused by the CLI
//! install pipeline and any future install entry point (MCP wire,
//! daemon-side installs).
//!
//! Verification shells out to the `cosign` binary in keyless mode against
//! the Sigstore Fulcio CA + Rekor transparency log (both built into
//! `cosign`). There is no baked-in public key: trust is anchored on the
//! signer **identity** (the `<repo>/.github/workflows/release.yml@refs/tags/v*`
//! SAN URI baked into the per-signing Fulcio cert) plus the OIDC issuer
//! (`https://token.actions.githubusercontent.com`). Every
//! `launchapp-dev/animus-*` release pipeline signs this way (see e.g.
//! `transport-graphql/.github/workflows/release.yml`); the legacy
//! key-based PEM path was removed in v0.4.12.
//!
//! See `docs/reference/security.md` for the operator-facing trust model
//! and `docs/architecture/plugin-signing.md` for the threat model.

use std::fmt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default OIDC issuer for GitHub Actions keyless signatures.
pub const GITHUB_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

/// Signature enforcement mode selected by the operator.
///
/// v0.4.12 ships `Warn` as the install default *only* to give operators
/// one release cycle to either upgrade to a v0.4.12-signed plugin or to
/// uninstall plugins installed before keyless signing existed. v0.4.13
/// flips this back to `Strict`. See `docs/reference/security.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// Fail-closed. Refuse install if signature is missing, invalid, or
    /// signed by an identity outside the trusted-publisher list.
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
    /// `Warn` for v0.4.12 to ease the migration from the pre-v0.4.12
    /// key-based path (which no longer exists). v0.4.13 flips this back
    /// to `Strict` — keyless verification has a real Sigstore trust
    /// anchor, so there is no longer a placeholder excuse.
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

/// A trust anchor for keyless cosign verification: maps a GitHub owner
/// to the identity regex + OIDC issuer that cosign must see on the
/// Fulcio-issued cert.
///
/// For `launchapp-dev/*` releases the per-signing cert SAN looks like
/// `https://github.com/launchapp-dev/<repo>/.github/workflows/release.yml@refs/tags/v0.2.3`,
/// which is what [`TrustedPublisher::launchapp_dev`] matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedPublisher {
    /// GitHub owner (org or user) this publisher entry covers.
    pub owner: String,
    /// Regex passed to `cosign verify-blob --certificate-identity-regexp`.
    /// Anchors should match the workflow URI baked into the Fulcio cert.
    pub identity_regex: String,
    /// OIDC issuer passed to `cosign verify-blob --certificate-oidc-issuer`.
    pub oidc_issuer: String,
}

impl TrustedPublisher {
    /// Built-in publisher entry for `launchapp-dev/*` plugins. Matches the
    /// cert SAN emitted by every standardized launchapp-dev release
    /// pipeline (`/.github/workflows/release.yml@refs/tags/v*`).
    #[must_use]
    pub fn launchapp_dev() -> Self {
        Self {
            owner: "launchapp-dev".to_string(),
            identity_regex: "^https://github\\.com/launchapp-dev/[^/]+/\\.github/workflows/release\\.yml@refs/tags/v.*"
                .to_string(),
            oidc_issuer: GITHUB_OIDC_ISSUER.to_string(),
        }
    }
}

/// Full signature policy: enforcement mode plus the keyless trust
/// anchors the install pipeline consults for a given owner.
#[derive(Debug, Clone)]
pub struct SignaturePolicy {
    pub mode: PolicyMode,
    /// Keyless trust anchors. The install pipeline picks the entry whose
    /// `owner` matches the install source's GitHub owner.
    pub trusted_publishers: Vec<TrustedPublisher>,
    /// Repos that are exempt from signature checks even under `Strict`.
    /// MUST be empty by default — populating this list is a security
    /// regression and should only happen when signing is genuinely
    /// infeasible for that repo.
    pub allow_unsigned_for: Vec<String>,
}

impl SignaturePolicy {
    /// Build a default install-time policy: `Warn` (v0.4.12 transition
    /// default; flips to `Strict` in v0.4.13), trusted publisher set to
    /// the built-in `launchapp-dev` keyless anchor, no exemptions.
    #[must_use]
    pub fn default_install() -> Self {
        Self {
            mode: PolicyMode::default_for_install(),
            trusted_publishers: vec![TrustedPublisher::launchapp_dev()],
            allow_unsigned_for: Vec::new(),
        }
    }

    /// Returns `true` when the policy allows the install to proceed without
    /// signature verification for the supplied `owner/repo`.
    #[must_use]
    pub fn allows_unsigned_repo(&self, owner_repo: &str) -> bool {
        self.allow_unsigned_for.iter().any(|r| r == owner_repo)
    }

    /// Resolve the trusted publisher entry for a given `<owner>`, if any.
    #[must_use]
    pub fn find_trusted_publisher_for(&self, owner: &str) -> Option<&TrustedPublisher> {
        self.trusted_publishers.iter().find(|p| p.owner == owner)
    }
}

/// Outcome of a signature verification attempt for a single asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VerificationResult {
    /// Cosign verified the asset against the trusted publisher's keyless
    /// identity (Fulcio cert SAN + OIDC issuer matched).
    Verified { identity: String, bundle_path: String },
    /// No signature bundle was published or available for this install.
    Unsigned { reason: String },
    /// A signature bundle exists but cosign rejected it cryptographically.
    Invalid { identity_pattern: String, message: String },
    /// Cosign accepted the bundle but the signer identity is not on the
    /// trusted publisher list.
    UntrustedSigner { identity_pattern: String },
    /// Verification was skipped (policy = `Disabled` or repo is in
    /// `allow_unsigned_for`).
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

/// Verify a single asset against its cosign keyless bundle.
///
/// Shells out to:
///
/// ```text
/// cosign verify-blob \
///   --certificate-identity-regexp <identity_regex> \
///   --certificate-oidc-issuer <oidc_issuer> \
///   --bundle <signature_bundle_path> \
///   <artifact_path>
/// ```
///
/// Returns `Verified` on success or `Invalid` with cosign's stderr on
/// cryptographic failure. Returns `Err` only when cosign itself could
/// not be spawned.
pub fn verify_plugin_binary_keyless(
    artifact_path: &Path,
    signature_bundle_path: &Path,
    identity_regex: &str,
    oidc_issuer: &str,
) -> Result<VerificationResult> {
    let mut cmd = Command::new("cosign");
    cmd.arg("verify-blob");
    cmd.arg("--certificate-identity-regexp").arg(identity_regex);
    cmd.arg("--certificate-oidc-issuer").arg(oidc_issuer);
    cmd.arg("--bundle").arg(signature_bundle_path);
    cmd.arg(artifact_path);

    let output = cmd.output().context("failed to spawn cosign verify-blob")?;
    if output.status.success() {
        Ok(VerificationResult::Verified {
            identity: identity_regex.to_string(),
            bundle_path: signature_bundle_path.display().to_string(),
        })
    } else {
        Ok(VerificationResult::Invalid {
            identity_pattern: identity_regex.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// Apply the supplied [`SignaturePolicy`] to a downloaded plugin asset.
///
/// `repo_spec` should be `<owner>/<repo>` (e.g.
/// `launchapp-dev/animus-provider-claude`).
/// `artifact_path` is the asset that was signed (typically the `.tar.gz`,
/// not the extracted binary — that's a CLI-side detail).
/// `signature_bundle_path` is the cosign `.bundle` next to the asset, or
/// `None` when no bundle was published.
///
/// Returns:
/// - `Skipped` when `policy.mode == Disabled` or `repo_spec` is in
///   `policy.allow_unsigned_for`.
/// - `Unsigned` when `signature_bundle_path` is `None`, when no trusted
///   publisher matches the owner, or when cosign is not installed.
/// - Otherwise whatever [`verify_plugin_binary_keyless`] returns.
///
/// This function never **enforces** the policy — it only reports. Callers
/// must check `VerificationResult::is_strict_failure()` against
/// `policy.mode` and turn `Strict` failures into install errors.
pub fn verify_plugin_install(
    repo_spec: &str,
    artifact_path: &Path,
    signature_bundle_path: Option<&Path>,
    policy: &SignaturePolicy,
) -> Result<VerificationResult> {
    if matches!(policy.mode, PolicyMode::Disabled) {
        return Ok(VerificationResult::Skipped);
    }
    if policy.allows_unsigned_repo(repo_spec) {
        return Ok(VerificationResult::Skipped);
    }

    let Some(bundle_path) = signature_bundle_path else {
        return Ok(VerificationResult::Unsigned {
            reason: "no cosign signature bundle published alongside this asset".to_string(),
        });
    };

    let (owner, _repo) = split_owner_repo(repo_spec)
        .ok_or_else(|| anyhow::anyhow!("repo_spec must be '<owner>/<repo>', got '{repo_spec}'"))?;

    let Some(publisher) = policy.find_trusted_publisher_for(owner) else {
        return Ok(VerificationResult::UntrustedSigner {
            identity_pattern: format!("(no trusted publisher configured for owner '{owner}')"),
        });
    };

    if !cosign_available() {
        return Ok(VerificationResult::Unsigned {
            reason: "cosign binary not found on PATH; install cosign from https://github.com/sigstore/cosign to enable signature verification".to_string(),
        });
    }

    verify_plugin_binary_keyless(artifact_path, bundle_path, &publisher.identity_regex, &publisher.oidc_issuer)
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
        // v0.4.12 ships Warn as the install default to ease the migration
        // from the (removed) key-based path. v0.4.13 flips this back to
        // Strict — see docs/reference/security.md.
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
    fn default_install_seeds_launchapp_dev_publisher() {
        let p = SignaturePolicy::default_install();
        assert!(p.allow_unsigned_for.is_empty(), "default policy must NOT exempt any repos");
        assert_eq!(p.mode, PolicyMode::default_for_install());
        let lp = p.find_trusted_publisher_for("launchapp-dev").expect("launchapp-dev publisher must be seeded");
        assert_eq!(lp.oidc_issuer, GITHUB_OIDC_ISSUER);
        assert!(lp.identity_regex.contains("launchapp-dev"));
        assert!(lp.identity_regex.contains("workflows/release"));
        assert!(lp.identity_regex.contains("refs/tags/v"));
    }

    #[test]
    fn launchapp_dev_identity_regex_is_anchored_and_escaped() {
        let p = TrustedPublisher::launchapp_dev();
        // The regex must escape `.` to avoid matching `releaseXyml` etc.
        assert!(p.identity_regex.contains("\\.github"));
        assert!(p.identity_regex.contains("release\\.yml"));
        // Anchored at start: no prefix attack (`evil/launchapp-dev/...`).
        assert!(p.identity_regex.starts_with("^https://github\\.com/launchapp-dev/"));
    }

    #[test]
    fn allow_unsigned_for_matches_exact_repo() {
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_publishers: vec![TrustedPublisher::launchapp_dev()],
            allow_unsigned_for: vec!["launchapp-dev/animus-provider-mock".to_string()],
        };
        assert!(p.allows_unsigned_repo("launchapp-dev/animus-provider-mock"));
        assert!(!p.allows_unsigned_repo("launchapp-dev/animus-provider-claude"));
    }

    #[test]
    fn find_trusted_publisher_returns_match() {
        let p = SignaturePolicy::default_install();
        assert!(p.find_trusted_publisher_for("launchapp-dev").is_some());
        assert!(p.find_trusted_publisher_for("evil-org").is_none());
    }

    #[test]
    fn find_trusted_publisher_supports_custom_owner() {
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_publishers: vec![
                TrustedPublisher::launchapp_dev(),
                TrustedPublisher {
                    owner: "alice".to_string(),
                    identity_regex: "^https://github\\.com/alice/.+".to_string(),
                    oidc_issuer: GITHUB_OIDC_ISSUER.to_string(),
                },
            ],
            allow_unsigned_for: Vec::new(),
        };
        let alice = p.find_trusted_publisher_for("alice").expect("alice configured");
        assert_eq!(alice.identity_regex, "^https://github\\.com/alice/.+");
    }

    #[test]
    fn verify_install_returns_skipped_when_disabled() {
        let p = SignaturePolicy {
            mode: PolicyMode::Disabled,
            trusted_publishers: Vec::new(),
            allow_unsigned_for: Vec::new(),
        };
        let result = verify_plugin_install("any/thing", Path::new("/tmp/x"), None, &p).unwrap();
        assert_eq!(result, VerificationResult::Skipped);
    }

    #[test]
    fn verify_install_returns_skipped_when_repo_is_exempt() {
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_publishers: Vec::new(),
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
    fn verify_install_returns_untrusted_signer_when_no_publisher() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("x.bundle");
        std::fs::write(&bundle, b"bundle").unwrap();
        let binary = tmp.path().join("x.tar.gz");
        std::fs::write(&binary, b"bin").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_publishers: vec![TrustedPublisher::launchapp_dev()],
            allow_unsigned_for: Vec::new(),
        };
        // Owner is not in trusted_publishers — should fail closed under
        // Strict via UntrustedSigner.
        let result = verify_plugin_install("evil-org/animus-provider-claude", &binary, Some(&bundle), &p).unwrap();
        match result {
            VerificationResult::UntrustedSigner { identity_pattern } => {
                assert!(identity_pattern.contains("evil-org"), "unexpected pattern: {identity_pattern}");
            }
            other => panic!("expected UntrustedSigner, got {other:?}"),
        }
    }

    #[test]
    fn verify_install_rejects_malformed_repo_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("x.bundle");
        std::fs::write(&bundle, b"bundle").unwrap();
        let p = SignaturePolicy {
            mode: PolicyMode::Strict,
            trusted_publishers: Vec::new(),
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
    fn verification_result_labels_are_stable() {
        assert_eq!(VerificationResult::Skipped.label(), "skipped");
        assert_eq!(VerificationResult::Verified { identity: "x".into(), bundle_path: "y".into() }.label(), "verified");
        assert_eq!(VerificationResult::Unsigned { reason: "x".into() }.label(), "unsigned");
        assert_eq!(
            VerificationResult::Invalid { identity_pattern: "x".into(), message: "y".into() }.label(),
            "invalid"
        );
        assert_eq!(VerificationResult::UntrustedSigner { identity_pattern: "x".into() }.label(), "untrusted_signer");
    }

    /// Threat-model guard: the launchapp-dev TrustedPublisher regex MUST
    /// reject every cert SAN shape an attacker would try to slip past the
    /// installer. This test compiles the regex (via the `regex` crate
    /// semantics that cosign uses internally) and probes the boundary:
    ///
    /// - matches the canonical workflow URI emitted by every launchapp-dev
    ///   release pipeline,
    /// - rejects non-tag refs (branch pushes, PR builds),
    /// - rejects non-release workflows on the same repo,
    /// - rejects prefix-attack owners like `evil-launchapp-dev`,
    /// - rejects owners that *contain* `launchapp-dev` mid-string.
    ///
    /// If this test ever fails because the regex was loosened, the keyless
    /// install path stops being a real trust boundary — every malicious
    /// signature in the listed shapes would suddenly verify.
    #[test]
    fn launchapp_dev_regex_rejects_known_attack_shapes() {
        let pat = TrustedPublisher::launchapp_dev().identity_regex;
        let re = regex::Regex::new(&pat).expect("launchapp-dev regex must compile");

        let must_match = [
            "https://github.com/launchapp-dev/animus-provider-claude/.github/workflows/release.yml@refs/tags/v0.1.0",
            "https://github.com/launchapp-dev/animus-subject-linear/.github/workflows/release.yml@refs/tags/v1.2.3",
            "https://github.com/launchapp-dev/animus-transport-graphql/.github/workflows/release.yml@refs/tags/v0.4.12-rc.1",
        ];
        for cert_san in must_match {
            assert!(re.is_match(cert_san), "must accept canonical launchapp-dev release SAN: {cert_san}");
        }

        let must_reject = [
            "https://github.com/launchapp-dev/foo/.github/workflows/release.yml@refs/heads/main",
            "https://github.com/launchapp-dev/foo/.github/workflows/release.yml@refs/pull/1/merge",
            "https://github.com/launchapp-dev/foo/.github/workflows/malicious.yml@refs/tags/v0.1.0",
            "https://github.com/evil-launchapp-dev/foo/.github/workflows/release.yml@refs/tags/v0.1.0",
            "https://github.com/notlaunchapp-dev/foo/.github/workflows/release.yml@refs/tags/v0.1.0",
            "http://github.com/launchapp-dev/foo/.github/workflows/release.yml@refs/tags/v0.1.0",
            "https://gitlab.com/launchapp-dev/foo/.github/workflows/release.yml@refs/tags/v0.1.0",
        ];
        for cert_san in must_reject {
            assert!(!re.is_match(cert_san), "must reject suspicious SAN: {cert_san}");
        }
    }

    /// Sanity check: when cosign is not on PATH, the keyless API returns a
    /// `VerificationResult::Invalid` (cosign spawn succeeded -> non-zero) or
    /// surfaces a spawn error. Either way we never panic. Skipped when
    /// cosign is actually available.
    #[test]
    fn verify_plugin_binary_keyless_with_fake_artifact_does_not_panic() {
        if cosign_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let artifact = tmp.path().join("a.tar.gz");
        let bundle = tmp.path().join("a.tar.gz.bundle");
        std::fs::write(&artifact, b"data").unwrap();
        std::fs::write(&bundle, b"bundle").unwrap();
        let _ = verify_plugin_binary_keyless(&artifact, &bundle, "^https://example/.+", GITHUB_OIDC_ISSUER);
    }
}
