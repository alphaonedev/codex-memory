// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Shared domain model for stored memories, search, prompt export, and stats.

use anyhow::{Result, bail};
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

impl CreateMemory {
    pub fn validate(&self) -> Result<()> {
        if self.content.trim().is_empty() {
            bail!("content must not be empty");
        }
        if self.scope.trim().is_empty() {
            bail!("scope must not be empty");
        }
        if self.source.trim().is_empty() {
            bail!("source must not be empty");
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            bail!("confidence must be between 0.0 and 1.0");
        }
        if self.priority < 0 || self.priority > 100 {
            bail!("priority must be between 0 and 100");
        }
        if let Some(summary) = &self.summary
            && summary.trim().is_empty()
        {
            bail!("summary must not be empty when provided");
        }
        Ok(())
    }
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

impl UpdateMemory {
    pub fn validate(&self) -> Result<()> {
        if let Some(content) = &self.content
            && content.trim().is_empty()
        {
            bail!("content must not be empty when provided");
        }
        if let Some(summary) = &self.summary
            && let Some(summary) = summary
            && summary.trim().is_empty()
        {
            bail!("summary must not be empty when provided");
        }
        if let Some(scope) = &self.scope
            && scope.trim().is_empty()
        {
            bail!("scope must not be empty when provided");
        }
        if let Some(source) = &self.source
            && source.trim().is_empty()
        {
            bail!("source must not be empty when provided");
        }
        if let Some(confidence) = self.confidence
            && !(0.0..=1.0).contains(&confidence)
        {
            bail!("confidence must be between 0.0 and 1.0");
        }
        if let Some(priority) = self.priority
            && !(0..=100).contains(&priority)
        {
            bail!("priority must be between 0 and 100");
        }
        Ok(())
    }
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

impl TranscriptMessage {
    pub fn validate(&self) -> Result<()> {
        if self.role.trim().is_empty() {
            bail!("transcript message role must not be empty");
        }
        if self.content.trim().is_empty() {
            bail!("transcript message content must not be empty");
        }
        Ok(())
    }
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

impl TranscriptIngestRequest {
    pub fn validate(&self) -> Result<()> {
        if self.messages.is_empty() {
            bail!("transcript ingest requires at least one message");
        }
        if self.source.trim().is_empty() {
            bail!("source must not be empty");
        }
        if self.max_memories == 0 {
            bail!("max_memories must be greater than zero");
        }
        for message in &self.messages {
            message.validate()?;
        }
        Ok(())
    }
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

    use super::{
        CaptureMemory, CaptureMode, CreateMemory, MemoryKind, PromptFormat, PromptRequest,
        SearchRequest, TranscriptIngestRequest, TranscriptMessage, UpdateMemory,
    };

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
        let request: SearchRequest =
            serde_json::from_value(json!({ "query": "rust" })).expect("request");
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

    #[test]
    fn create_memory_validation_rejects_empty_content() {
        let memory = CreateMemory {
            content: "   ".into(),
            summary: None,
            kind: MemoryKind::Fact,
            scope: "local".into(),
            source: "manual".into(),
            tags: Vec::new(),
            priority: 50,
            confidence: 0.7,
            session: None,
            role: None,
            project: Default::default(),
            expires_at: None,
        };
        assert!(memory.validate().is_err());
    }

    #[test]
    fn transcript_ingest_validation_rejects_empty_messages() {
        let request = TranscriptIngestRequest {
            messages: Vec::new(),
            source: "codex-session".into(),
            session: None,
            project: Default::default(),
            mode: CaptureMode::Upsert,
            max_memories: 12,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn update_validation_rejects_empty_summary() {
        let update = UpdateMemory {
            summary: Some(Some("   ".into())),
            ..UpdateMemory::default()
        };
        assert!(update.validate().is_err());
    }

    #[test]
    fn transcript_message_validation_rejects_blank_role() {
        let message = TranscriptMessage {
            role: " ".into(),
            content: "hello".into(),
        };
        assert!(message.validate().is_err());
    }
}
