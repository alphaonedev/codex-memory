#!/usr/bin/env bash
set -euo pipefail

# Copyright (c) 2026 AlphaOne LLC
# SPDX-License-Identifier: MIT
#
# End-to-end localhost validation against a temporary daemon and transcript ingest flow.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmpdir="$(mktemp -d)"

cleanup() {
  if [[ -n "${daemon_pid:-}" ]]; then
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
  rm -rf "${tmpdir}"
}
trap cleanup EXIT

cat > "${tmpdir}/config.toml" <<EOF
bind = "127.0.0.1:8899"
database_path = "${tmpdir}/memory.db"
default_limit = 8
max_limit = 64
EOF

cat > "${tmpdir}/transcript.json" <<'EOF'
[
  {"role":"user","content":"Please avoid unsafe Rust in this repo. I prefer concise answers."},
  {"role":"assistant","content":"Decision: use sqlite for local state and axum for the daemon."}
]
EOF

cd "${repo_root}"
source "${HOME}/.cargo/env"

cargo build >/dev/null
./target/debug/codex-memoryd --config "${tmpdir}/config.toml" >"${tmpdir}/daemon.log" 2>&1 &
daemon_pid=$!

for _ in {1..40}; do
  if ./target/debug/codex-memory --config "${tmpdir}/config.toml" health >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

./target/debug/codex-memory --config "${tmpdir}/config.toml" add \
  --content "Repository uses Rust and Axum" \
  --kind fact \
  --project-id demo-repo \
  --task-id task-1 \
  --tag rust \
  --tag axum >/dev/null

./target/debug/codex-memory --config "${tmpdir}/config.toml" ingest-transcript \
  --file "${tmpdir}/transcript.json" \
  --project-id demo-repo \
  --task-id task-1 \
  --session live-test >/dev/null

./target/debug/codex-memory --config "${tmpdir}/config.toml" search rust \
  --project-id demo-repo \
  --task-id task-1 >/dev/null

./target/debug/codex-memory --config "${tmpdir}/config.toml" prompt rust \
  --project-id demo-repo \
  --task-id task-1 \
  --format toon \
  --token-budget 500 >/dev/null

./target/debug/codex-memory --config "${tmpdir}/config.toml" stats >/dev/null

echo "localhost memory check passed"
