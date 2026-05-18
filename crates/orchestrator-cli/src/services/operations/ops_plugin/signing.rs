//! Cosign / sigstore signature verification for plugin install.
//!
//! Shells out to the `cosign` binary for v0.4.x. The CLI surface stays
//! stable when we switch to the `sigstore` Rust crate in v0.5.
//!
//! See `docs/architecture/plugin-signing.md` for the threat model and
//! the v0.5+ migration plan.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

/// Default OIDC issuer for GitHub Actions keyless signatures.
pub(crate) const GITHUB_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

/// Outcome of attempting to verify a release asset's cosign bundle.
///
/// The string variants are stable values written into `plugins.yaml` under
/// `signature_status` and surfaced by `animus plugin list`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum SignatureStatus {
    Verified { identity: String, bundle_path: String },
    Unsigned { reason: String },
    Invalid { identity_pattern: String, message: String },
    UntrustedSigner { identity_pattern: String },
    Skipped,
}

impl SignatureStatus {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Verified { .. } => "verified",
            Self::Unsigned { .. } => "unsigned",
            Self::Invalid { .. } => "invalid",
            Self::UntrustedSigner { .. } => "untrusted_signer",
            Self::Skipped => "skipped",
        }
    }
}

/// Returns `true` if the `cosign` binary is on `$PATH`.
pub(crate) fn cosign_available() -> bool {
    Command::new("cosign")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `cosign verify-blob` against an asset and its bundle.
pub(crate) fn verify_with_cosign(
    asset_path: &Path,
    bundle_path: &Path,
    expected_identity_regexp: Option<&str>,
    issuer: &str,
) -> Result<SignatureStatus> {
    let mut cmd = Command::new("cosign");
    cmd.arg("verify-blob");
    cmd.arg("--bundle").arg(bundle_path);
    cmd.arg("--certificate-oidc-issuer").arg(issuer);
    let regex = expected_identity_regexp.unwrap_or(".*");
    cmd.arg("--certificate-identity-regexp").arg(regex);
    cmd.arg(asset_path);

    let output = cmd.output().context("failed to spawn cosign verify-blob")?;
    if output.status.success() {
        Ok(SignatureStatus::Verified { identity: regex.to_string(), bundle_path: bundle_path.display().to_string() })
    } else {
        Ok(SignatureStatus::Invalid {
            identity_pattern: regex.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

// =================== Trusted signers config ===================

#[derive(Debug, Clone, Default)]
pub(crate) struct TrustedSigners {
    pub(crate) entries: Vec<TrustedSigner>,
    #[allow(dead_code)]
    pub(crate) source: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct TrustedSigner {
    pub(crate) identity: String,
    #[allow(dead_code)]
    pub(crate) issuer: String,
}

impl TrustedSigners {
    /// Returns `true` when at least one entry's identity glob matches the
    /// supplied `owner/repo` slug. An empty config is permissive — matches all.
    pub(crate) fn matches_repo(&self, owner_repo: &str) -> bool {
        if self.entries.is_empty() {
            return true;
        }
        self.entries.iter().any(|e| glob_match(&e.identity, owner_repo))
    }

    /// Returns the regex pattern (suitable for `--certificate-identity-regexp`)
    /// rooted at the supplied `<owner>/<repo>`.
    pub(crate) fn identity_regexp_for(&self, owner: &str, repo: &str) -> String {
        let escaped_owner = regex_escape(owner);
        let escaped_repo = regex_escape(repo);
        format!("^https://github\\.com/{}/{}/.+", escaped_owner, escaped_repo)
    }
}

/// Resolve the trusted-signers file path.
///
/// Order: explicit CLI override > `$ANIMUS_TRUSTED_SIGNERS` > `~/.animus/trusted-signers.yaml`.
pub(crate) fn resolve_trusted_signers_path(cli_override: Option<&Path>) -> PathBuf {
    if let Some(p) = cli_override {
        return p.to_path_buf();
    }
    if let Ok(env) = std::env::var("ANIMUS_TRUSTED_SIGNERS") {
        let trimmed = env.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".animus").join("trusted-signers.yaml")
}

/// Load the trusted-signers config from disk. Missing file returns `None` so
/// the caller knows to fall back to the permissive default.
pub(crate) fn load_trusted_signers(path: &Path) -> Result<Option<TrustedSigners>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))?;
    let Some(arr) = value.get("trusted_signers").and_then(|v| v.as_sequence()) else {
        return Ok(Some(TrustedSigners { entries: Vec::new(), source: Some(path.to_path_buf()) }));
    };
    let mut entries = Vec::new();
    for entry in arr {
        let Some(map) = entry.as_mapping() else { continue };
        let identity = map.get(serde_yaml::Value::String("identity".to_string())).and_then(|v| v.as_str());
        let issuer = map
            .get(serde_yaml::Value::String("issuer".to_string()))
            .and_then(|v| v.as_str())
            .unwrap_or(GITHUB_OIDC_ISSUER);
        if let Some(identity) = identity {
            entries.push(TrustedSigner { identity: identity.to_string(), issuer: issuer.to_string() });
        }
    }
    Ok(Some(TrustedSigners { entries, source: Some(path.to_path_buf()) }))
}

// =================== Helpers ===================

/// Minimal glob matcher for `*` and `?` against `<owner>/<repo>` slugs.
pub(crate) fn glob_match(pattern: &str, candidate: &str) -> bool {
    fn helper(p: &[u8], c: &[u8]) -> bool {
        if p.is_empty() {
            return c.is_empty();
        }
        match p[0] {
            b'*' => {
                if helper(&p[1..], c) {
                    return true;
                }
                if !c.is_empty() {
                    return helper(p, &c[1..]);
                }
                false
            }
            b'?' => !c.is_empty() && helper(&p[1..], &c[1..]),
            b => !c.is_empty() && c[0] == b && helper(&p[1..], &c[1..]),
        }
    }
    helper(pattern.as_bytes(), candidate.as_bytes())
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_handles_star() {
        assert!(glob_match("launchapp-dev/animus-*", "launchapp-dev/animus-provider-claude"));
        assert!(glob_match("launchapp-dev/*", "launchapp-dev/animus-subject-linear"));
        assert!(!glob_match("launchapp-dev/animus-*", "someoneelse/animus-foo"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("a/b", "a/b"));
        assert!(!glob_match("a/b", "a/c"));
    }

    #[test]
    fn glob_match_handles_question_mark() {
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(!glob_match("a?c", "abbc"));
    }

    #[test]
    fn trusted_signers_glob_matches_launchapp_dev() {
        let cfg = TrustedSigners {
            entries: vec![TrustedSigner {
                identity: "launchapp-dev/animus-*".to_string(),
                issuer: GITHUB_OIDC_ISSUER.to_string(),
            }],
            source: None,
        };
        assert!(cfg.matches_repo("launchapp-dev/animus-provider-claude"));
        assert!(cfg.matches_repo("launchapp-dev/animus-subject-linear"));
        assert!(!cfg.matches_repo("evil-org/animus-provider-claude"));
    }

    #[test]
    fn empty_trusted_signers_matches_everything() {
        let cfg = TrustedSigners::default();
        assert!(cfg.matches_repo("launchapp-dev/animus-provider-claude"));
        assert!(cfg.matches_repo("anyone/anything"));
    }

    #[test]
    fn identity_regexp_escapes_owner_and_repo() {
        let cfg = TrustedSigners::default();
        let pat = cfg.identity_regexp_for("launchapp-dev", "animus-provider-claude");
        assert!(pat.starts_with("^https://github\\.com/"));
        assert!(pat.contains("launchapp-dev"));
        assert!(pat.contains("animus-provider-claude"));
    }

    #[test]
    fn signature_status_labels_are_stable() {
        assert_eq!(SignatureStatus::Skipped.label(), "skipped");
        assert_eq!(SignatureStatus::Verified { identity: "x".into(), bundle_path: "y".into() }.label(), "verified");
        assert_eq!(SignatureStatus::Unsigned { reason: "n/a".into() }.label(), "unsigned");
        assert_eq!(SignatureStatus::Invalid { identity_pattern: "x".into(), message: "y".into() }.label(), "invalid");
        assert_eq!(SignatureStatus::UntrustedSigner { identity_pattern: "x".into() }.label(), "untrusted_signer");
    }

    #[test]
    fn load_trusted_signers_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let nope = tmp.path().join("nope.yaml");
        assert!(load_trusted_signers(&nope).unwrap().is_none());
    }

    #[test]
    fn load_trusted_signers_parses_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("trusted.yaml");
        std::fs::write(
            &p,
            r#"
trusted_signers:
  - identity: "launchapp-dev/animus-*"
    issuer: "https://token.actions.githubusercontent.com"
  - identity: "alice/my-plugin"
"#,
        )
        .unwrap();
        let cfg = load_trusted_signers(&p).unwrap().expect("config should load");
        assert_eq!(cfg.entries.len(), 2);
        assert_eq!(cfg.entries[0].identity, "launchapp-dev/animus-*");
        assert_eq!(cfg.entries[1].identity, "alice/my-plugin");
        assert_eq!(cfg.entries[1].issuer, GITHUB_OIDC_ISSUER);
    }
}
