#!/usr/bin/env bash
set -euo pipefail

# Copyright (c) 2026 AlphaOne LLC
# SPDX-License-Identifier: MIT
#
# Standard project test runner for local contributors.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

source "${HOME}/.cargo/env"

cargo test --all-targets
