// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// Configuration loading and platform-specific default paths.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub bind: SocketAddr,
    pub database_path: PathBuf,
    pub default_limit: usize,
    pub max_limit: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7878".parse().expect("valid bind addr"),
            database_path: default_database_path(),
            default_limit: 8,
            max_limit: 64,
        }
    }
}

impl AppConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path
            .map(PathBuf::from)
            .or_else(default_config_path)
            .unwrap_or_else(|| PathBuf::from("codex-memory.toml"));

        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let mut config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;

        if config.default_limit == 0 {
            config.default_limit = Self::default().default_limit;
        }
        if config.max_limit == 0 {
            config.max_limit = Self::default().max_limit;
        }

        Ok(config)
    }

    pub fn ensure_parent_dirs(&self) -> Result<()> {
        if let Some(parent) = self.database_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory {}", parent.display())
            })?;
        }
        Ok(())
    }
}

pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("codex-memory").join("config.toml"))
}

pub fn default_database_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("codex-memory")
        .join("memory.db")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::AppConfig;

    #[test]
    fn load_applies_zero_value_defaults() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "bind = \"127.0.0.1:9999\"\ndatabase_path = \"/tmp/example.db\"\ndefault_limit = 0\nmax_limit = 0\n",
        )
        .expect("write");

        let config = AppConfig::load(Some(&path)).expect("load");
        assert_eq!(config.default_limit, 8);
        assert_eq!(config.max_limit, 64);
    }

    #[test]
    fn ensure_parent_dirs_creates_database_directory() {
        let dir = tempdir().expect("tempdir");
        let database_path = dir.path().join("nested").join("memory.db");
        let config = AppConfig {
            bind: "127.0.0.1:7878".parse().expect("bind"),
            database_path: database_path.clone(),
            default_limit: 8,
            max_limit: 64,
        };

        config.ensure_parent_dirs().expect("dirs");
        assert!(database_path.parent().expect("parent").exists());
    }
}
