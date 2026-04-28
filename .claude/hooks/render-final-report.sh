#!/bin/bash

set -euo pipefail

STATE_FILE=".claude/state/autopilot-state.json"
DONE_FILE=".claude/state/done-check-report.txt"
UNSET_FILE=".claude/state/unset-config-report.txt"
QA_FILE=".claude/state/qa-report.md"
VERIFY_FILE=".claude/state/verify-report.md"
OUT_FILE=".claude/state/final-report.md"

if [ ! -f "$STATE_FILE" ]; then
  echo "final-report 실패: $STATE_FILE 파일이 없습니다." >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "final-report 실패: jq가 필요합니다." >&2
  exit 2
fi

mkdir -p "$(dirname "$OUT_FILE")"

goal="$(jq -r '.goal // ""' "$STATE_FILE")"
status="$(jq -r '.status // ""' "$STATE_FILE")"
cycle="$(jq -r '.current_cycle // 0' "$STATE_FILE")"
deferred_count="$(jq '.deferred_decisions | length' "$STATE_FILE")"
assumption_count="$(jq '.assumptions | length' "$STATE_FILE")"
followup_count="$(jq '.manual_followups | length' "$STATE_FILE")"

{
  echo "# Final Report"
  echo
  echo "- goal: ${goal}"
  echo "- status: ${status}"
  echo "- cycles: ${cycle}"
  echo
  echo "## Summary"
  echo
  echo "- deferred_decisions: ${deferred_count}"
  echo "- assumptions: ${assumption_count}"
  echo "- manual_followups: ${followup_count}"
  echo
  echo "## Deferred Decisions"
  jq -r 'if (.deferred_decisions | length) == 0 then "- none" else .deferred_decisions[] | "- " + .detail end' "$STATE_FILE"
  echo
  echo "## Assumptions"
  jq -r 'if (.assumptions | length) == 0 then "- none" else .assumptions[] | "- " + .detail end' "$STATE_FILE"
  echo
  echo "## Manual Follow Ups"
  jq -r 'if (.manual_followups | length) == 0 then "- none" else .manual_followups[] | "- " + .detail end' "$STATE_FILE"
  echo
  echo "## Done Check"
  if [ -f "$DONE_FILE" ]; then
    sed 's/^/- /' "$DONE_FILE"
  else
    echo "- none"
  fi
  echo
  echo "## Unset Config"
  if [ -f "$UNSET_FILE" ]; then
    sed 's/^/- /' "$UNSET_FILE"
  else
    echo "- none"
  fi
  echo
  echo "## Latest Verification"
  if [ -f "$VERIFY_FILE" ]; then
    sed 's/^/- /' "$VERIFY_FILE"
  else
    echo "- none"
  fi
  echo
  echo "## Latest QA"
  if [ -f "$QA_FILE" ]; then
    sed 's/^/- /' "$QA_FILE"
  else
    echo "- none"
  fi
} > "$OUT_FILE"

cat "$OUT_FILE"
