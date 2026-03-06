use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn runner_binary() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin!("ao-workflow-runner").to_path_buf()
}

fn mock_ao_script(exit_code: i32) -> String {
    #[cfg(unix)]
    {
        format!("#!/bin/sh\nexit {exit_code}\n")
    }
    #[cfg(not(unix))]
    {
        format!("@echo off\r\nexit /B {exit_code}\r\n")
    }
}

fn setup_mock_ao(temp_dir: &TempDir, exit_code: i32) -> std::path::PathBuf {
    #[cfg(unix)]
    let ao_path = temp_dir.path().join("ao");
    #[cfg(not(unix))]
    let ao_path = temp_dir.path().join("ao.exe");

    fs::write(&ao_path, mock_ao_script(exit_code)).expect("should write mock ao");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&ao_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ao_path, perms).unwrap();
    }

    ao_path
}

#[test]
fn runner_delegates_to_ao_and_forwards_success() {
    let temp_dir = TempDir::new().unwrap();
    let ao_path = setup_mock_ao(&temp_dir, 0);

    let output = Command::new(runner_binary())
        .arg("execute")
        .arg("--task-id")
        .arg("TASK-TEST")
        .arg("--project-root")
        .arg(temp_dir.path())
        .env("AO_BIN", &ao_path)
        .output()
        .expect("runner should execute");

    assert!(
        output.status.success(),
        "runner should exit 0 when ao exits 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("runner_start"), "should emit runner_start event");
    assert!(stderr.contains("runner_complete"), "should emit runner_complete event");
    assert!(stderr.contains("TASK-TEST"), "events should include task id");
}

#[test]
fn runner_delegates_to_ao_and_forwards_failure() {
    let temp_dir = TempDir::new().unwrap();
    let ao_path = setup_mock_ao(&temp_dir, 1);

    let output = Command::new(runner_binary())
        .arg("execute")
        .arg("--task-id")
        .arg("TASK-FAIL")
        .arg("--project-root")
        .arg(temp_dir.path())
        .env("AO_BIN", &ao_path)
        .output()
        .expect("runner should execute");

    assert!(
        !output.status.success(),
        "runner should forward non-zero exit code from ao"
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"exit_code\":1"), "completion event should contain exit code 1");
}

#[test]
fn runner_fails_when_ao_binary_missing() {
    let temp_dir = TempDir::new().unwrap();

    let output = Command::new(runner_binary())
        .arg("execute")
        .arg("--task-id")
        .arg("TASK-MISSING")
        .arg("--project-root")
        .arg(temp_dir.path())
        .env("AO_BIN", "/nonexistent/path/ao")
        .output()
        .expect("runner should execute");

    assert!(
        !output.status.success(),
        "runner should fail when ao binary is not found"
    );
}

#[test]
fn runner_forwards_pipeline_arg() {
    let temp_dir = TempDir::new().unwrap();

    #[cfg(unix)]
    let ao_path = temp_dir.path().join("ao");
    #[cfg(not(unix))]
    let ao_path = temp_dir.path().join("ao.exe");

    #[cfg(unix)]
    fs::write(
        &ao_path,
        "#!/bin/sh\necho \"$@\" >&2\nexit 0\n",
    )
    .unwrap();
    #[cfg(not(unix))]
    fs::write(&ao_path, "@echo off\r\necho %* 1>&2\r\nexit /B 0\r\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&ao_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ao_path, perms).unwrap();
    }

    let output = Command::new(runner_binary())
        .arg("execute")
        .arg("--task-id")
        .arg("TASK-PIPE")
        .arg("--pipeline")
        .arg("custom-pipeline")
        .arg("--project-root")
        .arg(temp_dir.path())
        .env("AO_BIN", &ao_path)
        .output()
        .expect("runner should execute");

    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--pipeline-id") && stderr.contains("custom-pipeline"),
        "should forward pipeline as --pipeline-id to ao; stderr: {stderr}"
    );
}
