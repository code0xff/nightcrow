#!/bin/bash

# intent-context.sh — plan/build/review 단계 간 컨텍스트 전달 유틸리티
# source로 로드하여 사용한다.

set -euo pipefail

INTENT_DIR=".claude/state/intents"

# 특정 intent의 최신 artifact 파일 경로를 반환한다.
# 없으면 빈 문자열을 반환한다.
find_latest_artifact() {
  local intent_type="$1"
  local found=""
  if [ -d "$INTENT_DIR" ]; then
    found="$(ls -t "$INTENT_DIR"/${intent_type}-*.md 2>/dev/null | head -n 1 || true)"
  fi
  echo "$found"
}

# artifact 파일 내용을 읽어서 반환한다. 메타데이터 헤더는 제거하고 본문만 반환.
read_artifact_body() {
  local artifact_path="$1"
  if [ -z "$artifact_path" ] || [ ! -f "$artifact_path" ]; then
    echo ""
    return 0
  fi
  # "# Engine Intent Artifact" 헤더 블록 이후의 실제 내용만 추출
  awk '
    BEGIN { past_header=0; blank_after_header=0 }
    /^## / { past_header=1 }
    past_header==1 { print }
    !past_header && /^$/ && NR>1 { blank_after_header++ }
  ' "$artifact_path"
}

read_doc_file() {
  local doc_path="$1"
  local max_lines="${2:-200}"
  if [ -z "$doc_path" ] || [ ! -f "$doc_path" ]; then
    echo ""
    return 0
  fi
  head -n "$max_lines" "$doc_path"
}

# 프로젝트 문서 파일들을 읽어서 컨텍스트 블록으로 반환한다.
# 존재하는 파일만 포함하며, 각 파일을 최대 200줄까지 잘라서 포함한다.
collect_project_docs() {
  local max_lines="${1:-200}"
  local docs=""
  local doc_files=(
    "docs/project-goal.md"
    "docs/scope.md"
    "docs/architecture.md"
    "docs/stack-decision.md"
    "docs/acceptance-criteria.md"
    "docs/roadmap.md"
    "docs/execution-plan.md"
  )

  for f in "${doc_files[@]}"; do
    if [ -f "$f" ]; then
      local content
      content="$(head -n "$max_lines" "$f")"
      docs="${docs}

--- ${f} ---
${content}"
    fi
  done

  echo "$docs"
}

# 프로젝트 파일 트리를 간결하게 반환한다 (최대 depth 3, 숨김 제외).
collect_file_tree() {
  local max_depth="${1:-3}"
  if command -v find >/dev/null 2>&1; then
    find . -maxdepth "$max_depth" \
      -not -path './.git/*' \
      -not -path './.claude/state/*' \
      -not -name '.DS_Store' \
      -type f 2>/dev/null | sort | head -n 150
  fi
}

# 최근 git 변경 요약을 반환한다.
collect_recent_changes() {
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "=== Staged/Unstaged Changes ==="
    git diff --stat HEAD 2>/dev/null | tail -n 20 || true
    echo ""
    echo "=== Recent Commits ==="
    git log --oneline -10 2>/dev/null || true
  fi
}

# intent artifact를 intent 유형별로 최신 N개만 남기고 정리한다.
cleanup_old_artifacts() {
  local keep="${1:-20}"
  [ -d "$INTENT_DIR" ] || return 0
  local intent_type
  for intent_type in plan build review build-step; do
    local old_files
    old_files="$(ls -t "${INTENT_DIR}/${intent_type}"-*.md 2>/dev/null | tail -n "+$((keep + 1))" || true)"
    if [ -n "$old_files" ]; then
      echo "$old_files" | xargs rm -f
    fi
  done
}

# build step 로그를 반환한다.
collect_build_log() {
  local log_file=".claude/state/build-steps.log"
  if [ -f "$log_file" ]; then
    tail -n 50 "$log_file"
  fi
}

# autopilot state에서 deferred/assumptions/followups를 반환한다.
collect_deferred_items() {
  local state_file=".claude/state/autopilot-state.json"
  if [ -f "$state_file" ] && command -v jq >/dev/null 2>&1; then
    local deferred assumptions followups
    deferred="$(jq -r 'if (.deferred_decisions | length) == 0 then "" else .deferred_decisions[] | "- [deferred] " + .detail end' "$state_file" 2>/dev/null || true)"
    assumptions="$(jq -r 'if (.assumptions | length) == 0 then "" else .assumptions[] | "- [assumption] " + .detail end' "$state_file" 2>/dev/null || true)"
    followups="$(jq -r 'if (.manual_followups | length) == 0 then "" else .manual_followups[] | "- [followup] " + .detail end' "$state_file" 2>/dev/null || true)"
    local result=""
    [ -n "$deferred" ] && result="${result}${deferred}"$'\n'
    [ -n "$assumptions" ] && result="${result}${assumptions}"$'\n'
    [ -n "$followups" ] && result="${result}${followups}"$'\n'
    echo "$result"
  fi
}
