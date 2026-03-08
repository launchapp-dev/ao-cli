use std::sync::Arc;

use anyhow::{anyhow, Result};
use orchestrator_core::{services::ServiceHub, workflow_ref_for_task, STANDARD_WORKFLOW_REF};
use orchestrator_daemon_runtime::{
    enqueue_subject_dispatch, hold_subject, queue_snapshot, queue_stats, release_subject,
    reorder_subjects,
};
use protocol::SubjectDispatch;

use crate::{print_ok, print_value, QueueCommand};

pub(crate) async fn handle_queue(
    command: QueueCommand,
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    match command {
        QueueCommand::List => print_value(queue_snapshot(project_root)?, json),
        QueueCommand::Stats => print_value(queue_stats(project_root)?, json),
        QueueCommand::Enqueue(args) => {
            let input = args
                .input_json
                .map(|value| serde_json::from_str(&value))
                .transpose()?;
            let dispatch = match (args.task_id, args.requirement_id, args.title) {
                (Some(task_id), None, None) => {
                    let task = hub.tasks().get(&task_id).await?;
                    let workflow_ref = args
                        .workflow_ref
                        .unwrap_or_else(|| workflow_ref_for_task(&task));
                    SubjectDispatch::for_task_with_metadata(
                        task.id.clone(),
                        workflow_ref,
                        "manual-queue-enqueue",
                        chrono::Utc::now(),
                    )
                    .with_input(input)
                }
                (None, Some(requirement_id), None) => {
                    hub.planning().get_requirement(&requirement_id).await?;
                    SubjectDispatch::for_requirement(
                        requirement_id,
                        args.workflow_ref
                            .unwrap_or_else(|| STANDARD_WORKFLOW_REF.to_string()),
                        "manual-queue-enqueue",
                    )
                    .with_input(input)
                }
                (None, None, Some(title)) => SubjectDispatch::for_custom(
                    title,
                    args.description.unwrap_or_default(),
                    args.workflow_ref
                        .unwrap_or_else(|| STANDARD_WORKFLOW_REF.to_string()),
                    input,
                    "manual-queue-enqueue",
                ),
                (None, None, None) => {
                    return Err(anyhow!(
                        "one of --task-id, --requirement-id, or --title must be provided"
                    ));
                }
                _ => {
                    return Err(anyhow!(
                        "--task-id, --requirement-id, and --title are mutually exclusive"
                    ));
                }
            };
            let result = enqueue_subject_dispatch(project_root, dispatch)?;
            if !json {
                if result.enqueued {
                    print_ok("subject dispatch enqueued", false);
                    return Ok(());
                }
                print_ok("subject dispatch already queued", false);
                return Ok(());
            }
            print_value(result, true)
        }
        QueueCommand::Hold(args) => {
            let held = hold_subject(project_root, &args.subject_id)?;
            if !json {
                if held {
                    print_ok("queue subject held", false);
                    return Ok(());
                }
                return Err(anyhow!("queue subject not found or not pending"));
            }
            print_value(
                serde_json::json!({ "held": held, "subject_id": args.subject_id }),
                true,
            )
        }
        QueueCommand::Release(args) => {
            let released = release_subject(project_root, &args.subject_id)?;
            if !json {
                if released {
                    print_ok("queue subject released", false);
                    return Ok(());
                }
                return Err(anyhow!("queue subject not found or not held"));
            }
            print_value(
                serde_json::json!({ "released": released, "subject_id": args.subject_id }),
                true,
            )
        }
        QueueCommand::Reorder(args) => {
            let reordered = reorder_subjects(project_root, args.subject_ids.clone())?;
            if !json {
                if reordered {
                    print_ok("queue reordered", false);
                    return Ok(());
                }
                print_ok("queue order unchanged", false);
                return Ok(());
            }
            print_value(
                serde_json::json!({ "reordered": reordered, "subject_ids": args.subject_ids }),
                true,
            )
        }
    }
}
