#!/bin/bash

set -euo pipefail

# Write/Edit/MultiEdit 도구가 핵심 정책/규칙 파일을 직접 수정할 때 경고하거나 차단한다.
# preapproval_enforcement=block 이면 차단, 그 외에는 경고 로그만 남긴다.

AUTOMATION_FILE=".claude/project-automation.md"
WARN_FILE=".claude/state/policy-warnings.log"

if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

if [ -z "$FILE_PATH" ]; then
  exit 0
fi

PROTECTED_PATH_PATTERNS=(
  "CLAUDE.md"
  ".claude/project-profile.md"
  ".claude/project-approvals.md"
  ".claude/project-automation.md"
  ".claude/completion-contract.md"
  ".claude/settings.json"
  ".claude/rules/"
  ".claude/skills/"
)

is_protected_path() {
  local path="$1"
  for pattern in "${PROTECTED_PATH_PATTERNS[@]}"; do
    if [[ "$path" == *"$pattern" ]]; then
      return 0
    fi
  done
  return 1
}

get_enforcement() {
  if [ -f "$AUTOMATION_FILE" ]; then
    grep -E "^- preapproval_enforcement:" "$AUTOMATION_FILE" | head -n 1 \
      | sed -E "s/^- preapproval_enforcement:[[:space:]]*//" || true
  fi
}

warn_or_block() {
  local msg="$1"
  local enforcement="$2"
  mkdir -p "$(dirname "$WARN_FILE")"
  printf '%s [file-protection] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$msg" >> "$WARN_FILE"
  if [ "$enforcement" = "block" ]; then
    echo "파일 보호 정책: $msg" >&2
    exit 2
  fi
  echo "파일 보호 경고: $msg" >&2
}

if is_protected_path "$FILE_PATH"; then
  enforcement="$(get_enforcement)"
  [ -z "$enforcement" ] && enforcement="report"
  warn_or_block "핵심 정책 파일 직접 수정 감지: $FILE_PATH (autonomy.md '사용자 확인 필요' 대상)" "$enforcement"
fi

exit 0
