#!/bin/bash

# run-codex-intent.sh — codex engine adapter
#
# Plugin mode: codex-mcp-server configured in .mcp.json and locally available.
#   Runs via claude -p; prompts Claude to use codex MCP tools in-session.
# CLI mode: codex CLI installed. Runs via codex exec.
# None mode: neither available. Falls back to claude -p without codex tools.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=intent-context.sh
source "${SCRIPT_DIR}/intent-context.sh"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

PLUGIN_CHECK="${SCRIPT_DIR}/check-codex-plugin.sh"
INTENT="${1:-}"
GOAL="${2:-autopilot-goal}"
MODEL="${3:-unset}"

if [ -z "$INTENT" ]; then
  echo "usage: $0 <intent> [goal] [model]" >&2
  exit 2
fi

# ── 컨텍스트 빌드 (기존과 동일) ──

build_context_block() {
  local intent="$1"
  local goal="$2"
  local ctx=""

  local tree
  tree="$(collect_file_tree 3)"
  if [ -n "$tree" ]; then
    ctx="${ctx}

## Project File Tree
\`\`\`
${tree}
\`\`\`"
  fi

  local docs
  docs="$(collect_project_docs 200)"
  if [ -n "$docs" ]; then
    ctx="${ctx}

## Project Documents
${docs}"
  fi

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

      local changes
      changes="$(collect_recent_changes)"
      if [ -n "$changes" ]; then
        ctx="${ctx}

## Recent Changes
${changes}"
      fi

      local build_log
      build_log="$(collect_build_log)"
      if [ -n "$build_log" ]; then
        ctx="${ctx}

## Build Step Log
\`\`\`
${build_log}
\`\`\`"
      fi

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

# ── 플러그인 모드 프롬프트 (Claude 세션에서 codex 도구 사용 지시) ──

build_plugin_prompt() {
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

IMPORTANT: You have access to the Codex plugin. Use the codex tool to delegate the planning work:
- Call /codex:rescue with the goal and context to generate a high-quality plan
- If /codex:rescue is not available, create the plan yourself using all context provided

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

You may have access to the Codex MCP plugin. If the codex tools are available in this session, use /codex:rescue to delegate complex implementation subtasks.
If codex tools are not available or do not respond, implement the plan yourself using all context provided.

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

IMPORTANT: You have access to the Codex plugin. Use it for review:
- Call /codex:review for standard code review
- Call /codex:adversarial-review for deep analysis of design decisions and tradeoffs
- Synthesize the codex review findings with your own analysis
- If codex tools are not available, perform the review yourself

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

# ── CLI fallback 프롬프트 (codex exec 직접 호출) ──

build_cli_prompt() {
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

# ── 테스트 모드 ──

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

# ── 실행 모드 결정 ──

codex_mode="cli"
if [ -x "$PLUGIN_CHECK" ]; then
  codex_mode="$("$PLUGIN_CHECK" check)"
fi

case "$codex_mode" in
  plugin)
    # 플러그인 모드: Claude Code 세션 내에서 codex 도구 사용
    PROMPT="$(build_plugin_prompt "$INTENT" "$GOAL")"
    if [ "$MODEL" != "unset" ]; then
      claude --model "$MODEL" -p "$PROMPT"
    else
      claude -p "$PROMPT"
    fi
    ;;
  cli)
    # CLI fallback: codex exec 직접 호출
    PROMPT="$(build_cli_prompt "$INTENT" "$GOAL")"
    if [ "$MODEL" != "unset" ]; then
      codex exec --skip-git-repo-check --model "$MODEL" "$PROMPT"
    else
      codex exec --skip-git-repo-check "$PROMPT"
    fi
    ;;
  none)
    # codex unavailable -> fallback to Claude without codex tools
    echo "codex-intent warning: codex plugin and CLI both unavailable, falling back to Claude (no codex tools will be used)" >&2
    PROMPT="$(build_cli_prompt "$INTENT" "$GOAL")"
    if [ "$MODEL" != "unset" ]; then
      claude --model "$MODEL" -p "$PROMPT"
    else
      claude -p "$PROMPT"
    fi
    ;;
esac
