#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=intent-context.sh
source "${SCRIPT_DIR}/intent-context.sh"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

INTENT="${1:-}"
GOAL="${2:-autopilot-goal}"
MODEL="${3:-unset}"

if [ -z "$INTENT" ]; then
  echo "usage: $0 <intent> [goal] [model]" >&2
  exit 2
fi

build_context_block() {
  local intent="$1"
  local goal="$2"
  local ctx=""

  # 프로젝트 파일 트리
  local tree
  tree="$(collect_file_tree 3)"
  if [ -n "$tree" ]; then
    ctx="${ctx}

## Project File Tree
\`\`\`
${tree}
\`\`\`"
  fi

  # 프로젝트 문서
  local docs
  docs="$(collect_project_docs 200)"
  if [ -n "$docs" ]; then
    ctx="${ctx}

## Project Documents
${docs}"
  fi

  # intent별 이전 단계 산출물 주입
  case "$intent" in
    build)
      local plan_artifact plan_body
      plan_artifact="$(find_latest_artifact "plan")"
      plan_body="$(read_artifact_body "$plan_artifact")"
      if [ -n "$plan_body" ]; then
        ctx="${ctx}

## Plan Output (from previous stage)
${plan_body}"
      fi
      ;;
    review)
      local plan_artifact plan_body build_artifact build_body
      plan_artifact="$(find_latest_artifact "plan")"
      plan_body="$(read_artifact_body "$plan_artifact")"
      build_artifact="$(find_latest_artifact "build")"
      build_body="$(read_artifact_body "$build_artifact")"
      if [ -n "$plan_body" ]; then
        ctx="${ctx}

## Plan Output (from plan stage)
${plan_body}"
      fi
      if [ -n "$build_body" ]; then
        ctx="${ctx}

## Build Output (from build stage)
${build_body}"
      fi

      # review에는 최근 변경 내역도 포함
      local changes
      changes="$(collect_recent_changes)"
      if [ -n "$changes" ]; then
        ctx="${ctx}

## Recent Changes
${changes}"
      fi

      # build step 실행 로그
      local build_log
      build_log="$(collect_build_log)"
      if [ -n "$build_log" ]; then
        ctx="${ctx}

## Build Step Log
\`\`\`
${build_log}
\`\`\`"
      fi

      # 보류된 결정과 가정
      local deferred
      deferred="$(collect_deferred_items)"
      if [ -n "$deferred" ]; then
        ctx="${ctx}

## Deferred Decisions And Assumptions
${deferred}"
      fi
      ;;
  esac

  echo "$ctx"
}

build_prompt() {
  local intent="$1"
  local goal="$2"
  local context
  context="$(build_context_block "$intent" "$goal")"

  case "$intent" in
    plan)
      cat <<EOF
[intent=plan] goal=${goal}
${context}

You are creating an implementation plan for the goal above.
Inspect the project files, documents, and structure provided.
For each Implementation Plan item, use a numbered list.
If a step is safe to run concurrently with other independent steps, prefix it with [parallel_safe].
If a step depends on earlier steps, add [depends_on:1,2] using step numbers.

Return markdown only.
You must include these exact headings:
## Goal And Constraints
## Acceptance Criteria
## Approach
## Implementation Plan
## Uncertainties
EOF
      ;;
    build)
      cat <<EOF
[intent=build] goal=${goal}
${context}

You are implementing the plan above.
Follow the Implementation Plan from the plan output exactly.
Make the required code/document changes before answering.
If the plan specifies specific files, modules, or interfaces, implement those.

Return markdown only.
You must include these exact headings:
## Build Changes
## Validation Results
## Updated Files
EOF
      ;;
    review)
      cat <<EOF
[intent=review] goal=${goal}
${context}

You are reviewing the implementation against the original plan.
Check that every item in the Implementation Plan was addressed.
Verify code quality, test coverage, and security.
Apply fixes for clear issues directly. Flag ambiguous items for user follow-up.

Return markdown only.
You must include these exact headings:
## Findings
## Applied Fixes
## User Follow Ups
EOF
      ;;
    *)
      printf '[intent=%s] goal=%s\n%s\n' "$intent" "$goal" "$context"
      ;;
  esac
}

PROMPT="$(build_prompt "$INTENT" "$GOAL")"

if nightwalker_is_test_mode; then
  case "$INTENT" in
    plan)
      cat <<EOF
## Goal And Constraints
- goal: ${GOAL}
## Acceptance Criteria
- acceptance criteria captured in test mode
## Approach
- test mode
## Implementation Plan
1. noop
## Uncertainties
- none
EOF
      ;;
    build)
      cat <<EOF
## Build Changes
- test mode
## Validation Results
- skipped in test mode
## Updated Files
- none
EOF
      ;;
    review)
      cat <<EOF
## Findings
- none
## Applied Fixes
- none
## User Follow Ups
- none
EOF
      ;;
  esac
  exit 0
fi

if [ "$MODEL" != "unset" ]; then
  claude --model "$MODEL" -p "$PROMPT"
  exit 0
fi

claude -p "$PROMPT"
