#!/bin/bash

set -euo pipefail

STATE_DIR=".claude/state"
STATE_FILE="${STATE_DIR}/autopilot-state.json"

if ! command -v jq >/dev/null 2>&1; then
  echo "autopilot-state 실패: jq가 필요합니다." >&2
  exit 2
fi

mkdir -p "$STATE_DIR"

init_state_file() {
  if [ ! -f "$STATE_FILE" ]; then
    cat > "$STATE_FILE" <<'EOF'
{
  "session_id": "",
  "goal": "",
  "status": "idle",
  "current_cycle": 0,
  "last_stage": "",
  "last_gate": "",
  "last_gate_result": "",
  "updated_at": "",
  "error": "",
  "deferred_decisions": [],
  "assumptions": [],
  "manual_followups": [],
  "qa_remediations": [],
  "history": []
}
EOF
  fi
}

timestamp_utc() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

append_event() {
  local event="$1"
  local stage="$2"
  local detail="$3"
  local now
  now="$(timestamp_utc)"
  local tmp
  tmp="$(mktemp)"
  jq \
    --arg event "$event" \
    --arg stage "$stage" \
    --arg detail "$detail" \
    --arg now "$now" \
    '.history += [{"event": $event, "stage": $stage, "detail": $detail, "at": $now}] | .updated_at = $now' \
    "$STATE_FILE" > "$tmp"
  mv "$tmp" "$STATE_FILE"
}

update_state() {
  local tmp
  tmp="$(mktemp)"
  jq "$@" "$STATE_FILE" > "$tmp"
  mv "$tmp" "$STATE_FILE"
}

init_state_file

tmp="$(mktemp)"
jq '
  .deferred_decisions = (.deferred_decisions // [])
  | .assumptions = (.assumptions // [])
  | .manual_followups = (.manual_followups // [])
  | .qa_remediations = (.qa_remediations // [])
' "$STATE_FILE" > "$tmp"
mv "$tmp" "$STATE_FILE"

ACTION="${1:-show}"
shift || true

case "$ACTION" in
  start)
    GOAL="${1:-}"
    SID="$(date +%s)-$RANDOM"
    NOW="$(timestamp_utc)"
    update_state --arg sid "$SID" --arg goal "$GOAL" --arg now "$NOW" '
      .session_id = $sid
      | .goal = $goal
      | .status = "running"
      | .current_cycle = 1
      | .last_stage = "plan"
      | .last_gate = ""
      | .last_gate_result = ""
      | .error = ""
      | .deferred_decisions = []
      | .assumptions = []
      | .manual_followups = []
      | .qa_remediations = []
      | .updated_at = $now
      | .history = [{"event":"start","stage":"plan","detail":"autopilot session started","at":$now}]
    '
    ;;
  cycle)
    CYCLE="${1:-0}"
    update_state --argjson cycle "$CYCLE" '.current_cycle = $cycle'
    append_event "cycle" "loop" "cycle=$CYCLE"
    ;;
  checkpoint)
    STAGE="${1:-}"
    NOTE="${2:-}"
    update_state --arg stage "$STAGE" '.last_stage = $stage'
    append_event "checkpoint" "$STAGE" "$NOTE"
    ;;
  gate)
    GATE_NAME="${1:-}"
    RESULT="${2:-}"
    DETAIL="${3:-}"
    update_state --arg gate "$GATE_NAME" --arg result "$RESULT" '
      .last_stage = "validate"
      | .last_gate = $gate
      | .last_gate_result = $result
    '
    append_event "gate" "$GATE_NAME" "$DETAIL"
    ;;
  defer)
    KIND="${1:-manual_followups}"
    DETAIL="${2:-}"
    NOW="$(timestamp_utc)"
    case "$KIND" in
      deferred_decisions|assumptions|manual_followups) ;;
      *)
        echo "autopilot-state 실패: 알 수 없는 defer kind='$KIND'" >&2
        exit 2
        ;;
    esac
    update_state --arg kind "$KIND" --arg detail "$DETAIL" --arg now "$NOW" '
      .[$kind] += [{"detail": $detail, "at": $now}]
    '
    append_event "defer" "$KIND" "$DETAIL"
    ;;
  qa-remediation)
    SLUG="${1:-}"
    FILE_PATH="${2:-}"
    NOW="$(timestamp_utc)"
    update_state --arg slug "$SLUG" --arg file "$FILE_PATH" --arg now "$NOW" '
      .qa_remediations += [{"slug": $slug, "file": $file, "at": $now}]
    '
    append_event "qa-remediation" "qa" "slug=${SLUG} file=${FILE_PATH}"
    ;;
  fail)
    REASON="${1:-unknown}"
    update_state --arg reason "$REASON" '.status = "failed" | .error = $reason'
    append_event "failed" "error" "$REASON"
    ;;
  complete)
    update_state '.status = "completed" | .error = ""'
    append_event "completed" "done" "autopilot session completed"
    ;;
  show)
    cat "$STATE_FILE"
    ;;
  *)
    echo "usage: $0 {start <goal>|cycle <n>|checkpoint <stage> [note]|gate <name> <pass|fail> [detail]|defer <deferred_decisions|assumptions|manual_followups> <detail>|qa-remediation <slug> <file>|fail <reason>|complete|show}" >&2
    exit 2
    ;;
esac

exit 0
