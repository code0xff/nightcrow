#!/bin/bash

set -euo pipefail

PROFILE_FILE=".claude/project-profile.md"

if [ ! -f "$PROFILE_FILE" ]; then
  echo "project-profile 검증 실패: $PROFILE_FILE 파일이 없습니다." >&2
  echo "먼저 /init-harness로 프로파일을 고정하세요." >&2
  exit 2
fi

extract_value() {
  local key="$1"
  local value
  value=$(grep -E "^- ${key}:" "$PROFILE_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true)
  echo "$value"
}

PROFILE_NAME=$(extract_value "profile_name")
PLAN_ENGINE=$(extract_value "plan_engine")
BUILD_ENGINE=$(extract_value "build_engine")
REVIEW_ENGINE=$(extract_value "review_engine")
PLAN_GATE=$(extract_value "plan_gate")
REVIEW_GATE=$(extract_value "review_gate")

required_keys=("profile_name" "plan_engine" "build_engine" "review_engine" "plan_gate" "review_gate")
for key in "${required_keys[@]}"; do
  if [ -z "$(extract_value "$key")" ]; then
    echo "project-profile 검증 실패: '${key}' 값이 비어 있습니다." >&2
    exit 2
  fi
done

valid_engine_re='^(claude|codex|openai|cursor|gemini|copilot|user-selected)$'
if ! echo "$PLAN_ENGINE" | grep -Eq "$valid_engine_re"; then
  echo "project-profile 검증 실패: plan_engine='${PLAN_ENGINE}' 는 허용되지 않는 값입니다." >&2
  exit 2
fi
if ! echo "$BUILD_ENGINE" | grep -Eq "$valid_engine_re"; then
  echo "project-profile 검증 실패: build_engine='${BUILD_ENGINE}' 는 허용되지 않는 값입니다." >&2
  exit 2
fi
if ! echo "$REVIEW_ENGINE" | grep -Eq "$valid_engine_re"; then
  echo "project-profile 검증 실패: review_engine='${REVIEW_ENGINE}' 는 허용되지 않는 값입니다." >&2
  exit 2
fi

if [ "$PLAN_GATE" != "required" ] && [ "$PLAN_GATE" != "recommended" ]; then
  echo "project-profile 검증 실패: plan_gate는 required 또는 recommended여야 합니다." >&2
  exit 2
fi
if [ "$REVIEW_GATE" != "required" ] && [ "$REVIEW_GATE" != "recommended" ]; then
  echo "project-profile 검증 실패: review_gate는 required 또는 recommended여야 합니다." >&2
  exit 2
fi

if [ "$PROFILE_NAME" = "generic-ai" ]; then
  if [ "$PLAN_ENGINE" = "user-selected" ] || [ "$BUILD_ENGINE" = "user-selected" ] || [ "$REVIEW_ENGINE" = "user-selected" ]; then
    echo "project-profile 검증 실패: generic-ai는 user-selected placeholder를 허용하지 않습니다." >&2
    echo "실제 엔진 이름으로 고정하세요." >&2
    exit 2
  fi
fi

exit 0
