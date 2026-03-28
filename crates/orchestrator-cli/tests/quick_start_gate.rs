#[path = "support/test_harness.rs"]
mod test_harness;

use anyhow::Result;
use serde_json::Value;
use test_harness::CliHarness;

#[test]
fn quick_start_doctor_runs_without_error() -> Result<()> {
    let harness = CliHarness::new()?;

    // AC1: Runs `ao doctor` and verifies exit code 0 or graceful output (no panic)
    let doctor_output = harness.run_json_ok(&["doctor"])?;
    assert_eq!(doctor_output.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(doctor_output.get("schema").and_then(Value::as_str), Some("ao.cli.v1"));
    assert!(doctor_output.pointer("/data/doctor/checks").and_then(Value::as_array).is_some(), "doctor should include checks array");

    Ok(())
}

#[test]
fn quick_start_setup_creates_ao_directory() -> Result<()> {
    let harness = CliHarness::new()?;

    // AC2: Runs `ao setup` in a temp directory and verifies .ao/ directory is created
    let setup_result = harness.run_json_ok(&[
        "setup",
        "--non-interactive",
        "--auto-merge",
        "false",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "false",
    ])?;
    assert_eq!(setup_result.pointer("/data/stage").and_then(Value::as_str), Some("apply"));
    assert_eq!(setup_result.pointer("/data/apply/applied").and_then(Value::as_bool), Some(true));

    // Verify .ao/ directory was created
    let ao_dir = harness.project_root().join(".ao");
    assert!(ao_dir.exists(), ".ao directory should be created after setup");
    assert!(ao_dir.is_dir(), ".ao should be a directory");

    // Verify config.json exists
    let config_json = ao_dir.join("config.json");
    assert!(config_json.exists(), "config.json should be created in .ao directory");

    Ok(())
}

#[test]
fn quick_start_task_create_returns_task_id() -> Result<()> {
    let harness = CliHarness::new()?;

    // Setup first
    harness.run_json_ok(&[
        "setup",
        "--non-interactive",
        "--auto-merge",
        "false",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "false",
    ])?;

    // AC3: Runs `ao task create --title "Test task" --task-type feature --priority high` and verifies TASK-* id is returned
    let created = harness.run_json_ok(&[
        "task",
        "create",
        "--title",
        "Test task",
        "--task-type",
        "feature",
        "--priority",
        "high",
    ])?;

    let task_id = created
        .pointer("/data/id")
        .and_then(Value::as_str)
        .expect("task create should return data.id");

    // Verify it's in TASK-* format
    assert!(task_id.starts_with("TASK-"), "task id should start with TASK-: {}", task_id);

    // Verify other fields
    assert_eq!(created.pointer("/data/title").and_then(Value::as_str), Some("Test task"));
    assert_eq!(created.pointer("/data/type").and_then(Value::as_str), Some("feature"));

    Ok(())
}

#[test]
fn quick_start_workflow_run_parses_without_panic() -> Result<()> {
    let harness = CliHarness::new()?;

    // Setup
    harness.run_json_ok(&[
        "setup",
        "--non-interactive",
        "--auto-merge",
        "false",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "false",
    ])?;

    // Create a task
    let created = harness.run_json_ok(&[
        "task",
        "create",
        "--title",
        "Test task for workflow",
        "--task-type",
        "feature",
        "--priority",
        "high",
    ])?;

    let task_id = created
        .pointer("/data/id")
        .and_then(Value::as_str)
        .expect("task create should return data.id")
        .to_string();

    // AC4: Runs `ao workflow run --task-id <created-task-id> --sync`
    // The command parses and errors gracefully if no agent configured, not panics
    let output = harness.run_json_output(&["workflow", "run", "--task-id", &task_id, "--sync"])?;

    // Command should either succeed or fail gracefully without panicking
    // We verify this by checking that it returned a valid JSON output (not a panic)
    let payload = serde_json::from_slice::<Value>(&output.stdout);
    if payload.is_ok() {
        let payload = payload.unwrap();
        assert_eq!(payload.get("schema").and_then(Value::as_str), Some("ao.cli.v1"), "should return valid ao.cli.v1 schema");
    } else {
        // Even if JSON parsing fails, the test passes as long as the command didn't panic.
        // Panics would result in non-JSON output (a stack trace) which would be caught by the test infrastructure.
        assert!(output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("thread"),
            "command should not panic: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

#[test]
fn quick_start_status_returns_valid_schema() -> Result<()> {
    let harness = CliHarness::new()?;

    // Setup
    harness.run_json_ok(&[
        "setup",
        "--non-interactive",
        "--auto-merge",
        "false",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "false",
    ])?;

    // AC5: Runs `ao status` and verifies JSON output matches ao.status.v1 schema (has schema, daemon, task_summary fields)
    let status = harness.run_json_ok(&["status"])?;

    assert_eq!(status.get("schema").and_then(Value::as_str), Some("ao.cli.v1"), "status should return ao.cli.v1 schema");
    assert_eq!(status.get("ok").and_then(Value::as_bool), Some(true), "status should return ok=true");

    // Verify required fields exist in data
    let data = status.pointer("/data").expect("status should include /data");
    assert!(data.get("schema").and_then(Value::as_str).is_some(), "status.data should include schema field");
    assert!(data.get("daemon").is_some(), "status.data should include daemon field");
    assert!(data.get("task_summary").is_some(), "status.data should include task_summary field");

    Ok(())
}

#[test]
fn quick_start_all_commands_together() -> Result<()> {
    let harness = CliHarness::new()?;

    // This test exercises the full quick-start flow in sequence to catch integration issues

    // 1. Doctor check
    let doctor = harness.run_json_ok(&["doctor"])?;
    assert_eq!(doctor.get("ok").and_then(Value::as_bool), Some(true));

    // 2. Setup
    let setup = harness.run_json_ok(&[
        "setup",
        "--non-interactive",
        "--auto-merge",
        "false",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "false",
    ])?;
    assert_eq!(setup.pointer("/data/apply/applied").and_then(Value::as_bool), Some(true));

    // 3. Task create
    let created = harness.run_json_ok(&[
        "task",
        "create",
        "--title",
        "Integration test task",
        "--task-type",
        "feature",
        "--priority",
        "high",
    ])?;
    let task_id = created.pointer("/data/id").and_then(Value::as_str).unwrap().to_string();
    assert!(task_id.starts_with("TASK-"));

    // 4. Status check (should show the newly created task)
    let status = harness.run_json_ok(&["status"])?;
    assert_eq!(status.get("ok").and_then(Value::as_bool), Some(true));
    assert!(status.pointer("/data/task_summary").is_some());

    // 5. Task list (should show the newly created task)
    let list = harness.run_json_ok(&["task", "list"])?;
    assert_eq!(list.get("ok").and_then(Value::as_bool), Some(true));
    let tasks = list.pointer("/data").and_then(Value::as_array).expect("task list should return data as array");
    assert!(!tasks.is_empty(), "task list should include the created task");

    Ok(())
}
