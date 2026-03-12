use std::collections::HashMap;

use super::phase_executor::load_agent_runtime_config;
use super::phase_output::{build_workflow_pipeline_context, format_prior_phase_outputs, load_prior_phase_outputs};
use super::runtime_contract_builder::{
    load_phase_capabilities, phase_decision_contract_for, phase_output_contract_for,
    phase_system_prompt_for,
};
use serde::Serialize;
use serde_json::{Map, Value};

pub(super) const WORKFLOW_PHASE_PROMPT_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/prompts/runtime/workflow_phase.prompt"
));

#[derive(Debug, Clone, Default, Serialize)]
pub struct PhasePromptInputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rework_context: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub pipeline_vars: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_input: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedPhasePrompt {
    pub project_root: String,
    pub execution_cwd: String,
    pub workflow_id: String,
    pub subject_id: String,
    pub subject_title: String,
    pub subject_description: String,
    pub phase_id: String,
    pub inputs: PhasePromptInputs,
    pub capabilities: protocol::PhaseCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_output_contract: Option<orchestrator_core::PhaseOutputContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_decision_contract: Option<orchestrator_core::PhaseDecisionContract>,
    pub phase_directive: String,
    pub phase_action_rule: String,
    pub product_change_rule: String,
    pub phase_safety_rules: String,
    pub phase_decision_rule: String,
    pub structured_result_rule: String,
    pub pipeline_context: String,
    pub prior_phase_outputs: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    pub phase_prompt_body: String,
    pub final_prompt: String,
}

pub fn phase_directive_for(project_root: &str, phase_id: &str) -> String {
    let config = load_agent_runtime_config(project_root);
    config
        .phase_directive(phase_id)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            "Execute the current workflow phase with production-quality output.".to_string()
        })
}

pub fn build_phase_prompt(
    project_root: &str,
    execution_cwd: &str,
    workflow_id: &str,
    subject_id: &str,
    subject_title: &str,
    subject_description: &str,
    phase_id: &str,
    rework_context: Option<&str>,
    pipeline_vars: Option<&std::collections::HashMap<String, String>>,
) -> String {
    let dispatch_input = std::env::var("AO_DISPATCH_INPUT")
        .ok()
        .filter(|value| !value.is_empty());
    let schedule_input = std::env::var("AO_SCHEDULE_INPUT")
        .ok()
        .filter(|value| !value.is_empty());
    let inputs = PhasePromptInputs {
        rework_context: rework_context.map(ToOwned::to_owned),
        pipeline_vars: pipeline_vars.cloned().unwrap_or_default(),
        dispatch_input,
        schedule_input,
    };
    render_phase_prompt(
        project_root,
        execution_cwd,
        workflow_id,
        subject_id,
        subject_title,
        subject_description,
        phase_id,
        inputs,
    )
    .final_prompt
}

pub fn render_phase_prompt(
    project_root: &str,
    execution_cwd: &str,
    workflow_id: &str,
    subject_id: &str,
    subject_title: &str,
    subject_description: &str,
    phase_id: &str,
    inputs: PhasePromptInputs,
) -> RenderedPhasePrompt {
    let caps = load_phase_capabilities(project_root, phase_id);
    let phase_decision_contract = phase_decision_contract_for(project_root, phase_id);
    let phase_action_rule = if caps.writes_files {
        "Requirements:\n- Make concrete file changes in this repository."
    } else {
        "Requirements:\n- This is a READ-ONLY phase. Do NOT create, edit, or write any files. Do NOT run commands that modify the repository.\n- Read and analyze the codebase to assess the task. Your only output should be your assessment and phase decision."
    };
    let phase_contract = phase_output_contract_for(project_root, phase_id);
    let require_commit_message = phase_requires_commit_message_with_config(project_root, phase_id);
    let product_change_rule = if caps.enforce_product_changes {
        "- For this phase, changes must include product source/config/test files outside `.ao/` unless the task is explicitly documentation-only."
    } else {
        ""
    };
    let phase_directive = phase_directive_for(project_root, phase_id);
    let phase_safety_rules = phase_safety_rules(&caps);
    let decision_extra_field_rule = phase_decision_contract
        .as_ref()
        .map(phase_decision_extra_field_rule)
        .unwrap_or_default();
    let result_field_description_rule = phase_contract
        .as_ref()
        .map(phase_output_field_rule)
        .unwrap_or_default();
    let structured_result_rule = match (phase_contract.as_ref(), phase_decision_contract.as_ref()) {
        (Some(contract), Some(_)) => {
            let required_fields = if contract.required_fields.is_empty() {
                "- The top-level result object has no extra required fields beyond its kind."
                    .to_string()
            } else {
                format!(
                    "- The top-level result object must include these required fields: {}.",
                    contract
                        .required_fields
                        .iter()
                        .map(|field| format!("`{field}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let result_example = phase_result_example_for_prompt(
                contract,
                phase_id,
                phase_decision_contract.as_ref(),
            );
            format!(
                "- Before finishing, emit one JSON line as the FINAL line of output with your phase result and nested phase decision:\n  {}\n{}\n{}\n- Put any prose summary BEFORE the JSON line and emit nothing after it.",
                result_example,
                required_fields,
                result_field_description_rule,
            )
        }
        (Some(contract), None) => {
            let result_rule = if require_commit_message {
                format!(
                    "- Before finishing, emit one JSON line exactly like: {{\"kind\":\"{}\",\"commit_message\":\"<clear commit subject>\"}}.",
                    contract.kind
                )
            } else {
                format!(
                    "- Before finishing, emit one JSON line as the FINAL line of output with kind `{}`.",
                    contract.kind
                )
            };
            let required_fields = if contract.required_fields.is_empty() {
                String::new()
            } else {
                format!(
                    "\n- Include these required result fields: {}.",
                    contract
                        .required_fields
                        .iter()
                        .map(|field| format!("`{field}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!("{result_rule}{required_fields}\n{result_field_description_rule}")
        }
        (None, _) => String::new(),
    };
    let phase_decision_rule = if phase_contract.is_some() {
        if phase_decision_contract.is_some() {
            format!(
                "- The nested `phase_decision` object must describe whether this phase should advance, rework, fail, or skip.\n- Set `phase_decision.verdict` to `advance` if work is complete and correct.\n- Set `phase_decision.verdict` to `rework` if issues remain that need another pass.\n- Set `phase_decision.verdict` to `fail` only if problems are unrecoverable.\n- Set `phase_decision.verdict` to `skip` to close the task without further work. Use with a reason from: `already_done`, `duplicate`, `no_longer_valid`, `out_of_scope`.\n{}",
                decision_extra_field_rule
            )
        } else {
            String::new()
        }
    } else if let Some(contract) = phase_decision_contract.as_ref() {
        let required_evidence = if contract.required_evidence.is_empty() {
            "- Include evidence entries when they materially support your verdict.".to_string()
        } else {
            format!(
                "- Evidence must include these kinds when applicable: {}.",
                contract
                    .required_evidence
                    .iter()
                    .map(|kind| serde_json::to_string(kind)
                        .unwrap_or_else(|_| "\"custom\"".to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let missing_decision_rule = if contract.allow_missing_decision {
            ""
        } else {
            "\n- A missing phase_decision is invalid. Do not finish without emitting it."
        };
        let decision_example = phase_decision_example_for_prompt(phase_id, Some(contract));
        format!(
            "- Before finishing, emit one JSON line with your phase assessment as the FINAL line of output:\n  {}\n- Set verdict to \"advance\" if work is complete and correct.\n- Set verdict to \"rework\" if issues remain that need another pass.\n- Set verdict to \"fail\" only if problems are unrecoverable.\n- Set verdict to \"skip\" to close the task without further work. Use with a reason from: \"already_done\", \"duplicate\", \"no_longer_valid\", \"out_of_scope\".\n- Confidence must be at least {} unless you truly cannot justify a decision.\n- Risk must not exceed {:?} unless you are explicitly failing the phase.\n{}\n{}\n- Put any prose summary BEFORE the JSON line and emit nothing after it.{}",
            decision_example,
            contract.min_confidence,
            contract.max_risk,
            required_evidence,
            decision_extra_field_rule,
            missing_decision_rule
        )
    } else {
        String::new()
    };

    let (pipeline_context, phase_order) =
        build_workflow_pipeline_context(project_root, workflow_id, phase_id);
    let prior_outputs = load_prior_phase_outputs(project_root, workflow_id, phase_id, &phase_order);
    let prior_phase_context = format_prior_phase_outputs(&prior_outputs);
    let rework_context = inputs
        .rework_context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut prior_context = prior_phase_context;
    if let Some(context) = rework_context {
        prior_context.push_str("\n\nFailure context:\n");
        prior_context.push_str(context);
    }

    let mut phase_prompt = WORKFLOW_PHASE_PROMPT_TEMPLATE
        .replace("__PROJECT_ROOT__", project_root)
        .replace("__EXECUTION_CWD__", execution_cwd)
        .replace("__WORKFLOW_ID__", workflow_id)
        .replace("__SUBJECT_ID__", subject_id)
        .replace("__SUBJECT_TITLE__", subject_title)
        .replace("__SUBJECT_DESCRIPTION__", subject_description)
        .replace("__PHASE_ID__", phase_id)
        .replace("__PHASE_DIRECTIVE__", phase_directive.trim())
        .replace("__PHASE_ACTION_RULE__", phase_action_rule)
        .replace("__PRODUCT_CHANGE_RULE__", product_change_rule)
        .replace("__PHASE_SAFETY_RULES__", phase_safety_rules)
        .replace("__PHASE_DECISION_RULE__", &phase_decision_rule)
        .replace(
            "__IMPLEMENTATION_COMMIT_RULE__",
            structured_result_rule.as_str(),
        )
        .replace("__WORKFLOW_PIPELINE_CONTEXT__", &pipeline_context)
        .replace("__PRIOR_PHASE_OUTPUTS__", &prior_context);

    if !inputs.pipeline_vars.is_empty() {
        phase_prompt = orchestrator_core::workflow_config::expand_variables(
            &phase_prompt,
            &inputs.pipeline_vars,
        );
    }

    if let Some(dispatch_input) = inputs.dispatch_input.as_deref().filter(|value| !value.is_empty())
    {
        phase_prompt.push_str("\n\nDispatch input:\n");
        phase_prompt.push_str(dispatch_input);
    } else if let Some(schedule_input) =
        inputs.schedule_input.as_deref().filter(|value| !value.is_empty())
    {
        phase_prompt.push_str("\n\nSchedule trigger input:\n");
        phase_prompt.push_str(schedule_input);
    }

    let system_prompt = phase_system_prompt_for(project_root, phase_id).and_then(|prompt| {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            None
        } else if inputs.pipeline_vars.is_empty() {
            Some(prompt)
        } else {
            Some(orchestrator_core::workflow_config::expand_variables(
                &prompt,
                &inputs.pipeline_vars,
            ))
        }
    });
    let final_prompt = match system_prompt.as_deref() {
        Some(system_prompt) => format!("{system_prompt}\n\n{phase_prompt}"),
        None => phase_prompt.clone(),
    };

    RenderedPhasePrompt {
        project_root: project_root.to_string(),
        execution_cwd: execution_cwd.to_string(),
        workflow_id: workflow_id.to_string(),
        subject_id: subject_id.to_string(),
        subject_title: subject_title.to_string(),
        subject_description: subject_description.to_string(),
        phase_id: phase_id.to_string(),
        inputs,
        capabilities: caps,
        phase_output_contract: phase_contract,
        phase_decision_contract,
        phase_directive,
        phase_action_rule: phase_action_rule.to_string(),
        product_change_rule: product_change_rule.to_string(),
        phase_safety_rules: phase_safety_rules.to_string(),
        phase_decision_rule,
        structured_result_rule,
        pipeline_context,
        prior_phase_outputs: prior_context,
        system_prompt,
        phase_prompt_body: phase_prompt,
        final_prompt,
    }
}

fn phase_decision_example_for_prompt(
    phase_id: &str,
    contract: Option<&orchestrator_core::PhaseDecisionContract>,
) -> String {
    let mut object = Map::new();
    object.insert(
        "kind".to_string(),
        Value::String("phase_decision".to_string()),
    );
    object.insert("phase_id".to_string(), Value::String(phase_id.to_string()));
    object.insert(
        "verdict".to_string(),
        Value::String("advance|rework|fail|skip".to_string()),
    );
    object.insert("confidence".to_string(), serde_json::json!(0.95));
    object.insert(
        "risk".to_string(),
        Value::String("low|medium|high".to_string()),
    );
    object.insert("reason".to_string(), Value::String("...".to_string()));
    object.insert(
        "evidence".to_string(),
        Value::Array(vec![serde_json::json!({
            "kind": "...",
            "description": "..."
        })]),
    );
    if let Some(contract) = contract {
        for (field_name, field) in &contract.fields {
            object.insert(
                field_name.clone(),
                phase_field_placeholder(field_name, field),
            );
        }
    }
    serde_json::to_string(&Value::Object(object))
        .unwrap_or_else(|_| "{\"kind\":\"phase_decision\"}".to_string())
}

fn phase_result_example_for_prompt(
    contract: &orchestrator_core::PhaseOutputContract,
    phase_id: &str,
    decision_contract: Option<&orchestrator_core::PhaseDecisionContract>,
) -> String {
    let mut object = Map::new();
    object.insert("kind".to_string(), Value::String(contract.kind.clone()));
    for field_name in &contract.required_fields {
        object.insert(
            field_name.clone(),
            Value::String(format!("<{}>", field_name.replace('_', " "))),
        );
    }
    for (field_name, field) in &contract.fields {
        object.insert(
            field_name.clone(),
            phase_field_placeholder(field_name, field),
        );
    }
    object.insert(
        "phase_decision".to_string(),
        serde_json::from_str::<Value>(&phase_decision_example_for_prompt(
            phase_id,
            decision_contract,
        ))
        .unwrap_or_else(|_| Value::Object(Map::new())),
    );
    serde_json::to_string(&Value::Object(object)).unwrap_or_else(|_| {
        format!(
            "{{\"kind\":\"{}\",\"phase_decision\":{{\"kind\":\"phase_decision\"}}}}",
            contract.kind
        )
    })
}

fn phase_field_placeholder(
    field_name: &str,
    field: &orchestrator_core::agent_runtime_config::PhaseFieldDefinition,
) -> Value {
    match field.field_type.as_str() {
        "string" => field
            .enum_values
            .first()
            .map(|value| Value::String(value.clone()))
            .unwrap_or_else(|| Value::String(format!("<{}>", field_name.replace('_', " ")))),
        "number" => serde_json::json!(0.0),
        "integer" => serde_json::json!(0),
        "boolean" => Value::Bool(false),
        "array" => Value::Array(vec![field
            .items
            .as_ref()
            .map(|item| phase_field_placeholder(field_name, item))
            .unwrap_or_else(|| Value::String("...".to_string()))]),
        "object" => {
            let mut map = Map::new();
            for (nested_name, nested_field) in &field.fields {
                map.insert(
                    nested_name.clone(),
                    phase_field_placeholder(nested_name, nested_field),
                );
            }
            Value::Object(map)
        }
        _ => Value::String(format!("<{}>", field_name.replace('_', " "))),
    }
}

fn phase_output_field_rule(contract: &orchestrator_core::PhaseOutputContract) -> String {
    if contract.fields.is_empty() {
        return String::new();
    }

    let mut lines =
        vec!["- The top-level result object may include these config-defined fields:".to_string()];
    for (field_name, field) in &contract.fields {
        lines.push(format!(
            "  - `{field_name}` ({}){}{}",
            field.field_type,
            if field.required { ", required" } else { "" },
            field
                .description
                .as_deref()
                .map(|value| format!(": {value}"))
                .unwrap_or_default()
        ));
    }

    lines.join("\n")
}

fn phase_decision_extra_field_rule(contract: &orchestrator_core::PhaseDecisionContract) -> String {
    let mut lines = Vec::new();
    let mut required_fields = contract
        .fields
        .iter()
        .filter_map(|(field_name, field)| {
            if field.required {
                Some(field_name.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if let Some(schema) = contract.extra_json_schema.as_ref() {
        let extra_required = schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for field_name in extra_required {
            if !required_fields
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&field_name))
            {
                required_fields.push(field_name);
            }
        }
    }

    if !required_fields.is_empty() {
        lines.push(format!(
            "- The `phase_decision` object must also include these config-required fields: {}.",
            required_fields
                .iter()
                .map(|field| format!("`{field}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let mut optional_fields = contract
        .fields
        .keys()
        .filter(|field| !required_fields.iter().any(|required| required == *field))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(schema) = contract.extra_json_schema.as_ref() {
        let property_names = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for field_name in property_names {
            if !required_fields
                .iter()
                .any(|required| required == &field_name)
                && !optional_fields
                    .iter()
                    .any(|existing| existing == &field_name)
            {
                optional_fields.push(field_name);
            }
        }
    }
    let optional_fields = optional_fields.into_iter().collect::<Vec<_>>();
    if !optional_fields.is_empty() {
        lines.push(format!(
            "- The `phase_decision` object may include these additional config-defined fields when relevant: {}.",
            optional_fields
                .iter()
                .map(|field| format!("`{field}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if !contract.fields.is_empty() {
        lines.push("- Decision field descriptions:".to_string());
        for (field_name, field) in &contract.fields {
            let detail = field
                .description
                .as_deref()
                .unwrap_or("No description provided.");
            lines.push(format!(
                "  - `{field_name}` ({}){}: {}",
                field.field_type,
                if field.required { ", required" } else { "" },
                detail
            ));
        }
    }

    lines.join("\n")
}

pub(super) fn phase_safety_rules(caps: &protocol::PhaseCapabilities) -> &'static str {
    if caps.is_research {
        return "- For research phases, treat greenfield repositories as valid: missing app source files is not a blocker by itself.\n- Do targeted discovery only: inspect first-party code (`src/`, `apps/`, `db/`, `tests/`) and active `.ao` task/requirement docs; avoid broad recursive listings.\n- Do not scan dependency or checkpoint trees unless explicitly required: skip `node_modules/`, `.git/`, `.ao/workflow-state/checkpoints/`, and `.ao/runs/`.\n- If code context is limited, produce concrete assumptions, risks, and a build-ready plan in repository artifacts instead of stopping.";
    }

    ""
}

pub fn phase_requires_commit_message(phase_id: &str) -> bool {
    protocol::PhaseCapabilities::defaults_for_phase(phase_id).requires_commit
}

pub fn phase_requires_commit_message_with_config(project_root: &str, phase_id: &str) -> bool {
    phase_output_contract_for(project_root, phase_id)
        .map(|contract| contract.requires_field("commit_message"))
        .unwrap_or_else(|| phase_requires_commit_message(phase_id))
}

pub(super) fn phase_result_kind_for(project_root: &str, phase_id: &str) -> String {
    phase_output_contract_for(project_root, phase_id)
        .map(|contract| contract.kind)
        .filter(|kind| !kind.trim().is_empty())
        .unwrap_or_else(|| "implementation_result".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    struct EnvVarGuard {
        key: String,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: impl Into<String>, value: Option<&str>) -> Self {
            let key = key.into();
            let original = std::env::var(&key).ok();
            match value {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(&self.key, value),
                None => std::env::remove_var(&self.key),
            }
        }
    }

    fn write_prompt_test_config(project_root: &Path) {
        let mut workflow = orchestrator_core::builtin_workflow_config();
        let default_agent = orchestrator_core::builtin_agent_runtime_config()
            .agent_profile("default")
            .expect("default agent profile")
            .clone();
        workflow
            .agent_profiles
            .insert("default".to_string(), default_agent);
        let phase = workflow.phase_definitions.entry("implementation".to_string()).or_insert(
            orchestrator_core::PhaseExecutionDefinition {
                mode: orchestrator_core::PhaseExecutionMode::Agent,
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
            },
        );
        phase.directive = Some("Implement {{release_name}} safely.".to_string());
        phase.system_prompt = Some("System guidance for {{release_name}}.".to_string());
        orchestrator_core::write_workflow_config(project_root, &workflow).expect("write config");
    }

    #[test]
    fn triage_prompt_requires_final_phase_decision_line() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_root = temp.path().to_str().expect("project root");
        let prompt = build_phase_prompt(
            project_root,
            project_root,
            "wf-1",
            "TASK-1",
            "Task title",
            "Task description",
            "triage",
            None,
            None,
        );

        assert!(prompt.contains("FINAL line of output"));
        assert!(prompt.contains("\"kind\":\"phase_decision\""));
        assert!(prompt.contains("Put any prose summary BEFORE the JSON line"));
    }

    #[test]
    fn implementation_prompt_requires_nested_phase_decision_in_final_json() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_root = temp.path().to_str().expect("project root");
        let prompt = build_phase_prompt(
            project_root,
            project_root,
            "wf-1",
            "TASK-1",
            "Task title",
            "Task description",
            "implementation",
            None,
            None,
        );

        assert!(prompt.contains("\"phase_decision\":{"));
        assert!(prompt.contains("nested phase decision"));
        assert!(prompt.contains("FINAL line of output"));
    }

    #[test]
    fn decision_prompt_renders_configured_field_descriptions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_root = temp.path();
        let mut workflow = orchestrator_core::builtin_workflow_config();
        let default_agent = orchestrator_core::builtin_agent_runtime_config()
            .agent_profile("default")
            .expect("default agent profile")
            .clone();
        workflow
            .agent_profiles
            .insert("default".to_string(), default_agent);
        workflow.phase_definitions.insert(
            "triage".to_string(),
            orchestrator_core::PhaseExecutionDefinition {
                mode: orchestrator_core::PhaseExecutionMode::Agent,
                agent_id: Some("default".to_string()),
                directive: None,
                system_prompt: None,
                runtime: None,
                capabilities: None,
                output_contract: None,
                output_json_schema: None,
                decision_contract: Some(orchestrator_core::PhaseDecisionContract {
                    required_evidence: Vec::new(),
                    min_confidence: 0.6,
                    max_risk: orchestrator_core::WorkflowDecisionRisk::Medium,
                    allow_missing_decision: false,
                    extra_json_schema: None,
                    fields: std::collections::BTreeMap::from([(
                        "skip_reason".to_string(),
                        orchestrator_core::agent_runtime_config::PhaseFieldDefinition {
                            field_type: "string".to_string(),
                            required: false,
                            description: Some(
                                "When verdict is skip, explain why the task should stop."
                                    .to_string(),
                            ),
                            enum_values: vec![],
                            items: None,
                            fields: std::collections::BTreeMap::new(),
                        },
                    )]),
                }),
                retry: None,
                skills: Vec::new(),
                command: None,
                manual: None,
                default_tool: None,
            },
        );
        orchestrator_core::write_workflow_config(project_root, &workflow).expect("write config");
        let prompt = build_phase_prompt(
            project_root.to_str().expect("project root"),
            project_root.to_str().expect("project root"),
            "wf-1",
            "TASK-1",
            "Task title",
            "Task description",
            "triage",
            None,
            None,
        );

        assert!(prompt.contains("skip_reason"));
        assert!(prompt.contains("When verdict is skip, explain why the task should stop."));
    }

    #[test]
    fn decision_prompt_example_uses_numeric_confidence() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_root = temp.path().to_str().expect("project root");
        let prompt = build_phase_prompt(
            project_root,
            project_root,
            "wf-1",
            "TASK-1",
            "Task title",
            "Task description",
            "triage",
            None,
            None,
        );

        assert!(prompt.contains("\"confidence\":0.95"));
        assert!(!prompt.contains("\"confidence\":\"0.0-1.0\""));
    }

    #[test]
    fn structured_render_matches_build_phase_prompt_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_prompt_test_config(temp.path());
        let project_root = temp.path().to_str().expect("project root");
        let mut pipeline_vars = HashMap::new();
        pipeline_vars.insert("release_name".to_string(), "Mercury".to_string());
        let _dispatch_input =
            EnvVarGuard::set("AO_DISPATCH_INPUT", Some("{\"ticket\":\"REL-9\"}"));
        let _schedule_input =
            EnvVarGuard::set("AO_SCHEDULE_INPUT", Some("{\"window\":\"nightly\"}"));

        let rendered = render_phase_prompt(
            project_root,
            project_root,
            "wf-1",
            "TASK-1",
            "Task title",
            "Task description",
            "implementation",
            PhasePromptInputs {
                rework_context: Some("Fix the remaining release issues.".to_string()),
                pipeline_vars: pipeline_vars.clone(),
                dispatch_input: Some("{\"ticket\":\"REL-9\"}".to_string()),
                schedule_input: Some("{\"window\":\"nightly\"}".to_string()),
            },
        );
        let built = build_phase_prompt(
            project_root,
            project_root,
            "wf-1",
            "TASK-1",
            "Task title",
            "Task description",
            "implementation",
            Some("Fix the remaining release issues."),
            Some(&pipeline_vars),
        );

        assert_eq!(rendered.final_prompt, built);
    }

    #[test]
    fn structured_render_prefers_dispatch_input_over_schedule_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_root = temp.path().to_str().expect("project root");
        let rendered = render_phase_prompt(
            project_root,
            project_root,
            "wf-1",
            "schedule:nightly",
            "Nightly",
            "Nightly schedule",
            "implementation",
            PhasePromptInputs {
                rework_context: None,
                pipeline_vars: HashMap::new(),
                dispatch_input: Some("{\"source\":\"dispatch\"}".to_string()),
                schedule_input: Some("{\"source\":\"schedule\"}".to_string()),
            },
        );

        assert!(rendered.final_prompt.contains("Dispatch input:"));
        assert!(rendered.final_prompt.contains("{\"source\":\"dispatch\"}"));
        assert!(!rendered.final_prompt.contains("Schedule trigger input:"));
    }
}
