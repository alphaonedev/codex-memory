// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Heuristic transcript ingestion for extracting durable memories from Codex-style sessions.

use anyhow::{Context, Result};
use serde_json::Value;

use crate::model::{
    CaptureMemory, CreateMemory, MemoryKind, ProjectScope, TranscriptIngestRequest,
    TranscriptMessage,
};

pub fn extract_memories(request: &TranscriptIngestRequest) -> Vec<CaptureMemory> {
    let mut extracted = Vec::new();

    for message in &request.messages {
        let role = message.role.to_lowercase();
        for sentence in split_sentences(&message.content) {
            if let Some(memory) = classify_sentence(&role, sentence, request) {
                extracted.push(memory);
                if extracted.len() >= request.max_memories {
                    return dedup(extracted);
                }
            }
        }
    }

    dedup(extracted)
}

fn split_sentences(content: &str) -> Vec<&str> {
    content
        .split(['\n', '.', '!', '?'])
        .map(str::trim)
        .filter(|line| line.len() >= 12)
        .collect()
}

fn classify_sentence(
    role: &str,
    sentence: &str,
    request: &TranscriptIngestRequest,
) -> Option<CaptureMemory> {
    let trimmed = sentence.trim();
    let normalized = normalize_sentence(trimmed);
    if normalized.len() < 12 || is_low_signal_sentence(trimmed, &normalized, request) {
        return None;
    }

    let lower = normalized.to_lowercase();
    let (kind, priority, confidence, tags) = if starts_with_any(
        &lower,
        &[
            "must ",
            "do not ",
            "don't ",
            "avoid ",
            "never ",
            "constraint",
        ],
    ) || lower.contains("must not")
        || lower.contains("avoid ")
    {
        (
            MemoryKind::Constraint,
            92,
            0.9,
            vec!["constraint".to_owned()],
        )
    } else if starts_with_any(
        &lower,
        &[
            "prefer ",
            "preference",
            "i prefer",
            "user prefers",
            "please ",
        ],
    ) || (role == "user" && lower.contains("prefer"))
    {
        (
            MemoryKind::Preference,
            88,
            0.85,
            vec!["preference".to_owned()],
        )
    } else if starts_with_any(
        &lower,
        &["todo ", "next ", "follow up", "we need to", "need to "],
    ) {
        (MemoryKind::Todo, 78, 0.75, vec!["todo".to_owned()])
    } else if starts_with_any(
        &lower,
        &["decision", "we decided", "decided to", "the plan is"],
    ) {
        (MemoryKind::Decision, 84, 0.8, vec!["decision".to_owned()])
    } else if starts_with_any(
        &lower,
        &[
            "remember ",
            "important ",
            "note that",
            "uses ",
            "repository uses",
        ],
    ) || lower.contains("uses ")
        || lower.contains("configured")
    {
        (MemoryKind::Fact, 68, 0.7, vec!["fact".to_owned()])
    } else if role == "assistant"
        && starts_with_any(
            &lower,
            &["summary:", "summary ", "in summary", "to summarize"],
        )
    {
        (MemoryKind::Summary, 72, 0.72, vec!["summary".to_owned()])
    } else {
        return None;
    };

    let mut tags = tags;
    tags.extend(infer_tags(&lower));
    let summary = Some(summarize(&normalized));
    Some(CaptureMemory {
        memory: CreateMemory {
            content: normalized,
            summary,
            kind,
            scope: "session".to_owned(),
            source: request.source.clone(),
            tags,
            priority,
            confidence,
            session: request.session.clone(),
            role: Some(role.to_owned()),
            project: ProjectScope {
                project_id: request.project.project_id.clone(),
                repo_root: request.project.repo_root.clone(),
                git_branch: request.project.git_branch.clone(),
                worktree: request.project.worktree.clone(),
                task_id: request.project.task_id.clone(),
            },
            expires_at: None,
        },
        mode: request.mode,
    })
}

fn summarize(sentence: &str) -> String {
    let trimmed = sentence.trim();
    if trimmed.len() <= 72 {
        trimmed.to_owned()
    } else {
        format!("{}...", &trimmed[..69])
    }
}

fn is_low_signal_sentence(
    original: &str,
    normalized: &str,
    request: &TranscriptIngestRequest,
) -> bool {
    let lower = normalized.to_lowercase();
    let original_lower = original.to_lowercase();

    if looks_like_assignment(normalized) {
        return true;
    }

    if request.source == "codex-session"
        && starts_with_any(
            &lower,
            &[
                "i'm checking",
                "i am checking",
                "i'm wiring",
                "i am wiring",
                "i'm updating",
                "i am updating",
                "i'm doing",
                "i am doing",
                "i'm switching",
                "i am switching",
                "i found",
                "if you want",
                "the build ",
                "build is ",
            ],
        )
    {
        return true;
    }

    if request.source == "codex-session" && starts_with_any(&original_lower, &["- `", "* `"]) {
        return true;
    }

    false
}

fn normalize_sentence(sentence: &str) -> String {
    sentence
        .trim()
        .trim_start_matches(['-', '*', '#', ' '])
        .replace('`', "")
        .trim()
        .to_owned()
}

fn looks_like_assignment(sentence: &str) -> bool {
    let Some((left, right)) = sentence.split_once('=') else {
        return false;
    };

    !left.trim().is_empty() && !right.trim().is_empty()
}

fn infer_tags(lower: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let map = [
        ("rust", "rust"),
        ("cargo", "cargo"),
        ("sqlite", "sqlite"),
        ("database", "database"),
        ("axum", "axum"),
        ("tokio", "tokio"),
        ("test", "testing"),
        ("coverage", "quality"),
        ("prompt", "prompting"),
        ("memory", "memory"),
    ];
    for (needle, tag) in map {
        if lower.contains(needle) {
            tags.push(tag.to_owned());
        }
    }
    tags
}

fn dedup(memories: Vec<CaptureMemory>) -> Vec<CaptureMemory> {
    let mut seen = std::collections::HashSet::new();
    memories
        .into_iter()
        .filter(|memory| seen.insert(memory.memory.content.to_lowercase()))
        .collect()
}

fn starts_with_any(input: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| input.starts_with(prefix))
}

pub fn expand_query(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = query.to_lowercase();
    for token in lower
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-'))
        .filter(|token| !token.is_empty())
    {
        out.push(token.to_owned());
        for synonym in synonyms(token) {
            out.push((*synonym).to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn synonyms(token: &str) -> &'static [&'static str] {
    match token {
        "bug" | "issue" | "failure" => &["error", "problem", "broken"],
        "error" => &["failure", "bug"],
        "test" | "tests" => &["testing", "coverage", "spec"],
        "config" | "configuration" => &["settings", "env"],
        "memory" => &["context", "recall"],
        "repo" | "repository" => &["project", "codebase"],
        "prompt" => &["context", "instructions"],
        "rust" => &["cargo", "crate"],
        "cli" => &["command", "terminal"],
        _ => &[],
    }
}

pub fn load_transcript_messages(path: &std::path::Path) -> Result<Vec<TranscriptMessage>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read transcript at {}", path.display()))?;
    parse_transcript_messages(&raw)
}

pub fn parse_transcript_messages(raw: &str) -> Result<Vec<TranscriptMessage>> {
    if raw.trim_start().starts_with('[') {
        let messages: Vec<TranscriptMessage> =
            serde_json::from_str(raw).context("failed to parse transcript JSON")?;
        return Ok(messages);
    }

    let mut messages = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with('{') {
            if let Some(message) = parse_codex_session_line(line)? {
                messages.push(message);
            }
            continue;
        }

        if let Some((role, content)) = line.split_once(':') {
            messages.push(TranscriptMessage {
                role: role.trim().to_owned(),
                content: content.trim().to_owned(),
            });
        } else {
            messages.push(TranscriptMessage {
                role: "user".to_owned(),
                content: line.to_owned(),
            });
        }
    }
    Ok(messages)
}

pub fn find_latest_codex_session_file(
    sessions_root: &std::path::Path,
    cwd_filter: Option<&str>,
    after_epoch: Option<u64>,
) -> Result<Option<std::path::PathBuf>> {
    let mut candidates = Vec::new();
    collect_jsonl_files(sessions_root, &mut candidates)?;
    candidates.sort_by(|left, right| {
        let left_mtime = left.metadata().and_then(|meta| meta.modified()).ok();
        let right_mtime = right.metadata().and_then(|meta| meta.modified()).ok();
        right_mtime.cmp(&left_mtime).then_with(|| right.cmp(left))
    });

    for path in candidates {
        if let Some(after_epoch) = after_epoch {
            let modified = path
                .metadata()
                .and_then(|meta| meta.modified())
                .context("failed to read candidate session metadata")?;
            let modified_epoch = modified
                .duration_since(std::time::UNIX_EPOCH)
                .context("candidate session mtime was before unix epoch")?
                .as_secs();
            if modified_epoch < after_epoch {
                continue;
            }
        }

        if let Some(expected_cwd) = cwd_filter {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read Codex session at {}", path.display()))?;
            let Some(actual_cwd) = extract_session_meta_cwd(&raw)? else {
                continue;
            };
            if actual_cwd != expected_cwd {
                continue;
            }
        }

        return Ok(Some(path));
    }

    Ok(None)
}

fn collect_jsonl_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read sessions directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_jsonl_files(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }

    Ok(())
}

fn parse_codex_session_line(line: &str) -> Result<Option<TranscriptMessage>> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return Ok(None);
    }

    let payload = match value.get("payload") {
        Some(payload) => payload,
        None => return Ok(None),
    };

    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return Ok(None);
    }

    let role = match payload.get("role").and_then(Value::as_str) {
        Some("user") => "user",
        Some("assistant") => {
            let phase = payload.get("phase").and_then(Value::as_str);
            if phase != Some("final_answer") {
                return Ok(None);
            }
            "assistant"
        }
        _ => return Ok(None),
    };

    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                    Some("input_text") | Some("output_text") => {
                        item.get("text").and_then(Value::as_str).map(str::to_owned)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    if content.is_empty()
        || content.starts_with("<environment_context>")
        || content.starts_with("<permissions instructions>")
        || content.starts_with("<collaboration_mode>")
        || content.starts_with("<skills_instructions>")
    {
        return Ok(None);
    }

    Ok(Some(TranscriptMessage {
        role: role.to_owned(),
        content,
    }))
}

fn extract_session_meta_cwd(raw: &str) -> Result<Option<String>> {
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !line.starts_with('{') {
            continue;
        }

        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }

        return Ok(value
            .get("payload")
            .and_then(|payload| payload.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_owned));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use crate::model::{CaptureMode, ProjectScope, TranscriptIngestRequest, TranscriptMessage};

    use super::{
        expand_query, extract_memories, find_latest_codex_session_file, parse_transcript_messages,
    };

    #[test]
    fn extracts_constraints_preferences_and_todos() {
        let request = TranscriptIngestRequest {
            messages: vec![
                TranscriptMessage {
                    role: "user".into(),
                    content: "Please avoid unsafe Rust. I prefer concise answers. Next we need to add tests.".into(),
                },
                TranscriptMessage {
                    role: "assistant".into(),
                    content: "Decision: use sqlite for local state.".into(),
                },
            ],
            source: "test".into(),
            session: Some("s1".into()),
            project: ProjectScope {
                project_id: Some("repo-a".into()),
                ..ProjectScope::default()
            },
            mode: CaptureMode::Upsert,
            max_memories: 10,
        };

        let memories = extract_memories(&request);
        assert!(
            memories
                .iter()
                .any(|memory| memory.memory.kind == crate::model::MemoryKind::Constraint)
        );
        assert!(
            memories
                .iter()
                .any(|memory| memory.memory.kind == crate::model::MemoryKind::Preference)
        );
        assert!(
            memories
                .iter()
                .any(|memory| memory.memory.kind == crate::model::MemoryKind::Todo)
        );
        assert!(
            memories
                .iter()
                .any(|memory| memory.memory.kind == crate::model::MemoryKind::Decision)
        );
    }

    #[test]
    fn expands_query_with_synonyms() {
        let expanded = expand_query("rust test bug");
        assert!(expanded.contains(&"cargo".to_owned()));
        assert!(expanded.contains(&"coverage".to_owned()));
        assert!(expanded.contains(&"error".to_owned()));
    }

    #[test]
    fn parses_codex_session_jsonl_messages() {
        let raw = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"please avoid unsafe rust"}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"I'm checking the build now."}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"Decision: use sqlite for local state."}]}}
"#;

        let messages = parse_transcript_messages(raw).expect("parse messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
        assert!(messages[1].content.contains("sqlite"));
    }

    #[test]
    fn codex_session_ingest_filters_operational_chatter() {
        let request = TranscriptIngestRequest {
            messages: vec![
                TranscriptMessage {
                    role: "assistant".into(),
                    content: "I'm checking the build now. Summary: use sqlite for local memory. - `CODEX_MEMORY_START_KIND=summary`".into(),
                },
                TranscriptMessage {
                    role: "user".into(),
                    content: "Please avoid unsafe Rust. I prefer concise answers.".into(),
                },
            ],
            source: "codex-session".into(),
            session: Some("s1".into()),
            project: ProjectScope::default(),
            mode: CaptureMode::Upsert,
            max_memories: 10,
        };

        let memories = extract_memories(&request);
        assert!(
            memories
                .iter()
                .any(|memory| memory.memory.content == "Summary: use sqlite for local memory")
        );
        assert!(
            memories
                .iter()
                .any(|memory| memory.memory.kind == crate::model::MemoryKind::Constraint)
        );
        assert!(
            memories
                .iter()
                .all(|memory| !memory.memory.content.contains("checking the build"))
        );
        assert!(
            memories
                .iter()
                .all(|memory| !memory.memory.content.contains("CODEX_MEMORY_START_KIND"))
        );
    }

    #[test]
    fn finds_latest_matching_codex_session_file() {
        let root = std::env::temp_dir().join(format!(
            "codex-memory-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("2026/03/25")).expect("mkdir");
        let first = root.join("2026/03/25/first.jsonl");
        let second = root.join("2026/03/25/second.jsonl");
        std::fs::write(
            &first,
            r#"{"type":"session_meta","payload":{"cwd":"/repo/one"}}"#,
        )
        .expect("write first");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            &second,
            r#"{"type":"session_meta","payload":{"cwd":"/repo/two"}}"#,
        )
        .expect("write second");

        let found = find_latest_codex_session_file(&root, Some("/repo/two"), None)
            .expect("find")
            .expect("path");
        assert_eq!(found, second);

        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
