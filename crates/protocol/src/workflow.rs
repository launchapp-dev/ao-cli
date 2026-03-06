use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowDecisionRisk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseEvidenceKind {
    TestsPassed,
    TestsFailed,
    CodeReviewClean,
    CodeReviewIssues,
    FilesModified,
    RequirementsMet,
    ResearchComplete,
    ManualVerification,
    #[serde(other)]
    Custom,
}
