#[path = "support/test_harness.rs"]
mod test_harness;

use anyhow::Result;
use serde_json::Value;
use test_harness::CliHarness;

#[test]
fn setup_guided_mode_requires_interactive_terminal() -> Result<()> {
    let harness = CliHarness::new()?;

    let (payload, status) = harness.run_json_err_with_exit(&["setup", "--plan"])?;
    assert_eq!(status, 2);
    assert_eq!(
        payload.pointer("/error/code").and_then(Value::as_str),
        Some("invalid_input")
    );
    assert!(payload
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("guided setup must be run in an interactive terminal"));

    Ok(())
}

#[test]
fn setup_non_interactive_requires_explicit_inputs() -> Result<()> {
    let harness = CliHarness::new()?;

    let (payload, status) =
        harness.run_json_err_with_exit(&["setup", "--non-interactive", "--plan"])?;
    assert_eq!(status, 2);
    assert_eq!(
        payload.pointer("/error/code").and_then(Value::as_str),
        Some("invalid_input")
    );
    assert!(payload
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("missing required non-interactive setup inputs"));

    Ok(())
}

#[test]
fn setup_plan_apply_and_idempotent_rerun_are_stable() -> Result<()> {
    let harness = CliHarness::new()?;
    let setup_flags = [
        "setup",
        "--non-interactive",
        "--auto-merge",
        "true",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "true",
    ];

    let plan = harness.run_json_ok(&[
        "setup",
        "--non-interactive",
        "--plan",
        "--auto-merge",
        "true",
        "--auto-pr",
        "false",
        "--auto-commit-before-merge",
        "true",
    ])?;
    assert_eq!(
        plan.pointer("/data/stage").and_then(Value::as_str),
        Some("plan")
    );
    assert_eq!(
        plan.pointer("/data/mode").and_then(Value::as_str),
        Some("non_interactive")
    );
    assert_eq!(
        plan.pointer("/data/apply/applied").and_then(Value::as_bool),
        Some(false)
    );

    let first_apply = harness.run_json_ok(&setup_flags)?;
    assert_eq!(
        first_apply.pointer("/data/stage").and_then(Value::as_str),
        Some("apply")
    );
    assert_eq!(
        first_apply
            .pointer("/data/apply/daemon_config_updated")
            .and_then(Value::as_bool),
        Some(true)
    );

    let second_apply = harness.run_json_ok(&setup_flags)?;
    assert_eq!(
        second_apply
            .pointer("/data/apply/daemon_config_updated")
            .and_then(Value::as_bool),
        Some(false)
    );

    let pm_config_path = harness.project_root().join(".ao").join("pm-config.json");
    assert!(
        pm_config_path.exists(),
        "setup apply should persist pm-config"
    );
    let pm_config_content = std::fs::read_to_string(pm_config_path)?;
    let pm_config: Value = serde_json::from_str(&pm_config_content)?;
    assert_eq!(
        pm_config.get("auto_merge_enabled").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        pm_config.get("auto_pr_enabled").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        pm_config
            .get("auto_commit_before_merge")
            .and_then(Value::as_bool),
        Some(true)
    );

    Ok(())
}

#[test]
fn doctor_reports_stable_checks_and_fix_actions() -> Result<()> {
    let harness = CliHarness::new()?;

    let doctor = harness.run_json_ok(&["doctor"])?;
    let checks = doctor
        .pointer("/data/doctor/checks")
        .and_then(Value::as_array)
        .expect("doctor checks array should exist");
    assert!(!checks.is_empty(), "doctor checks should not be empty");
    for check in checks {
        assert!(check.get("id").and_then(Value::as_str).is_some());
        assert!(check.get("status").and_then(Value::as_str).is_some());
        assert!(check
            .pointer("/remediation/id")
            .and_then(Value::as_str)
            .is_some());
        assert!(
            check
                .pointer("/remediation/available")
                .and_then(Value::as_bool)
                .is_some(),
            "remediation availability should be included"
        );
    }

    let fixed = harness.run_json_ok(&["doctor", "--fix"])?;
    assert_eq!(
        fixed
            .pointer("/data/fix/requested")
            .and_then(Value::as_bool),
        Some(true)
    );
    let actions = fixed
        .pointer("/data/fix/actions")
        .and_then(Value::as_array)
        .expect("fix actions should be an array");
    assert!(
        !actions.is_empty(),
        "doctor --fix should report action results"
    );
    assert!(actions.iter().all(|action| {
        action.get("id").and_then(Value::as_str).is_some()
            && action.get("status").and_then(Value::as_str).is_some()
            && action.get("details").and_then(Value::as_str).is_some()
    }));

    Ok(())
}
