use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use orchestrator_config::agent_runtime_config::{
    AgentProfile, AgentRuntimeConfig, Idempotency, PhaseExecutionDefinition, PhaseExecutionMode,
};
use orchestrator_config::parse_yaml_workflow_config;
use tempfile::TempDir;
use workflow_runner_v2::{
    block_reason_sideeffecting, block_reason_unknown, classify_phase_recovery, is_phase_completed,
    phase_completion_marker_path, write_phase_completion_marker, PhaseCompletionMarker, PhaseRecoveryAction,
};

fn home_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner())
}

fn pin_home(tmp: &TempDir) {
    std::env::set_var("HOME", tmp.path());
}

fn runtime_with_phase(phase_id: &str, idempotency: Idempotency) -> AgentRuntimeConfig {
    let mut phases = BTreeMap::new();
    phases.insert(
        phase_id.to_string(),
        PhaseExecutionDefinition {
            mode: PhaseExecutionMode::Agent,
            agent_id: Some("default".to_string()),
            directive: None,
            system_prompt: None,
            runtime: None,
            capabilities: None,
            output_contract: None,
            output_json_schema: None,
            decision_contract: None,
            retry: None,
            skills: Vec::new(),
            command: None,
            manual: None,
            default_tool: None,
            idempotency,
        },
    );
    let mut agents = BTreeMap::new();
    agents.insert("default".to_string(), AgentProfile::default());
    AgentRuntimeConfig {
        schema: "animus.agent-runtime.v2".to_string(),
        version: 2,
        tools_allowlist: Vec::new(),
        agents,
        phases,
        cli_tools: BTreeMap::new(),
    }
}

#[test]
fn idempotency_field_parses_from_workflow_yaml() {
    let yaml = r#"
agents:
  default:
    description: ""
    system_prompt: ""
phases:
  research:
    mode: agent
    agent: default
    idempotency: idempotent
  impl:
    mode: agent
    agent: default
    idempotency: sideeffecting
  review:
    mode: agent
    agent: default
    idempotency: unknown
  legacy:
    mode: agent
    agent: default
workflows:
  - id: test
    phases: [research, impl, review, legacy]
"#;
    let config = parse_yaml_workflow_config(yaml).expect("parse yaml");
    let research = config.phase_definitions.get("research").expect("research phase");
    let impl_phase = config.phase_definitions.get("impl").expect("impl phase");
    let review = config.phase_definitions.get("review").expect("review phase");
    let legacy = config.phase_definitions.get("legacy").expect("legacy phase");

    assert_eq!(research.idempotency, Idempotency::Idempotent);
    assert_eq!(impl_phase.idempotency, Idempotency::Sideeffecting);
    assert_eq!(review.idempotency, Idempotency::Unknown);
    assert_eq!(legacy.idempotency, Idempotency::Unknown, "missing field defaults to Unknown");
}

#[test]
fn recovery_blocks_unknown_phase_with_actionable_reason() {
    let runtime = runtime_with_phase("legacy", Idempotency::Unknown);
    let action = classify_phase_recovery(&runtime, "legacy");
    match action {
        PhaseRecoveryAction::BlockUnknown { reason } => {
            assert!(reason.contains("legacy"), "reason mentions phase id: {reason}");
            assert!(reason.contains("no idempotency annotation"), "reason guides user to YAML edit: {reason}");
            assert!(reason.contains("Mark in workflow YAML"), "reason includes remediation guidance: {reason}");
        }
        other => panic!("expected BlockUnknown, got {other:?}"),
    }
}

#[test]
fn recovery_blocks_sideeffecting_phase() {
    let runtime = runtime_with_phase("commit-and-push", Idempotency::Sideeffecting);
    let action = classify_phase_recovery(&runtime, "commit-and-push");
    match action {
        PhaseRecoveryAction::BlockSideeffecting { reason } => {
            assert!(reason.contains("commit-and-push"));
            assert!(reason.contains("partially executed"));
            assert!(reason.contains("--force"), "reason advertises --force escape hatch: {reason}");
        }
        other => panic!("expected BlockSideeffecting, got {other:?}"),
    }
}

#[test]
fn recovery_auto_retries_idempotent_phase() {
    let runtime = runtime_with_phase("lint", Idempotency::Idempotent);
    let action = classify_phase_recovery(&runtime, "lint");
    assert_eq!(action, PhaseRecoveryAction::AutoRetry);
    assert!(!action.is_blocking());
    assert!(action.reason().is_none());
}

#[test]
fn resume_with_force_bypasses_idempotency_block() {
    let block_msg = block_reason_unknown("impl");
    assert!(block_msg.contains("no idempotency annotation"));
    let sideeff = block_reason_sideeffecting("impl");
    assert!(sideeff.contains("--force"));

    let action = PhaseRecoveryAction::BlockUnknown { reason: block_msg.clone() };
    assert!(action.is_blocking());
    assert_eq!(action.reason(), Some(block_msg.as_str()));
}

#[test]
fn phase_completion_marker_atomic_rename_from_tmp() {
    let _guard = home_lock();
    let tmp = TempDir::new().expect("tempdir");
    pin_home(&tmp);
    let project = TempDir::new().expect("project tempdir");
    let project_root = project.path().to_string_lossy().to_string();
    let workflow_id = "wf-marker-001";
    let phase_id = "research";

    write_phase_completion_marker(&project_root, workflow_id, phase_id).expect("write marker");

    let marker_path = phase_completion_marker_path(&project_root, workflow_id, phase_id);
    assert!(marker_path.exists(), "marker file exists at {marker_path:?}");
    let contents = std::fs::read_to_string(&marker_path).expect("read marker");
    let parsed: PhaseCompletionMarker = serde_json::from_str(&contents).expect("parse marker JSON");
    assert_eq!(parsed.phase_id, phase_id);
    assert_eq!(parsed.output_path, format!("{phase_id}.json"));
    assert!(!parsed.completed_at.is_empty());

    let parent_dir = marker_path.parent().unwrap();
    let leftover_tmp: Vec<_> = std::fs::read_dir(parent_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftover_tmp.is_empty(), "no .tmp residue after atomic rename");
}

#[test]
fn executor_skips_phase_when_completion_marker_present() {
    let _guard = home_lock();
    let tmp = TempDir::new().expect("tempdir");
    pin_home(&tmp);
    let project = TempDir::new().expect("project tempdir");
    let project_root = project.path().to_string_lossy().to_string();
    let workflow_id = "wf-skip-001";
    let phase_id = "research";

    assert!(!is_phase_completed(&project_root, workflow_id, phase_id));
    write_phase_completion_marker(&project_root, workflow_id, phase_id).expect("write marker");
    assert!(is_phase_completed(&project_root, workflow_id, phase_id));
}

#[test]
fn executor_does_not_skip_when_only_output_exists_without_marker() {
    let _guard = home_lock();
    let tmp = TempDir::new().expect("tempdir");
    pin_home(&tmp);
    let project = TempDir::new().expect("project tempdir");
    let project_root = project.path().to_string_lossy().to_string();
    let workflow_id = "wf-no-marker-001";
    let phase_id = "research";

    let marker_path = phase_completion_marker_path(&project_root, workflow_id, phase_id);
    let dir = marker_path.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let output_path = dir.join(format!("{phase_id}.json"));
    std::fs::write(&output_path, r#"{"phase_id":"research","completed_at":"2026-05-23T00:00:00Z"}"#).unwrap();

    assert!(!is_phase_completed(&project_root, workflow_id, phase_id), "marker absence => not skipped");
}
