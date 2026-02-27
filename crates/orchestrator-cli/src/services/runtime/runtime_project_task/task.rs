use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use orchestrator_core::{
    evaluate_task_priority_policy, services::ServiceHub, TaskCreateInput, TaskFilter,
    TaskPriorityPolicyReport, TaskType, TaskUpdateInput, DEFAULT_HIGH_PRIORITY_BUDGET_PERCENT,
};
use serde::Serialize;

use crate::services::runtime::{stale_in_progress_summary, StaleInProgressSummary};
use crate::{
    ensure_destructive_confirmation, parse_dependency_type, parse_input_json_or,
    parse_priority_opt, parse_task_status, parse_task_type_opt, print_value, TaskCommand,
};

#[derive(Debug, Serialize)]
struct TaskStatsOutput {
    #[serde(flatten)]
    stats: orchestrator_core::TaskStatistics,
    stale_in_progress: StaleInProgressSummary,
    priority_policy: TaskPriorityPolicyReport,
}

const UNLINKED_REQUIREMENTS_WARNING: &str = "warning: creating non-chore task without linked requirements; pass --linked-requirement <REQ_ID> to improve traceability";

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn project_root_for_git_identity() -> Option<PathBuf> {
    non_empty_env("PROJECT_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
}

fn git_local_config_value(project_root: &PathBuf, key: &str) -> Option<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["config", "--local", "--get", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn infer_human_assignee_identity() -> Option<String> {
    if let Some(user_id) = non_empty_env("AO_ASSIGNEE_USER_ID") {
        return Some(user_id);
    }
    if let Some(user_id) = non_empty_env("AO_USER_ID") {
        return Some(user_id);
    }
    if let Some(project_root) = project_root_for_git_identity() {
        if let Some(user_id) = git_local_config_value(&project_root, "user.email") {
            return Some(user_id);
        }
        if let Some(user_id) = git_local_config_value(&project_root, "user.name") {
            return Some(user_id);
        }
    }
    non_empty_env("USER").or_else(|| non_empty_env("USERNAME"))
}

async fn set_task_status_with_assignee_inference(
    tasks: Arc<dyn orchestrator_core::TaskServiceApi>,
    task_id: &str,
    status: orchestrator_core::TaskStatus,
) -> Result<orchestrator_core::OrchestratorTask> {
    if status == orchestrator_core::TaskStatus::InProgress {
        if let Some(user_id) = infer_human_assignee_identity() {
            let _ = tasks.assign_human(task_id, user_id.clone(), user_id).await;
        }
    }
    tasks.set_status(task_id, status).await
}

fn has_non_empty_linked_requirements(input: &TaskCreateInput) -> bool {
    input
        .linked_requirements
        .iter()
        .any(|requirement| !requirement.trim().is_empty())
}

fn should_warn_missing_linked_requirements(input: &TaskCreateInput) -> bool {
    input.task_type.unwrap_or(TaskType::Feature) != TaskType::Chore
        && !has_non_empty_linked_requirements(input)
}

pub(crate) async fn handle_task(
    command: TaskCommand,
    hub: Arc<dyn ServiceHub>,
    json: bool,
) -> Result<()> {
    let tasks = hub.tasks();

    match command {
        TaskCommand::List(args) => {
            let filter = TaskFilter {
                task_type: parse_task_type_opt(args.task_type.as_deref())?,
                status: match args.status {
                    Some(status) => Some(parse_task_status(&status)?),
                    None => None,
                },
                priority: parse_priority_opt(args.priority.as_deref())?,
                risk: None,
                assignee_type: args.assignee_type,
                tags: if args.tag.is_empty() {
                    None
                } else {
                    Some(args.tag)
                },
                linked_requirement: args.linked_requirement,
                linked_architecture_entity: args.linked_architecture_entity,
                search_text: args.search,
            };

            if filter.task_type.is_none()
                && filter.status.is_none()
                && filter.priority.is_none()
                && filter.risk.is_none()
                && filter.assignee_type.is_none()
                && filter.tags.is_none()
                && filter.linked_requirement.is_none()
                && filter.linked_architecture_entity.is_none()
                && filter.search_text.is_none()
            {
                print_value(tasks.list().await?, json)
            } else {
                print_value(tasks.list_filtered(filter).await?, json)
            }
        }
        TaskCommand::Prioritized => print_value(tasks.list_prioritized().await?, json),
        TaskCommand::Next => print_value(tasks.next_task().await?, json),
        TaskCommand::Stats(args) => {
            let task_list = tasks.list().await?;
            let stats = tasks.statistics().await?;
            let stale_in_progress =
                stale_in_progress_summary(&task_list, args.stale_threshold_hours, Utc::now());
            let priority_policy =
                evaluate_task_priority_policy(&task_list, DEFAULT_HIGH_PRIORITY_BUDGET_PERCENT)?;
            print_value(
                TaskStatsOutput {
                    stats,
                    stale_in_progress,
                    priority_policy,
                },
                json,
            )
        }
        TaskCommand::Get(args) => print_value(tasks.get(&args.id).await?, json),
        TaskCommand::Create(args) => {
            let input = parse_input_json_or(args.input_json, || {
                Ok(TaskCreateInput {
                    title: args.title,
                    description: args.description,
                    task_type: parse_task_type_opt(args.task_type.as_deref())?,
                    priority: parse_priority_opt(args.priority.as_deref())?,
                    created_by: Some("ao-cli".to_string()),
                    tags: Vec::new(),
                    linked_requirements: args.linked_requirement,
                    linked_architecture_entities: args.linked_architecture_entity,
                })
            })?;
            if should_warn_missing_linked_requirements(&input) {
                eprintln!("{UNLINKED_REQUIREMENTS_WARNING}");
            }
            print_value(tasks.create(input).await?, json)
        }
        TaskCommand::Update(args) => {
            let input = parse_input_json_or(args.input_json, || {
                Ok(TaskUpdateInput {
                    title: args.title,
                    description: args.description,
                    priority: parse_priority_opt(args.priority.as_deref())?,
                    status: match args.status {
                        Some(status) => Some(parse_task_status(&status)?),
                        None => None,
                    },
                    assignee: args.assignee,
                    tags: None,
                    updated_by: Some("ao-cli".to_string()),
                    deadline: None,
                    linked_architecture_entities: if args.replace_linked_architecture_entities
                        || !args.linked_architecture_entity.is_empty()
                    {
                        Some(args.linked_architecture_entity)
                    } else {
                        None
                    },
                })
            })?;
            print_value(tasks.update(&args.id, input).await?, json)
        }
        TaskCommand::Delete(args) => {
            let task = tasks.get(&args.id).await?;
            if args.dry_run {
                let task_id = task.id.clone();
                let task_title = task.title.clone();
                let task_status = task.status.clone();
                let task_paused = task.paused;
                let task_cancelled = task.cancelled;
                return print_value(
                    serde_json::json!({
                        "operation": "task.delete",
                        "target": {
                            "task_id": task_id.clone(),
                        },
                        "action": "task.delete",
                        "dry_run": true,
                        "destructive": true,
                        "requires_confirmation": true,
                        "planned_effects": [
                            "delete task from project state",
                        ],
                        "next_step": format!(
                            "rerun 'ao task delete --id {} --confirm {}' to apply",
                            task_id,
                            task_id
                        ),
                        "task": {
                            "id": task_id.clone(),
                            "title": task_title,
                            "status": task_status,
                            "paused": task_paused,
                            "cancelled": task_cancelled,
                        },
                    }),
                    json,
                );
            }

            ensure_destructive_confirmation(
                args.confirm.as_deref(),
                &args.id,
                "task delete",
                "--id",
            )?;
            tasks.delete(&args.id).await?;
            print_value(
                serde_json::json!({
                    "success": true,
                    "message": "task deleted",
                    "task_id": args.id,
                }),
                json,
            )
        }
        TaskCommand::Assign(args) => {
            print_value(tasks.assign(&args.id, args.assignee).await?, json)
        }
        TaskCommand::AssignAgent(args) => print_value(
            tasks
                .assign_agent(&args.id, args.role, args.model, args.updated_by)
                .await?,
            json,
        ),
        TaskCommand::AssignHuman(args) => print_value(
            tasks
                .assign_human(&args.id, args.user_id, args.updated_by)
                .await?,
            json,
        ),
        TaskCommand::ChecklistAdd(args) => print_value(
            tasks
                .add_checklist_item(&args.id, args.description, args.updated_by)
                .await?,
            json,
        ),
        TaskCommand::ChecklistUpdate(args) => print_value(
            tasks
                .update_checklist_item(&args.id, &args.item_id, args.completed, args.updated_by)
                .await?,
            json,
        ),
        TaskCommand::DependencyAdd(args) => {
            let dependency_type = parse_dependency_type(&args.dependency_type)?;
            print_value(
                tasks
                    .add_dependency(
                        &args.id,
                        &args.dependency_id,
                        dependency_type,
                        args.updated_by,
                    )
                    .await?,
                json,
            )
        }
        TaskCommand::DependencyRemove(args) => print_value(
            tasks
                .remove_dependency(&args.id, &args.dependency_id, args.updated_by)
                .await?,
            json,
        ),
        TaskCommand::Status(args) => {
            let status = parse_task_status(&args.status)?;
            print_value(
                set_task_status_with_assignee_inference(tasks.clone(), &args.id, status).await?,
                json,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::{Assignee, InMemoryServiceHub, Priority, TaskStatus};
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let previous = std::env::var(key).ok();
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn init_git_repo(path: &TempDir) {
        let init = ProcessCommand::new("git")
            .arg("init")
            .current_dir(path.path())
            .status()
            .expect("git init should run");
        assert!(init.success(), "git init should succeed");
    }

    fn git_config(path: &TempDir, key: &str, value: &str) {
        let status = ProcessCommand::new("git")
            .args(["config", "--local", key, value])
            .current_dir(path.path())
            .status()
            .expect("git config should run");
        assert!(status.success(), "git config should succeed");
    }

    fn task_create_input(
        task_type: Option<TaskType>,
        linked_requirements: Vec<&str>,
    ) -> TaskCreateInput {
        TaskCreateInput {
            title: "task".to_string(),
            description: String::new(),
            task_type,
            priority: None,
            created_by: None,
            tags: Vec::new(),
            linked_requirements: linked_requirements
                .into_iter()
                .map(str::to_string)
                .collect(),
            linked_architecture_entities: Vec::new(),
        }
    }

    #[test]
    fn warns_for_default_feature_tasks_without_links() {
        let input = task_create_input(None, Vec::new());
        assert!(should_warn_missing_linked_requirements(&input));
    }

    #[test]
    fn warns_for_non_chore_tasks_without_links() {
        let input = task_create_input(Some(TaskType::Feature), Vec::new());
        assert!(should_warn_missing_linked_requirements(&input));
    }

    #[test]
    fn does_not_warn_for_chore_tasks_without_links() {
        let input = task_create_input(Some(TaskType::Chore), Vec::new());
        assert!(!should_warn_missing_linked_requirements(&input));
    }

    #[test]
    fn does_not_warn_when_linked_requirements_are_present() {
        let input = task_create_input(Some(TaskType::Feature), vec!["REQ-123"]);
        assert!(!should_warn_missing_linked_requirements(&input));
    }

    #[test]
    fn warns_when_linked_requirements_are_blank() {
        let input = task_create_input(Some(TaskType::Feature), vec!["", "   "]);
        assert!(should_warn_missing_linked_requirements(&input));
    }

    #[test]
    fn does_not_warn_when_at_least_one_linked_requirement_is_non_blank() {
        let input = task_create_input(Some(TaskType::Feature), vec!["", "REQ-123", "   "]);
        assert!(!should_warn_missing_linked_requirements(&input));
    }

    #[test]
    fn infer_human_assignee_prefers_ao_assignee_user_id() {
        let _lock = env_lock().lock().expect("env lock should be available");
        let _ao_assignee = EnvVarGuard::set("AO_ASSIGNEE_USER_ID", Some("assignee-user"));
        let _ao_user = EnvVarGuard::set("AO_USER_ID", Some("ao-user"));
        let _user = EnvVarGuard::set("USER", Some("shell-user"));
        let _username = EnvVarGuard::set("USERNAME", Some("shell-username"));
        let _project_root = EnvVarGuard::set("PROJECT_ROOT", None);

        assert_eq!(
            infer_human_assignee_identity().as_deref(),
            Some("assignee-user")
        );
    }

    #[test]
    fn infer_human_assignee_prefers_git_identity_before_shell_user() {
        let _lock = env_lock().lock().expect("env lock should be available");
        let _ao_assignee = EnvVarGuard::set("AO_ASSIGNEE_USER_ID", None);
        let _ao_user = EnvVarGuard::set("AO_USER_ID", None);
        let _user = EnvVarGuard::set("USER", Some("shell-user"));
        let _username = EnvVarGuard::set("USERNAME", Some("shell-username"));

        let repo = TempDir::new().expect("temp dir should be created");
        init_git_repo(&repo);
        git_config(&repo, "user.email", "git-email@example.com");
        git_config(&repo, "user.name", "Git Name");
        let _project_root = EnvVarGuard::set("PROJECT_ROOT", repo.path().to_str());

        assert_eq!(
            infer_human_assignee_identity().as_deref(),
            Some("git-email@example.com")
        );
    }

    #[tokio::test]
    async fn set_task_status_in_progress_assigns_human_when_identity_is_available() {
        let _lock = env_lock().lock().expect("env lock should be available");
        let _ao_assignee = EnvVarGuard::set("AO_ASSIGNEE_USER_ID", Some("operator@example.com"));
        let _ao_user = EnvVarGuard::set("AO_USER_ID", None);
        let _project_root = EnvVarGuard::set("PROJECT_ROOT", None);

        let hub = Arc::new(InMemoryServiceHub::new());
        let created = hub
            .tasks()
            .create(TaskCreateInput {
                title: "status-assignee".to_string(),
                description: "auto assign on in-progress".to_string(),
                task_type: Some(TaskType::Feature),
                priority: Some(Priority::Medium),
                created_by: Some("test".to_string()),
                tags: Vec::new(),
                linked_requirements: Vec::new(),
                linked_architecture_entities: Vec::new(),
            })
            .await
            .expect("task should be created");

        let updated = set_task_status_with_assignee_inference(
            hub.tasks(),
            &created.id,
            TaskStatus::InProgress,
        )
        .await
        .expect("status update should succeed");
        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(
            updated.assignee,
            Assignee::Human {
                user_id: "operator@example.com".to_string()
            }
        );
        assert_eq!(updated.metadata.updated_by, "operator@example.com");
    }

    #[tokio::test]
    async fn set_task_status_in_progress_keeps_unassigned_when_identity_is_unavailable() {
        let _lock = env_lock().lock().expect("env lock should be available");
        let _ao_assignee = EnvVarGuard::set("AO_ASSIGNEE_USER_ID", None);
        let _ao_user = EnvVarGuard::set("AO_USER_ID", None);
        let _user = EnvVarGuard::set("USER", None);
        let _username = EnvVarGuard::set("USERNAME", None);
        let repo = TempDir::new().expect("temp dir should be created");
        let _project_root = EnvVarGuard::set("PROJECT_ROOT", repo.path().to_str());

        let hub = Arc::new(InMemoryServiceHub::new());
        let created = hub
            .tasks()
            .create(TaskCreateInput {
                title: "status-unassigned".to_string(),
                description: "keep unassigned when no identity".to_string(),
                task_type: Some(TaskType::Feature),
                priority: Some(Priority::Medium),
                created_by: Some("test".to_string()),
                tags: Vec::new(),
                linked_requirements: Vec::new(),
                linked_architecture_entities: Vec::new(),
            })
            .await
            .expect("task should be created");

        let updated = set_task_status_with_assignee_inference(
            hub.tasks(),
            &created.id,
            TaskStatus::InProgress,
        )
        .await
        .expect("status update should succeed");
        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(updated.assignee, Assignee::Unassigned);
    }

    #[tokio::test]
    async fn set_task_status_non_in_progress_does_not_assign_human() {
        let _lock = env_lock().lock().expect("env lock should be available");
        let _ao_assignee = EnvVarGuard::set("AO_ASSIGNEE_USER_ID", Some("operator@example.com"));
        let _project_root = EnvVarGuard::set("PROJECT_ROOT", None);

        let hub = Arc::new(InMemoryServiceHub::new());
        let created = hub
            .tasks()
            .create(TaskCreateInput {
                title: "status-ready".to_string(),
                description: "no auto-assign outside in-progress".to_string(),
                task_type: Some(TaskType::Feature),
                priority: Some(Priority::Medium),
                created_by: Some("test".to_string()),
                tags: Vec::new(),
                linked_requirements: Vec::new(),
                linked_architecture_entities: Vec::new(),
            })
            .await
            .expect("task should be created");

        let updated =
            set_task_status_with_assignee_inference(hub.tasks(), &created.id, TaskStatus::Ready)
                .await
                .expect("status update should succeed");
        assert_eq!(updated.status, TaskStatus::Ready);
        assert_eq!(updated.assignee, Assignee::Unassigned);
    }
}
