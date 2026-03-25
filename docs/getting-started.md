# Getting Started

MIT licensed open source project by AlphaOne LLC.

## Fastest path for non-technical users

1. Install Rust from `https://rustup.rs/`
2. Download or clone this repository
3. Open Terminal in the project folder
4. Run `./scripts/install.sh`
5. Run `codex-memory health`

If the command prints `ok`, the local memory daemon is running.

## Local development

Run the daemon:

```bash
cargo run --bin codex-memoryd
```

In another shell:

```bash
cargo run --bin codex-memory -- add \
  --content "Repo requires concise answers" \
  --kind constraint \
  --tag repo

cargo run --bin codex-memory -- prompt repo --format toon
```

## User service install

Install with systemd user services:

```bash
./scripts/install.sh
```

This performs four steps:

1. builds release binaries
2. installs them into `~/.local/bin`
3. writes `~/.config/codex-memory/config.toml` if it does not already exist
4. starts a background user service using `launchctl` on macOS or `systemctl --user` on Linux

## Verify

```bash
codex-memory health
codex-memory stats
systemctl --user status codex-memory.service
journalctl --user -u codex-memory.service -n 50
```

## Remove

```bash
./scripts/uninstall.sh
```

That removes the user service and binaries but leaves your config and SQLite database intact.
