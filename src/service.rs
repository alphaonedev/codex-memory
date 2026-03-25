// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Daemon bootstrap, shared app state, healthcheck client, and graceful shutdown.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::info;

use crate::api::router;
use crate::config::AppConfig;
use crate::storage::MemoryStore;

#[derive(Clone)]
pub struct AppState {
    pub store: MemoryStore,
    pub config: AppConfig,
}

pub async fn serve(config: AppConfig) -> Result<()> {
    config.ensure_parent_dirs()?;
    let store = MemoryStore::open(&config.database_path, config.default_limit, config.max_limit)?;
    let state = Arc::new(AppState {
        store,
        config: config.clone(),
    });
    let app = router(state);
    let listener = TcpListener::bind(config.bind).await?;
    info!(bind = %config.bind, db = %config.database_path.display(), "codex-memory daemon listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

pub async fn healthcheck(bind: SocketAddr) -> Result<()> {
    let url = format!("http://{bind}/health");
    reqwest::get(url).await?.error_for_status()?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut signal =
            signal::unix::signal(signal::unix::SignalKind::terminate()).expect("signal handler");
        signal.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use tempfile::tempdir;
    use tokio::time::{Duration, sleep};

    use super::{healthcheck, serve};
    use crate::config::AppConfig;

    fn reserve_bind() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        drop(listener);
        addr
    }

    #[tokio::test]
    async fn serve_exposes_healthcheck() {
        let dir = tempdir().expect("tempdir");
        let config = AppConfig {
            bind: reserve_bind(),
            database_path: dir.path().join("memory.db"),
            default_limit: 8,
            max_limit: 64,
        };

        let bind = config.bind;
        let handle = tokio::spawn(async move { serve(config).await });

        let mut healthy = false;
        for _ in 0..20 {
            if healthcheck(bind).await.is_ok() {
                healthy = true;
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        handle.abort();
        let _ = handle.await;

        assert!(healthy, "daemon did not become healthy");
    }
}
