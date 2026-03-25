// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT

use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::tempdir;

fn reserve_bind() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    address.to_string()
}

fn write_config(path: &PathBuf, bind: &str, db_path: &str) {
    std::fs::write(
        path,
        format!(
            "bind = \"{bind}\"\ndatabase_path = \"{db_path}\"\ndefault_limit = 8\nmax_limit = 64\n"
        ),
    )
    .expect("config");
}

fn spawn_process(program: &str, args: &[&str]) -> Child {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn")
}

fn wait_for_health(cli_bin: &str, config: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let status = Command::new(cli_bin)
            .args(["--config", config.to_str().expect("config"), "health"])
            .status()
            .expect("health");
        if status.success() {
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
    panic!("daemon did not become healthy in time");
}

fn run_json(cli_bin: &str, config: &Path, args: &[&str]) -> Value {
    let output = Command::new(cli_bin)
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .expect("output");
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json")
}

fn run_text(cli_bin: &str, config: &Path, args: &[&str]) -> String {
    let output = Command::new(cli_bin)
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .expect("output");
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8")
}

#[test]
fn cli_end_to_end_flow_via_codex_memory_serve() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("config.toml");
    let database = temp.path().join("memory.db");
    let bind = reserve_bind();
    write_config(&config, &bind, database.to_str().expect("db"));

    let cli_bin = env!("CARGO_BIN_EXE_codex-memory");
    let mut daemon = spawn_process(
        cli_bin,
        &["--config", config.to_str().expect("config"), "serve"],
    );
    wait_for_health(cli_bin, &config);

    let created = run_json(
        cli_bin,
        &config,
        &[
            "add",
            "--content",
            "User prefers terse answers",
            "--summary",
            "style",
            "--kind",
            "preference",
            "--scope",
            "repo",
            "--source",
            "integration-test",
            "--tag",
            "style",
            "--tag",
            "user",
            "--priority",
            "90",
            "--session",
            "session 1",
            "--role",
            "user",
        ],
    );
    let id = created["id"].as_str().expect("id").to_owned();

    let search = run_json(cli_bin, &config, &["search", "terse", "--tag", "style"]);
    assert_eq!(search["total"], 1);

    let list = run_json(
        cli_bin,
        &config,
        &[
            "list",
            "--tag",
            "style",
            "--session",
            "session 1",
            "--kind",
            "preference",
        ],
    );
    assert_eq!(list["total"], 1);

    let prompt = run_text(cli_bin, &config, &["prompt", "terse", "--format", "toon"]);
    assert!(prompt.starts_with("sections["));

    let stats = run_json(cli_bin, &config, &["stats"]);
    assert_eq!(stats["total_memories"], 1);

    let deleted = Command::new(cli_bin)
        .arg("--config")
        .arg(&config)
        .args(["forget", &id])
        .output()
        .expect("forget");
    assert!(deleted.status.success());

    let prune = run_json(cli_bin, &config, &["prune"]);
    assert_eq!(prune["removed"], 0);

    daemon.kill().expect("kill");
    let _ = daemon.wait();
}

#[test]
fn daemon_binary_serves_health_endpoint_for_cli() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("config.toml");
    let database = temp.path().join("memory.db");
    let bind = reserve_bind();
    write_config(&config, &bind, database.to_str().expect("db"));

    let daemon_bin = env!("CARGO_BIN_EXE_codex-memoryd");
    let cli_bin = env!("CARGO_BIN_EXE_codex-memory");
    let mut daemon = spawn_process(daemon_bin, &["--config", config.to_str().expect("config")]);

    wait_for_health(cli_bin, &config);

    let output = Command::new(cli_bin)
        .arg("--config")
        .arg(&config)
        .arg("health")
        .output()
        .expect("health");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).expect("utf8").trim(), "ok");

    daemon.kill().expect("kill");
    let _ = daemon.wait();
}

#[test]
fn ingest_codex_session_command_extracts_from_jsonl_file() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("config.toml");
    let database = temp.path().join("memory.db");
    let bind = reserve_bind();
    write_config(&config, &bind, database.to_str().expect("db"));

    let cli_bin = env!("CARGO_BIN_EXE_codex-memory");
    let mut daemon = spawn_process(
        cli_bin,
        &["--config", config.to_str().expect("config"), "serve"],
    );
    wait_for_health(cli_bin, &config);

    let session_file = temp.path().join("codex-session.jsonl");
    std::fs::write(
        &session_file,
        r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Please avoid unsafe Rust. I prefer concise answers."}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"I'm checking the build now."}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"Decision: use sqlite for local memory."}]}}
"#,
    )
    .expect("session file");

    let ingest = run_json(
        cli_bin,
        &config,
        &[
            "ingest-codex-session",
            "--file",
            session_file.to_str().expect("session file"),
            "--session",
            "codex-session-1",
            "--project-id",
            "repo-a",
            "--max-memories",
            "8",
        ],
    );
    assert_eq!(ingest["captured"].as_u64().expect("captured"), 3);

    let list = run_json(
        cli_bin,
        &config,
        &[
            "list",
            "--session",
            "codex-session-1",
            "--limit",
            "8",
        ],
    );
    assert_eq!(list["total"].as_u64().expect("total"), 3);

    daemon.kill().expect("kill");
    let _ = daemon.wait();
}
