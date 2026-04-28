#!/bin/bash

set -euo pipefail

PROFILE_FILE=".claude/project-profile.md"
AUTOMATION_FILE=".claude/project-automation.md"
STATE_DIR=".claude/state/intents"

if [ ! -f "$PROFILE_FILE" ]; then
  echo "engine-intent 실패: $PROFILE_FILE 파일이 없습니다." >&2
  exit 2
fi
if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "engine-intent 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

get_profile_value() {
  local key="$1"
  grep -E "^- ${key}:" "$PROFILE_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//"
}

get_automation_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//"
}

shell_quote() {
  printf "%q" "$1"
}

sed_escape_replacement() {
  printf '%s' "$1" | sed -e 's/[\\/&]/\\&/g'
}

validate_intent_artifact() {
  local intent="$1"
  local artifact="$2"
  local required=()

  case "$intent" in
    plan)
      required=("## Goal And Constraints" "## Acceptance Criteria" "## Approach" "## Implementation Plan" "## Uncertainties")
      ;;
    build)
      required=("## Build Changes" "## Validation Results" "## Updated Files")
      ;;
    review)
      required=("## Findings" "## Applied Fixes" "## User Follow Ups")
      ;;
  esac

  if [ "${#required[@]}" -eq 0 ]; then
    return 0
  fi

  local heading
  for heading in "${required[@]}"; do
    if ! grep -Fqx "$heading" "$artifact"; then
      echo "engine-intent 실패: intent output contract 불충족 ($intent missing: $heading)" >&2
      return 1
    fi
  done

  return 0
}

INTENT="${1:-}"
GOAL="${2:-autopilot-goal}"
if [ "$INTENT" != "plan" ] && [ "$INTENT" != "build" ] && [ "$INTENT" != "review" ]; then
  echo "usage: $0 {plan|build|review} [goal]" >&2
  exit 2
fi

engine_key="${INTENT}_engine"
model_key="${INTENT}_model"
engine="$(get_profile_value "$engine_key")"
model="$(get_profile_value "$model_key")"
[ -z "$model" ] && model="unset"

runtime_mode="$(get_automation_value "engine_runtime_mode")"
allow_stub="$(get_automation_value "allow_engine_stub")"
execute_engine_commands="$(get_automation_value "execute_engine_commands")"
retry_attempts="$(get_automation_value "intent_retry_attempts")"
timeout_seconds="$(get_automation_value "intent_timeout_seconds")"
[ -z "$retry_attempts" ] && retry_attempts="1"
[ -z "$timeout_seconds" ] && timeout_seconds="0"
adapter_cmd="unset"

case "$engine" in
  codex) adapter_cmd="$(get_automation_value engine_cmd_codex)" ;;
  claude) adapter_cmd="$(get_automation_value engine_cmd_claude)" ;;
  openai) adapter_cmd="$(get_automation_value engine_cmd_openai)" ;;
  cursor) adapter_cmd="$(get_automation_value engine_cmd_cursor)" ;;
  gemini) adapter_cmd="$(get_automation_value engine_cmd_gemini)" ;;
  copilot) adapter_cmd="$(get_automation_value engine_cmd_copilot)" ;;
esac

mkdir -p "$STATE_DIR"
artifact="${STATE_DIR}/${INTENT}-$(date +%s)-$RANDOM.md"

prompt="[intent=${INTENT}] goal=${GOAL}"
cmd=""

if [ -n "$adapter_cmd" ] && [ "$adapter_cmd" != "unset" ]; then
  quoted_intent="$(shell_quote "$INTENT")"
  quoted_goal="$(shell_quote "$GOAL")"
  quoted_model="$(shell_quote "$model")"
  quoted_prompt="$(shell_quote "$prompt")"
  esc_intent="$(sed_escape_replacement "$quoted_intent")"
  esc_goal="$(sed_escape_replacement "$quoted_goal")"
  esc_model="$(sed_escape_replacement "$quoted_model")"
  esc_prompt="$(sed_escape_replacement "$quoted_prompt")"
  cmd="$(printf '%s' "$adapter_cmd" | sed \
    -e "s/{intent}/${esc_intent}/g" \
    -e "s/{goal}/${esc_goal}/g" \
    -e "s/{model}/${esc_model}/g" \
    -e "s/{prompt}/${esc_prompt}/g")"
else
  case "$engine" in
    codex)
      cmd="codex exec --skip-git-repo-check \"$prompt\""
      ;;
    claude)
      cmd="claude -p \"$prompt\""
      ;;
    openai)
      cmd="openai api responses.create -d '{\"model\":\"${model}\",\"input\":\"${prompt}\"}'"
      ;;
    cursor|gemini|copilot)
      cmd="echo \"${engine} adapter placeholder: ${prompt}\""
      ;;
    *)
      cmd="echo \"unknown engine=${engine} intent=${INTENT}\""
      ;;
  esac
fi

{
  echo "# Engine Intent Artifact"
  echo
  echo "- intent: $INTENT"
  echo "- engine: $engine"
  echo "- model: $model"
  echo "- runtime_mode: $runtime_mode"
  echo "- command: $cmd"
  echo "- goal: $GOAL"
} > "$artifact"

binary="$(echo "$cmd" | awk '{print $1}')"

run_with_timeout() {
  local timeout_value="$1"
  shift
  if [ "$timeout_value" -le 0 ]; then
    eval "$*"
    return $?
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$timeout_value" "$@" <<'PY'
import subprocess, sys
timeout = int(sys.argv[1])
cmd = " ".join(sys.argv[2:])
completed = subprocess.run(cmd, shell=True, timeout=timeout)
sys.exit(completed.returncode)
PY
    return $?
  fi

  eval "$*"
}

if [ "$execute_engine_commands" != "true" ]; then
  {
    echo
    echo "[stub]"
    echo "execute_engine_commands=false. configured command is not executed."
  } >> "$artifact"
  exit 0
fi

STATE_HOOK=".claude/hooks/autopilot-state.sh"

record_artifact() {
  if [ -x "$STATE_HOOK" ] && [ "${AUTOPILOT_ACTIVE:-false}" = "true" ]; then
    "$STATE_HOOK" checkpoint "$INTENT" "artifact=${artifact}" >/dev/null 2>&1 || true
  fi
}

if command -v "$binary" >/dev/null 2>&1; then
  attempt=1
  while [ "$attempt" -le "$retry_attempts" ]; do
    if run_with_timeout "$timeout_seconds" "$cmd" >> "$artifact" 2>&1; then
      validate_intent_artifact "$INTENT" "$artifact"
      record_artifact
      exit 0
    fi
    if [ "$attempt" -lt "$retry_attempts" ]; then
      {
        echo
        echo "[retry]"
        echo "attempt=${attempt} failed. retrying."
      } >> "$artifact"
    fi
    attempt=$((attempt + 1))
  done
  if [ "$runtime_mode" = "strict" ] || [ "$allow_stub" != "true" ]; then
    echo "engine-intent 실패: intent 실행 실패 ($INTENT/$engine)" >&2
    exit 2
  fi
  {
    echo
    echo "[stub]"
    echo "binary=${binary} execution failed. stub-fallback mode로 통과."
  } >> "$artifact"
  exit 0
fi

if [ "$runtime_mode" = "strict" ] || [ "$allow_stub" != "true" ]; then
  echo "engine-intent 실패: ${binary}를 찾을 수 없습니다. strict runtime에서는 stub이 금지됩니다." >&2
  exit 2
fi

{
  echo
  echo "[stub]"
  echo "binary=${binary} not found. stub-fallback mode로 통과."
} >> "$artifact"

exit 0
