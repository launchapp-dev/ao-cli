use super::{
    push_opt, SubjectCreateInput, SubjectGetInput, SubjectListInput, SubjectNextInput, SubjectStatusInput,
    SubjectUpdateInput,
};

pub(super) fn build_subject_list_args(input: &SubjectListInput) -> Vec<String> {
    let mut args = vec!["subject".to_string(), "list".to_string(), "--kind".to_string(), input.kind.clone()];
    push_opt(&mut args, "--status", input.status.clone());
    if let Some(limit) = input.limit {
        args.push("--limit".to_string());
        args.push(limit.to_string());
    }
    args
}

pub(super) fn build_subject_get_args(input: &SubjectGetInput) -> Vec<String> {
    vec![
        "subject".to_string(),
        "get".to_string(),
        "--kind".to_string(),
        input.kind.clone(),
        "--id".to_string(),
        input.id.clone(),
    ]
}

pub(super) fn build_subject_create_args(input: &SubjectCreateInput) -> Vec<String> {
    let mut args = vec![
        "subject".to_string(),
        "create".to_string(),
        "--kind".to_string(),
        input.kind.clone(),
        "--title".to_string(),
        input.title.clone(),
    ];
    push_opt(&mut args, "--status", input.status.clone());
    push_opt(&mut args, "--priority", input.priority.clone());
    if !input.labels.is_empty() {
        args.push("--labels".to_string());
        args.push(input.labels.join(","));
    }
    push_opt(&mut args, "--body", input.body.clone());
    args
}

pub(super) fn build_subject_update_args(input: &SubjectUpdateInput) -> Vec<String> {
    let mut args = vec![
        "subject".to_string(),
        "update".to_string(),
        "--kind".to_string(),
        input.kind.clone(),
        "--id".to_string(),
        input.id.clone(),
    ];
    push_opt(&mut args, "--status", input.status.clone());
    push_opt(&mut args, "--priority", input.priority.clone());
    if !input.labels.is_empty() {
        args.push("--labels".to_string());
        args.push(input.labels.join(","));
    }
    args
}

pub(super) fn build_subject_next_args(input: &SubjectNextInput) -> Vec<String> {
    vec!["subject".to_string(), "next".to_string(), "--kind".to_string(), input.kind.clone()]
}

pub(super) fn build_subject_status_args(input: &SubjectStatusInput) -> Vec<String> {
    vec![
        "subject".to_string(),
        "status".to_string(),
        "--kind".to_string(),
        input.kind.clone(),
        "--id".to_string(),
        input.id.clone(),
        "--status".to_string(),
        input.status.clone(),
    ]
}
