# codex-memory

Local-first persistent memory for Codex, implemented as a lightweight Rust daemon and CLI.

- [Getting Started](./getting-started)
- [Quickstart](./quickstart)
- [Operations](./operations)
- [Platforms](./platforms)
- [Testing](./testing)
- [License](https://github.com/alphaonedev/codex-memory/blob/main/LICENSE)

## For Non-Technical Users

Copy and paste these commands one at a time:

```bash
git clone https://github.com/alphaonedev/codex-memory.git
cd codex-memory
./scripts/install.sh
codex-memory health
```

If the last command prints `ok`, the local memory daemon is running.

Store your first memory:

```bash
codex-memory add --content "I prefer concise technical answers" --kind preference --project-id my-project --tag user
codex-memory search concise --project-id my-project
```

If you want the easiest full walkthrough, use the [Quickstart](./quickstart) page.

## Goals

- Keep durable memory outside the model session
- Retrieve the highest-value local context quickly
- Export prompt context in TOON to reduce token cost
- Stay simple enough to self-host on a laptop without extra services

## System overview

`codex-memoryd` serves a localhost HTTP API backed by SQLite. `codex-memory` is the operator-facing CLI. Memories are stored with metadata such as kind, scope, session, tags, priority, and optional expiration. Full-text search is provided through SQLite FTS5.

## Memory model

Each memory record stores:

- content
- optional summary
- kind
- scope
- source
- tags
- priority
- session
- role
- timestamps
- optional expiry

## TOON prompt compression

Prompt export supports JSON and TOON. TOON is used because repeated field names dominate prompt cost when memory results are table-shaped. TOON declares headers once:

```toon
memories[2]{id,kind,priority,tags,summary,content}:
  a1	preference	90	style|user	terse	User prefers terse responses
  a2	constraint	85	rust|safety	ffi	Avoid unsafe unless justified
```

That makes large memory snapshots cheaper to hand back to a model than equivalent JSON arrays.

## Suggested integrations

- wrap Codex CLI startup with a preflight `prompt` call
- append explicit user preferences after important conversations
- store repo-specific decisions from CI failures or incident response
- export session-scoped memory for large multi-step tasks

## Deployment

The repo includes:

- `scripts/install.sh` for local install
- `scripts/uninstall.sh` for cleanup
- `scripts/smoke.sh` for end-to-end validation
- `systemd/codex-memory.service.in` for user-service deployment
- `launchd/com.alphaone.codex-memory.plist.in` for macOS LaunchAgent deployment

## License and attribution

Licensed under MIT. Copyright 2026 AlphaOne LLC.

## Repo publishing

This project is structured for a public GitHub repo. Enable GitHub Pages from the `docs/` folder to publish this page directly.
