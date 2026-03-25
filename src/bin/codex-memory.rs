// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Operator-facing CLI for interacting with the local codex-memory daemon.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use codex_memory::config::AppConfig;
use codex_memory::ingest::{find_latest_codex_session_file, load_transcript_messages};
use codex_memory::model::{
    CaptureMemory, CaptureMode, CreateMemory, MemoryKind, ProjectScope, PromptFormat,
    PromptRequest, ReinforceMemory, SearchRequest, TranscriptIngestRequest, UpdateMemory,
};
use codex_memory::service::healthcheck;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "codex-memory", about = "CLI for the codex-memory daemon")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    url: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve,
    Health,
    Add(AddArgs),
    Capture(CaptureArgs),
    IngestTranscript(IngestTranscriptArgs),
    IngestCodexSession(IngestCodexSessionArgs),
    Get { id: String },
    Update(UpdateArgs),
    Reinforce(ReinforceArgs),
    Archive(ArchiveArgs),
    Search(SearchArgs),
    List(ListArgs),
    Prompt(PromptArgs),
    Stats,
    Forget { id: String },
    Prune,
}

#[derive(Debug, Args, Clone, Default)]
struct ProjectArgs {
    #[arg(long)]
    project_id: Option<String>,
    #[arg(long)]
    repo_root: Option<String>,
    #[arg(long)]
    git_branch: Option<String>,
    #[arg(long)]
    worktree: Option<String>,
    #[arg(long)]
    task_id: Option<String>,
}

impl From<ProjectArgs> for ProjectScope {
    fn from(value: ProjectArgs) -> Self {
        Self {
            project_id: value.project_id,
            repo_root: value.repo_root,
            git_branch: value.git_branch,
            worktree: value.worktree,
            task_id: value.task_id,
        }
    }
}

#[derive(Debug, Args)]
struct AddArgs {
    #[arg(long)]
    content: String,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long, default_value = "fact")]
    kind: MemoryKind,
    #[arg(long, default_value = "local")]
    scope: String,
    #[arg(long, default_value = "manual")]
    source: String,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long, default_value_t = 50)]
    priority: i64,
    #[arg(long, default_value_t = 0.7)]
    confidence: f64,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    role: Option<String>,
    #[command(flatten)]
    project: ProjectArgs,
}

#[derive(Debug, Args)]
struct CaptureArgs {
    #[command(flatten)]
    add: AddArgs,
    #[arg(long, value_enum, default_value_t = CaptureModeArg::Upsert)]
    mode: CaptureModeArg,
}

#[derive(Debug, Args)]
struct IngestTranscriptArgs {
    #[arg(long)]
    file: PathBuf,
    #[arg(long, default_value = "transcript")]
    source: String,
    #[arg(long)]
    session: Option<String>,
    #[arg(long, value_enum, default_value_t = CaptureModeArg::Upsert)]
    mode: CaptureModeArg,
    #[arg(long, default_value_t = 12)]
    max_memories: usize,
    #[command(flatten)]
    project: ProjectArgs,
}

#[derive(Debug, Args)]
struct IngestCodexSessionArgs {
    #[arg(long)]
    file: Option<PathBuf>,
    #[arg(long)]
    sessions_root: Option<PathBuf>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long)]
    after_epoch: Option<u64>,
    #[arg(long, default_value = "codex-session")]
    source: String,
    #[arg(long)]
    session: Option<String>,
    #[arg(long, value_enum, default_value_t = CaptureModeArg::Upsert)]
    mode: CaptureModeArg,
    #[arg(long, default_value_t = 12)]
    max_memories: usize,
    #[command(flatten)]
    project: ProjectArgs,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CaptureModeArg {
    Insert,
    Upsert,
    Reinforce,
}

impl From<CaptureModeArg> for CaptureMode {
    fn from(value: CaptureModeArg) -> Self {
        match value {
            CaptureModeArg::Insert => CaptureMode::Insert,
            CaptureModeArg::Upsert => CaptureMode::Upsert,
            CaptureModeArg::Reinforce => CaptureMode::Reinforce,
        }
    }
}

#[derive(Debug, Args)]
struct UpdateArgs {
    id: String,
    #[arg(long)]
    content: Option<String>,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long)]
    clear_summary: bool,
    #[arg(long)]
    priority: Option<i64>,
    #[arg(long)]
    confidence: Option<f64>,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long)]
    archived: Option<bool>,
}

#[derive(Debug, Args)]
struct ReinforceArgs {
    id: String,
    #[arg(long, default_value_t = 1)]
    delta: i64,
    #[arg(long)]
    confidence_boost: Option<f64>,
}

#[derive(Debug, Args)]
struct ArchiveArgs {
    id: String,
    #[arg(long, default_value_t = true)]
    archived: bool,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: Option<String>,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    kind: Option<MemoryKind>,
    #[arg(long, default_value_t = 8)]
    limit: usize,
    #[arg(long)]
    include_archived: bool,
    #[command(flatten)]
    project: ProjectArgs,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    kind: Option<MemoryKind>,
    #[arg(long, default_value_t = 8)]
    limit: usize,
    #[arg(long)]
    include_expired: bool,
    #[arg(long)]
    include_archived: bool,
    #[command(flatten)]
    project: ProjectArgs,
}

#[derive(Debug, Args)]
struct PromptArgs {
    query: Option<String>,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    kind: Option<MemoryKind>,
    #[arg(long, default_value_t = 8)]
    limit: usize,
    #[arg(long, value_enum, default_value_t = OutputFormat::Toon)]
    format: OutputFormat,
    #[arg(long, default_value_t = 1500)]
    token_budget: usize,
    #[command(flatten)]
    project: ProjectArgs,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Toon,
}

impl From<OutputFormat> for PromptFormat {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Json => PromptFormat::Json,
            OutputFormat::Toon => PromptFormat::Toon,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load(cli.config.as_deref())?;

    match cli.command {
        Command::Serve => codex_memory::service::serve(config).await,
        Command::Health => {
            healthcheck(config.bind).await?;
            println!("ok");
            Ok(())
        }
        other => {
            let client = Client::new(cli.url.unwrap_or_else(|| format!("http://{}", config.bind)));
            match other {
                Command::Add(args) => {
                    let response: codex_memory::model::MemoryRecord =
                        client.post("/v1/memories", &create_memory(args)).await?;
                    print_json(&response)?;
                }
                Command::Capture(args) => {
                    let response: codex_memory::model::MemoryRecord = client
                        .post(
                            "/v1/capture",
                            &CaptureMemory {
                                memory: create_memory(args.add),
                                mode: args.mode.into(),
                            },
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::IngestTranscript(args) => {
                    let response: serde_json::Value = client
                        .post(
                            "/v1/ingest/transcript",
                            &TranscriptIngestRequest {
                                messages: load_transcript_messages(&args.file)?,
                                source: args.source,
                                session: args.session,
                                project: args.project.into(),
                                mode: args.mode.into(),
                                max_memories: args.max_memories,
                            },
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::IngestCodexSession(args) => {
                    let session_file = match args.file {
                        Some(path) => path,
                        None => {
                            let sessions_root = args
                                .sessions_root
                                .unwrap_or_else(default_codex_sessions_root);
                            let cwd = args
                                .cwd
                                .as_ref()
                                .map(|path| path.to_string_lossy().into_owned());
                            find_latest_codex_session_file(
                                &sessions_root,
                                cwd.as_deref(),
                                args.after_epoch,
                            )?
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "no matching Codex session file found under {}",
                                    sessions_root.display()
                                )
                            })?
                        }
                    };

                    let response: serde_json::Value = client
                        .post(
                            "/v1/ingest/transcript",
                            &TranscriptIngestRequest {
                                messages: load_transcript_messages(&session_file)?,
                                source: args.source,
                                session: args.session,
                                project: args.project.into(),
                                mode: args.mode.into(),
                                max_memories: args.max_memories,
                            },
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::Get { id } => {
                    let response: serde_json::Value =
                        client.get(&format!("/v1/memories/{id}")).await?;
                    print_json(&response)?;
                }
                Command::Update(args) => {
                    let response: serde_json::Value = client
                        .patch(
                            &format!("/v1/memories/{}", args.id),
                            &UpdateMemory {
                                content: args.content,
                                summary: if args.clear_summary {
                                    Some(None)
                                } else {
                                    args.summary.map(Some)
                                },
                                tags: (!args.tag.is_empty()).then_some(args.tag),
                                priority: args.priority,
                                confidence: args.confidence,
                                archived: args.archived,
                                ..UpdateMemory::default()
                            },
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::Reinforce(args) => {
                    let response: serde_json::Value = client
                        .post(
                            &format!("/v1/memories/{}/reinforce", args.id),
                            &ReinforceMemory {
                                delta: args.delta,
                                confidence_boost: args.confidence_boost,
                            },
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::Archive(args) => {
                    let response: serde_json::Value = client
                        .post(
                            &format!("/v1/memories/{}/archive", args.id),
                            &serde_json::json!({ "archived": args.archived }),
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::Search(args) => {
                    let response: codex_memory::model::SearchResponse = client
                        .post(
                            "/v1/search",
                            &SearchRequest {
                                query: args.query,
                                tags: args.tag,
                                kind: args.kind,
                                session: args.session,
                                limit: Some(args.limit),
                                include_expired: false,
                                include_archived: args.include_archived,
                                project: args.project.into(),
                            },
                        )
                        .await?;
                    print_json(&response)?;
                }
                Command::List(args) => {
                    let path = build_list_path(args);
                    let response: serde_json::Value = client.get(&path).await?;
                    print_json(&response)?;
                }
                Command::Prompt(args) => {
                    let bundle: serde_json::Value = client
                        .post(
                            "/v1/prompt",
                            &PromptRequest {
                                search: SearchRequest {
                                    query: args.query,
                                    tags: args.tag,
                                    kind: args.kind,
                                    session: args.session,
                                    limit: Some(args.limit),
                                    include_expired: false,
                                    include_archived: false,
                                    project: args.project.into(),
                                },
                                format: args.format.into(),
                                token_budget: args.token_budget,
                            },
                        )
                        .await?;
                    if let Some(payload) = bundle.get("payload").and_then(|value| value.as_str()) {
                        println!("{payload}");
                    } else {
                        print_json(&bundle)?;
                    }
                }
                Command::Stats => {
                    let response: serde_json::Value = client.get("/v1/stats").await?;
                    print_json(&response)?;
                }
                Command::Forget { id } => {
                    client.delete(&format!("/v1/memories/{id}")).await?;
                    println!("deleted {id}");
                }
                Command::Prune => {
                    let response: serde_json::Value =
                        client.post("/v1/maintenance/prune", &()).await?;
                    print_json(&response)?;
                }
                Command::Serve | Command::Health => bail!("handled above"),
            }
            Ok(())
        }
    }
}

fn create_memory(args: AddArgs) -> CreateMemory {
    CreateMemory {
        content: args.content,
        summary: args.summary,
        kind: args.kind,
        scope: args.scope,
        source: args.source,
        tags: args.tag,
        priority: args.priority,
        confidence: args.confidence,
        session: args.session,
        role: args.role,
        project: args.project.into(),
        expires_at: None,
    }
}

struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    fn new(base: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base,
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self.http.get(format!("{}{}", self.base, path)).send().await?;
        parse_response(response).await
    }

    async fn post<T: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let response = self
            .http
            .post(format!("{}{}", self.base, path))
            .json(body)
            .send()
            .await?;
        parse_response(response).await
    }

    async fn patch<T: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let response = self
            .http
            .patch(format!("{}{}", self.base, path))
            .json(body)
            .send()
            .await?;
        parse_response(response).await
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let response = self.http.delete(format!("{}{}", self.base, path)).send().await?;
        response.error_for_status().context("delete request failed")?;
        Ok(())
    }
}

async fn parse_response<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let response = response.error_for_status().context("daemon request failed")?;
    Ok(response.json::<T>().await?)
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn build_list_path(args: ListArgs) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("limit", &args.limit.to_string());
    serializer.append_pair("include_expired", &args.include_expired.to_string());
    serializer.append_pair("include_archived", &args.include_archived.to_string());
    for tag in args.tag {
        serializer.append_pair("tag", &tag);
    }
    if let Some(session) = args.session {
        serializer.append_pair("session", &session);
    }
    if let Some(kind) = args.kind {
        serializer.append_pair("kind", &kind.to_string());
    }
    append_project_pairs(&mut serializer, args.project);
    format!("/v1/memories?{}", serializer.finish())
}

fn default_codex_sessions_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("sessions")
}

fn append_project_pairs(
    serializer: &mut url::form_urlencoded::Serializer<'_, String>,
    project: ProjectArgs,
) {
    if let Some(value) = project.project_id {
        serializer.append_pair("project_id", &value);
    }
    if let Some(value) = project.repo_root {
        serializer.append_pair("repo_root", &value);
    }
    if let Some(value) = project.git_branch {
        serializer.append_pair("git_branch", &value);
    }
    if let Some(value) = project.worktree {
        serializer.append_pair("worktree", &value);
    }
    if let Some(value) = project.task_id {
        serializer.append_pair("task_id", &value);
    }
}

#[cfg(test)]
mod tests {
    use super::{ListArgs, ProjectArgs, build_list_path};
    use codex_memory::model::MemoryKind;

    #[test]
    fn list_path_encodes_special_characters() {
        let path = build_list_path(ListArgs {
            tag: vec!["rust style".into(), "c++".into()],
            session: Some("session/1".into()),
            kind: Some(MemoryKind::Preference),
            limit: 12,
            include_expired: true,
            include_archived: true,
            project: ProjectArgs {
                project_id: Some("repo one".into()),
                repo_root: None,
                git_branch: Some("feature/x".into()),
                worktree: None,
                task_id: Some("task/1".into()),
            },
        });

        assert!(path.starts_with("/v1/memories?"));
        assert!(path.contains("tag=rust+style"));
        assert!(path.contains("tag=c%2B%2B"));
        assert!(path.contains("session=session%2F1"));
        assert!(path.contains("kind=preference"));
        assert!(path.contains("project_id=repo+one"));
        assert!(path.contains("git_branch=feature%2Fx"));
        assert!(path.contains("task_id=task%2F1"));
    }
}
