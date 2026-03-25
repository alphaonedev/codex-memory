// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// SQLite-backed memory persistence, hybrid retrieval, prompt assembly, and stats.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::ingest::expand_query;
use crate::model::{
    CaptureMemory, CaptureMode, CreateMemory, MemoryKind, MemoryRecord, PromptBundle,
    PromptFormat, PromptRequest, PromptSection, ReinforceMemory, SearchRequest, SearchResponse,
    Stats, UpdateMemory,
};
use crate::toon;

#[derive(Clone)]
pub struct MemoryStore {
    connection: Arc<Mutex<Connection>>,
    default_limit: usize,
    max_limit: usize,
}

#[derive(Debug, Clone)]
struct RankedMemory {
    score: f64,
    memory: MemoryRecord,
}

impl MemoryStore {
    pub fn open(path: &Path, default_limit: usize, max_limit: usize) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            default_limit,
            max_limit,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn insert(&self, input: CreateMemory) -> Result<MemoryRecord> {
        let record = self.build_record(input);
        self.insert_record(&record)?;
        self.get(record.id)?
            .ok_or_else(|| anyhow!("memory disappeared after insert"))
    }

    pub fn capture(&self, capture: CaptureMemory) -> Result<MemoryRecord> {
        match capture.mode {
            CaptureMode::Insert => self.insert(capture.memory),
            CaptureMode::Reinforce => {
                if let Some(existing) = self.find_duplicate(&capture.memory)? {
                    self.reinforce(
                        existing.id,
                        ReinforceMemory {
                            delta: 1,
                            confidence_boost: Some(0.05),
                        },
                    )?
                    .ok_or_else(|| anyhow!("memory disappeared after reinforce"))
                } else {
                    self.insert(capture.memory)
                }
            }
            CaptureMode::Upsert => {
                if let Some(existing) = self.find_duplicate(&capture.memory)? {
                    self.update(
                        existing.id,
                        UpdateMemory {
                            summary: capture
                                .memory
                                .summary
                                .map(Some)
                                .or(Some(existing.summary.clone())),
                            tags: Some(merge_tags(existing.tags.clone(), capture.memory.tags)),
                            priority: Some(existing.priority.max(capture.memory.priority)),
                            confidence: Some(
                                (existing.confidence + capture.memory.confidence).clamp(0.0, 1.0),
                            ),
                            content: Some(prefer_longer(existing.content.clone(), capture.memory.content)),
                            source: Some(capture.memory.source),
                            expires_at: Some(capture.memory.expires_at.or(existing.expires_at)),
                            ..UpdateMemory::default()
                        },
                    )?
                    .ok_or_else(|| anyhow!("memory disappeared after update"))
                } else {
                    self.insert(capture.memory)
                }
            }
        }
    }

    pub fn get(&self, id: Uuid) -> Result<Option<MemoryRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(&format!("{} WHERE id = ?1", select_sql()))?;
        let row = stmt.query_row([id.to_string()], map_memory).optional()?;
        Ok(row)
    }

    pub fn update(&self, id: Uuid, changes: UpdateMemory) -> Result<Option<MemoryRecord>> {
        let Some(current) = self.get(id)? else {
            return Ok(None);
        };

        let updated = MemoryRecord {
            id: current.id,
            content: changes.content.unwrap_or(current.content),
            summary: changes.summary.unwrap_or(current.summary),
            kind: changes.kind.unwrap_or(current.kind),
            scope: changes.scope.unwrap_or(current.scope),
            source: changes.source.unwrap_or(current.source),
            tags: changes.tags.map(normalize_tags).unwrap_or(current.tags),
            priority: changes.priority.unwrap_or(current.priority).clamp(0, 100),
            confidence: changes
                .confidence
                .unwrap_or(current.confidence)
                .clamp(0.0, 1.0),
            session: changes.session.unwrap_or(current.session),
            role: changes.role.unwrap_or(current.role),
            project_id: changes.project_id.unwrap_or(current.project_id),
            repo_root: changes.repo_root.unwrap_or(current.repo_root),
            git_branch: changes.git_branch.unwrap_or(current.git_branch),
            worktree: changes.worktree.unwrap_or(current.worktree),
            task_id: changes.task_id.unwrap_or(current.task_id),
            archived: changes.archived.unwrap_or(current.archived),
            access_count: current.access_count,
            reinforcement_count: current.reinforcement_count,
            created_at: current.created_at,
            updated_at: Utc::now(),
            last_accessed_at: current.last_accessed_at,
            expires_at: changes.expires_at.unwrap_or(current.expires_at),
        };

        self.replace_record(&updated)?;
        self.get(id)
    }

    pub fn archive(&self, id: Uuid, archived: bool) -> Result<Option<MemoryRecord>> {
        self.update(
            id,
            UpdateMemory {
                archived: Some(archived),
                ..UpdateMemory::default()
            },
        )
    }

    pub fn reinforce(&self, id: Uuid, reinforcement: ReinforceMemory) -> Result<Option<MemoryRecord>> {
        let Some(current) = self.get(id)? else {
            return Ok(None);
        };
        let now = Utc::now();
        let updated = MemoryRecord {
            updated_at: now,
            last_accessed_at: Some(now),
            access_count: current.access_count + 1,
            reinforcement_count: current.reinforcement_count + reinforcement.delta.max(1),
            confidence: (current.confidence + reinforcement.confidence_boost.unwrap_or(0.03))
                .clamp(0.0, 1.0),
            priority: (current.priority + reinforcement.delta).clamp(0, 100),
            ..current
        };
        self.replace_record(&updated)?;
        self.get(id)
    }

    pub fn list(&self, request: &SearchRequest) -> Result<SearchResponse> {
        let limit = self.resolve_limit(request.limit);
        let mut memories = self.fetch_candidates(request, None)?;
        memories.sort_by(compare_ranked);
        let memories = memories.into_iter().take(limit).map(|entry| entry.memory).collect::<Vec<_>>();
        self.mark_accessed(&memories)?;
        Ok(SearchResponse {
            total: memories.len(),
            memories,
        })
    }

    pub fn search(&self, request: &SearchRequest) -> Result<SearchResponse> {
        let limit = self.resolve_limit(request.limit);
        let query = request.query.as_deref().unwrap_or_default().trim();
        if query.is_empty() {
            return self.list(request);
        }

        let mut memories = self.fetch_candidates(request, Some(query))?;
        memories.sort_by(compare_ranked);
        let memories = memories.into_iter().take(limit).map(|entry| entry.memory).collect::<Vec<_>>();
        self.mark_accessed(&memories)?;
        Ok(SearchResponse {
            total: memories.len(),
            memories,
        })
    }

    pub fn delete(&self, id: Uuid) -> Result<bool> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM memories_fts WHERE id = ?1", [id.to_string()])?;
        let rows = conn.execute("DELETE FROM memories WHERE id = ?1", [id.to_string()])?;
        Ok(rows > 0)
    }

    pub fn prune_expired(&self) -> Result<usize> {
        let conn = self.lock()?;
        let ids = {
            let mut stmt = conn.prepare(
                "SELECT id FROM memories WHERE expires_at IS NOT NULL AND expires_at <= CURRENT_TIMESTAMP",
            )?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for id in &ids {
            conn.execute("DELETE FROM memories_fts WHERE id = ?1", [id])?;
            conn.execute("DELETE FROM memories WHERE id = ?1", [id])?;
        }
        Ok(ids.len())
    }

    pub fn stats(&self) -> Result<Stats> {
        let conn = self.lock()?;
        let total_memories = scalar_count(&conn, "SELECT COUNT(*) FROM memories")?;
        let active_memories = scalar_count(
            &conn,
            "SELECT COUNT(*) FROM memories WHERE archived = 0 AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
        )?;
        let expired_memories = scalar_count(
            &conn,
            "SELECT COUNT(*) FROM memories WHERE expires_at IS NOT NULL AND expires_at <= CURRENT_TIMESTAMP",
        )?;
        let archived_memories =
            scalar_count(&conn, "SELECT COUNT(*) FROM memories WHERE archived = 1")?;
        let sessions = scalar_count(
            &conn,
            "SELECT COUNT(DISTINCT session) FROM memories WHERE session IS NOT NULL AND session != ''",
        )?;
        let projects = scalar_count(
            &conn,
            "SELECT COUNT(DISTINCT project_id) FROM memories WHERE project_id IS NOT NULL AND project_id != ''",
        )?;
        let distinct_tags = scalar_count(
            &conn,
            "SELECT COUNT(DISTINCT value) FROM memories, json_each(memories.tags)",
        )?;

        Ok(Stats {
            total_memories,
            active_memories,
            expired_memories,
            archived_memories,
            sessions,
            projects,
            distinct_tags,
        })
    }

    pub fn prompt_bundle(&self, request: &PromptRequest) -> Result<PromptBundle> {
        let mut search = request.search.clone();
        search.limit = Some(self.resolve_limit(Some(self.max_limit)));
        let response = self.search(&search)?;
        let sections = pack_sections(&response.memories, request.token_budget);
        let selected = sections
            .iter()
            .flat_map(|section| section.memories.clone())
            .collect::<Vec<_>>();
        let estimated_tokens = sections.iter().map(|section| section.estimated_tokens).sum();
        let payload = match request.format {
            PromptFormat::Json => serde_json::to_string_pretty(&sections)?,
            PromptFormat::Toon => toon::encode(&sections, "sections")?,
        };
        Ok(PromptBundle {
            total: selected.len(),
            format: request.format,
            estimated_tokens,
            payload,
            sections,
            memories: selected,
        })
    }

    fn build_record(&self, input: CreateMemory) -> MemoryRecord {
        let now = Utc::now();
        MemoryRecord {
            id: Uuid::new_v4(),
            content: input.content.trim().to_owned(),
            summary: input
                .summary
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            kind: input.kind,
            scope: input.scope,
            source: input.source,
            tags: normalize_tags(input.tags),
            priority: input.priority.clamp(0, 100),
            confidence: input.confidence.clamp(0.0, 1.0),
            session: normalize_optional(input.session),
            role: normalize_optional(input.role),
            project_id: normalize_optional(input.project.project_id),
            repo_root: normalize_optional(input.project.repo_root),
            git_branch: normalize_optional(input.project.git_branch),
            worktree: normalize_optional(input.project.worktree),
            task_id: normalize_optional(input.project.task_id),
            archived: false,
            access_count: 0,
            reinforcement_count: 0,
            created_at: now,
            updated_at: now,
            last_accessed_at: None,
            expires_at: input.expires_at,
        }
    }

    fn insert_record(&self, record: &MemoryRecord) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO memories (
                id, content, summary, kind, scope, source, tags, priority, confidence, session, role,
                project_id, repo_root, git_branch, worktree, task_id, archived, access_count,
                reinforcement_count, created_at, updated_at, last_accessed_at, expires_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
            )",
            params![
                record.id.to_string(),
                record.content,
                record.summary,
                record.kind.to_string(),
                record.scope,
                record.source,
                serde_json::to_string(&record.tags)?,
                record.priority,
                record.confidence,
                record.session,
                record.role,
                record.project_id,
                record.repo_root,
                record.git_branch,
                record.worktree,
                record.task_id,
                record.archived as i64,
                record.access_count,
                record.reinforcement_count,
                record.created_at,
                record.updated_at,
                record.last_accessed_at,
                record.expires_at
            ],
        )?;
        self.upsert_fts_locked(&conn, record)?;
        Ok(())
    }

    fn replace_record(&self, record: &MemoryRecord) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE memories SET
                content = ?2,
                summary = ?3,
                kind = ?4,
                scope = ?5,
                source = ?6,
                tags = ?7,
                priority = ?8,
                confidence = ?9,
                session = ?10,
                role = ?11,
                project_id = ?12,
                repo_root = ?13,
                git_branch = ?14,
                worktree = ?15,
                task_id = ?16,
                archived = ?17,
                access_count = ?18,
                reinforcement_count = ?19,
                created_at = ?20,
                updated_at = ?21,
                last_accessed_at = ?22,
                expires_at = ?23
             WHERE id = ?1",
            params![
                record.id.to_string(),
                record.content,
                record.summary,
                record.kind.to_string(),
                record.scope,
                record.source,
                serde_json::to_string(&record.tags)?,
                record.priority,
                record.confidence,
                record.session,
                record.role,
                record.project_id,
                record.repo_root,
                record.git_branch,
                record.worktree,
                record.task_id,
                record.archived as i64,
                record.access_count,
                record.reinforcement_count,
                record.created_at,
                record.updated_at,
                record.last_accessed_at,
                record.expires_at
            ],
        )?;
        self.upsert_fts_locked(&conn, record)?;
        Ok(())
    }

    fn fetch_candidates(
        &self,
        request: &SearchRequest,
        query: Option<&str>,
    ) -> Result<Vec<RankedMemory>> {
        let mut base = if let Some(query) = query {
            self.search_candidates(request, query)?
        } else {
            self.list_candidates(request)?
        };
        let mut seen = HashSet::new();
        base.retain(|entry| seen.insert(entry.memory.id));
        Ok(base)
    }

    fn list_candidates(&self, request: &SearchRequest) -> Result<Vec<RankedMemory>> {
        let mut sql = format!("{} WHERE 1=1", select_sql());
        let params = self.apply_filters(&mut sql, request);
        sql.push_str(" ORDER BY priority DESC, confidence DESC, updated_at DESC");
        sql.push_str(&format!(" LIMIT {}", self.resolve_limit(request.limit) * 4));

        let conn = self.lock()?;
        let mut stmt = conn.prepare(&sql)?;
        let memories = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), map_memory)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(memories
            .into_iter()
            .map(|memory| RankedMemory {
                score: metadata_score(&memory, request) + recency_score(&memory),
                memory,
            })
            .collect())
    }

    fn search_candidates(&self, request: &SearchRequest, query: &str) -> Result<Vec<RankedMemory>> {
        let expanded_terms = expand_query(query);
        let mut scored = match self.fts_candidates(request, query) {
            Ok(memories) => memories,
            Err(_) => self.fallback_candidates(request, &expanded_terms)?,
        };
        if scored.is_empty() {
            scored = self.fallback_candidates(request, &expanded_terms)?;
        }
        for entry in &mut scored {
            entry.score += text_score(&entry.memory, query, &expanded_terms);
        }
        Ok(scored)
    }

    fn fts_candidates(&self, request: &SearchRequest, query: &str) -> Result<Vec<RankedMemory>> {
        let mut sql = "SELECT m.id, m.content, m.summary, m.kind, m.scope, m.source, m.tags, m.priority,
                    m.confidence, m.session, m.role, m.project_id, m.repo_root, m.git_branch,
                    m.worktree, m.task_id, m.archived, m.access_count, m.reinforcement_count,
                    m.created_at, m.updated_at, m.last_accessed_at, m.expires_at
             FROM memories_fts f
             JOIN memories m ON m.id = f.id
             WHERE memories_fts MATCH ?"
            .to_string();
        let mut params = vec![sanitize_fts_query(query)];
        params.extend(self.apply_filters(&mut sql, request));
        sql.push_str(" ORDER BY bm25(memories_fts), m.priority DESC, m.updated_at DESC");
        sql.push_str(&format!(" LIMIT {}", self.resolve_limit(request.limit) * 6));

        let conn = self.lock()?;
        let mut stmt = conn.prepare(&sql)?;
        let memories = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), map_memory)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(memories
            .into_iter()
            .map(|memory| RankedMemory {
                score: metadata_score(&memory, request) + recency_score(&memory),
                memory,
            })
            .collect())
    }

    fn fallback_candidates(&self, request: &SearchRequest, expanded_terms: &[String]) -> Result<Vec<RankedMemory>> {
        let mut sql =
            format!("{} WHERE 1=1", select_sql());
        let mut params = Vec::new();
        for term in expanded_terms {
            sql.push_str(" AND (lower(content) LIKE ? OR lower(coalesce(summary, '')) LIKE ? OR lower(tags) LIKE ?)");
            let needle = format!("%{}%", term.to_lowercase());
            params.push(needle.clone());
            params.push(needle.clone());
            params.push(needle);
        }
        params.extend(self.apply_filters(&mut sql, request));
        sql.push_str(" ORDER BY priority DESC, confidence DESC, updated_at DESC");
        sql.push_str(&format!(" LIMIT {}", self.resolve_limit(request.limit) * 6));

        let conn = self.lock()?;
        let mut stmt = conn.prepare(&sql)?;
        let memories = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), map_memory)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(memories
            .into_iter()
            .map(|memory| RankedMemory {
                score: metadata_score(&memory, request)
                    + recency_score(&memory)
                    + text_score(&memory, &expanded_terms.join(" "), expanded_terms),
                memory,
            })
            .collect())
    }

    fn apply_filters(&self, sql: &mut String, request: &SearchRequest) -> Vec<String> {
        let mut params = Vec::new();
        if !request.include_expired {
            sql.push_str(" AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)");
        }
        if !request.include_archived {
            sql.push_str(" AND archived = 0");
        }
        if let Some(kind) = &request.kind {
            sql.push_str(" AND kind = ?");
            params.push(kind.to_string());
        }
        if let Some(session) = &request.session {
            sql.push_str(" AND session = ?");
            params.push(session.clone());
        }
        append_project_filter(sql, &mut params, "project_id", &request.project.project_id);
        append_project_filter(sql, &mut params, "repo_root", &request.project.repo_root);
        append_project_filter(sql, &mut params, "git_branch", &request.project.git_branch);
        append_project_filter(sql, &mut params, "worktree", &request.project.worktree);
        append_project_filter(sql, &mut params, "task_id", &request.project.task_id);
        for tag in &request.tags {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM json_each(memories.tags) WHERE json_each.value = ?)",
            );
            params.push(tag.to_lowercase());
        }
        params
    }

    fn mark_accessed(&self, memories: &[MemoryRecord]) -> Result<()> {
        if memories.is_empty() {
            return Ok(());
        }
        let now = Utc::now();
        let conn = self.lock()?;
        for memory in memories {
            conn.execute(
                "UPDATE memories
                 SET access_count = access_count + 1, last_accessed_at = ?2
                 WHERE id = ?1",
                params![memory.id.to_string(), now],
            )?;
        }
        Ok(())
    }

    fn find_duplicate(&self, input: &CreateMemory) -> Result<Option<MemoryRecord>> {
        let normalized_content = input.content.trim();
        let normalized_summary = input.summary.as_deref().unwrap_or_default().trim();
        let project = &input.project;
        let conn = self.lock()?;
        let mut stmt = conn.prepare(&format!(
            "{} WHERE lower(content) = lower(?) AND kind = ? AND coalesce(project_id, '') = coalesce(?, '') AND coalesce(task_id, '') = coalesce(?, '') AND coalesce(session, '') = coalesce(?, '') LIMIT 1",
            select_sql()
        ))?;
        let row = stmt
            .query_row(
                params![
                    normalized_content,
                    input.kind.to_string(),
                    project.project_id,
                    project.task_id,
                    input.session,
                ],
                map_memory,
            )
            .optional()?;
        if row.is_some() {
            return Ok(row);
        }
        if normalized_summary.is_empty() {
            return Ok(None);
        }
        let mut stmt = conn.prepare(&format!(
            "{} WHERE lower(coalesce(summary, '')) = lower(?) AND kind = ? AND coalesce(project_id, '') = coalesce(?, '') LIMIT 1",
            select_sql()
        ))?;
        Ok(stmt
            .query_row(
                params![normalized_summary, input.kind.to_string(), project.project_id],
                map_memory,
            )
            .optional()?)
    }

    fn upsert_fts_locked(&self, conn: &Connection, record: &MemoryRecord) -> Result<()> {
        let scope_terms = [
            record.project_id.clone().unwrap_or_default(),
            record.repo_root.clone().unwrap_or_default(),
            record.git_branch.clone().unwrap_or_default(),
            record.worktree.clone().unwrap_or_default(),
            record.task_id.clone().unwrap_or_default(),
            record.session.clone().unwrap_or_default(),
            record.tags.join(" "),
        ]
        .join(" ");

        conn.execute("DELETE FROM memories_fts WHERE id = ?1", [record.id.to_string()])?;
        conn.execute(
            "INSERT INTO memories_fts (id, content, summary, tags) VALUES (?1, ?2, ?3, ?4)",
            params![
                record.id.to_string(),
                record.content,
                record.summary.clone().unwrap_or_default(),
                scope_terms
            ],
        )?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                summary TEXT,
                kind TEXT NOT NULL,
                scope TEXT NOT NULL,
                source TEXT NOT NULL,
                tags TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 50,
                confidence REAL NOT NULL DEFAULT 0.7,
                session TEXT,
                role TEXT,
                project_id TEXT,
                repo_root TEXT,
                git_branch TEXT,
                worktree TEXT,
                task_id TEXT,
                archived INTEGER NOT NULL DEFAULT 0,
                access_count INTEGER NOT NULL DEFAULT 0,
                reinforcement_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_accessed_at TEXT,
                expires_at TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                content,
                summary,
                tags
            );

            CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id);
            CREATE INDEX IF NOT EXISTS idx_memories_repo_root ON memories(repo_root);
            CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session);
            CREATE INDEX IF NOT EXISTS idx_memories_task_id ON memories(task_id);
            CREATE INDEX IF NOT EXISTS idx_memories_archived ON memories(archived);
            ",
        )?;
        ensure_column(&conn, "memories", "confidence", "REAL NOT NULL DEFAULT 0.7")?;
        ensure_column(&conn, "memories", "project_id", "TEXT")?;
        ensure_column(&conn, "memories", "repo_root", "TEXT")?;
        ensure_column(&conn, "memories", "git_branch", "TEXT")?;
        ensure_column(&conn, "memories", "worktree", "TEXT")?;
        ensure_column(&conn, "memories", "task_id", "TEXT")?;
        ensure_column(&conn, "memories", "archived", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "memories", "access_count", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "memories", "reinforcement_count", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "memories", "last_accessed_at", "TEXT")?;
        self.rebuild_fts_locked(&conn)?;
        Ok(())
    }

    fn rebuild_fts_locked(&self, conn: &Connection) -> Result<()> {
        conn.execute("DELETE FROM memories_fts", [])?;
        let mut stmt = conn.prepare(select_sql())?;
        let memories = stmt
            .query_map([], map_memory)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for memory in &memories {
            self.upsert_fts_locked(conn, memory)?;
        }
        Ok(())
    }

    fn resolve_limit(&self, requested: Option<usize>) -> usize {
        requested
            .unwrap_or(self.default_limit)
            .clamp(1, self.max_limit)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow!("sqlite connection lock poisoned"))
    }
}

fn select_sql() -> &'static str {
    "SELECT id, content, summary, kind, scope, source, tags, priority, confidence, session, role,
        project_id, repo_root, git_branch, worktree, task_id, archived, access_count,
        reinforcement_count, created_at, updated_at, last_accessed_at, expires_at
     FROM memories"
}

fn append_project_filter(
    sql: &mut String,
    params: &mut Vec<String>,
    column: &str,
    value: &Option<String>,
) {
    if let Some(value) = value {
        sql.push_str(&format!(" AND {column} = ?"));
        params.push(value.clone());
    }
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !columns.iter().any(|existing| existing == column) {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut tags = tags
        .into_iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    tags
}

fn merge_tags(left: Vec<String>, right: Vec<String>) -> Vec<String> {
    let mut merged = left;
    merged.extend(right);
    normalize_tags(merged)
}

fn prefer_longer(existing: String, incoming: String) -> String {
    if incoming.trim().len() > existing.trim().len() {
        incoming.trim().to_owned()
    } else {
        existing
    }
}

fn map_memory(row: &Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let id = row.get::<_, String>(0)?;
    let kind = row.get::<_, String>(3)?;
    let tags = row.get::<_, String>(6)?;
    Ok(MemoryRecord {
        id: Uuid::parse_str(&id).map_err(sql_error(0))?,
        content: row.get(1)?,
        summary: row.get(2)?,
        kind: kind.parse::<MemoryKind>().map_err(sql_error(3))?,
        scope: row.get(4)?,
        source: row.get(5)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        priority: row.get(7)?,
        confidence: row.get(8)?,
        session: row.get(9)?,
        role: row.get(10)?,
        project_id: row.get(11)?,
        repo_root: row.get(12)?,
        git_branch: row.get(13)?,
        worktree: row.get(14)?,
        task_id: row.get(15)?,
        archived: row.get::<_, i64>(16)? != 0,
        access_count: row.get(17)?,
        reinforcement_count: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
        last_accessed_at: row.get(21)?,
        expires_at: row.get(22)?,
    })
}

fn sql_error<E>(index: usize) -> impl FnOnce(E) -> rusqlite::Error
where
    E: std::fmt::Display + Send + Sync + 'static,
{
    move |error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    }
}

fn scalar_count(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?)
}

fn sanitize_fts_query(query: &str) -> String {
    let expanded = expand_query(query);
    query
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-'))
        .filter(|token| !token.is_empty())
        .chain(expanded.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn recency_score(memory: &MemoryRecord) -> f64 {
    let age_days = (Utc::now() - memory.updated_at).num_hours().max(0) as f64 / 24.0;
    (30.0 - age_days.min(30.0)) / 30.0
}

fn metadata_score(memory: &MemoryRecord, request: &SearchRequest) -> f64 {
    let mut score = memory.priority as f64 / 25.0 + memory.confidence * 2.0;
    score += (memory.reinforcement_count.min(10) as f64) * 0.2;
    score += (memory.access_count.min(10) as f64) * 0.05;
    if request.project.project_id.is_some() && memory.project_id == request.project.project_id {
        score += 2.5;
    }
    if request.project.task_id.is_some() && memory.task_id == request.project.task_id {
        score += 2.0;
    }
    if request.session.is_some() && memory.session == request.session {
        score += 1.5;
    }
    if matches!(memory.kind, MemoryKind::Constraint | MemoryKind::Preference) {
        score += 0.5;
    }
    score
}

fn text_score(memory: &MemoryRecord, query: &str, expanded_terms: &[String]) -> f64 {
    let haystack = format!(
        "{} {} {} {}",
        memory.content.to_lowercase(),
        memory.summary.clone().unwrap_or_default().to_lowercase(),
        memory.tags.join(" "),
        [
            memory.project_id.clone().unwrap_or_default(),
            memory.repo_root.clone().unwrap_or_default(),
            memory.git_branch.clone().unwrap_or_default(),
            memory.task_id.clone().unwrap_or_default()
        ]
        .join(" ")
    );
    let normalized_query = query.to_lowercase();
    let tokens = if expanded_terms.is_empty() {
        normalized_query
            .split_whitespace()
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    } else {
        expanded_terms.to_vec()
    };
    if tokens.is_empty() {
        return 0.0;
    }
    let overlap = tokens.iter().filter(|token| haystack.contains(token.as_str())).count() as f64;
    let exact_phrase = if haystack.contains(&normalized_query) {
        1.5
    } else {
        0.0
    };
    overlap / tokens.len() as f64 * 4.0 + exact_phrase
}

fn compare_ranked(left: &RankedMemory, right: &RankedMemory) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.memory.priority.cmp(&left.memory.priority))
        .then_with(|| right.memory.updated_at.cmp(&left.memory.updated_at))
}

fn estimate_tokens(memory: &MemoryRecord) -> usize {
    let chars = memory.content.len()
        + memory.summary.as_deref().unwrap_or_default().len()
        + memory.tags.iter().map(|tag| tag.len()).sum::<usize>()
        + memory.project_id.as_deref().unwrap_or_default().len()
        + 80;
    chars.div_ceil(4)
}

fn section_name(memory: &MemoryRecord) -> &'static str {
    match memory.kind {
        MemoryKind::Constraint | MemoryKind::Preference => "critical_context",
        MemoryKind::Todo | MemoryKind::Decision | MemoryKind::Summary => "active_task",
        MemoryKind::Fact | MemoryKind::Artifact => "supporting_context",
    }
}

fn pack_sections(memories: &[MemoryRecord], token_budget: usize) -> Vec<PromptSection> {
    let mut budget = token_budget.max(200);
    let mut buckets: HashMap<&'static str, Vec<MemoryRecord>> = HashMap::new();
    for memory in memories {
        let cost = estimate_tokens(memory);
        let must_include =
            matches!(memory.kind, MemoryKind::Constraint | MemoryKind::Preference) || memory.priority >= 85;
        if !must_include && cost > budget {
            continue;
        }
        if cost <= budget || must_include {
            budget = budget.saturating_sub(cost.min(budget));
            buckets.entry(section_name(memory)).or_default().push(memory.clone());
        }
    }
    ["critical_context", "active_task", "supporting_context"]
        .into_iter()
        .filter_map(|name| {
            let memories = buckets.remove(name)?;
            let estimated_tokens = memories.iter().map(estimate_tokens).sum();
            Some(PromptSection {
                name: name.to_owned(),
                estimated_tokens,
                memories,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    use super::MemoryStore;
    use crate::model::{
        CaptureMemory, CaptureMode, CreateMemory, MemoryKind, ProjectScope, PromptFormat,
        PromptRequest, ReinforceMemory, SearchRequest, UpdateMemory,
    };

    fn store() -> MemoryStore {
        let dir = tempdir().expect("tempdir");
        MemoryStore::open(&dir.path().join("memory.db"), 8, 64).expect("store")
    }

    fn scoped(project_id: &str, task_id: &str) -> ProjectScope {
        ProjectScope {
            project_id: Some(project_id.into()),
            repo_root: Some(format!("/tmp/{project_id}")),
            git_branch: Some("main".into()),
            worktree: None,
            task_id: Some(task_id.into()),
        }
    }

    #[test]
    fn insert_and_search_round_trip() {
        let store = store();
        store
            .insert(CreateMemory {
                content: "User prefers concise Rust answers".into(),
                summary: Some("concise rust".into()),
                kind: MemoryKind::Preference,
                scope: "repo".into(),
                source: "test".into(),
                tags: vec!["rust".into(), "style".into()],
                priority: 90,
                confidence: 0.9,
                session: Some("abc".into()),
                role: Some("user".into()),
                project: scoped("repo-a", "task-1"),
                expires_at: None,
            })
            .expect("insert");

        let response = store
            .search(&SearchRequest {
                query: Some("concise".into()),
                project: scoped("repo-a", "task-1"),
                ..SearchRequest::default()
            })
            .expect("search");

        assert_eq!(response.total, 1);
        assert_eq!(response.memories[0].kind, MemoryKind::Preference);
    }

    #[test]
    fn list_filters_tags_exactly_instead_of_by_substring() {
        let store = store();
        for tag in ["rust", "rusty"] {
            store
                .insert(CreateMemory {
                    content: format!("{tag} memory"),
                    summary: None,
                    kind: MemoryKind::Fact,
                    scope: "local".into(),
                    source: "test".into(),
                    tags: vec![tag.into()],
                    priority: 90,
                    confidence: 0.5,
                    session: None,
                    role: None,
                    project: scoped("repo-a", "task-1"),
                    expires_at: None,
                })
                .expect("insert");
        }

        let response = store
            .list(&SearchRequest {
                tags: vec!["rust".into()],
                project: scoped("repo-a", "task-1"),
                ..SearchRequest::default()
            })
            .expect("list");

        assert_eq!(response.total, 1);
        assert_eq!(response.memories[0].tags, vec!["rust"]);
    }

    #[test]
    fn delete_removes_memory_from_future_search_results() {
        let store = store();
        let memory = store
            .insert(CreateMemory {
                content: "delete me".into(),
                summary: Some("temporary".into()),
                kind: MemoryKind::Todo,
                scope: "local".into(),
                source: "test".into(),
                tags: vec!["ephemeral".into()],
                priority: 60,
                confidence: 0.6,
                session: Some("cleanup".into()),
                role: None,
                project: scoped("repo-a", "task-2"),
                expires_at: None,
            })
            .expect("insert");

        assert!(store.delete(memory.id).expect("delete"));

        let response = store
            .search(&SearchRequest {
                query: Some("delete".into()),
                project: scoped("repo-a", "task-2"),
                ..SearchRequest::default()
            })
            .expect("search");

        assert_eq!(response.total, 0);
    }

    #[test]
    fn prunes_expired_records() {
        let store = store();
        store
            .insert(CreateMemory {
                content: "short lived".into(),
                summary: None,
                kind: MemoryKind::Fact,
                scope: "local".into(),
                source: "test".into(),
                tags: vec![],
                priority: 1,
                confidence: 0.4,
                session: None,
                role: None,
                project: scoped("repo-a", "task-1"),
                expires_at: Some(Utc::now() - Duration::hours(1)),
            })
            .expect("insert");

        let removed = store.prune_expired().expect("prune");
        assert_eq!(removed, 1);
    }

    #[test]
    fn builds_toon_prompt_with_sections() {
        let store = store();
        store
            .insert(CreateMemory {
                content: "Avoid unsafe unless justified".into(),
                summary: Some("constraint".into()),
                kind: MemoryKind::Constraint,
                scope: "local".into(),
                source: "test".into(),
                tags: vec!["important".into()],
                priority: 95,
                confidence: 0.95,
                session: None,
                role: None,
                project: scoped("repo-a", "task-1"),
                expires_at: None,
            })
            .expect("insert");

        let bundle = store
            .prompt_bundle(&PromptRequest {
                search: SearchRequest {
                    query: Some("unsafe".into()),
                    project: scoped("repo-a", "task-1"),
                    ..SearchRequest::default()
                },
                format: PromptFormat::Toon,
                token_budget: 500,
            })
            .expect("bundle");

        assert!(!bundle.sections.is_empty());
        assert!(bundle.payload.starts_with("sections["));
    }

    #[test]
    fn stats_report_active_expired_sessions_and_distinct_tags() {
        let store = store();
        store
            .insert(CreateMemory {
                content: "active".into(),
                summary: None,
                kind: MemoryKind::Fact,
                scope: "local".into(),
                source: "test".into(),
                tags: vec!["shared".into(), "alpha".into()],
                priority: 10,
                confidence: 0.4,
                session: Some("session-a".into()),
                role: None,
                project: scoped("repo-a", "task-1"),
                expires_at: None,
            })
            .expect("insert");
        store
            .insert(CreateMemory {
                content: "expired".into(),
                summary: None,
                kind: MemoryKind::Fact,
                scope: "local".into(),
                source: "test".into(),
                tags: vec!["shared".into(), "beta".into()],
                priority: 10,
                confidence: 0.4,
                session: Some("session-b".into()),
                role: None,
                project: scoped("repo-b", "task-2"),
                expires_at: Some(Utc::now() - Duration::hours(1)),
            })
            .expect("insert");
        let archived = store
            .insert(CreateMemory {
                content: "archived".into(),
                summary: None,
                kind: MemoryKind::Fact,
                scope: "local".into(),
                source: "test".into(),
                tags: vec!["gamma".into()],
                priority: 10,
                confidence: 0.4,
                session: None,
                role: None,
                project: scoped("repo-b", "task-3"),
                expires_at: None,
            })
            .expect("insert");
        store.archive(archived.id, true).expect("archive");

        let stats = store.stats().expect("stats");
        assert_eq!(stats.total_memories, 3);
        assert_eq!(stats.active_memories, 1);
        assert_eq!(stats.expired_memories, 1);
        assert_eq!(stats.archived_memories, 1);
        assert_eq!(stats.sessions, 2);
        assert_eq!(stats.projects, 2);
        assert_eq!(stats.distinct_tags, 4);
    }

    #[test]
    fn capture_upsert_reinforces_existing_memory() {
        let store = store();
        let first = store
            .capture(CaptureMemory {
                memory: CreateMemory {
                    content: "Repo uses axum".into(),
                    summary: Some("stack".into()),
                    kind: MemoryKind::Fact,
                    scope: "repo".into(),
                    source: "test".into(),
                    tags: vec!["rust".into()],
                    priority: 50,
                    confidence: 0.5,
                    session: Some("s1".into()),
                    role: None,
                    project: scoped("repo-a", "task-1"),
                    expires_at: None,
                },
                mode: CaptureMode::Upsert,
            })
            .expect("capture");
        let second = store
            .capture(CaptureMemory {
                memory: CreateMemory {
                    content: "Repo uses axum".into(),
                    summary: Some("web stack".into()),
                    kind: MemoryKind::Fact,
                    scope: "repo".into(),
                    source: "test".into(),
                    tags: vec!["backend".into()],
                    priority: 70,
                    confidence: 0.6,
                    session: Some("s1".into()),
                    role: None,
                    project: scoped("repo-a", "task-1"),
                    expires_at: None,
                },
                mode: CaptureMode::Upsert,
            })
            .expect("capture");

        assert_eq!(first.id, second.id);
        assert!(second.tags.contains(&"backend".to_owned()));
        assert!(second.priority >= 70);
    }

    #[test]
    fn update_and_reinforce_memory_lifecycle() {
        let store = store();
        let memory = store
            .insert(CreateMemory {
                content: "original".into(),
                summary: None,
                kind: MemoryKind::Fact,
                scope: "local".into(),
                source: "test".into(),
                tags: vec!["one".into()],
                priority: 10,
                confidence: 0.3,
                session: None,
                role: None,
                project: scoped("repo-a", "task-1"),
                expires_at: None,
            })
            .expect("insert");

        let updated = store
            .update(
                memory.id,
                UpdateMemory {
                    content: Some("updated".into()),
                    tags: Some(vec!["two".into()]),
                    confidence: Some(0.8),
                    ..UpdateMemory::default()
                },
            )
            .expect("update")
            .expect("memory");
        assert_eq!(updated.content, "updated");
        assert_eq!(updated.tags, vec!["two"]);

        let reinforced = store
            .reinforce(
                memory.id,
                ReinforceMemory {
                    delta: 3,
                    confidence_boost: Some(0.1),
                },
            )
            .expect("reinforce")
            .expect("memory");
        assert!(reinforced.reinforcement_count >= 3);
        assert!(reinforced.confidence > 0.8);
    }

    #[test]
    fn project_scope_prevents_cross_repo_contamination() {
        let store = store();
        for project in ["repo-a", "repo-b"] {
            store
                .insert(CreateMemory {
                    content: format!("{project} uses rust"),
                    summary: None,
                    kind: MemoryKind::Fact,
                    scope: "repo".into(),
                    source: "test".into(),
                    tags: vec!["stack".into()],
                    priority: 40,
                    confidence: 0.5,
                    session: None,
                    role: None,
                    project: scoped(project, "task"),
                    expires_at: None,
                })
                .expect("insert");
        }
        let response = store
            .search(&SearchRequest {
                query: Some("uses rust".into()),
                project: scoped("repo-b", "task"),
                ..SearchRequest::default()
            })
            .expect("search");
        assert_eq!(response.total, 1);
        assert_eq!(response.memories[0].project_id.as_deref(), Some("repo-b"));
    }

    #[test]
    fn malformed_search_falls_back_to_like_query() {
        let store = store();
        store
            .insert(CreateMemory {
                content: "find [brackets] safely".into(),
                summary: None,
                kind: MemoryKind::Fact,
                scope: "repo".into(),
                source: "test".into(),
                tags: vec![],
                priority: 20,
                confidence: 0.4,
                session: None,
                role: None,
                project: scoped("repo-a", "task"),
                expires_at: None,
            })
            .expect("insert");
        let response = store
            .search(&SearchRequest {
                query: Some("[brackets]".into()),
                project: scoped("repo-a", "task"),
                ..SearchRequest::default()
            })
            .expect("search");
        assert_eq!(response.total, 1);
    }
}
