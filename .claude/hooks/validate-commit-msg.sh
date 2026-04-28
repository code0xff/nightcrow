#!/bin/bash
# 커밋 메시지 형식 검증: type: message
# commits.md에 정의된 type: feat, fix, refactor, test, docs, chore

set -euo pipefail

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""')

# git commit -m 이 아닌 경우 통과
if ! echo "$COMMAND" | grep -q 'git commit.*-m'; then
  exit 0
fi

VALID_TYPES="feat|fix|refactor|test|docs|chore"

# 커밋 메시지에 유효한 type: message 패턴이 있는지 확인
if echo "$COMMAND" | grep -qE "($VALID_TYPES): .+"; then
  exit 0
fi

echo "커밋 메시지가 'type: message' 형식이 아닙니다. (type: $VALID_TYPES)" >&2
exit 2
