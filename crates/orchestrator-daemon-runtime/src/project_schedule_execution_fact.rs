use crate::{ScheduleDispatch, SubjectExecutionFact};

pub(crate) fn project_schedule_execution_fact(root: &str, fact: &SubjectExecutionFact) {
    if let Some(schedule_id) = fact.schedule_id.as_deref() {
        ScheduleDispatch::update_completion_state(root, schedule_id, fact.completion_status());
    }
}
