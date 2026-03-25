#!/usr/bin/env bash
set -euo pipefail

# Copyright (c) 2026 AlphaOne LLC
# SPDX-License-Identifier: MIT
#
# Runs line coverage with cargo-llvm-cov and enforces a minimum threshold.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
minimum_line_coverage="${CODEX_MEMORY_MIN_LINE_COVERAGE:-85}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-llvm-cov is required for coverage runs.

Recommended setup:
  rustup component add llvm-tools-preview
  cargo install cargo-llvm-cov

Then rerun:
  ./scripts/coverage.sh
EOF
  exit 1
fi

cd "${repo_root}"

cargo llvm-cov \
  --workspace \
  --all-features \
  --all-targets \
  --html \
  --fail-under-lines "${minimum_line_coverage}"

cargo llvm-cov report \
  --lcov \
  --output-path target/llvm-cov/lcov.info
