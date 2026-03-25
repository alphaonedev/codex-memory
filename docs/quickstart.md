---
layout: page
title: Quickstart
---

# Quickstart

MIT licensed open source project by AlphaOne LLC.

## Fastest install for most people

1. Install Rust from `https://rustup.rs/`
2. Download or clone this repository
3. Open Terminal in the project folder
4. Run:

```bash
./scripts/install.sh
```

5. Then run:

```bash
codex-memory health
```

If you see `ok`, the daemon is working.

## First memory

Run:

```bash
codex-memory add \
  --content "I prefer concise technical answers" \
  --kind preference \
  --project-id my-project \
  --tag user
```

Then verify:

```bash
codex-memory search concise --project-id my-project
```

## If something goes wrong

macOS:

```bash
launchctl list | grep codex-memory
tail -n 50 ~/Library/Logs/codex-memory/codex-memory.stderr.log
```

Ubuntu or Fedora:

```bash
systemctl --user status codex-memory.service
journalctl --user -u codex-memory.service -n 50
```

## Service control

Linux:

```bash
systemctl --user start codex-memory.service
systemctl --user stop codex-memory.service
systemctl --user restart codex-memory.service
```

macOS:

```bash
launchctl load ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
launchctl unload ~/Library/LaunchAgents/com.alphaone.codex-memory.plist
```

## Remove it

```bash
./scripts/uninstall.sh
```
