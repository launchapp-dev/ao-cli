use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use orchestrator_core::{OrchestratorWorkflow, WorkflowPhaseStatus, WorkflowStatus};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub(super) enum OutputStreamType {
    Stdout,
    Stderr,
    System,
}

pub(super) struct OutputLine {
    pub text: String,
    pub stream_type: OutputStreamType,
    pub is_json: bool,
}

pub(super) struct WorkflowMonitorState {
    pub workflows: Vec<OrchestratorWorkflow>,
    pub selected_idx: usize,
    pub output_buffer: VecDeque<OutputLine>,
    pub scroll_lock: bool,
    pub scroll_offset: usize,
    pub last_refresh: DateTime<Utc>,
    pub status_line: String,
    pub filter: String,
    pub filter_mode: bool,
    pub buffer_limit: usize,
}

impl WorkflowMonitorState {
    pub fn new(buffer_limit: usize) -> Self {
        Self {
            workflows: Vec::new(),
            selected_idx: 0,
            output_buffer: VecDeque::new(),
            scroll_lock: true,
            scroll_offset: 0,
            last_refresh: Utc::now(),
            status_line: "Loading workflows...".to_string(),
            filter: String::new(),
            filter_mode: false,
            buffer_limit,
        }
    }

    pub fn push_output(&mut self, text: String, stream_type: OutputStreamType) {
        if self.output_buffer.len() >= self.buffer_limit {
            self.output_buffer.pop_front();
        }
        let is_json = serde_json::from_str::<serde_json::Value>(&text).is_ok();
        self.output_buffer
            .push_back(OutputLine { text, stream_type, is_json });
    }

    pub fn clear_output(&mut self) {
        self.output_buffer.clear();
        self.scroll_offset = 0;
    }

    pub fn filtered_workflows(&self) -> Vec<&OrchestratorWorkflow> {
        if self.filter.is_empty() {
            self.workflows.iter().collect()
        } else {
            let filter_lower = self.filter.to_ascii_lowercase();
            self.workflows
                .iter()
                .filter(|w| {
                    w.id.to_ascii_lowercase().contains(&filter_lower)
                        || w.task_id.to_ascii_lowercase().contains(&filter_lower)
                })
                .collect()
        }
    }

    pub fn selected_workflow(&self) -> Option<&OrchestratorWorkflow> {
        self.filtered_workflows().get(self.selected_idx).copied()
    }

    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.filtered_workflows().len().saturating_sub(1);
        if self.selected_idx < max {
            self.selected_idx += 1;
        }
    }

    pub fn clamp_selection(&mut self) {
        let len = self.filtered_workflows().len();
        if len == 0 {
            self.selected_idx = 0;
        } else if self.selected_idx >= len {
            self.selected_idx = len - 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            self.scroll_lock = false;
        }
    }

    pub fn scroll_down(&mut self, max_lines: usize) {
        if self.scroll_offset + 1 < max_lines {
            self.scroll_offset += 1;
        }
    }
}

pub(super) fn workflow_status_icon(status: WorkflowStatus) -> &'static str {
    match status {
        WorkflowStatus::Pending => "○",
        WorkflowStatus::Running => "◐",
        WorkflowStatus::Paused => "⏸",
        WorkflowStatus::Completed => "●",
        WorkflowStatus::Failed => "✗",
        WorkflowStatus::Cancelled => "⊘",
    }
}

pub(super) fn phase_status_icon(status: WorkflowPhaseStatus) -> &'static str {
    match status {
        WorkflowPhaseStatus::Pending => "○",
        WorkflowPhaseStatus::Ready => "◌",
        WorkflowPhaseStatus::Running => "◐",
        WorkflowPhaseStatus::Success => "●",
        WorkflowPhaseStatus::Failed => "✗",
        WorkflowPhaseStatus::Skipped => "–",
    }
}
