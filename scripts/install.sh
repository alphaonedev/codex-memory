#!/usr/bin/env bash
set -euo pipefail

# Copyright (c) 2026 AlphaOne LLC
# SPDX-License-Identifier: MIT
#
# Cross-platform user install for macOS LaunchAgent and Linux systemd user service.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
config_dir="${HOME}/.config/codex-memory"
data_dir="${HOME}/.local/share/codex-memory"
systemd_dir="${HOME}/.config/systemd/user"
launchd_dir="${HOME}/Library/LaunchAgents"
log_dir="${HOME}/Library/Logs/codex-memory"
service_name="codex-memory.service"
launchd_name="com.alphaone.codex-memory.plist"
os_name="$(uname -s)"

mkdir -p "${bin_dir}" "${config_dir}" "${data_dir}"

echo "building release binaries"
cargo build --release --manifest-path "${repo_root}/Cargo.toml"

install -m 0755 "${repo_root}/target/release/codex-memory" "${bin_dir}/codex-memory"
install -m 0755 "${repo_root}/target/release/codex-memoryd" "${bin_dir}/codex-memoryd"

config_path="${config_dir}/config.toml"
if [[ ! -f "${config_path}" ]]; then
  cp "${repo_root}/examples/codex-memory.toml" "${config_path}"
  sed -i "s|/tmp/codex-memory/memory.db|${data_dir}/memory.db|g" "${config_path}"
  echo "wrote default config to ${config_path}"
else
  echo "keeping existing config at ${config_path}"
fi

service_path="${systemd_dir}/${service_name}"

case "${os_name}" in
  Darwin)
    mkdir -p "${launchd_dir}" "${log_dir}"
    launchd_path="${launchd_dir}/${launchd_name}"
    sed \
      -e "s|@CODEX_MEMORYD@|${bin_dir}/codex-memoryd|g" \
      -e "s|@CONFIG_PATH@|${config_path}|g" \
      -e "s|@LOG_DIR@|${log_dir}|g" \
      "${repo_root}/launchd/com.alphaone.codex-memory.plist.in" > "${launchd_path}"
    launchctl unload "${launchd_path}" >/dev/null 2>&1 || true
    launchctl load "${launchd_path}"
    echo
    echo "installed LaunchAgent ${launchd_name}"
    echo "status: launchctl list | grep codex-memory"
    ;;
  Linux)
    mkdir -p "${systemd_dir}"
    sed \
      -e "s|@CODEX_MEMORYD@|${bin_dir}/codex-memoryd|g" \
      -e "s|@CONFIG_PATH@|${config_path}|g" \
      "${repo_root}/systemd/codex-memory.service.in" > "${service_path}"
    if ! systemctl --user daemon-reload >/dev/null 2>&1; then
      echo >&2
      echo "install completed, but the user systemd bus is not available in this environment." >&2
      echo "binaries installed to ${bin_dir}" >&2
      echo "config written to ${config_path}" >&2
      echo "on a normal desktop login session, rerun ./scripts/install.sh or run:" >&2
      echo "  systemctl --user daemon-reload" >&2
      echo "  systemctl --user enable --now ${service_name}" >&2
      exit 0
    fi
    systemctl --user enable --now "${service_name}"
    echo
    echo "installed ${service_name}"
    echo "status: systemctl --user status ${service_name}"
    ;;
  *)
    echo "unsupported operating system: ${os_name}" >&2
    exit 1
    ;;
esac

echo "health: ${bin_dir}/codex-memory health"
