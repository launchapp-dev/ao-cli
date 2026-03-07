use std::collections::HashSet;

use crate::RequirementLifecycleTransition;

pub fn collect_requirement_lifecycle_transitions(
    before: &[orchestrator_core::RequirementItem],
    after: &[orchestrator_core::RequirementItem],
) -> Vec<RequirementLifecycleTransition> {
    let mut seen_comment_keys = HashSet::new();
    for requirement in before {
        for comment in &requirement.comments {
            let Some(phase) = comment
                .phase
                .as_deref()
                .and_then(normalize_requirement_lifecycle_phase)
            else {
                continue;
            };
            seen_comment_keys.insert(requirement_lifecycle_comment_key(
                &requirement.id,
                phase,
                &comment.content,
            ));
        }
    }

    let mut transitions = Vec::new();
    for requirement in after {
        for comment in &requirement.comments {
            let Some(phase) = comment
                .phase
                .as_deref()
                .and_then(normalize_requirement_lifecycle_phase)
            else {
                continue;
            };
            let key = requirement_lifecycle_comment_key(&requirement.id, phase, &comment.content);
            if seen_comment_keys.contains(&key) {
                continue;
            }
            transitions.push(RequirementLifecycleTransition {
                requirement_id: requirement.id.clone(),
                requirement_title: requirement.title.clone(),
                phase: phase.to_string(),
                status: requirement.status.to_string(),
                transition_at: comment.timestamp.to_rfc3339(),
                comment: {
                    let trimmed = comment.content.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                },
            });
        }
    }

    transitions.sort_by(|a, b| {
        a.transition_at
            .cmp(&b.transition_at)
            .then(a.requirement_id.cmp(&b.requirement_id))
            .then(a.phase.cmp(&b.phase))
    });
    transitions
}

fn normalize_requirement_lifecycle_phase(phase: &str) -> Option<&'static str> {
    match phase.trim().to_ascii_lowercase().as_str() {
        "refine" | "refined" => Some("refine"),
        "po-review" | "po_review" | "poreview" => Some("po-review"),
        "em-review" | "em_review" | "emreview" => Some("em-review"),
        "rework" | "needs-rework" | "needs_rework" => Some("rework"),
        "research" => Some("research"),
        "approved" => Some("approved"),
        _ => None,
    }
}

fn requirement_lifecycle_comment_key(requirement_id: &str, phase: &str, content: &str) -> String {
    format!(
        "{}|{}|{}",
        requirement_id,
        phase.trim().to_ascii_lowercase(),
        content.trim().to_ascii_lowercase()
    )
}
