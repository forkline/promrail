use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewArtifactStatus {
    Pending,
    Classified,
    Applied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewItemKind {
    NewComponent,
    ConflictingFile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Promote,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReviewSummary {
    pub auto_files: usize,
    pub retained_files: usize,
    pub review_items: usize,
    pub new_components: usize,
    pub conflicting_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewItem {
    pub id: String,
    pub kind: ReviewItemKind,
    pub component: String,
    pub files: Vec<String>,
    pub candidate_sources: Vec<String>,
    pub reason: String,
    pub decision: Option<ReviewDecision>,
    pub selected_source: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewArtifact {
    pub version: u32,
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: ReviewArtifactStatus,
    pub sources: Vec<String>,
    pub dest: String,
    pub filters: Vec<String>,
    pub route_key: String,
    pub fingerprint: String,
    pub summary: ReviewSummary,
    pub items: Vec<ReviewItem>,
}
