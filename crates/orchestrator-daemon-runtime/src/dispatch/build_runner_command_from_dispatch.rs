use std::path::PathBuf;

use protocol::{McpRuntimeConfig, PhaseRoutingConfig, SubjectDispatch};
use tracing::warn;

pub fn build_runner_command_from_dispatch(dispatch: &SubjectDispatch, project_root: &str) -> std::process::Command {
    build_runner_command(dispatch, project_root, None, None)
}

pub fn build_runner_command(
    dispatch: &SubjectDispatch,
    project_root: &str,
    phase_routing: Option<&PhaseRoutingConfig>,
    mcp_config: Option<&McpRuntimeConfig>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(resolve_workflow_runner_binary());
    cmd.arg("execute");

    match dispatch.subject.to_workflow_subject() {
        protocol::orchestrator::WorkflowSubject::Task { id } => {
            cmd.arg("--task-id").arg(id);
        }
        protocol::orchestrator::WorkflowSubject::Requirement { id } => {
            cmd.arg("--requirement-id").arg(id);
        }
        protocol::orchestrator::WorkflowSubject::Custom { title, description } => {
            cmd.arg("--title").arg(title);
            cmd.arg("--description").arg(description);
        }
    }

    if let Some(input) = &dispatch.input {
        cmd.arg("--input-json").arg(input.to_string());
    }

    cmd.arg("--workflow-ref").arg(&dispatch.workflow_ref).arg("--project-root").arg(project_root);

    if let Some(routing) = phase_routing {
        if let Ok(json) = serde_json::to_string(routing) {
            cmd.arg("--phase-routing-json").arg(json);
        }
    }
    if let Some(mcp) = mcp_config {
        if let Ok(json) = serde_json::to_string(mcp) {
            cmd.arg("--mcp-config-json").arg(json);
        }
    }
    cmd
}

fn resolve_workflow_runner_binary() -> PathBuf {
    if let Ok(path) = std::env::var("ANIMUS_WORKFLOW_RUNNER_BIN") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    // Primary lookup: the v0.4.16+ binary name (`animus-workflow-runner`).
    // Back-compat fallback: the legacy v0.4.x name (`ao-workflow-runner`).
    // Resolution order for each name is:
    //   1. sibling of `current_exe` (and `current_exe/../` when running
    //      from `target/debug/deps/` so `cargo test` finds the runner),
    //   2. PATH search via `which`.
    // The new name takes precedence over the legacy name at every step:
    // an `animus-workflow-runner` on PATH wins over a sibling
    // `ao-workflow-runner`, but a sibling `animus-workflow-runner` wins
    // over a PATH-only `ao-workflow-runner`. When the legacy binary is what
    // we resolve to we emit a `warn!` line so operators upgrading the
    // daemon between binary-name eras get a nudge to re-run the installer.
    if let Some(found) = find_workflow_runner_binary(workflow_runner_binary_name()) {
        return found;
    }

    if let Some(legacy) = find_workflow_runner_binary(legacy_workflow_runner_binary_name()) {
        warn!(
            "found legacy ao-workflow-runner; reinstall with `curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | sh` to upgrade"
        );
        return legacy;
    }

    PathBuf::from(workflow_runner_binary_name())
}

fn find_workflow_runner_binary(binary_name: &str) -> Option<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let sibling = exe_dir.join(binary_name);
            if sibling.exists() {
                return Some(sibling);
            }

            if exe_dir.file_name().is_some_and(|name| name == "deps") {
                if let Some(parent) = exe_dir.parent() {
                    let parent_sibling = parent.join(binary_name);
                    if parent_sibling.exists() {
                        return Some(parent_sibling);
                    }
                }
            }
        }
    }

    // Fall back to PATH lookup. Handles the case where the daemon binary
    // was installed via `cargo install` / a different prefix than the
    // workflow runner (eg the daemon lives in `~/.cargo/bin/animus` but the
    // runner lives in `~/.local/bin/ao-workflow-runner` per a v0.4.x
    // installer run). Without this branch a PATH-only legacy install
    // disappears from the resolver as soon as the daemon binary moves.
    find_on_path(binary_name)
}

fn find_on_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(binary_name);
        if !candidate.is_file() {
            continue;
        }
        if !is_executable(&candidate) {
            continue;
        }
        return Some(candidate);
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &std::path::Path) -> bool {
    // On Windows the PATH lookup uses extension-based resolution which the
    // OS handles at spawn time. Returning true preserves existing
    // behavior; callers fall through to `Command::spawn` and let the OS
    // surface any non-executability.
    true
}

fn workflow_runner_binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "animus-workflow-runner.exe"
    }

    #[cfg(not(target_os = "windows"))]
    {
        "animus-workflow-runner"
    }
}

fn legacy_workflow_runner_binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "ao-workflow-runner.exe"
    }

    #[cfg(not(target_os = "windows"))]
    {
        "ao-workflow-runner"
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    #[cfg(unix)]
    use std::sync::Mutex;

    use protocol::orchestrator::WorkflowSubject;
    use protocol::{SubjectDispatch, SubjectDispatchExt};
    use serde_json::json;

    use super::build_runner_command_from_dispatch;

    #[test]
    fn runner_command_uses_subject_workflow_ref_and_input_from_dispatch() {
        let dispatch = SubjectDispatch::for_custom(
            "schedule:nightly",
            "nightly dispatch",
            "ops",
            Some(json!({"nightly":true})),
            "schedule",
        );
        let command = build_runner_command_from_dispatch(&dispatch, "/tmp/project");
        let program = command.get_program().to_string_lossy().into_owned();
        let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();

        assert_eq!(
            Path::new(&program).file_name().and_then(|name| name.to_str()),
            Some(super::workflow_runner_binary_name())
        );
        assert_eq!(
            args,
            vec![
                "execute",
                "--title",
                "schedule:nightly",
                "--description",
                "nightly dispatch",
                "--input-json",
                "{\"nightly\":true}",
                "--workflow-ref",
                "ops",
                "--project-root",
                "/tmp/project",
            ]
        );
        assert_eq!(
            &dispatch.subject.to_workflow_subject(),
            &WorkflowSubject::Custom {
                title: "schedule:nightly".to_string(),
                description: "nightly dispatch".to_string(),
            }
        );
    }

    #[test]
    fn workflow_runner_binary_name_uses_new_animus_prefix() {
        // P3 housekeeping: as of v0.4.16 the daemon resolves
        // `animus-workflow-runner` first. The legacy `ao-workflow-runner`
        // name is only used as a back-compat fallback. Keep both helpers in
        // place so the migration window lasts the v0.4.x cycle.
        let primary = super::workflow_runner_binary_name();
        assert!(primary.starts_with("animus-workflow-runner"), "primary name should start with animus-: {primary}");
        assert!(
            !primary.starts_with("ao-workflow-runner"),
            "primary name must not be the legacy ao- prefix: {primary}"
        );

        let legacy = super::legacy_workflow_runner_binary_name();
        assert!(legacy.starts_with("ao-workflow-runner"), "legacy name should retain the ao- prefix: {legacy}");
    }

    // Shared lock for tests that mutate `ANIMUS_WORKFLOW_RUNNER_BIN` or
    // `PATH`. Routed through the dispatch-wide shared lock so we also
    // serialize against `process_manager`'s env-mutating tests.
    #[cfg(unix)]
    fn env_lock() -> &'static Mutex<()> {
        crate::dispatch::test_env::lock()
    }

    #[cfg(unix)]
    struct EnvVarGuard {
        key: &'static str,
        original: Option<String>,
    }

    #[cfg(unix)]
    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let original = std::env::var(key).ok();
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
            Self { key, original }
        }
    }

    #[cfg(unix)]
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn resolve_picks_new_binary_when_both_exist_alongside_current_exe() {
        // Back-compat regression: when an operator upgrades the daemon
        // binary but still has the legacy `ao-workflow-runner` on the side
        // (because the installer also drops a back-compat symlink), the
        // resolver MUST select `animus-workflow-runner` first. Otherwise
        // we silently keep invoking the legacy name and the rename is a
        // no-op.
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock().lock().unwrap_or_else(|p| p.into_inner());
        let _clear = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", None);

        // Resolve `current_exe`'s parent dir; that's where the resolver
        // looks for siblings. We can't move `current_exe`, but we CAN
        // create siblings next to it. Cargo test binaries live under
        // `target/debug/deps/` so the resolver also probes the parent
        // (`target/debug/`).
        let exe = std::env::current_exe().expect("current_exe");
        let exe_dir = exe.parent().expect("exe parent");
        // Pick the `deps` parent (target/debug) so the siblings don't
        // collide with the real release-built runner under
        // `target/debug/animus-workflow-runner` on developer machines.
        let bin_dir = if exe_dir.file_name().is_some_and(|n| n == "deps") {
            exe_dir.parent().expect("target/debug").to_path_buf()
        } else {
            exe_dir.to_path_buf()
        };

        // If a real runner already sits there (cargo build -p
        // workflow-runner-v2 was run), don't clobber it. Skip the test
        // instead — the assertion below would either be redundant or
        // wedge on a stale `ao-workflow-runner` artifact.
        let new_path = bin_dir.join(super::workflow_runner_binary_name());
        let legacy_path = bin_dir.join(super::legacy_workflow_runner_binary_name());
        if new_path.exists() || legacy_path.exists() {
            return;
        }

        let payload = b"#!/bin/sh\nexit 0\n";
        std::fs::write(&new_path, payload).expect("write new runner");
        std::fs::write(&legacy_path, payload).expect("write legacy runner");
        let mut perms = std::fs::metadata(&new_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&new_path, perms.clone()).unwrap();
        std::fs::set_permissions(&legacy_path, perms).unwrap();

        let resolved = super::resolve_workflow_runner_binary();

        // Tidy up before asserting so a failed assertion still cleans the
        // sibling files we wrote into `target/debug/`.
        let _ = std::fs::remove_file(&new_path);
        let _ = std::fs::remove_file(&legacy_path);

        assert_eq!(
            resolved.file_name().and_then(|n| n.to_str()),
            Some(super::workflow_runner_binary_name()),
            "resolver must pick the new animus-workflow-runner when both names exist; got {}",
            resolved.display()
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_falls_back_to_legacy_when_new_missing() {
        // Back-compat: a freshly upgraded daemon binary running against a
        // user environment that still only has `ao-workflow-runner` (no
        // re-run of the installer yet) must keep dispatching. The
        // resolver's job is to find the legacy binary and emit a warn
        // line via `tracing::warn!`. We can't assert the warn easily
        // here without wiring a tracing subscriber, but we can prove the
        // path is selected.
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock().lock().unwrap_or_else(|p| p.into_inner());
        let _clear = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", None);
        // Clear PATH so the resolver's PATH-fallback can't surface a real
        // `animus-workflow-runner` from a developer's environment and pre-empt
        // the legacy-sibling path we're trying to exercise here.
        let _path = EnvVarGuard::set("PATH", Some(""));

        let exe = std::env::current_exe().expect("current_exe");
        let exe_dir = exe.parent().expect("exe parent");
        let bin_dir = if exe_dir.file_name().is_some_and(|n| n == "deps") {
            exe_dir.parent().expect("target/debug").to_path_buf()
        } else {
            exe_dir.to_path_buf()
        };

        let new_path = bin_dir.join(super::workflow_runner_binary_name());
        let legacy_path = bin_dir.join(super::legacy_workflow_runner_binary_name());
        // Bail out if the env already has a real runner — we don't want to
        // mutate cargo build artifacts.
        if new_path.exists() || legacy_path.exists() {
            return;
        }

        let payload = b"#!/bin/sh\nexit 0\n";
        std::fs::write(&legacy_path, payload).expect("write legacy runner");
        let mut perms = std::fs::metadata(&legacy_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&legacy_path, perms).unwrap();

        let resolved = super::resolve_workflow_runner_binary();

        let _ = std::fs::remove_file(&legacy_path);

        assert_eq!(
            resolved.file_name().and_then(|n| n.to_str()),
            Some(super::legacy_workflow_runner_binary_name()),
            "resolver must fall back to ao-workflow-runner when animus-workflow-runner is missing; got {}",
            resolved.display()
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_falls_back_to_legacy_on_path_when_no_sibling() {
        // PATH-only fallback: covers the case where the daemon binary lives
        // in a different prefix than the runner (eg `cargo install animus`
        // drops the daemon in `~/.cargo/bin/` while a prior v0.4.x installer
        // left `ao-workflow-runner` in `~/.local/bin/`). Without PATH
        // fallback the resolver would emit a literal "animus-workflow-runner"
        // path and spawn would fail. With it we surface the legacy binary
        // and the warn nudge, so the daemon keeps dispatching.
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock().lock().unwrap_or_else(|p| p.into_inner());
        let _clear = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", None);

        // Make sure no sibling lookup succeeds: pick a scratch PATH dir
        // that's far from `current_exe`.
        let temp = tempfile::tempdir().expect("tempdir");
        let legacy_path = temp.path().join(super::legacy_workflow_runner_binary_name());
        std::fs::write(&legacy_path, b"#!/bin/sh\nexit 0\n").expect("write legacy on PATH");
        let mut perms = std::fs::metadata(&legacy_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&legacy_path, perms).unwrap();

        // Scope PATH down to the scratch dir so we don't accidentally pick
        // up a real `animus-workflow-runner` from the developer's
        // environment.
        let _path = EnvVarGuard::set("PATH", Some(temp.path().to_str().expect("utf-8 tempdir")));

        // If the sibling probe still finds a real animus-workflow-runner
        // built into `target/debug/`, skip the test — that artifact predates
        // the test and we can't isolate around it without removing it.
        let exe = std::env::current_exe().expect("current_exe");
        let exe_dir = exe.parent().expect("exe parent");
        let bin_dir = if exe_dir.file_name().is_some_and(|n| n == "deps") {
            exe_dir.parent().expect("target/debug").to_path_buf()
        } else {
            exe_dir.to_path_buf()
        };
        // Skip if EITHER sibling exists in `target/debug/` — a stale
        // pre-rename build artifact (`ao-workflow-runner`) would also
        // pre-empt the PATH probe and wedge the assertion below.
        if bin_dir.join(super::workflow_runner_binary_name()).exists()
            || bin_dir.join(super::legacy_workflow_runner_binary_name()).exists()
        {
            return;
        }

        let resolved = super::resolve_workflow_runner_binary();
        assert_eq!(
            resolved.file_name().and_then(|n| n.to_str()),
            Some(super::legacy_workflow_runner_binary_name()),
            "resolver must fall back to ao-workflow-runner on PATH when no sibling exists; got {}",
            resolved.display()
        );
        // And the path must be the one we wrote, not just a name.
        assert_eq!(resolved, legacy_path, "resolved path must point at the PATH-discovered legacy binary");
    }
}
