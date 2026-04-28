#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=intent-context.sh
source "${SCRIPT_DIR}/intent-context.sh"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

PROFILE_FILE=".claude/project-profile.md"
AUTOMATION_FILE=".claude/project-automation.md"
REPORT_FILE=".claude/state/qa-report.md"
DONE_CHECK_FILE=".claude/state/done-check-report.txt"
VERIFY_REPORT_FILE=".claude/state/verify-report.md"
STATE_FILE=".claude/state/autopilot-state.json"
GOAL="${1:-autopilot-goal}"

get_profile_value() {
  local key="$1"
  grep -E "^- ${key}:" "$PROFILE_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

get_automation_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

validate_report() {
  local report="$1"
  local heading
  for heading in "# QA Report" "## Requirement Coverage" "## Findings" "## Follow Up Workstreams"; do
    if ! grep -Fqx "$heading" "$report"; then
      echo "qa-check 실패: 보고서 형식이 올바르지 않습니다. missing=$heading" >&2
      return 2
    fi
  done
}

build_prompt() {
  local project_docs file_tree recent_changes plan_body build_body review_body
  local plan_artifact build_artifact review_artifact

  # 프로젝트 문서 수집
  project_docs="$(collect_project_docs 200)"

  # 파일 트리 수집
  file_tree="$(collect_file_tree 3)"

  # 최근 변경 수집
  recent_changes="$(collect_recent_changes)"

  # 이전 intent 산출물 수집
  plan_artifact="$(find_latest_artifact "plan")"
  plan_body="$(read_artifact_body "$plan_artifact")"
  build_artifact="$(find_latest_artifact "build")"
  build_body="$(read_artifact_body "$build_artifact")"
  review_artifact="$(find_latest_artifact "review")"
  review_body="$(read_artifact_body "$review_artifact")"

  cat <<EOF
You are performing the final QA pass after verify, review, and quality gates.
Your job is to confirm that the implementation matches the original goals and identify any remaining remediation work.

Goal: ${GOAL}

## Project File Tree
\`\`\`
${file_tree}
\`\`\`
EOF

  if [ -n "$project_docs" ]; then
    cat <<EOF

## Project Documents
${project_docs}
EOF
  fi

  if [ -f "$STATE_FILE" ]; then
    cat <<EOF

## Autopilot State
\`\`\`json
$(cat "$STATE_FILE")
\`\`\`
EOF
  fi

  if [ -f "$DONE_CHECK_FILE" ]; then
    cat <<EOF

## Done Check Report
$(cat "$DONE_CHECK_FILE")
EOF
  fi

  if [ -f "$VERIFY_REPORT_FILE" ]; then
    cat <<EOF

## Verification Report
$(cat "$VERIFY_REPORT_FILE")
EOF
  fi

  if [ -n "$plan_body" ]; then
    cat <<EOF

## Plan Stage Output
${plan_body}
EOF
  fi

  if [ -n "$build_body" ]; then
    cat <<EOF

## Build Stage Output
${build_body}
EOF
  fi

  if [ -n "$review_body" ]; then
    cat <<EOF

## Review Stage Output
${review_body}
EOF
  fi

  if [ -n "$recent_changes" ]; then
    cat <<EOF

## Recent Changes
${recent_changes}
EOF
  fi

  cat <<EOF

---

Based on ALL the context above, evaluate whether the implementation satisfies the original goal and requirements after verify/review/quality have completed.

Return markdown only and include these exact headings:
# QA Report
- status: pass|fail
- summary: short summary
## Requirement Coverage
- bullet list mapping each requirement to its implementation status
## Findings
- use '- none' if there are no QA issues
- otherwise each finding must start with '- [severity:<low|medium|high>]'
## Follow Up Workstreams
- use '- none' if there is nothing to register
- otherwise each line must start with '- QA workstream:'
EOF
}

mkdir -p "$(dirname "$REPORT_FILE")"

if nightwalker_is_test_mode; then
  cat > "$REPORT_FILE" <<EOF
# QA Report
- status: pass
- summary: test mode QA pass
## Requirement Coverage
- initial requirement coverage validated in test mode
## Findings
- none
## Follow Up Workstreams
- none
EOF
  cat "$REPORT_FILE"
  exit 0
fi

if [ ! -f "$PROFILE_FILE" ] || [ ! -f "$AUTOMATION_FILE" ]; then
  echo "qa-check 실패: profile/automation 파일이 필요합니다." >&2
  exit 2
fi

engine="$(get_profile_value review_engine)"
model="$(get_profile_value review_model)"
[ -z "$model" ] && model="unset"
prompt="$(build_prompt)"

case "$engine" in
  codex)
    if [ "$model" != "unset" ]; then
      codex exec --skip-git-repo-check --model "$model" "$prompt" > "$REPORT_FILE"
    else
      codex exec --skip-git-repo-check "$prompt" > "$REPORT_FILE"
    fi
    ;;
  claude)
    if [ "$model" != "unset" ]; then
      claude --model "$model" -p "$prompt" > "$REPORT_FILE"
    else
      claude -p "$prompt" > "$REPORT_FILE"
    fi
    ;;
  *)
    echo "qa-check 실패: review_engine=$engine 는 지원되지 않습니다." >&2
    exit 2
    ;;
esac

validate_report "$REPORT_FILE"
cat "$REPORT_FILE"

status="$(grep -E '^- status:' "$REPORT_FILE" | head -n 1 | sed -E 's/^- status:[[:space:]]*//')"
if [ "$status" = "pass" ]; then
  exit 0
fi

exit 1
