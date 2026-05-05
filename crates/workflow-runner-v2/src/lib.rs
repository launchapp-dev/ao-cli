pub mod agent_state;
pub mod config_context;
pub mod direct_exec;
pub mod ensure_execution_cwd;
pub mod ipc;
pub mod payload_traversal;
pub mod phase_command;
pub mod phase_executor;
pub mod phase_failover;
pub mod phase_git;
pub mod phase_output;
pub mod phase_prompt;
pub mod phase_targets;
pub mod runtime_contract;
pub mod runtime_support;
pub mod skill_dispatch;
pub mod workflow_execute;
pub mod workflow_helpers;
pub mod workflow_merge_recovery;

pub use agent_state::{
    append_agent_memory, clear_agent_memory, list_agent_messages, load_agent_memory, send_agent_message,
    AgentMemoryDocument, AgentMemoryEntry, AgentMessage,
};
pub use ensure_execution_cwd::ensure_execution_cwd;
pub use ipc::*;
pub use payload_traversal::{
    fallback_implementation_commit_message, parse_commit_message_from_text, parse_phase_decision_from_text,
};
pub use phase_executor::{
    load_agent_runtime_config, run_workflow_phase, CliPhaseExecutor, PhaseExecuteOverrides, PhaseExecutionMetadata,
    PhaseExecutionOutcome, PhaseExecutionSignal, PhaseRunParams, PhaseRunResult,
};
pub use phase_failover::{classify_phase_failure, PhaseFailureClassifier, PhaseFailureKind};
pub use phase_git::{commit_implementation_changes, ensure_git_identity, git_has_pending_changes, is_git_repo};
pub use phase_output::{persist_phase_output, phase_output_dir, PersistedPhaseOutput};
pub use phase_prompt::{
    build_phase_prompt, phase_requires_commit_message, phase_requires_commit_message_with_config, render_phase_prompt,
    PhasePromptInputs, PhasePromptParams, PhaseRenderParams, RenderedPhasePrompt,
};
pub use phase_targets::PhaseTargetPlanner;
pub use runtime_support::*;
pub use workflow_execute::{execute_workflow, PhaseEvent, WorkflowExecuteParams, WorkflowExecuteResult};
pub use workflow_helpers::{
    task_requires_research, workflow_has_active_research, workflow_has_completed_research, PhaseExecutionEvent,
};
pub use workflow_merge_recovery::MergeConflictContext;

#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Returns the per-process test home directory and pins HOME to it on first call.
    pub fn stable_test_home() -> &'static std::path::Path {
        static HOME: OnceLock<std::path::PathBuf> = OnceLock::new();
        HOME.get_or_init(|| {
            let home_dir = std::env::temp_dir()
                .join(format!("ao-workflow-runner-v2-test-home-{}", std::process::id()))
                .join("home");
            std::fs::create_dir_all(&home_dir).expect("create shared workflow-runner-v2 test home");
            std::env::set_var("HOME", &home_dir);
            home_dir
        })
    }

    /// Process-wide lock for tests that depend on `protocol::scoped_state_root`. Hold the guard
    /// for the entirety of the test body. Diagnosed cause: under parallel cargo-test execution,
    /// scope-dir state in `~/.ao/.../` accumulates from many concurrent tempdirs and triggers
    /// `find_existing_scope_by_origin` collisions / partial state visibility that flips
    /// scoped_state_root's resolved path between writes and reads. Serializing avoids the race.
    pub fn scoped_state_serializer() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        stable_test_home();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
