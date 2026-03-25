#!/usr/bin/env bash
set -euo pipefail

# Copyright (c) 2026 AlphaOne LLC
# SPDX-License-Identifier: MIT

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
cleanup() {
  if [[ -n "${daemon_pid:-}" ]]; then
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

cat > "${tmp_dir}/config.toml" <<EOF
bind = "127.0.0.1:8787"
database_path = "${tmp_dir}/memory.db"
default_limit = 8
max_limit = 64
EOF

cargo build --manifest-path "${repo_root}/Cargo.toml"
"${repo_root}/target/debug/codex-memoryd" --config "${tmp_dir}/config.toml" &
daemon_pid=$!

for _ in {1..30}; do
  if "${repo_root}/target/debug/codex-memory" --config "${tmp_dir}/config.toml" health >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

"${repo_root}/target/debug/codex-memory" --config "${tmp_dir}/config.toml" add \
  --content "User prefers terse answers" \
  --kind preference \
  --tag style >/dev/null

"${repo_root}/target/debug/codex-memory" --config "${tmp_dir}/config.toml" search terse >/dev/null
"${repo_root}/target/debug/codex-memory" --config "${tmp_dir}/config.toml" prompt terse --format toon >/dev/null

echo "smoke test passed"
