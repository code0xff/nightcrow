#!/bin/bash

# check-codex-plugin.sh — codex MCP server availability check
#
# Usage: check-codex-plugin.sh check
# Output: plugin | cli | none
#
# Decision order:
#   1. .mcp.json defines mcpServers.codex AND the MCP package is locally available → plugin
#   2. codex CLI is installed → cli
#   3. otherwise → none
#
# Plugin detection uses npx --no-install to verify the package is already
# cached/installed, avoiding network calls and false positives when only
# npx itself is present but the package is not installed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# Allow REPO_ROOT override for testing
REPO_ROOT="${REPO_ROOT:-$(cd "${SCRIPT_DIR}/../.." && pwd)}"
MCP_JSON="${REPO_ROOT}/.mcp.json"

# Check if the MCP server package is actually available (without downloading it)
is_mcp_server_available() {
  local cmd="$1"
  local args_json="$2"

  # For npx-based commands, extract the package name from args and verify
  # it's already installed/cached using --no-install (no network required)
  if [ "$cmd" = "npx" ] && command -v npx >/dev/null 2>&1; then
    local pkg
    pkg="$(echo "$args_json" | jq -r '[.[] | select(startswith("-") | not)] | first // empty' 2>/dev/null || true)"
    if [ -n "$pkg" ]; then
      # Some MCP server packages do not exit cleanly on --version, so bound the probe.
      if command -v python3 >/dev/null 2>&1; then
        if python3 - "$pkg" <<'PY' >/dev/null 2>&1
import subprocess
import sys

pkg = sys.argv[1]
try:
    completed = subprocess.run(
        ["npx", "--no-install", pkg, "--version"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=3,
    )
    raise SystemExit(completed.returncode)
except subprocess.TimeoutExpired:
    raise SystemExit(1)
PY
        then
          return 0
        fi
      elif npx --no-install "$pkg" --version >/dev/null 2>&1; then
        return 0
      fi
    fi
    return 1
  fi

  # For other commands, check if the binary exists and is executable
  if command -v "$cmd" >/dev/null 2>&1; then
    return 0
  fi

  return 1
}

check_plugin() {
  # Check .mcp.json for codex MCP server configuration
  if [ -f "$MCP_JSON" ] && command -v jq >/dev/null 2>&1; then
    local server_cmd args_json
    server_cmd="$(jq -r '.mcpServers.codex.command // empty' "$MCP_JSON" 2>/dev/null || true)"
    args_json="$(jq -c '.mcpServers.codex.args // []' "$MCP_JSON" 2>/dev/null || echo "[]")"
    if [ -n "$server_cmd" ] && is_mcp_server_available "$server_cmd" "$args_json"; then
      echo "plugin"
      return
    fi
  fi

  # codex CLI fallback
  if command -v codex >/dev/null 2>&1; then
    echo "cli"
    return
  fi

  echo "none"
}

CMD="${1:-check}"

case "$CMD" in
  check)
    check_plugin
    ;;
  *)
    echo "usage: $0 check" >&2
    exit 2
    ;;
esac
