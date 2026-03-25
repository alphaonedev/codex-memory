// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Localhost daemon entrypoint for codex-memory.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use codex_memory::config::AppConfig;
use codex_memory::service::serve;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "codex-memoryd", about = "Lightweight localhost memory daemon for Codex")]
struct Args {
    #[arg(long)]
    config: Option<PathBuf>,
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}

fn load_config(args: &Args) -> Result<AppConfig> {
    AppConfig::load(args.config.as_deref())
}

async fn run(args: Args) -> Result<()> {
    init_tracing();
    let config = load_config(&args)?;
    serve(config).await
}

#[tokio::main]
async fn main() -> Result<()> {
    run(Args::parse()).await
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use tempfile::tempdir;

    use super::{Args, init_tracing, load_config, run};

    #[test]
    fn parses_config_argument() {
        let args = Args::try_parse_from(["codex-memoryd", "--config", "/tmp/config.toml"])
            .expect("args");
        assert_eq!(args.config.as_deref(), Some(std::path::Path::new("/tmp/config.toml")));
    }

    #[test]
    fn load_config_uses_explicit_path() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "bind = \"127.0.0.1:7788\"\ndatabase_path = \"/tmp/codex-memoryd.db\"\ndefault_limit = 4\nmax_limit = 16\n",
        )
        .expect("write");

        let args = Args {
            config: Some(path.clone()),
        };
        let config = load_config(&args).expect("config");
        assert_eq!(config.bind.to_string(), "127.0.0.1:7788");
        assert_eq!(config.default_limit, 4);
        assert_eq!(config.max_limit, 16);
    }

    #[test]
    fn tracing_init_is_idempotent() {
        init_tracing();
        init_tracing();
    }

    #[tokio::test]
    async fn run_returns_config_error_for_invalid_toml() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("broken.toml");
        std::fs::write(&path, "bind = [not valid toml").expect("write");

        let error = run(Args { config: Some(path) }).await.expect_err("error");
        assert!(error.to_string().contains("failed to parse config"));
    }
}
