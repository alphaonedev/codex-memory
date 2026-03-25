// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Shared domain model for stored memories, search, prompt export, and stats.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    #[default]
    Fact,
    Preference,
    Constraint,
    Summary,
    Decision,
    Todo,
    Artifact,
}

impl std::fmt::Display for MemoryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Constraint => "constraint",
            Self::Summary => "summary",
            Self::Decision => "decision",
            Self::Todo => "todo",
            Self::Artifact => "artifact",
        };
        f.write_str(value)
    }
}

impl std::str::FromStr for MemoryKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fact" => Ok(Self::Fact),
            "preference" => Ok(Self::Preference),
            "constraint" => Ok(Self::Constraint),
            "summary" => Ok(Self::Summary),
            "decision" => Ok(Self::Decision),
            "todo" => Ok(Self::Todo),
            "artifact" => Ok(Self::Artifact),
            _ => Err(format!("unknown memory kind: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProjectScope {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: Uuid,
    pub content: String,
    pub summary: Option<String>,
    pub kind: MemoryKind,
    pub scope: String,
    pub source: String,
    pub tags: Vec<String>,
    pub priority: i64,
    pub confidence: f64,
    pub session: Option<String>,
    pub role: Option<String>,
    pub project_id: Option<String>,
    pub repo_root: Option<String>,
    pub git_branch: Option<String>,
    pub worktree: Option<String>,
    pub task_id: Option<String>,
    pub archived: bool,
    pub access_count: i64,
    pub reinforcement_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemory {
    pub content: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub kind: MemoryKind,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_priority")]
    pub priority: i64,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(flatten)]
    pub project: ProjectScope,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateMemory {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub summary: Option<Option<String>>,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub session: Option<Option<String>>,
    #[serde(default)]
    pub role: Option<Option<String>>,
    #[serde(default)]
    pub project_id: Option<Option<String>>,
    #[serde(default)]
    pub repo_root: Option<Option<String>>,
    #[serde(default)]
    pub git_branch: Option<Option<String>>,
    #[serde(default)]
    pub worktree: Option<Option<String>>,
    #[serde(default)]
    pub task_id: Option<Option<String>>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default)]
    pub expires_at: Option<Option<DateTime<Utc>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureMemory {
    #[serde(flatten)]
    pub memory: CreateMemory,
    #[serde(default = "default_capture_mode")]
    pub mode: CaptureMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReinforceMemory {
    #[serde(default = "default_reinforcement_delta")]
    pub delta: i64,
    #[serde(default)]
    pub confidence_boost: Option<f64>,
}

fn default_capture_mode() -> CaptureMode {
    CaptureMode::Upsert
}

fn default_reinforcement_delta() -> i64 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    Insert,
    Upsert,
    Reinforce,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchRequest {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub include_expired: bool,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(flatten)]
    pub project: ProjectScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub total: usize,
    pub memories: Vec<MemoryRecord>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptFormat {
    Json,
    Toon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    #[serde(flatten)]
    pub search: SearchRequest,
    #[serde(default = "default_format")]
    pub format: PromptFormat,
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
}

fn default_format() -> PromptFormat {
    PromptFormat::Toon
}

fn default_token_budget() -> usize {
    1500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSection {
    pub name: String,
    pub estimated_tokens: usize,
    pub memories: Vec<MemoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptBundle {
    pub total: usize,
    pub format: PromptFormat,
    pub estimated_tokens: usize,
    pub payload: String,
    pub sections: Vec<PromptSection>,
    pub memories: Vec<MemoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_memories: i64,
    pub active_memories: i64,
    pub expired_memories: i64,
    pub archived_memories: i64,
    pub sessions: i64,
    pub projects: i64,
    pub distinct_tags: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptIngestRequest {
    pub messages: Vec<TranscriptMessage>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(flatten)]
    pub project: ProjectScope,
    #[serde(default = "default_capture_mode")]
    pub mode: CaptureMode,
    #[serde(default = "default_max_ingested_memories")]
    pub max_memories: usize,
}

fn default_max_ingested_memories() -> usize {
    12
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptIngestResponse {
    pub extracted: usize,
    pub captured: usize,
    pub skipped: usize,
    pub memories: Vec<MemoryRecord>,
}

fn default_scope() -> String {
    "local".to_owned()
}

fn default_source() -> String {
    "manual".to_owned()
}

fn default_priority() -> i64 {
    50
}

fn default_confidence() -> f64 {
    0.7
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CaptureMemory, CaptureMode, MemoryKind, PromptFormat, PromptRequest, SearchRequest};

    #[test]
    fn memory_kind_round_trips_through_strings() {
        let kinds = [
            ("fact", MemoryKind::Fact),
            ("preference", MemoryKind::Preference),
            ("constraint", MemoryKind::Constraint),
            ("summary", MemoryKind::Summary),
            ("decision", MemoryKind::Decision),
            ("todo", MemoryKind::Todo),
            ("artifact", MemoryKind::Artifact),
        ];

        for (text, kind) in kinds {
            assert_eq!(text.parse::<MemoryKind>().expect("parse"), kind);
            assert_eq!(kind.to_string(), text);
        }
    }

    #[test]
    fn search_request_defaults_missing_fields() {
        let request: SearchRequest = serde_json::from_value(json!({ "query": "rust" })).expect("request");
        assert_eq!(request.query.as_deref(), Some("rust"));
        assert!(request.tags.is_empty());
        assert!(!request.include_expired);
        assert!(!request.include_archived);
    }

    #[test]
    fn prompt_request_defaults_to_toon() {
        let request: PromptRequest =
            serde_json::from_value(json!({ "query": "memory" })).expect("request");
        assert_eq!(request.format, PromptFormat::Toon);
        assert!(request.search.tags.is_empty());
        assert_eq!(request.token_budget, 1500);
    }

    #[test]
    fn capture_defaults_to_upsert() {
        let request: CaptureMemory = serde_json::from_value(json!({
            "content": "remember this"
        }))
        .expect("capture");
        assert_eq!(request.mode, CaptureMode::Upsert);
        assert_eq!(request.memory.kind, MemoryKind::Fact);
    }
}
