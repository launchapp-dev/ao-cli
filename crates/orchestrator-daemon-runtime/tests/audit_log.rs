//! Integration tests for the security audit log
//! ([`orchestrator_daemon_runtime::audit`]). Kept out of the in-tree `lib.rs`
//! test surface so they can run even when sibling modules' unit tests are
//! temporarily broken.

use std::fs;
use std::path::Path;

use orchestrator_daemon_runtime::audit::{
    audit_log_path, log_plugin_install, Audit, AuditActor, AuditEvent, AuditEventKind,
};
use serde_json::{json, Value};
use tempfile::tempdir;

fn read_lines(path: &Path) -> Vec<Value> {
    let body = fs::read_to_string(path).expect("read audit log");
    body.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).expect("audit jsonl")).collect()
}

#[test]
fn audit_log_appends_install_event_with_signature_status() {
    let dir = tempdir().unwrap();
    let sha = "deadbeef".repeat(8);
    log_plugin_install(
        dir.path(),
        AuditEventKind::PluginInstall,
        "launchapp-dev/animus-provider-claude",
        "v0.2.2",
        sha.as_str(),
        "verified",
    );
    let lines = read_lines(&audit_log_path(dir.path()));
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["event"], "plugin_install");
    assert_eq!(line["actor"], "user");
    let details = &line["details"];
    assert_eq!(details["repo"], "launchapp-dev/animus-provider-claude");
    assert_eq!(details["version"], "v0.2.2");
    assert_eq!(details["sha256"], sha);
    assert_eq!(details["signature_status"], "verified");
}

#[test]
fn audit_log_rotates_at_10mb() {
    // Use a small max-bytes so a couple of writes trip rotation; the
    // 10 MiB production constant is exercised in lib unit tests for the
    // module-local default. This integration test verifies the rotation
    // **behavior** end-to-end without writing 10 MiB to disk.
    let dir = tempdir().unwrap();
    let path = audit_log_path(dir.path());
    let audit = Audit::at_path(path.clone()).with_max_bytes(256);
    for i in 0..15 {
        audit.log_event(AuditEvent::new(
            AuditActor::User,
            AuditEventKind::PluginInstall,
            json!({"i": i, "padding": "x".repeat(150)}),
        ));
    }
    assert!(path.exists(), "active audit log must remain after rotation");
    let rotated_1 = dir.path().join("audit.1.jsonl");
    assert!(rotated_1.exists(), "expected at least one rotated file (audit.1.jsonl)");
    // Sanity-check: the active file's most recent line is parseable JSON.
    let last = read_lines(&path).pop().expect("at least one line in the active log");
    assert_eq!(last["event"], "plugin_install");
}

#[test]
fn audit_log_handles_multiple_event_kinds() {
    let dir = tempdir().unwrap();
    let audit = Audit::at_scoped_root(dir.path());
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::PluginInstall, json!({"x": 1})));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::PluginUpgrade, json!({"x": 2})));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::PluginUninstall, json!({"x": 3})));
    audit.log_event(AuditEvent::new(AuditActor::Daemon, AuditEventKind::SignatureSkipped, json!({"x": 4})));
    audit.log_event(AuditEvent::new(AuditActor::Daemon, AuditEventKind::SignatureInvalid, json!({"x": 5})));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::TrustKeyAdded, json!({"x": 6})));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::TrustPublisherAdded, json!({"x": 7})));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::PolicyOverride, json!({"x": 8})));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::LockfileMismatch, json!({"x": 9})));

    let lines = read_lines(audit.path());
    assert_eq!(lines.len(), 9);
    let observed: Vec<&str> = lines.iter().map(|l| l["event"].as_str().unwrap()).collect();
    assert_eq!(
        observed,
        vec![
            "plugin_install",
            "plugin_upgrade",
            "plugin_uninstall",
            "signature_skipped",
            "signature_invalid",
            "trust_key_added",
            "trust_publisher_added",
            "policy_override",
            "lockfile_mismatch",
        ]
    );
}

#[test]
fn audit_log_never_panics_when_target_dir_is_missing() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a").join("b").join("c");
    let audit = Audit::at_path(nested.join("audit.jsonl"));
    audit.log_event(AuditEvent::new(AuditActor::User, AuditEventKind::PolicyOverride, json!({"flag": "--force"})));
    let lines = read_lines(audit.path());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "policy_override");
}
