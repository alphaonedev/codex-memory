---
layout: page
title: Platforms
---

# Platforms

`codex-memory` is intended to run on:

- macOS
- Ubuntu
- Fedora

## Easiest install

1. Install Rust from `https://rustup.rs/`
2. Open Terminal
3. Clone the repository
4. Run `./scripts/install.sh`
5. Run `codex-memory health`

That is the intended beginner path.

## macOS

The install script uses `launchctl` and a user LaunchAgent. No root access is required.

Useful commands:

```bash
launchctl load ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl unload ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl list | grep codex-memory
tail -f ~/Library/Logs/codex-memory/codex-memory.stderr.log
```

## Ubuntu

The install script uses `systemctl --user`. If `systemctl --user` is unavailable in your shell session, log out and back in, then rerun the install script.

Useful commands:

```bash
systemctl --user start codex-memory.service
systemctl --user stop codex-memory.service
systemctl --user restart codex-memory.service
systemctl --user status codex-memory.service
journalctl --user -u codex-memory.service -n 50
```

## Fedora

Fedora uses the same user-level systemd flow as Ubuntu.

Useful commands:

```bash
systemctl --user start codex-memory.service
systemctl --user stop codex-memory.service
systemctl --user restart codex-memory.service
systemctl --user status codex-memory.service
journalctl --user -u codex-memory.service -n 50
```

## Manual developer run

If you do not want background service management, you can always run:

```bash
cargo run --bin codex-memoryd
```
