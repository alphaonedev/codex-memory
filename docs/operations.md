# Operations

MIT licensed open source project by AlphaOne LLC.

## Service management

Linux:

```bash
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
tail -f ~/Library/Logs/codex-memory/codex-memory.stderr.log
```

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
