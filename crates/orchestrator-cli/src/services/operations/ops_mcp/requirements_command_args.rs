use super::{push_opt, RequirementCreateInput, RequirementUpdateInput};

pub(super) fn build_requirements_get_args(id: String) -> Vec<String> {
    vec![
        "requirements".to_string(),
        "get".to_string(),
        "--id".to_string(),
        id,
    ]
}

pub(super) fn build_requirements_create_args(input: &RequirementCreateInput) -> Vec<String> {
    let mut args = vec![
        "requirements".to_string(),
        "create".to_string(),
        "--title".to_string(),
        input.title.clone(),
        "--description".to_string(),
        input.description.clone().unwrap_or_default(),
    ];
    push_opt(&mut args, "--priority", input.priority.clone());
    push_opt(&mut args, "--category", input.category.clone());
    push_opt(&mut args, "--type", input.requirement_type.clone());
    push_opt(&mut args, "--source", input.source.clone());
    for criterion in &input.acceptance_criterion {
        args.push("--acceptance-criterion".to_string());
        args.push(criterion.clone());
    }
    push_opt(&mut args, "--input-json", input.input_json.clone());
    args
}

pub(super) fn build_requirements_update_args(input: &RequirementUpdateInput) -> Vec<String> {
    let mut args = vec![
        "requirements".to_string(),
        "update".to_string(),
        "--id".to_string(),
        input.id.clone(),
    ];
    push_opt(&mut args, "--title", input.title.clone());
    push_opt(&mut args, "--description", input.description.clone());
    push_opt(&mut args, "--priority", input.priority.clone());
    push_opt(&mut args, "--status", input.status.clone());
    push_opt(&mut args, "--category", input.category.clone());
    push_opt(&mut args, "--type", input.requirement_type.clone());
    push_opt(&mut args, "--source", input.source.clone());
    for task_id in &input.linked_task_id {
        args.push("--linked-task-id".to_string());
        args.push(task_id.clone());
    }
    for criterion in &input.acceptance_criterion {
        args.push("--acceptance-criterion".to_string());
        args.push(criterion.clone());
    }
    if input.replace_acceptance_criteria {
        args.push("--replace-acceptance-criteria".to_string());
    }
    push_opt(&mut args, "--input-json", input.input_json.clone());
    args
}

pub(super) fn build_requirements_delete_args(id: String) -> Vec<String> {
    vec![
        "requirements".to_string(),
        "delete".to_string(),
        "--id".to_string(),
        id,
    ]
}

