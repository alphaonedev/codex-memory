// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// HTTP API surface for daemon health, memory CRUD, retrieval, capture, and maintenance.

use std::sync::Arc;

use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use tokio::task;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::ingest::extract_memories;
use crate::model::{
    CaptureMemory, CreateMemory, MemoryKind, ProjectScope, PromptRequest, ReinforceMemory,
    SearchRequest, TranscriptIngestRequest, TranscriptIngestResponse, UpdateMemory,
};
use crate::service::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/memories", get(list_memories).post(create_memory))
        .route(
            "/v1/memories/{id}",
            get(get_memory).patch(update_memory).delete(delete_memory),
        )
        .route("/v1/memories/{id}/archive", post(archive_memory))
        .route("/v1/memories/{id}/reinforce", post(reinforce_memory))
        .route("/v1/capture", post(capture_memory))
        .route("/v1/ingest/transcript", post(ingest_transcript))
        .route("/v1/search", post(search_memories))
        .route("/v1/prompt", post(prompt_bundle))
        .route("/v1/stats", get(stats))
        .route("/v1/maintenance/prune", post(prune_expired))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "bind": state.config.bind,
        "database_path": state.config.database_path,
    }))
}

async fn create_memory(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateMemory>,
) -> Result<Json<crate::model::MemoryRecord>, ApiError> {
    payload
        .validate()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(
        blocking(move || state.store.insert(payload))
            .await?
            .map_err(ApiError::from)?,
    ))
}

async fn capture_memory(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CaptureMemory>,
) -> Result<Json<crate::model::MemoryRecord>, ApiError> {
    payload
        .memory
        .validate()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(
        blocking(move || state.store.capture(payload))
            .await?
            .map_err(ApiError::from)?,
    ))
}

async fn ingest_transcript(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TranscriptIngestRequest>,
) -> Result<Json<TranscriptIngestResponse>, ApiError> {
    payload
        .validate()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let extracted = extract_memories(&payload);
    let total = extracted.len();
    let store = state.store.clone();
    let memories = blocking(move || {
        let mut captured = Vec::new();
        for memory in extracted {
            captured.push(store.capture(memory)?);
        }
        Ok::<_, anyhow::Error>(captured)
    })
    .await?
    .map_err(ApiError::from)?;

    Ok(Json(TranscriptIngestResponse {
        extracted: total,
        captured: memories.len(),
        skipped: total.saturating_sub(memories.len()),
        memories,
    }))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    limit: Option<usize>,
    session: Option<String>,
    kind: Option<MemoryKind>,
    #[serde(default)]
    tag: Vec<String>,
    include_expired: Option<bool>,
    include_archived: Option<bool>,
    project_id: Option<String>,
    repo_root: Option<String>,
    git_branch: Option<String>,
    worktree: Option<String>,
    task_id: Option<String>,
}

async fn list_memories(
    State(state): State<Arc<AppState>>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<crate::model::SearchResponse>, ApiError> {
    let query = ListQuery::parse(raw_query.as_deref())?;
    let request = SearchRequest {
        query: None,
        tags: query.tag,
        kind: query.kind,
        session: query.session,
        limit: query.limit,
        include_expired: query.include_expired.unwrap_or(false),
        include_archived: query.include_archived.unwrap_or(false),
        project: ProjectScope {
            project_id: query.project_id,
            repo_root: query.repo_root,
            git_branch: query.git_branch,
            worktree: query.worktree,
            task_id: query.task_id,
        },
    };
    Ok(Json(
        blocking(move || state.store.list(&request))
            .await?
            .map_err(ApiError::from)?,
    ))
}

impl ListQuery {
    fn parse(raw_query: Option<&str>) -> Result<Self, ApiError> {
        let mut query = Self::default();
        let Some(raw_query) = raw_query else {
            return Ok(query);
        };

        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            match key.as_ref() {
                "limit" => {
                    query.limit = Some(value.parse::<usize>().map_err(|_| {
                        ApiError::bad_request(format!("invalid limit value: {value}"))
                    })?);
                }
                "session" => query.session = Some(value.into_owned()),
                "kind" => query.kind = Some(value.parse().map_err(ApiError::bad_request)?),
                "tag" => query.tag.push(value.into_owned()),
                "include_expired" => {
                    query.include_expired = Some(value.parse::<bool>().map_err(|_| {
                        ApiError::bad_request(format!("invalid include_expired value: {value}"))
                    })?);
                }
                "include_archived" => {
                    query.include_archived = Some(value.parse::<bool>().map_err(|_| {
                        ApiError::bad_request(format!("invalid include_archived value: {value}"))
                    })?);
                }
                "project_id" => query.project_id = Some(value.into_owned()),
                "repo_root" => query.repo_root = Some(value.into_owned()),
                "git_branch" => query.git_branch = Some(value.into_owned()),
                "worktree" => query.worktree = Some(value.into_owned()),
                "task_id" => query.task_id = Some(value.into_owned()),
                _ => {}
            }
        }

        Ok(query)
    }
}

async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::model::MemoryRecord>, ApiError> {
    blocking(move || state.store.get(id))
        .await?
        .map_err(ApiError::from)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("memory not found"))
}

async fn update_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateMemory>,
) -> Result<Json<crate::model::MemoryRecord>, ApiError> {
    payload
        .validate()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    blocking(move || state.store.update(id, payload))
        .await?
        .map_err(ApiError::from)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("memory not found"))
}

async fn reinforce_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ReinforceMemory>,
) -> Result<Json<crate::model::MemoryRecord>, ApiError> {
    blocking(move || state.store.reinforce(id, payload))
        .await?
        .map_err(ApiError::from)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("memory not found"))
}

#[derive(Debug, Deserialize)]
struct ArchiveRequest {
    archived: bool,
}

async fn archive_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ArchiveRequest>,
) -> Result<Json<crate::model::MemoryRecord>, ApiError> {
    blocking(move || state.store.archive(id, payload.archived))
        .await?
        .map_err(ApiError::from)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("memory not found"))
}

async fn search_memories(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<crate::model::SearchResponse>, ApiError> {
    Ok(Json(
        blocking(move || state.store.search(&request))
            .await?
            .map_err(ApiError::from)?,
    ))
}

async fn prompt_bundle(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PromptRequest>,
) -> Result<Json<crate::model::PromptBundle>, ApiError> {
    Ok(Json(
        blocking(move || state.store.prompt_bundle(&request))
            .await?
            .map_err(ApiError::from)?,
    ))
}

async fn stats(State(state): State<Arc<AppState>>) -> Result<Json<crate::model::Stats>, ApiError> {
    Ok(Json(
        blocking(move || state.store.stats())
            .await?
            .map_err(ApiError::from)?,
    ))
}

async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if blocking(move || state.store.delete(id))
        .await?
        .map_err(ApiError::from)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

async fn prune_expired(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = blocking(move || state.store.prune_expired())
        .await?
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "removed": removed })))
}

async fn blocking<F, T>(work: F) -> Result<T, ApiError>
where
    F: Send + 'static + FnOnce() -> T,
    T: Send + 'static,
{
    task::spawn_blocking(work)
        .await
        .map_err(|error| ApiError::from(anyhow::anyhow!(error.to_string())))
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    error: anyhow::Error,
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: value.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(json!({
            "error": self.error.to_string(),
        }));
        (self.status, body).into_response()
    }
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: anyhow::anyhow!(message.into()),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error: anyhow::anyhow!(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use serde_json::{Value, json};
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::router;
    use crate::config::AppConfig;
    use crate::model::{CaptureMode, PromptFormat, SearchResponse};
    use crate::service::AppState;
    use crate::storage::MemoryStore;

    fn app() -> axum::Router {
        let dir = tempdir().expect("tempdir");
        let config = AppConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            database_path: dir.path().join("memory.db"),
            default_limit: 8,
            max_limit: 64,
        };
        config.ensure_parent_dirs().expect("dirs");
        let store = MemoryStore::open(
            &config.database_path,
            config.default_limit,
            config.max_limit,
        )
        .expect("store");
        let state = Arc::new(AppState { store, config });
        router(state)
    }

    #[tokio::test]
    async fn full_memory_api_flow_works() {
        let app = app();

        let create = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/capture")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "content": "User prefers concise answers",
                            "summary": "preference",
                            "kind": "preference",
                            "scope": "repo",
                            "source": "test",
                            "tags": ["style", "user"],
                            "priority": 90,
                            "confidence": 0.9,
                            "session": "s-1",
                            "role": "user",
                            "project_id": "repo-a",
                            "task_id": "task-1",
                            "mode": CaptureMode::Upsert
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(create.status(), StatusCode::OK);
        let created: Value = read_json(create.into_body()).await;
        let id = created["id"].as_str().expect("id").to_owned();

        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories?tag=style&session=s-1&kind=preference&project_id=repo-a&task_id=task-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(list.status(), StatusCode::OK);
        let list_body: SearchResponse = read_json(list.into_body()).await;
        assert_eq!(list_body.total, 1);

        let search = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/search")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "query": "concise",
                            "tags": ["style"],
                            "session": "s-1",
                            "project_id": "repo-a",
                            "task_id": "task-1"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(search.status(), StatusCode::OK);
        let search_body: SearchResponse = read_json(search.into_body()).await;
        assert_eq!(search_body.total, 1);

        let reinforce = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/v1/memories/{id}/reinforce"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "delta": 2, "confidence_boost": 0.1 }).to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(reinforce.status(), StatusCode::OK);

        let update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/v1/memories/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "summary": "updated summary" }).to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(update.status(), StatusCode::OK);

        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/memories/{id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(get.status(), StatusCode::OK);

        let prompt = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/prompt")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "query": "concise",
                            "project_id": "repo-a",
                            "format": PromptFormat::Toon,
                            "token_budget": 500
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(prompt.status(), StatusCode::OK);
        let prompt_body: Value = read_json(prompt.into_body()).await;
        assert!(
            prompt_body["payload"]
                .as_str()
                .expect("payload")
                .starts_with("sections[")
        );

        let archive = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/v1/memories/{id}/archive"))
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "archived": true }).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(archive.status(), StatusCode::OK);

        let stats = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stats")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(stats.status(), StatusCode::OK);
        let stats_body: Value = read_json(stats.into_body()).await;
        assert_eq!(stats_body["total_memories"], 1);

        let delete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/v1/memories/{id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(delete.status(), StatusCode::NO_CONTENT);

        let prune = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/maintenance/prune")
                    .body(Body::from("{}"))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(prune.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn transcript_ingest_extracts_memories() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/ingest/transcript")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "source": "session-log",
                            "session": "s-2",
                            "project_id": "repo-a",
                            "messages": [
                                {"role": "user", "content": "Please avoid unsafe Rust. I prefer concise output."},
                                {"role": "assistant", "content": "Decision: use sqlite for local memory."}
                            ]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = read_json(response.into_body()).await;
        assert!(body["captured"].as_u64().expect("captured") >= 2);
    }

    #[tokio::test]
    async fn transcript_ingest_rejects_empty_message_content() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/ingest/transcript")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "source": "session-log",
                            "messages": [
                                {"role": "user", "content": "   "}
                            ]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_memory_rejects_empty_content() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "content": "  ",
                            "kind": "fact"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    async fn read_json<T: serde::de::DeserializeOwned>(body: Body) -> T {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }
}
