use std::process::Command;

fn ao() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin!("ao").to_path_buf()
}

fn help_output(args: &[&str]) -> String {
    let binary = ao();
    let output = Command::new(&binary)
        .args(args)
        .arg("--help")
        .output()
        .expect("failed to run ao");
    assert!(
        output.status.success(),
        "ao {} --help exited with non-zero status\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("help output was not valid UTF-8")
}

#[test]
fn quick_start_step1_setup_exists() {
    let out = help_output(&["setup"]);
    assert!(
        out.contains("Guided onboarding"),
        "setup --help should describe the onboarding wizard"
    );
}

#[test]
fn quick_start_step2_vision_draft_via_workflow_run() {
    let out = help_output(&["workflow", "run"]);
    assert!(
        out.contains("builtin/vision-draft"),
        "workflow run --help should reference builtin/vision-draft as an example"
    );
    assert!(
        out.contains("--workflow-ref"),
        "workflow run --help should expose --workflow-ref flag"
    );
    assert!(
        out.contains("PIPELINE"),
        "workflow run --help should document the PIPELINE positional argument"
    );
}

#[test]
fn quick_start_step3_requirements_draft_via_workflow_run() {
    let out = help_output(&["workflow", "run"]);
    assert!(
        out.contains("builtin/vision-draft"),
        "workflow run --help should reference builtin workflow examples"
    );
    assert!(
        out.contains("--requirement-id"),
        "workflow run --help should expose --requirement-id flag"
    );
}

#[test]
fn quick_start_step4_requirements_execute_exists() {
    let out = help_output(&["requirements", "execute"]);
    assert!(
        out.contains("Execute requirements into implementation tasks"),
        "requirements execute --help should describe the command purpose"
    );
    assert!(
        out.contains("--workflow-ref"),
        "requirements execute --help should expose --workflow-ref flag"
    );
}

#[test]
fn quick_start_step5_daemon_start_has_autonomous_flag() {
    let out = help_output(&["daemon", "start"]);
    assert!(
        out.contains("--autonomous"),
        "daemon start --help should expose --autonomous flag"
    );
    assert!(
        out.contains("Run daemon in detached/background mode"),
        "daemon start --help should describe --autonomous behavior"
    );
}

#[test]
fn quick_start_step6_monitoring_commands_exist() {
    let task_stats = help_output(&["task", "stats"]);
    assert!(
        task_stats.contains("Show task statistics"),
        "task stats --help should describe the command"
    );

    let daemon_status = help_output(&["daemon", "status"]);
    assert!(
        !daemon_status.is_empty(),
        "daemon status --help should produce output"
    );

    let workflow_list = help_output(&["workflow", "list"]);
    assert!(
        workflow_list.contains("List workflows"),
        "workflow list --help should describe the command"
    );

    let output_monitor = help_output(&["output", "monitor"]);
    assert!(
        output_monitor.contains("Inspect run output"),
        "output monitor --help should describe the command"
    );
    assert!(
        output_monitor.contains("--run-id"),
        "output monitor --help should expose --run-id flag"
    );

    let status = help_output(&["status"]);
    assert!(
        status.contains("Show a unified project status dashboard"),
        "status --help should describe the dashboard command"
    );
}

#[test]
fn self_hosting_backlog_commands_exist() {
    let requirements_list = help_output(&["requirements", "list"]);
    assert!(
        requirements_list.contains("List requirements"),
        "requirements list --help should describe the command"
    );

    let task_prioritized = help_output(&["task", "prioritized"]);
    assert!(
        task_prioritized.contains("List tasks sorted by priority"),
        "task prioritized --help should describe the command"
    );

    let task_stats = help_output(&["task", "stats"]);
    assert!(
        task_stats.contains("Show task statistics"),
        "task stats --help should describe the command"
    );
}

#[test]
fn self_hosting_task_status_flags() {
    let out = help_output(&["task", "status"]);
    assert!(
        out.contains("--id <TASK_ID>"),
        "task status --help should expose --id <TASK_ID>"
    );
    assert!(
        out.contains("--status <STATUS>"),
        "task status --help should expose --status <STATUS>"
    );
    assert!(
        out.contains("in-progress"),
        "task status --help should enumerate in-progress as a valid status"
    );
    assert!(
        out.contains("done"),
        "task status --help should enumerate done as a valid status"
    );
    assert!(
        out.contains("ready"),
        "task status --help should enumerate ready as a valid status"
    );
}

#[test]
fn self_hosting_task_workflow_commands_exist() {
    let task_next = help_output(&["task", "next"]);
    assert!(
        task_next.contains("Get the next ready task"),
        "task next --help should describe the command"
    );
}

#[test]
fn self_hosting_autonomous_daemon_and_monitoring() {
    let daemon_start = help_output(&["daemon", "start"]);
    assert!(
        daemon_start.contains("--autonomous"),
        "daemon start --help should expose --autonomous flag"
    );

    let daemon_status = help_output(&["daemon", "status"]);
    assert!(
        !daemon_status.is_empty(),
        "daemon status --help should produce output"
    );

    let daemon_events = help_output(&["daemon", "events"]);
    assert!(
        daemon_events.contains("Stream or tail daemon event history"),
        "daemon events --help should describe the command"
    );
    assert!(
        daemon_events.contains("--follow"),
        "daemon events --help should expose --follow flag"
    );
}
