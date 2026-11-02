#!/usr/bin/env bash
# ABOUTME: Runs the canonical firmware check in the isolated source-rebuild probe.
# ABOUTME: Keeps public configuration and fresh build output separate from production.
set -euo pipefail
if [[ $# != 2 || ! -d "$1/.git" ]]; then
  echo 'Usage: check-recipient.sh RECIPIENT_GIT_DIRECTORY OUTPUT_LOG' >&2
  exit 2
fi
recipient=$(realpath "$1")
output=$(realpath -m "$2")
cd "$recipient"
env -i HOME="$HOME" USER="${USER:?}" \
  PATH="$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" \
  LANG=C.UTF-8 GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0 \
  scripts/check --firmware > "$output" 2>&1
