#!/bin/bash

set -euo pipefail

AUTOMATION_FILE=".claude/project-automation.md"
CLASSIFIER=".claude/hooks/classify-risk.sh"
WARN_FILE=".claude/state/policy-warnings.log"
STATE_HOOK=".claude/hooks/autopilot-state.sh"

if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "risk-policy 검증 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "risk-policy 검증 실패: jq가 필요합니다." >&2
  exit 2
fi
if [ ! -x "$CLASSIFIER" ]; then
  echo "risk-policy 검증 실패: $CLASSIFIER 실행 권한이 필요합니다." >&2
  exit 2
fi

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""')

if [ -z "$COMMAND" ]; then
  exit 0
fi

split_segments() {
  local cmd="$1"
  echo "$cmd" | sed -E 's/(\&\&|\|\||\||;)/\n/g'
}

get_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//"
}

warn_or_block() {
  local msg="$1"
  local enforcement="$2"
  mkdir -p "$(dirname "$WARN_FILE")"
  printf '%s [%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "risk" "$msg" >> "$WARN_FILE"
  if [ "${AUTOPILOT_ACTIVE:-false}" = "true" ] && [ -x "$STATE_HOOK" ]; then
    "$STATE_HOOK" defer deferred_decisions "risk: $msg" >/dev/null 2>&1 || true
  fi
  if [ "$enforcement" = "block" ]; then
    echo "$msg" >&2
    exit 2
  fi
  echo "risk-policy 경고: $msg" >&2
  return 0
}

csv_contains() {
  local csv="$1"
  local needle="$2"
  echo "$csv" | tr ',' '\n' | tr -d ' ' | grep -Fxq "$needle"
}

allow_auto_push=$(get_value "allow_auto_push")
require_user=$(get_value "require_user_for_risk_tier")
auto_apply=$(get_value "auto_apply_risk_tier")
risk_enforcement=$(get_value "risk_enforcement")
risk_enforcement=${risk_enforcement:-block}
risk_tier=$("$CLASSIFIER" "$COMMAND")

while IFS= read -r raw_segment; do
  segment=$(echo "$raw_segment" | awk '{$1=$1; print}')
  [ -z "$segment" ] && continue

  if [[ "$segment" =~ ^git[[:space:]]+push([[:space:]]|$) ]] && [ "$allow_auto_push" != "true" ]; then
    warn_or_block "allow_auto_push=false 상태에서 push 감지: command=$segment" "$risk_enforcement"
  fi
done < <(split_segments "$COMMAND")

if csv_contains "$require_user" "$risk_tier"; then
  warn_or_block "${risk_tier} 변경 감지(원칙상 사용자 승인 필요): command=$COMMAND" "$risk_enforcement"
fi

if csv_contains "$auto_apply" "$risk_tier"; then
  exit 0
fi

if [ "$risk_tier" = "high" ] || [ "$risk_tier" = "critical" ]; then
  warn_or_block "${risk_tier} 티어 자동 허용 목록 외 명령 감지: command=$COMMAND" "$risk_enforcement"
fi

exit 0
