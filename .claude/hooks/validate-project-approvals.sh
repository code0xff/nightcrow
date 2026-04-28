#!/bin/bash

set -euo pipefail

APPROVALS_FILE=".claude/project-approvals.md"

if [ ! -f "$APPROVALS_FILE" ]; then
  echo "project-approvals 검증 실패: $APPROVALS_FILE 파일이 없습니다." >&2
  echo "프로젝트 시작 시 사전 승인 범위를 먼저 정의하세요." >&2
  exit 2
fi

required_sections=(
  "^## Command Prefix Allowlist$"
  "^## Always Require Explicit Approval$"
  "^## Sandbox / Escalation Policy$"
)

for section in "${required_sections[@]}"; do
  if ! grep -Eq "$section" "$APPROVALS_FILE"; then
    echo "project-approvals 검증 실패: 필수 섹션이 없습니다. pattern=${section}" >&2
    exit 2
  fi
done

if ! grep -Eq '^- `[^`]+`$' "$APPROVALS_FILE"; then
  echo "project-approvals 검증 실패: Command Prefix Allowlist에 백틱 명령 목록이 필요합니다." >&2
  exit 2
fi

exit 0
