#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=intent-context.sh
source "${SCRIPT_DIR}/intent-context.sh"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

PROFILE_FILE=".claude/project-profile.md"
AUTOMATION_FILE=".claude/project-automation.md"
REPORT_FILE=".claude/state/verify-report.md"
STATE_FILE=".claude/state/autopilot-state.json"
GOAL="${1:-autopilot-goal}"

get_profile_value() {
  local key="$1"
  grep -E "^- ${key}:" "$PROFILE_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

validate_report() {
  local report="$1"
  local heading
  for heading in "# Verification Report" "## Acceptance Criteria Coverage" "## Findings" "## Fix Recommendations"; do
    if ! grep -Fqx "$heading" "$report"; then
      echo "verify-check 실패: 보고서 형식이 올바르지 않습니다. missing=$heading" >&2
      return 2
    fi
  done
}

build_prompt() {
  local project_docs file_tree recent_changes plan_body build_body acceptance_body
  local plan_artifact build_artifact

  project_docs="$(collect_project_docs 200)"
  file_tree="$(collect_file_tree 3)"
  recent_changes="$(collect_recent_changes)"
  acceptance_body="$(read_doc_file "docs/acceptance-criteria.md" 200)"
  plan_artifact="$(find_latest_artifact "plan")"
  plan_body="$(read_artifact_body "$plan_artifact")"
  build_artifact="$(find_latest_artifact "build")"
  build_body="$(read_artifact_body "$build_artifact")"

  cat <<EOF
You are verifying whether the implementation satisfies the project's acceptance criteria and original goal.
This is a verification pass, not a code review.

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

  if [ -n "$acceptance_body" ]; then
    cat <<EOF

## Acceptance Criteria
${acceptance_body}
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

  if [ -n "$recent_changes" ]; then
    cat <<EOF

## Recent Changes
${recent_changes}
EOF
  fi

  cat <<EOF

---

Evaluate whether the implementation satisfies the acceptance criteria and the original goal.
Focus on requirement coverage, behavior, and missing pieces. Do not perform style-only review.

Return markdown only and include these exact headings:
# Verification Report
- status: pass|fail
- summary: short summary
## Acceptance Criteria Coverage
- map each acceptance criterion or requirement to pass|partial|fail with rationale
## Findings
- use '- none' if there are no verification issues
- otherwise each finding must start with '- [severity:<low|medium|high>]'
## Fix Recommendations
- use '- none' if there is nothing left to fix
- otherwise describe the smallest changes required to satisfy the missing criteria
EOF
}

mkdir -p "$(dirname "$REPORT_FILE")"

if nightwalker_is_test_mode; then
  cat > "$REPORT_FILE" <<EOF
# Verification Report
- status: pass
- summary: test mode verification pass
## Acceptance Criteria Coverage
- acceptance criteria validated in test mode
## Findings
- none
## Fix Recommendations
- none
EOF
  cat "$REPORT_FILE"
  exit 0
fi

if [ ! -f "$PROFILE_FILE" ] || [ ! -f "$AUTOMATION_FILE" ]; then
  echo "verify-check 실패: profile/automation 파일이 필요합니다." >&2
  exit 2
fi

engine="$(get_profile_value verify_engine)"
[ -z "$engine" ] && engine="$(get_profile_value review_engine)"
model="$(get_profile_value verify_model)"
[ -z "$model" ] && model="$(get_profile_value review_model)"
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
    echo "verify-check 실패: verify engine=$engine 는 지원되지 않습니다." >&2
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
