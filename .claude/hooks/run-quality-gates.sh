#!/bin/bash

set -euo pipefail

AUTOMATION_FILE=".claude/project-automation.md"
STATE_HOOK=".claude/hooks/autopilot-state.sh"

if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "quality gates 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

get_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//"
}

EVENT="${1:-push}"
if [ "$EVENT" != "commit" ] && [ "$EVENT" != "push" ]; then
  echo "quality gates 실패: event는 commit 또는 push여야 합니다." >&2
  exit 2
fi

enabled=$(get_value "enable_quality_gates")
if [ "$enabled" != "true" ]; then
  exit 0
fi

run_on_commit=$(get_value "run_quality_on_commit")
run_on_push=$(get_value "run_quality_on_push")
if [ "$EVENT" = "commit" ] && [ "$run_on_commit" != "true" ]; then
  exit 0
fi
if [ "$EVENT" = "push" ] && [ "$run_on_push" != "true" ]; then
  exit 0
fi

if [ -x "$STATE_HOOK" ]; then
  "$STATE_HOOK" checkpoint "quality" "run-quality-gates event=${EVENT}"
fi

quality_cmd=$(get_value "quality_cmd")
if [ "$quality_cmd" = "unset" ]; then
  echo "quality gates 실패: enable_quality_gates=true 이면 quality_cmd를 지정해야 합니다." >&2
  exit 2
fi

if [[ "$quality_cmd" =~ ^echo[[:space:]]+ ]]; then
  echo "[quality-gate] skip (echo-placeholder: configure a real command)" >&2
  if [ -x "$STATE_HOOK" ]; then
    "$STATE_HOOK" gate "quality" "skip" "echo-placeholder: configure a real command"
  fi
else
  echo "[quality-gate] $quality_cmd" >&2
  if eval "$quality_cmd"; then
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "quality" "pass" "$quality_cmd"
    fi
  else
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "quality" "fail" "$quality_cmd"
      "$STATE_HOOK" fail "gate=quality"
    fi
    exit 2
  fi
fi

for extra_key in quality_coverage_cmd quality_perf_cmd quality_architecture_cmd; do
  extra_cmd=$(get_value "$extra_key")
  if [ "$extra_cmd" = "unset" ]; then
    continue
  fi
  if [[ "$extra_cmd" =~ ^echo[[:space:]]+ ]]; then
    echo "[quality-gate:${extra_key}] skip (echo-placeholder: configure a real command)" >&2
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "$extra_key" "skip" "echo-placeholder: configure a real command"
    fi
    continue
  fi
  echo "[quality-gate:${extra_key}] $extra_cmd" >&2
  if eval "$extra_cmd"; then
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "$extra_key" "pass" "$extra_cmd"
    fi
  else
    if [ -x "$STATE_HOOK" ]; then
      "$STATE_HOOK" gate "$extra_key" "fail" "$extra_cmd"
    fi
    exit 2
  fi
done

exit 0
