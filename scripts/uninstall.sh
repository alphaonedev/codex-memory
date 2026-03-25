#!/usr/bin/env bash
set -euo pipefail

# Copyright (c) 2026 AlphaOne LLC
# SPDX-License-Identifier: MIT
#
# Cross-platform user uninstall for macOS LaunchAgent and Linux systemd user service.

service_name="codex-memory.service"
launchd_name="com.alphaone.codex-memory.plist"
bin_dir="${HOME}/.local/bin"
systemd_dir="${HOME}/.config/systemd/user"
launchd_dir="${HOME}/Library/LaunchAgents"
os_name="$(uname -s)"

case "${os_name}" in
  Darwin)
    launchd_path="${launchd_dir}/${launchd_name}"
    launchctl unload "${launchd_path}" >/dev/null 2>&1 || true
    rm -f "${launchd_path}"
    ;;
  Linux)
    if systemctl --user --quiet is-enabled "${service_name}" 2>/dev/null; then
      systemctl --user disable --now "${service_name}"
    else
      systemctl --user stop "${service_name}" 2>/dev/null || true
    fi
    rm -f "${systemd_dir}/${service_name}"
    systemctl --user daemon-reload
    ;;
  *)
    echo "unsupported operating system: ${os_name}" >&2
    exit 1
    ;;
esac

rm -f "${bin_dir}/codex-memory" "${bin_dir}/codex-memoryd"

echo "removed service registration and installed binaries"
echo "config and database were left in place"
