#!/bin/bash

set -euo pipefail

APPROVALS_FILE=".claude/project-approvals.md"
AUTOMATION_FILE=".claude/project-automation.md"
WARN_FILE=".claude/state/policy-warnings.log"
STATE_HOOK=".claude/hooks/autopilot-state.sh"

if [ ! -f "$APPROVALS_FILE" ]; then
  echo "pre-approval 검증 실패: $APPROVALS_FILE 파일이 없습니다." >&2
  exit 2
fi
if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "pre-approval 검증 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "pre-approval 검증 실패: jq가 필요합니다." >&2
  exit 2
fi

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""')

if [ -z "$COMMAND" ]; then
  exit 0
fi

trim() {
  echo "$1" | awk '{$1=$1; print}'
}

split_segments() {
  local cmd="$1"
  echo "$cmd" | sed -E 's/(\&\&|\|\||\||;)/\n/g'
}

is_allowlisted() {
  local cmd="$1"
  local allowlist
  allowlist=$(awk '
    /^## Command Prefix Allowlist$/ { in_section=1; next }
    /^## / && in_section==1 { in_section=0 }
    in_section==1 && /^- `/ {
      line=$0
      sub(/^- `/, "", line)
      sub(/`$/, "", line)
      print line
    }
  ' "$APPROVALS_FILE")

  while IFS= read -r prefix; do
    [ -z "$prefix" ] && continue
    if [[ "$cmd" == "$prefix"* ]]; then
      return 0
    fi
  done <<< "$allowlist"

  return 1
}

get_automation_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

warn_or_block() {
  local msg="$1"
  local enforcement="$2"
  mkdir -p "$(dirname "$WARN_FILE")"
  printf '%s [%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "preapproval" "$msg" >> "$WARN_FILE"
  if [ "${AUTOPILOT_ACTIVE:-false}" = "true" ] && [ -x "$STATE_HOOK" ]; then
    "$STATE_HOOK" defer manual_followups "preapproval: $msg" >/dev/null 2>&1 || true
  fi
  if [ "$enforcement" = "block" ]; then
    echo "$msg" >&2
    exit 2
  fi
  echo "pre-approval 경고: $msg" >&2
  return 0
}

is_high_risk_pattern() {
  local cmd="$1"
  [[ "$cmd" =~ (^|[[:space:]])(npm|pnpm|yarn)[[:space:]]+(install|add|remove|uninstall|update)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])pip([0-9.]*)[[:space:]]+(install|uninstall)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])poetry[[:space:]]+(add|remove|update)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])cargo[[:space:]]+(add|remove)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])go[[:space:]]+get([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ git[[:space:]]+branch[[:space:]]+(-D|--delete)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ git[[:space:]]+push.*(--force|-f)([[:space:]]|$) ]] && return 0
  return 1
}

is_mutating_command() {
  local cmd="$1"
  [[ "$cmd" =~ ^git[[:space:]]+(add|commit|push|tag|branch|merge|rebase|cherry-pick|revert)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])(npm|pnpm|yarn|pip|pip3|poetry|cargo|go|make|uv)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])(rm|mv|cp|chmod|chown|mkdir|touch)([[:space:]]|$) ]] && return 0
  [[ "$cmd" =~ (^|[[:space:]])sed[[:space:]].*-i([[:space:]]|$) ]] && return 0
  return 1
}

# 세그먼트 단위로 검사하여 복합 커맨드 우회를 막는다.
enforcement_mode="$(get_automation_value preapproval_enforcement)"
if [ -z "$enforcement_mode" ]; then
  enforcement_mode="block"
fi

while IFS= read -r raw_segment; do
  segment=$(trim "$raw_segment")
  [ -z "$segment" ] && continue

  # 사용자 명시 승인이 필요한 패턴은 항상 차단한다.
  if is_high_risk_pattern "$segment"; then
    if ! is_allowlisted "$segment"; then
      warn_or_block "고위험 명령 미승인: command=$segment" "$enforcement_mode"
    fi
    continue
  fi

  # 변경 가능성이 높은 명령은 사전 승인된 prefix만 허용한다.
  if is_mutating_command "$segment"; then
    if ! is_allowlisted "$segment"; then
      warn_or_block "사전 승인되지 않은 변경 명령: command=$segment" "$enforcement_mode"
    fi
  fi
done < <(split_segments "$COMMAND")

exit 0
