use chrono::NaiveTime;

use crate::{DaemonRuntimeOptions, ScheduleDispatch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTickPlan {
    pub within_active_hours: bool,
    pub should_process_due_schedules: bool,
    pub should_prepare_ready_tasks: bool,
    pub ready_dispatch_limit: usize,
}

impl ProjectTickPlan {
    pub fn build(
        options: &DaemonRuntimeOptions,
        active_hours: Option<&str>,
        now: NaiveTime,
        pool_draining: bool,
        requested_ready_dispatch_limit: usize,
    ) -> Self {
        let within_active_hours = ScheduleDispatch::allows_proactive_dispatch(active_hours, now);
        let should_process_due_schedules = within_active_hours;
        let should_prepare_ready_tasks = !pool_draining && options.auto_run_ready;
        let ready_dispatch_limit = if should_prepare_ready_tasks {
            requested_ready_dispatch_limit
        } else {
            0
        };

        Self {
            within_active_hours,
            should_process_due_schedules,
            should_prepare_ready_tasks,
            ready_dispatch_limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveTime;

    use super::ProjectTickPlan;
    use crate::DaemonRuntimeOptions;

    #[test]
    fn disables_schedule_dispatch_outside_active_hours() {
        let plan = ProjectTickPlan::build(
            &DaemonRuntimeOptions::default(),
            Some("09:00-17:00"),
            NaiveTime::from_hms_opt(8, 30, 0).expect("time should be valid"),
            false,
            2,
        );

        assert!(!plan.within_active_hours);
        assert!(!plan.should_process_due_schedules);
        assert!(plan.should_prepare_ready_tasks);
        assert_eq!(plan.ready_dispatch_limit, 2);
    }

    #[test]
    fn disables_ready_task_preparation_while_pool_is_draining() {
        let plan = ProjectTickPlan::build(
            &DaemonRuntimeOptions::default(),
            None,
            NaiveTime::from_hms_opt(12, 0, 0).expect("time should be valid"),
            true,
            3,
        );

        assert!(plan.within_active_hours);
        assert!(plan.should_process_due_schedules);
        assert!(!plan.should_prepare_ready_tasks);
        assert_eq!(plan.ready_dispatch_limit, 0);
    }

    #[test]
    fn disables_ready_task_preparation_when_auto_run_ready_is_off() {
        let plan = ProjectTickPlan::build(
            &DaemonRuntimeOptions {
                auto_run_ready: false,
                ..DaemonRuntimeOptions::default()
            },
            None,
            NaiveTime::from_hms_opt(12, 0, 0).expect("time should be valid"),
            false,
            4,
        );

        assert!(plan.within_active_hours);
        assert!(plan.should_process_due_schedules);
        assert!(!plan.should_prepare_ready_tasks);
        assert_eq!(plan.ready_dispatch_limit, 0);
    }
}
