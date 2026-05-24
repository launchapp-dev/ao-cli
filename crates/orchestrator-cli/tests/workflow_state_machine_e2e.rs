#[path = "support/test_harness.rs"]
pub mod test_harness;

use anyhow::{Context, Result};
use serde_json::Value;
use test_harness::CliHarness;

#[test]
fn e2e_workflow_state_machine_json_contract_endpoints() -> Result<()> {
    let harness = CliHarness::new()?;

    let state_machine_get = harness.run_json_ok(&["workflow", "state-machine", "get"])?;
    assert_eq!(state_machine_get.pointer("/data/schema").and_then(Value::as_str), Some("animus.state-machines.v1"));
    let machine_path = state_machine_get
        .pointer("/data/path")
        .and_then(Value::as_str)
        .context("workflow state-machine get should return data.path")?;
    assert!(std::path::Path::new(machine_path).exists(), "state machine metadata path should exist: {machine_path}");
    let transitions = state_machine_get
        .pointer("/data/state_machines/workflow/transitions")
        .and_then(Value::as_array)
        .context("workflow state-machine get should include transitions array")?;
    assert!(!transitions.is_empty(), "workflow state machine transitions should not be empty");

    let state_machine_validate = harness.run_json_ok(&["workflow", "state-machine", "validate"])?;
    assert_eq!(state_machine_validate.pointer("/data/valid").and_then(Value::as_bool), Some(true));
    assert_eq!(
        state_machine_validate.pointer("/data/errors").and_then(Value::as_array).map(std::vec::Vec::len),
        Some(0)
    );

    Ok(())
}
