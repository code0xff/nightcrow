#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

CONTRACT_FILE="${CONTRACT_FILE:-.claude/completion-contract.md}"
SESSION_FILE="${SESSION_FILE:-$(nightwalker_resolve_session_file)}"

if [ ! -f "$CONTRACT_FILE" ]; then
  echo "completion-contract 검증 실패: $CONTRACT_FILE 파일이 없습니다." >&2
  exit 2
fi

required_keys=(
  "done_enforcement"
  "artifact_definition"
  "artifact_check_cmd"
  "run_smoke_cmd"
  "acceptance_test_cmd"
  "release_readiness_cmd"
)

for key in "${required_keys[@]}"; do
  if ! grep -Eq "^- ${key}:[[:space:]]*.+" "$CONTRACT_FILE"; then
    echo "completion-contract 검증 실패: '${key}' 값이 없습니다." >&2
    exit 2
  fi
done

get_value() {
  local key="$1"
  grep -E "^- ${key}:" "$CONTRACT_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//"
}

get_session_value() {
  local key="$1"
  [ -f "$SESSION_FILE" ] || return 0
  grep -E "^${key}:" "$SESSION_FILE" | head -n 1 | sed -E "s/^${key}:[[:space:]]*//" || true
}

done_enforcement="$(get_value done_enforcement)"
if [ "$done_enforcement" != "report" ] && [ "$done_enforcement" != "block" ]; then
  echo "completion-contract 검증 실패: done_enforcement는 report 또는 block이어야 합니다." >&2
  exit 2
fi

# system-platform archetype은 추가 완료 기준 키를 요구한다
project_archetype="$(get_session_value project_archetype)"
if [ "$project_archetype" = "system-platform" ]; then
  system_platform_keys=(
    "interface_contract_check"
    "compatibility_check"
    "failure_mode_check"
    "operability_check"
  )
  for key in "${system_platform_keys[@]}"; do
    if ! grep -Eq "^- ${key}:[[:space:]]*.+" "$CONTRACT_FILE"; then
      echo "completion-contract 검증 실패: system-platform 필수 키 '${key}'가 없습니다." >&2
      exit 2
    fi
  done
fi

exit 0
