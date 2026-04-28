#!/bin/bash

set -euo pipefail

AUTOMATION_FILE=".claude/project-automation.md"
STATE_HOOK=".claude/hooks/autopilot-state.sh"

if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "automation gates 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

get_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//"
}

EVENT="${1:-push}"
if [ "$EVENT" != "commit" ] && [ "$EVENT" != "push" ]; then
  echo "automation gates 실패: event는 commit 또는 push여야 합니다." >&2
  exit 2
fi

run_on_commit=$(get_value "run_gates_on_commit")
run_on_push=$(get_value "run_gates_on_push")

if [ "$EVENT" = "commit" ] && [ "$run_on_commit" != "true" ]; then
  exit 0
fi
if [ "$EVENT" = "push" ] && [ "$run_on_push" != "true" ]; then
  exit 0
fi

if [ -x "$STATE_HOOK" ]; then
  "$STATE_HOOK" checkpoint "validate" "run-automation-gates event=${EVENT}"
fi

run_gate() {
  local gate_name="$1"
  local cmd="$2"
  if [ "$cmd" = "unset" ]; then
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "$gate_name" "skip" "gate is unset"
    fi
    return 0
  fi
  if [[ "$cmd" =~ ^echo[[:space:]]+ ]]; then
    echo "[automation-gate] ${gate_name}: skip (echo-placeholder: configure a real command)" >&2
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "$gate_name" "skip" "echo-placeholder: configure a real command"
    fi
    return 0
  fi
  echo "[automation-gate] ${gate_name}: ${cmd}" >&2
  if eval "$cmd"; then
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "$gate_name" "pass" "$cmd"
    fi
    return 0
  fi

  if [ -x "$STATE_HOOK" ]; then
    "$STATE_HOOK" gate "$gate_name" "fail" "$cmd"
    "$STATE_HOOK" fail "gate=${gate_name}"
  fi
  return 2
}

run_gate "lint" "$(get_value lint_cmd)"
run_gate "build" "$(get_value build_cmd)"
run_gate "test" "$(get_value test_cmd)"
run_gate "security" "$(get_value security_cmd)"

exit 0
