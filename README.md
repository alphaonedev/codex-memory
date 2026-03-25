# codex-memory

`codex-memory` is a lightweight Rust localhost daemon and CLI for persistent, token-efficient memory retrieval in Codex or ChatGPT-adjacent workflows.

Open source license: MIT. Copyright 2026 AlphaOne LLC.

It is designed for local-first use:

- Rust daemon on `127.0.0.1`
- SQLite persistence with hybrid lexical retrieval and fallback matching
- project-aware scoping for `project_id`, `repo_root`, `git_branch`, `worktree`, and `task_id`
- lifecycle operations for capture, update, reinforce, archive, prune, and export
- TOON prompt export with sectioned packing to reduce token overhead versus verbose JSON
- GitHub Pages-ready docs in [`docs/`](./docs/)

## Why this exists

Codex is stronger when project context survives beyond a single session. This project stores durable local memory records such as:

- user preferences
- repository constraints
- architectural decisions
- task summaries
- TODO state
- reusable artifacts

The daemon exposes a simple HTTP API so editor tooling, shell scripts, MCP adapters, or wrapper CLIs can all talk to the same memory store.

## Architecture

1. `codex-memoryd` runs the daemon and owns the SQLite-backed memory index.
2. `codex-memory` is the CLI client for CRUD, retrieval, and prompt export.
3. `MemoryStore` persists records and maintains an FTS5 search index.
4. Retrieval uses hybrid ranking across lexical match, project/task/session scope, priority, confidence, reinforcement, and recency.
5. Prompt assembly returns either JSON or TOON, grouped into `critical_context`, `active_task`, and `supporting_context`.
6. TOON output compresses structured prompt sections for lower token cost.

## Quick start

The simplest path for most people is:

1. Install Rust from `https://rustup.rs/`
2. Clone this repository
3. Run `./scripts/install.sh`
4. Run `codex-memory health`
5. If it prints `ok`, the memory daemon is running

If you are not technical, start with [`docs/getting-started.md`](./docs/getting-started.md) and [`docs/quickstart.md`](./docs/quickstart.md).

```bash
cargo run --bin codex-memoryd
```

In a second shell:

```bash
cargo run --bin codex-memory -- add \
  --content "User prefers terse technical responses" \
  --kind preference \
  --project-id codex-memory \
  --tag style \
  --tag user

cargo run --bin codex-memory -- search terse

cargo run --bin codex-memory -- prompt terse --project-id codex-memory --format toon

cargo run --bin codex-memory -- capture \
  --content "Repository uses Axum and SQLite" \
  --kind fact \
  --project-id codex-memory \
  --mode upsert
```

## One-command install

For a local machine deployment with automatic restart and login persistence:

```bash
./scripts/install.sh
```

What it does:

- macOS: installs binaries, writes config, installs a LaunchAgent, and starts it with `launchctl`
- Ubuntu and Fedora: installs binaries, writes config, installs a user-level systemd unit, and starts it with `systemctl --user`

After install:

```bash
codex-memory health
codex-memory stats
```

Service management:

```bash
systemctl --user start codex-memory.service
systemctl --user stop codex-memory.service
systemctl --user restart codex-memory.service
systemctl --user status codex-memory.service
```

macOS:

```bash
launchctl load ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl unload ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl list | grep codex-memory
```

If you get stuck, use the troubleshooting steps in [`docs/quickstart.md`](./docs/quickstart.md).

To remove it cleanly:

```bash
./scripts/uninstall.sh
```

## Example TOON output

```toon
sections[2]{name,estimated_tokens,memories}:
  critical_context	96	[{...}]
  active_task	122	[{...}]
```

## Configuration

Default config path:

- Linux: `~/.config/codex-memory/config.toml`

Example:

```toml
bind = "127.0.0.1:7878"
database_path = "/home/user/.local/share/codex-memory/memory.db"
default_limit = 8
max_limit = 64
```

An example file is included at [`examples/codex-memory.toml`](./examples/codex-memory.toml).

Environment and service defaults are documented in [`docs/getting-started.md`](./docs/getting-started.md), [`docs/platforms.md`](./docs/platforms.md), [`systemd/codex-memory.service.in`](./systemd/codex-memory.service.in), and [`launchd/com.alphaone.codex-memory.plist.in`](./launchd/com.alphaone.codex-memory.plist.in).

## API surface

- `GET /health`
- `GET /v1/memories`
- `POST /v1/memories`
- `POST /v1/capture`
- `GET /v1/memories/:id`
- `PATCH /v1/memories/:id`
- `DELETE /v1/memories/:id`
- `POST /v1/memories/:id/archive`
- `POST /v1/memories/:id/reinforce`
- `POST /v1/search`
- `POST /v1/prompt`
- `GET /v1/stats`
- `POST /v1/maintenance/prune`

## Quality gates

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
./scripts/test.sh
./scripts/smoke.sh
./scripts/localhost-check.sh
./scripts/coverage.sh
```

Coverage policy:

- CI enforces a minimum line coverage threshold of `85%`
- coverage reports are generated with `cargo-llvm-cov`
- the threshold can be overridden locally with `CODEX_MEMORY_MIN_LINE_COVERAGE`

## GitHub Pages

The repository ships docs from [`docs/`](./docs/). If you publish the repo on GitHub, enable Pages from the `docs/` directory on the default branch.

## Public release hygiene

Before pushing to a public repository:

- scan the working tree and git history for credentials, keys, tokens, and private data
- verify install, uninstall, and service-management commands still work on the supported platforms
- run the automated test suite and localhost validation scripts
- prefer a no-reply or role-based author email for public commits if personal email addresses should not appear in git history

## Attribution

Maintained and published by AlphaOne LLC.
