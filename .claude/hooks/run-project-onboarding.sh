#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

SESSION_FILE="$(nightwalker_resolve_session_file)"
AUTOMATION_FILE=".claude/project-automation.md"
SUGGEST_HOOK=".claude/hooks/suggest-automation-gates.sh"
BOOTSTRAP_HOOK=".claude/hooks/bootstrap-init-harness.sh"
RENDER_HOOK=".claude/hooks/render-onboarding-docs.sh"
PROFILE_HOOK=".claude/hooks/validate-project-profile.sh"
APPROVALS_HOOK=".claude/hooks/validate-project-approvals.sh"
AUTOMATION_HOOK=".claude/hooks/validate-project-automation.sh"
CONTRACT_HOOK=".claude/hooks/validate-completion-contract.sh"
AUTOPILOT_HOOK=".claude/hooks/run-autopilot.sh"

require_hook() {
  local hook="$1"
  if [ ! -x "$hook" ]; then
    echo "run-project-onboarding 실패: 실행 권한이 없는 hook: $hook" >&2
    exit 2
  fi
}

ensure_session_file() {
  nightwalker_ensure_session_storage
  SESSION_FILE="$(nightwalker_session_file_default)"
  if [ -f "$SESSION_FILE" ]; then
    return 0
  fi

  cat > "$SESSION_FILE" <<'YAML'
schema_version: 1
status: draft
project_goal: unset
target_users: unset
core_features: unset
constraints: unset
project_archetype: unset
stack_candidates: unset
recommended_stack: unset
selected_stack: unset
open_questions: unset
decisions: unset
current_increment: 1
increment_status: unset
last_delivered_at: unset
YAML
}

get_value() {
  local key="$1"
  grep -E "^${key}:" "$SESSION_FILE" | head -n 1 | sed -E "s/^${key}:[[:space:]]*//" || true
}

set_status() {
  local next_status="$1"
  awk -v value="$next_status" '
    BEGIN { updated = 0 }
    $0 ~ /^status:/ {
      print "status: " value
      updated = 1
      next
    }
    { print }
    END {
      if (updated == 0) {
        print "status: " value
      }
    }
  ' "$SESSION_FILE" > "${SESSION_FILE}.tmp"
  mv "${SESSION_FILE}.tmp" "$SESSION_FILE"
}

is_ready_for_execution() {
  local goal users features archetype stack
  goal="$(get_value project_goal)"
  users="$(get_value target_users)"
  features="$(get_value core_features)"
  archetype="$(get_value project_archetype)"
  stack="$(get_value selected_stack)"

  if [ -z "$goal" ] || [ "$goal" = "unset" ] || \
     [ -z "$users" ] || [ "$users" = "unset" ] || \
     [ -z "$features" ] || [ "$features" = "unset" ] || \
     [ -z "$archetype" ] || [ "$archetype" = "unset" ] || \
     [ -z "$stack" ] || [ "$stack" = "unset" ]; then
    return 1
  fi

  return 0
}

has_required_gate_setup() {
  local lint_cmd build_cmd test_cmd security_cmd quality_cmd
  lint_cmd="$(get_automation_value lint_cmd)"
  build_cmd="$(get_automation_value build_cmd)"
  test_cmd="$(get_automation_value test_cmd)"
  security_cmd="$(get_automation_value security_cmd)"
  quality_cmd="$(get_automation_value quality_cmd)"

  if [ -z "$lint_cmd" ] || [ "$lint_cmd" = "unset" ] || \
     [ -z "$build_cmd" ] || [ "$build_cmd" = "unset" ] || \
     [ -z "$test_cmd" ] || [ "$test_cmd" = "unset" ] || \
     [ -z "$security_cmd" ] || [ "$security_cmd" = "unset" ] || \
     [ -z "$quality_cmd" ] || [ "$quality_cmd" = "unset" ]; then
    return 1
  fi

  return 0
}

has_required_done_check_setup() {
  local artifact_cmd smoke_cmd
  artifact_cmd="$(grep -E "^- artifact_check_cmd:" .claude/completion-contract.md | head -n 1 | sed -E 's/^- artifact_check_cmd:[[:space:]]*//' || true)"
  smoke_cmd="$(grep -E "^- run_smoke_cmd:" .claude/completion-contract.md | head -n 1 | sed -E 's/^- run_smoke_cmd:[[:space:]]*//' || true)"

  if [ -z "$artifact_cmd" ] || [ "$artifact_cmd" = "unset" ] || \
     [ -z "$smoke_cmd" ] || [ "$smoke_cmd" = "unset" ] || \
     [[ "$artifact_cmd" =~ ^echo[[:space:]]+ ]] || \
     [[ "$smoke_cmd" =~ ^echo[[:space:]]+ ]]; then
    return 1
  fi

  return 0
}

is_ready_for_autopilot() {
  is_ready_for_execution && has_required_gate_setup && has_required_done_check_setup
}

get_automation_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

set_automation_key() {
  local key="$1"
  local value="$2"
  awk -v key="$key" -v value="$value" '
    BEGIN { updated = 0 }
    $0 ~ "^- " key ":" {
      print "- " key ": " value
      updated = 1
      next
    }
    { print }
    END {
      if (updated == 0) {
        print "- " key ": " value
      }
    }
  ' "$AUTOMATION_FILE" > "${AUTOMATION_FILE}.tmp"
  mv "${AUTOMATION_FILE}.tmp" "$AUTOMATION_FILE"
}

sync_gate_enforcement() {
  local lint_cmd build_cmd test_cmd security_cmd
  lint_cmd="$(get_automation_value lint_cmd)"
  build_cmd="$(get_automation_value build_cmd)"
  test_cmd="$(get_automation_value test_cmd)"
  security_cmd="$(get_automation_value security_cmd)"

  local is_placeholder=false
  for _cmd in "$lint_cmd" "$build_cmd" "$test_cmd" "$security_cmd"; do
    if [ "$_cmd" = "unset" ] || [[ "$_cmd" =~ ^echo[[:space:]]+ ]]; then
      is_placeholder=true
      break
    fi
  done

  if [ "$is_placeholder" = "true" ]; then
    # Gate commands not fully configured — keep enforcement non-blocking.
    set_automation_key "run_gates_on_push" "false"
    set_automation_key "run_quality_on_push" "false"
    set_automation_key "run_gates_on_commit" "false"
    set_automation_key "run_quality_on_commit" "false"
    set_automation_key "enable_quality_gates" "false"
  else
    # All gate commands configured — restore enforcement to active.
    set_automation_key "run_gates_on_push" "true"
    set_automation_key "run_quality_on_push" "true"
    set_automation_key "run_gates_on_commit" "true"
    set_automation_key "run_quality_on_commit" "true"
    set_automation_key "enable_quality_gates" "true"
  fi
}

build_autopilot_goal() {
  local goal users stack
  goal="$(get_value project_goal)"
  users="$(get_value target_users)"
  stack="$(get_value selected_stack)"

  echo "${goal} [target_users=${users}; selected_stack=${stack}; execution_mode=plan_all_workstreams_then_build_sequentially; verification_mode=acceptance_first]"
}

maybe_start_autopilot() {
  local previous_status="$1"
  local auto_start goal
  auto_start="$(get_automation_value auto_start_autopilot_on_ready)"

  if [ "$auto_start" != "true" ]; then
    return 0
  fi
  if [ "${AUTOPILOT_ACTIVE:-false}" = "true" ]; then
    return 0
  fi
  if [ "$previous_status" = "ready" ]; then
    return 0
  fi
  if ! is_ready_for_autopilot; then
    return 0
  fi
  if [ ! -x "$AUTOPILOT_HOOK" ]; then
    echo "run-project-onboarding 실패: 실행 권한이 없는 hook: $AUTOPILOT_HOOK" >&2
    exit 2
  fi

  goal="$(build_autopilot_goal)"
  echo "run-project-onboarding: onboarding ready, starting autopilot."
  "$AUTOPILOT_HOOK" start "$goal"
}

render_ready_report() {
  local goal users archetype stack status
  goal="$(get_value project_goal)"
  users="$(get_value target_users)"
  archetype="$(get_value project_archetype)"
  stack="$(get_value selected_stack)"
  status="ready"

  if ! is_ready_for_execution; then
    status="pending-input"
  elif ! has_required_gate_setup || ! has_required_done_check_setup; then
    status="pending-automation"
  fi

  if [ "$status" = "ready" ]; then
    set_status "ready"
  else
    set_status "proposed"
  fi

  cat > ONBOARDING_READY.md <<EOF2
# Onboarding Ready Report

- status: ${status}
- project_goal: ${goal:-unset}
- target_users: ${users:-unset}
- project_archetype: ${archetype:-unset}
- selected_stack: ${stack:-unset}

## First Workstreams

1. Finalize contracts and boundaries from docs/architecture.md
2. Confirm acceptance criteria in docs/acceptance-criteria.md
3. Implement core flow from docs/execution-plan.md
4. Add build/test/security gates from .claude/project-automation.md

## Next Action

- If status is ready, run a quick docs sanity pass for docs/architecture.md, docs/roadmap.md, and docs/acceptance-criteria.md, then continue directly into autopilot without asking for next-action selection.
- If status is pending-input, fill project_goal, target_users, project_archetype, and selected_stack first.
- If status is pending-automation, confirm build/test/security/quality gates and completion contract commands before enabling autopilot.
EOF2

  if [ "$status" = "pending-input" ]; then
    echo "run-project-onboarding 경고: session.yaml에 미확정 값이 있습니다 (project_goal/target_users/core_features/project_archetype/selected_stack)."
  elif [ "$status" = "pending-automation" ]; then
    echo "run-project-onboarding 경고: autopilot 시작 전 gate/quality 또는 completion contract 명령을 먼저 확정해야 합니다."
  fi
}

require_hook "$SUGGEST_HOOK"
require_hook "$BOOTSTRAP_HOOK"
require_hook "$RENDER_HOOK"
require_hook "$PROFILE_HOOK"
require_hook "$APPROVALS_HOOK"
require_hook "$AUTOMATION_HOOK"
require_hook "$CONTRACT_HOOK"

ensure_session_file
previous_status="$(get_value status)"

"$SUGGEST_HOOK" >/dev/null
"$BOOTSTRAP_HOOK" >/dev/null
sync_gate_enforcement
"$PROFILE_HOOK"
"$APPROVALS_HOOK"
"$AUTOMATION_HOOK"
"$CONTRACT_HOOK"
"$RENDER_HOOK" >/dev/null
render_ready_report
maybe_start_autopilot "$previous_status"

echo "run-project-onboarding 완료: 문서/정책/검증이 최신화되었습니다."
exit 0
