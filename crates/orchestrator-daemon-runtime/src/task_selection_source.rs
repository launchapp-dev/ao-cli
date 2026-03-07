use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskSelectionSource {
    EmQueue,
    FallbackPicker,
}

impl TaskSelectionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmQueue => "em_queue",
            Self::FallbackPicker => "fallback_picker",
        }
    }
}
