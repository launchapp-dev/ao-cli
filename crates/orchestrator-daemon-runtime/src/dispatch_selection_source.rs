use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchSelectionSource {
    EmQueue,
    FallbackPicker,
}

impl DispatchSelectionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmQueue => "em_queue",
            Self::FallbackPicker => "fallback_picker",
        }
    }
}
