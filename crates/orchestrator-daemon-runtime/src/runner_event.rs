#[derive(Debug, Clone, serde::Deserialize)]
pub struct RunnerEvent {
    pub event: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub pipeline: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}
