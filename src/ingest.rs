// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Heuristic transcript ingestion for extracting durable memories from Codex-style sessions.

use crate::model::{
    CaptureMemory, CreateMemory, MemoryKind, ProjectScope, TranscriptIngestRequest,
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
    let lower = sentence.to_lowercase();
    let (kind, priority, confidence, tags) = if starts_with_any(
        &lower,
        &["must ", "do not ", "don't ", "avoid ", "never ", "constraint"],
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
        &["prefer ", "preference", "i prefer", "user prefers", "please "],
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
        (
            MemoryKind::Decision,
            84,
            0.8,
            vec!["decision".to_owned()],
        )
    } else if starts_with_any(
        &lower,
        &["remember ", "important ", "note that", "uses ", "repository uses"],
    ) || lower.contains("uses ")
        || lower.contains("configured")
    {
        (MemoryKind::Fact, 68, 0.7, vec!["fact".to_owned()])
    } else if role == "assistant" && lower.contains("summary") {
        (
            MemoryKind::Summary,
            72,
            0.72,
            vec!["summary".to_owned()],
        )
    } else {
        return None;
    };

    let mut tags = tags;
    tags.extend(infer_tags(&lower));
    let summary = Some(summarize(sentence));
    Some(CaptureMemory {
        memory: CreateMemory {
            content: sentence.trim().to_owned(),
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

#[cfg(test)]
mod tests {
    use crate::model::{CaptureMode, ProjectScope, TranscriptIngestRequest, TranscriptMessage};

    use super::{expand_query, extract_memories};

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
        assert!(memories.iter().any(|memory| memory.memory.kind == crate::model::MemoryKind::Constraint));
        assert!(memories.iter().any(|memory| memory.memory.kind == crate::model::MemoryKind::Preference));
        assert!(memories.iter().any(|memory| memory.memory.kind == crate::model::MemoryKind::Todo));
        assert!(memories.iter().any(|memory| memory.memory.kind == crate::model::MemoryKind::Decision));
    }

    #[test]
    fn expands_query_with_synonyms() {
        let expanded = expand_query("rust test bug");
        assert!(expanded.contains(&"cargo".to_owned()));
        assert!(expanded.contains(&"coverage".to_owned()));
        assert!(expanded.contains(&"error".to_owned()));
    }
}
