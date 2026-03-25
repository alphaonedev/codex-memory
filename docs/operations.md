# Operations

MIT licensed open source project by AlphaOne LLC.

## Service management

Linux:

```bash
systemctl --user start codex-memory.service
systemctl --user restart codex-memory.service
systemctl --user stop codex-memory.service
systemctl --user status codex-memory.service
journalctl --user -u codex-memory.service -f
```

macOS:

```bash
launchctl list | grep codex-memory
launchctl unload ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl load ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
```

Restart cleanly:

```bash
launchctl unload ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl load ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
tail -f ~/Library/Logs/codex-memory/codex-memory.stderr.log
```

## Install and remove

Install or redeploy:

```bash
./scripts/install.sh
```

Remove the user service and installed binaries:

```bash
./scripts/uninstall.sh
```

The uninstall script leaves your config and SQLite database in place.

## Backup

The default database path is under `~/.local/share/codex-memory/memory.db`. Stop the service before filesystem-level backups:

```bash
systemctl --user stop codex-memory.service
cp ~/.local/share/codex-memory/memory.db ~/backups/codex-memory.db
systemctl --user start codex-memory.service
```

## Suggested memory hygiene

- store stable preferences at high priority
- use session IDs for task-local scratch memory
- prune expired records regularly
- export TOON when passing batches of memories into an LLM prompt

## Smoke test

For a disposable end-to-end validation:

```bash
./scripts/smoke.sh
```

## Public repo security check

Before a public push or release:

```bash
rg -n --hidden --glob '!target/**' --glob '!.git/**' '(AKIA|ASIA|BEGIN PRIVATE KEY|ghp_|github_pat_|sk-|api[_-]?key|secret|password)'
git rev-list --all | xargs -r git grep -n -I -E '(AKIA|ASIA|BEGIN PRIVATE KEY|ghp_|github_pat_|sk-|api[_-]?key|secret|password)'
```

Review:

- commit author names and emails
- docs and examples for copied secrets or local home paths
- shell history snippets before copying them into docs
