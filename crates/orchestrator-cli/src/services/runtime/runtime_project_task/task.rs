use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::{
    services::ServiceHub, Complexity, ImpactArea, OrchestratorTask, RiskLevel, Scope,
    TaskCreateInput, TaskFilter, TaskUpdateInput,
};

use crate::{
    ensure_destructive_confirmation, invalid_input_error, parse_complexity_opt,
    parse_dependency_type, parse_impact_areas, parse_input_json_or, parse_priority_opt,
    parse_risk_opt, parse_scope_opt, parse_task_status, parse_task_type_opt, print_value,
    TaskCommand, TaskCreateArgs, TaskUpdateArgs,
};

#[derive(Debug, Default)]
struct TaskFieldPatch {
    risk: Option<RiskLevel>,
    scope: Option<Scope>,
    complexity: Option<Complexity>,
    impact_area: Option<Vec<ImpactArea>>,
    estimated_effort: Option<String>,
    max_cpu_percent: Option<f32>,
    max_memory_mb: Option<u64>,
    requires_network: Option<bool>,
    clear_max_cpu_percent: bool,
    clear_max_memory_mb: bool,
}

impl TaskFieldPatch {
    fn has_updates(&self) -> bool {
        self.risk.is_some()
            || self.scope.is_some()
            || self.complexity.is_some()
            || self.impact_area.is_some()
            || self.estimated_effort.is_some()
            || self.max_cpu_percent.is_some()
            || self.max_memory_mb.is_some()
            || self.requires_network.is_some()
            || self.clear_max_cpu_percent
            || self.clear_max_memory_mb
    }

    fn apply(self, task: &mut OrchestratorTask) {
        if let Some(risk) = self.risk {
            task.risk = risk;
        }
        if let Some(scope) = self.scope {
            task.scope = scope;
        }
        if let Some(complexity) = self.complexity {
            task.complexity = complexity;
        }
        if let Some(impact_area) = self.impact_area {
            task.impact_area = impact_area;
        }
        if let Some(estimated_effort) = self.estimated_effort {
            task.estimated_effort = if estimated_effort.trim().is_empty() {
                None
            } else {
                Some(estimated_effort)
            };
        }
        if self.clear_max_cpu_percent {
            task.resource_requirements.max_cpu_percent = None;
        }
        if self.clear_max_memory_mb {
            task.resource_requirements.max_memory_mb = None;
        }
        if let Some(max_cpu_percent) = self.max_cpu_percent {
            task.resource_requirements.max_cpu_percent = Some(max_cpu_percent);
        }
        if let Some(max_memory_mb) = self.max_memory_mb {
            task.resource_requirements.max_memory_mb = Some(max_memory_mb);
        }
        if let Some(requires_network) = self.requires_network {
            task.resource_requirements.requires_network = requires_network;
        }
    }
}

fn validate_max_cpu_percent(max_cpu_percent: Option<f32>) -> Result<Option<f32>> {
    let Some(value) = max_cpu_percent else {
        return Ok(None);
    };

    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(invalid_input_error(format!(
            "invalid --max-cpu-percent '{value}'; expected a number between 0 and 100; run the same command with --help"
        )));
    }

    Ok(Some(value))
}

fn build_create_field_patch(args: &TaskCreateArgs) -> Result<TaskFieldPatch> {
    let impact_area = if args.impact_area.is_empty() {
        None
    } else {
        Some(parse_impact_areas(&args.impact_area)?)
    };

    Ok(TaskFieldPatch {
        risk: parse_risk_opt(args.risk.as_deref())?,
        scope: parse_scope_opt(args.scope.as_deref())?,
        complexity: parse_complexity_opt(args.complexity.as_deref())?,
        impact_area,
        estimated_effort: args.estimated_effort.clone(),
        max_cpu_percent: validate_max_cpu_percent(args.max_cpu_percent)?,
        max_memory_mb: args.max_memory_mb,
        requires_network: args.requires_network,
        clear_max_cpu_percent: false,
        clear_max_memory_mb: false,
    })
}

fn build_update_field_patch(args: &TaskUpdateArgs) -> Result<TaskFieldPatch> {
    if args.clear_max_cpu_percent && args.max_cpu_percent.is_some() {
        return Err(invalid_input_error(
            "cannot combine --max-cpu-percent with --clear-max-cpu-percent",
        ));
    }
    if args.clear_max_memory_mb && args.max_memory_mb.is_some() {
        return Err(invalid_input_error(
            "cannot combine --max-memory-mb with --clear-max-memory-mb",
        ));
    }

    let impact_area = if args.replace_impact_area || !args.impact_area.is_empty() {
        Some(parse_impact_areas(&args.impact_area)?)
    } else {
        None
    };

    Ok(TaskFieldPatch {
        risk: parse_risk_opt(args.risk.as_deref())?,
        scope: parse_scope_opt(args.scope.as_deref())?,
        complexity: parse_complexity_opt(args.complexity.as_deref())?,
        impact_area,
        estimated_effort: args.estimated_effort.clone(),
        max_cpu_percent: validate_max_cpu_percent(args.max_cpu_percent)?,
        max_memory_mb: args.max_memory_mb,
        requires_network: args.requires_network,
        clear_max_cpu_percent: args.clear_max_cpu_percent,
        clear_max_memory_mb: args.clear_max_memory_mb,
    })
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
                risk: parse_risk_opt(args.risk.as_deref())?,
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
        TaskCommand::Stats => print_value(tasks.statistics().await?, json),
        TaskCommand::Get(args) => print_value(tasks.get(&args.id).await?, json),
        TaskCommand::Create(args) => {
            let field_patch = build_create_field_patch(&args)?;
            let input = parse_input_json_or(args.input_json, || {
                Ok(TaskCreateInput {
                    title: args.title,
                    description: args.description,
                    task_type: parse_task_type_opt(args.task_type.as_deref())?,
                    priority: parse_priority_opt(args.priority.as_deref())?,
                    created_by: Some("ao-cli".to_string()),
                    tags: Vec::new(),
                    linked_requirements: Vec::new(),
                    linked_architecture_entities: args.linked_architecture_entity,
                })
            })?;
            let mut task = tasks.create(input).await?;
            if field_patch.has_updates() {
                field_patch.apply(&mut task);
                task = tasks.replace(task).await?;
            }
            print_value(task, json)
        }
        TaskCommand::Update(args) => {
            let field_patch = build_update_field_patch(&args)?;
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
            let mut task = tasks.update(&args.id, input).await?;
            if field_patch.has_updates() {
                field_patch.apply(&mut task);
                task = tasks.replace(task).await?;
            }
            print_value(task, json)
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
            print_value(tasks.set_status(&args.id, status).await?, json)
        }
    }
}
