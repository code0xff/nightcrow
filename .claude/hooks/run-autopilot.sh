#!/bin/bash

set -euo pipefail

AUTOMATION_FILE=".claude/project-automation.md"
STATE_FILE=".claude/state/autopilot-state.json"
STATE_HOOK=".claude/hooks/autopilot-state.sh"
GATE_HOOK=".claude/hooks/run-automation-gates.sh"
QUALITY_HOOK=".claude/hooks/run-quality-gates.sh"
ENGINE_HOOK=".claude/hooks/run-engine-intent.sh"
ENGINE_READY_HOOK=".claude/hooks/check-engine-readiness.sh"
UNSET_REPORT_HOOK=".claude/hooks/report-unset-config.sh"
DONE_CHECK_HOOK=".claude/hooks/run-done-check.sh"
QA_REGISTER_HOOK=".claude/hooks/register-qa-workstream.sh"
FINAL_REPORT_HOOK=".claude/hooks/render-final-report.sh"

if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "run-autopilot 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "run-autopilot 실패: jq가 필요합니다." >&2
  exit 2
fi

if [ ! -x "$STATE_HOOK" ]; then
  echo "run-autopilot 실패: $STATE_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi

if [ ! -x "$GATE_HOOK" ]; then
  echo "run-autopilot 실패: $GATE_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$QUALITY_HOOK" ]; then
  echo "run-autopilot 실패: $QUALITY_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$ENGINE_HOOK" ]; then
  echo "run-autopilot 실패: $ENGINE_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$ENGINE_READY_HOOK" ]; then
  echo "run-autopilot 실패: $ENGINE_READY_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$UNSET_REPORT_HOOK" ]; then
  echo "run-autopilot 실패: $UNSET_REPORT_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$DONE_CHECK_HOOK" ]; then
  echo "run-autopilot 실패: $DONE_CHECK_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$QA_REGISTER_HOOK" ]; then
  echo "run-autopilot 실패: $QA_REGISTER_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi
if [ ! -x "$FINAL_REPORT_HOOK" ]; then
  echo "run-autopilot 실패: $FINAL_REPORT_HOOK 실행 권한이 필요합니다." >&2
  exit 2
fi

get_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

normalize_plan_cmd() {
  local cmd="$1"
  local legacy_prefix=".claude/hooks/run-project-onboarding.sh"
  local trimmed

  trimmed="$(echo "$cmd" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
  if [ -z "$trimmed" ] || [ "$trimmed" = "unset" ]; then
    echo "unset"
    return 0
  fi

  if [ "$trimmed" = "$legacy_prefix" ]; then
    echo "unset"
    return 0
  fi

  if [[ "$trimmed" == "$legacy_prefix && "* ]]; then
    echo "${trimmed#${legacy_prefix} && }"
    return 0
  fi

  echo "$trimmed"
}

has_worktree_changes() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return 1
  fi
  if ! git diff --quiet --ignore-submodules --; then
    return 0
  fi
  if ! git diff --cached --quiet --ignore-submodules --; then
    return 0
  fi
  if [ -n "$(git ls-files --others --exclude-standard)" ]; then
    return 0
  fi
  return 1
}

has_upstream_branch() {
  git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' >/dev/null 2>&1
}

# autopilot start 시 session과 roadmap의 increment 상태를 정렬
_align_increment_status() {
  local roadmap_hook session_hook session_file cur_incr status
  roadmap_hook="$(dirname "$STATE_HOOK")/roadmap-state.sh"
  session_hook="$(dirname "$STATE_HOOK")/nightwalker-session.sh"

  [ -f "$roadmap_hook" ] && [ -f "$session_hook" ] || return 0

  # shellcheck source=nightwalker-session.sh
  source "$session_hook"
  session_file="$(nightwalker_resolve_session_file 2>/dev/null || true)"
  [ -f "$session_file" ] || return 0

  cur_incr="$(nightwalker_read_current_increment "$session_file")"

  # roadmap에서 pending increment를 active로 승격
  # shellcheck source=roadmap-state.sh
  source "$roadmap_hook"
  status="$(get_increment_status "${cur_incr}" 2>/dev/null || echo "pending")"
  if [ "$status" = "pending" ]; then
    mark_increment_active "${cur_incr}" 2>/dev/null || true
  fi

  # session increment_status가 unset/pending이면 in-progress로 설정
  awk '
    /^increment_status:/ {
      val = $2
      if (val == "unset" || val == "pending" || val == "") {
        print "increment_status: in-progress"
      } else { print }
      next
    }
    { print }
  ' "$session_file" > "${session_file}.tmp" && mv "${session_file}.tmp" "$session_file"
}

build_commit_message() {
  local goal="$1"
  local normalized
  normalized="$(printf '%s' "$goal" | tr '\n' ' ' | sed -E 's/\[[^]]+\]//g; s/[^[:alnum:][:space:]_.\/-]+/ /g; s/[[:space:]]+/ /g; s/^ //; s/ $//')"
  normalized="$(printf '%s' "$normalized" | cut -c1-64)"
  if [ -z "$normalized" ]; then
    normalized="validated changes"
  fi
  printf 'chore: autopilot apply %s' "$normalized"
}

infer_stage_cmd() {
  local stage="$1"
  case "$stage" in
    implement)
      local build_cmd test_cmd
      build_cmd="$(get_value build_cmd)"
      test_cmd="$(get_value test_cmd)"
      if [ -n "$build_cmd" ] && [ "$build_cmd" != "unset" ]; then
        echo "$build_cmd"
        return 0
      fi
      if [ -n "$test_cmd" ] && [ "$test_cmd" != "unset" ]; then
        echo "$test_cmd"
        return 0
      fi
      ;;
    review)
      local quality_cmd test_cmd
      quality_cmd="$(get_value quality_cmd)"
      test_cmd="$(get_value test_cmd)"
      if [ -n "$quality_cmd" ] && [ "$quality_cmd" != "unset" ]; then
        echo "$quality_cmd"
        return 0
      fi
      if [ -n "$test_cmd" ] && [ "$test_cmd" != "unset" ]; then
        echo "$test_cmd"
        return 0
      fi
      ;;
  esac
  echo "unset"
}

run_stage_cmd() {
  local stage="$1"
  local cmd="$2"
  local source="$3"
  "$STATE_HOOK" checkpoint "$stage" "${source}: ${cmd}"
  if eval "$cmd"; then
    "$STATE_HOOK" checkpoint "$stage" "ok (${source})"
    return 0
  fi
  return 1
}

run_stage_with_fallback() {
  local stage="$1"
  local cmd="$2"
  local intent="$3"
  local goal="$4"
  local inferred_cmd
  inferred_cmd="$(infer_stage_cmd "$stage")"

  export AUTOPILOT_GOAL="$goal"

  if [ "$stage" = "plan" ]; then
    if [ "$cmd" != "unset" ] && run_stage_cmd "$stage" "$cmd" "stage-cmd"; then
      return 0
    fi
    "$STATE_HOOK" checkpoint "$stage" "engine-intent: ${intent}"
    if "$ENGINE_HOOK" "$intent" "$goal"; then
      "$STATE_HOOK" checkpoint "$stage" "ok (engine-intent)"
      return 0
    fi
    if [ "$inferred_cmd" != "unset" ] && [ "$inferred_cmd" != "$cmd" ] && run_stage_cmd "$stage" "$inferred_cmd" "inferred-cmd"; then
      return 0
    fi
    "$STATE_HOOK" fail "stage=${stage}"
    return 2
  fi

  if [ "$cmd" != "unset" ] && run_stage_cmd "$stage" "$cmd" "stage-cmd"; then
    return 0
  fi
  if [ "$inferred_cmd" != "unset" ] && [ "$inferred_cmd" != "$cmd" ] && run_stage_cmd "$stage" "$inferred_cmd" "inferred-cmd"; then
    return 0
  fi
  "$STATE_HOOK" checkpoint "$stage" "engine-intent: ${intent}"
  if "$ENGINE_HOOK" "$intent" "$goal"; then
    "$STATE_HOOK" checkpoint "$stage" "ok (engine-intent)"
    return 0
  fi
  "$STATE_HOOK" fail "stage=${stage}"
  return 2
}

resolve_fix_cmd() {
  local failed_gate="$1"
  local implement_cmd="$2"
  local fix_key fix_cmd gate_cmd

  fix_key="${failed_gate}_fix_cmd"
  fix_cmd="$(get_value "$fix_key")"
  if [ -n "$fix_cmd" ] && [ "$fix_cmd" != "unset" ]; then
    echo "$fix_cmd"
    return 0
  fi

  gate_cmd="unset"
  case "$failed_gate" in
    lint) gate_cmd="$(get_value lint_cmd)" ;;
    build) gate_cmd="$(get_value build_cmd)" ;;
    test) gate_cmd="$(get_value test_cmd)" ;;
    security) gate_cmd="$(get_value security_cmd)" ;;
  esac

  if [ -n "$gate_cmd" ] && [ "$gate_cmd" != "unset" ]; then
    echo "$gate_cmd"
    return 0
  fi

  if [ -n "$implement_cmd" ] && [ "$implement_cmd" != "unset" ]; then
    echo "$implement_cmd"
    return 0
  fi

  echo ".claude/hooks/suggest-automation-gates.sh"
}

run_validate_stage() {
  local max_fix_attempts="$1"
  local implement_cmd="$2"
  "$STATE_HOOK" checkpoint "validate" "run gates"
  local attempt=1
  while [ "$attempt" -le "$max_fix_attempts" ]; do
    if "$GATE_HOOK" push; then
      return 0
    fi

    failed_gate=$(jq -r '.last_gate // ""' "$STATE_FILE")
    [ -z "$failed_gate" ] && failed_gate="unknown"
    fix_cmd="$(resolve_fix_cmd "$failed_gate" "$implement_cmd")"

    "$STATE_HOOK" checkpoint "fix" "gate=${failed_gate} attempt=${attempt} cmd=${fix_cmd}"
    if ! eval "$fix_cmd"; then
      "$STATE_HOOK" fail "stage=fix gate=${failed_gate} attempt=${attempt}"
      return 2
    fi

    attempt=$((attempt + 1))
  done

  "$STATE_HOOK" fail "stage=validate retries_exceeded"
  return 2
}

run_quality_stage() {
  "$STATE_HOOK" checkpoint "quality" "run quality gates"
  if "$QUALITY_HOOK" push; then
    "$STATE_HOOK" checkpoint "quality" "ok"
    return 0
  fi
  "$STATE_HOOK" fail "stage=quality"
  return 2
}

run_verify_stage() {
  local max_fix_attempts="$1"
  local verify_cmd="$2"
  local implement_cmd="$3"
  local attempt=1

  if [ "$verify_cmd" = "unset" ]; then
    "$STATE_HOOK" checkpoint "verify" "skip (verify_cmd unset)"
    return 0
  fi

  while [ "$attempt" -le "$max_fix_attempts" ]; do
    "$STATE_HOOK" checkpoint "verify" "run verify attempt=${attempt}"
    if eval "$verify_cmd"; then
      "$STATE_HOOK" checkpoint "verify" "ok"
      return 0
    fi

    "$STATE_HOOK" checkpoint "fix" "verify attempt=${attempt} cmd=${implement_cmd}"
    if ! eval "$implement_cmd"; then
      "$STATE_HOOK" fail "stage=fix source=verify attempt=${attempt}"
      return 2
    fi

    attempt=$((attempt + 1))
  done

  "$STATE_HOOK" fail "stage=verify retries_exceeded"
  return 2
}

run_qa_stage() {
  local goal="$1"
  local qa_cmd="$2"

  if [ "$qa_cmd" = "unset" ]; then
    "$STATE_HOOK" checkpoint "qa" "skip (qa_cmd unset)"
    return 0
  fi

  "$STATE_HOOK" checkpoint "qa" "run qa"
  if eval "$qa_cmd"; then
    "$STATE_HOOK" checkpoint "qa" "ok"
    return 0
  fi

  "$STATE_HOOK" checkpoint "qa" "register remediation workstream"
  if "$QA_REGISTER_HOOK" "$goal"; then
    "$STATE_HOOK" checkpoint "plan" "qa remediation workstream registered"
    return 1
  fi

  "$STATE_HOOK" fail "stage=qa"
  return 2
}

run_delivery_stage() {
  local goal="$1"
  local unset_enforcement="$2"
  local auto_commit auto_push allow_auto_push commit_message done_report unset_report done_status pending_count failed_count

  "$STATE_HOOK" checkpoint "delivery" "run completion contract checks"
  done_report="$("$DONE_CHECK_HOOK" 2>&1)" && done_status=0 || done_status=$?
  if [ -n "$done_report" ]; then
    echo "$done_report" >&2
  fi
  pending_count="$(echo "$done_report" | grep -E '^pending_count=' | head -n 1 | sed -E 's/^pending_count=//' || true)"
  failed_count="$(echo "$done_report" | grep -E '^failed_count=' | head -n 1 | sed -E 's/^failed_count=//' || true)"
  [ -z "$pending_count" ] && pending_count=0
  [ -z "$failed_count" ] && failed_count=0

  if [ "$done_status" -ne 0 ]; then
    "$STATE_HOOK" fail "done_check_blocked"
    return 2
  fi
  if [ "$failed_count" -gt 0 ]; then
    "$STATE_HOOK" defer manual_followups "done-check failed: $(echo "$done_report" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g')" >/dev/null 2>&1 || true
  fi
  if [ "$pending_count" -gt 0 ]; then
    "$STATE_HOOK" defer manual_followups "done-check pending: $(echo "$done_report" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g')" >/dev/null 2>&1 || true
  fi

  unset_report="$("$UNSET_REPORT_HOOK" || true)"
  if [ "$unset_enforcement" = "block" ] && echo "$unset_report" | grep -q '^unset_count=[1-9]'; then
    "$STATE_HOOK" fail "unset_config_blocked"
    echo "run-autopilot 실패: unresolved_config_enforcement=block 이며 unset key가 남아 있습니다." >&2
    echo "$unset_report" >&2
    return 2
  fi
  if [ "$unset_enforcement" = "report" ] && echo "$unset_report" | grep -q '^unset_count=[1-9]'; then
    echo "run-autopilot 보고: 미확정 설정이 남아 있습니다." >&2
    echo "$unset_report" >&2
    "$STATE_HOOK" defer manual_followups "unset-config: $(echo "$unset_report" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g')" >/dev/null 2>&1 || true
  fi

  "$STATE_HOOK" checkpoint "delivery" "render final report"
  "$FINAL_REPORT_HOOK" >/dev/null

  # increment delivered 기록 (VCS 작업 전에 항상 실행)
  local roadmap_hook
  roadmap_hook="$(dirname "$STATE_HOOK")/roadmap-state.sh"
  local session_hook
  session_hook="$(dirname "$STATE_HOOK")/nightwalker-session.sh"

  if [ -f "$roadmap_hook" ] && [ -f "$session_hook" ]; then
    # shellcheck source=nightwalker-session.sh
    source "$session_hook"
    local session_file
    session_file="$(nightwalker_resolve_session_file)"

    if [ -f "$session_file" ]; then
      local cur_incr
      cur_incr="$(nightwalker_read_current_increment "$session_file")"

      # roadmap에서 현재 increment를 done으로 표시
      # shellcheck source=roadmap-state.sh
      source "$roadmap_hook"
      mark_increment_done "${cur_incr:-1}" 2>/dev/null || true

      # session.yaml 갱신
      local today
      today="$(date -u +%Y-%m-%d)"
      awk -v today="$today" '
        /^increment_status:/ { print "increment_status: delivered"; next }
        /^last_delivered_at:/ { print "last_delivered_at: " today; next }
        { print }
      ' "$session_file" > "${session_file}.tmp" && mv "${session_file}.tmp" "$session_file"

      "$STATE_HOOK" checkpoint "delivery" "increment ${cur_incr:-1} delivered" >/dev/null 2>&1 || true
    fi
  fi

  if [ "${AUTOPILOT_SKIP_VCS_WRITE:-false}" = "true" ]; then
    "$STATE_HOOK" checkpoint "delivery" "skip vcs writes (AUTOPILOT_SKIP_VCS_WRITE=true)"
    "$STATE_HOOK" defer manual_followups "push/deploy strategy deferred until after local completion" >/dev/null 2>&1 || true
    return 0
  fi

  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    "$STATE_HOOK" checkpoint "delivery" "skip vcs writes (not a git worktree)"
    return 0
  fi

  auto_commit="$(get_value "auto_commit_on_success")"
  auto_push="$(get_value "auto_push_on_success")"
  allow_auto_push="$(get_value "allow_auto_push")"

  if [ "$auto_commit" = "true" ] && has_worktree_changes; then
    "$STATE_HOOK" checkpoint "delivery" "stage all changes"
    git add -A

    if has_worktree_changes; then
      commit_message="$(build_commit_message "$goal")"
      "$STATE_HOOK" checkpoint "delivery" "git commit -m \"$commit_message\""
      git commit -m "$commit_message"
    fi
  fi

  if [ "$allow_auto_push" = "true" ] && [ "$auto_push" = "true" ]; then
    if has_upstream_branch; then
      "$STATE_HOOK" checkpoint "delivery" "git push"
      git push
    else
      "$STATE_HOOK" checkpoint "delivery" "skip push (no upstream branch configured)"
      "$STATE_HOOK" defer manual_followups "push skipped: no upstream branch configured" >/dev/null 2>&1 || true
    fi
  else
    "$STATE_HOOK" checkpoint "delivery" "skip push (post-development strategy)"
    "$STATE_HOOK" defer manual_followups "push/deploy strategy deferred until after local completion" >/dev/null 2>&1 || true
  fi

  return 0
}

run_sequence_from() {
  local start_stage="$1"
  local plan_cmd="$2"
  local implement_cmd="$3"
  local verify_cmd="$4"
  local review_cmd="$5"
  local qa_cmd="$6"
  local goal="$7"
  local max_fix_attempts="$8"

  local stages=()
  case "$start_stage" in
    plan)
      stages=(plan implement validate verify review quality qa delivery)
      ;;
    implement)
      stages=(implement validate verify review quality qa delivery)
      ;;
    validate)
      stages=(validate verify review quality qa delivery)
      ;;
    verify)
      stages=(verify review quality qa delivery)
      ;;
    review)
      stages=(review quality qa delivery)
      ;;
    quality)
      stages=(quality qa delivery)
      ;;
    qa)
      stages=(qa delivery)
      ;;
    delivery)
      stages=(delivery)
      ;;
    *)
      echo "run-autopilot 실패: 알 수 없는 stage='$start_stage'" >&2
      return 2
      ;;
  esac

  local stage
  for stage in "${stages[@]}"; do
    case "$stage" in
      plan)
        run_stage_with_fallback "plan" "$plan_cmd" "plan" "$goal" || return 2
        ;;
      implement)
        run_stage_with_fallback "implement" "$implement_cmd" "build" "$goal" || return 2
        ;;
      validate)
        run_validate_stage "$max_fix_attempts" "$implement_cmd" || return 2
        ;;
      verify)
        run_verify_stage "$max_fix_attempts" "$verify_cmd" "$implement_cmd" || return 2
        ;;
      review)
        run_stage_with_fallback "review" "$review_cmd" "review" "$goal" || return 2
        ;;
      quality)
        run_quality_stage || return 2
        ;;
      qa)
        run_qa_stage "$goal" "$qa_cmd" || return 2
        ;;
      delivery)
        run_delivery_stage "$goal" "$UNSET_ENFORCEMENT" || return 2
        ;;
    esac
  done
}

ACTION="${1:-start}"
shift || true
GOAL="${*:-autopilot-goal}"
export AUTOPILOT_ACTIVE="true"

max_cycles=$(get_value "max_autopilot_cycles")
max_fix_attempts=$(get_value "max_fix_attempts_per_gate")
unset_enforcement=$(get_value "unresolved_config_enforcement")
unset_enforcement=${unset_enforcement:-report}
UNSET_ENFORCEMENT="$unset_enforcement"
plan_cmd=$(get_value "plan_cmd")
implement_cmd=$(get_value "implement_cmd")
verify_cmd=$(get_value "verify_cmd")
review_cmd=$(get_value "review_cmd")
qa_cmd=$(get_value "qa_cmd")

normalized_plan_cmd="$(normalize_plan_cmd "$plan_cmd")"
if [ "$plan_cmd" != "$normalized_plan_cmd" ]; then
  echo "run-autopilot 경고: legacy plan_cmd에서 run-project-onboarding.sh를 제외하고 실행합니다." >&2
fi
plan_cmd="$normalized_plan_cmd"

cycle=1
start_stage="plan"

case "$ACTION" in
  start)
    "$ENGINE_READY_HOOK"
    "$STATE_HOOK" start "$GOAL"
    _align_increment_status
    cycle=1
    start_stage="plan"
    ;;
  resume)
    "$ENGINE_READY_HOOK"
    if [ ! -f "$STATE_FILE" ]; then
      echo "run-autopilot 실패: resume 대상 상태 파일이 없습니다." >&2
      exit 2
    fi
    status=$(jq -r '.status // "idle"' "$STATE_FILE")
    if [ "$status" = "completed" ]; then
      echo "run-autopilot: 이미 completed 상태입니다."
      exit 0
    fi
    cycle=$(jq -r '.current_cycle // 1' "$STATE_FILE")
    last_stage=$(jq -r '.last_stage // "plan"' "$STATE_FILE")
    if [ -z "$last_stage" ] || [ "$last_stage" = "null" ]; then
      last_stage="plan"
    fi
    start_stage="$last_stage"
    saved_goal=$(jq -r '.goal // ""' "$STATE_FILE")
    if [ -n "$saved_goal" ] && [ "$saved_goal" != "null" ]; then
      GOAL="$saved_goal"
    fi
    ;;
  *)
    echo "usage: $0 {start <goal>|resume}" >&2
    exit 2
    ;;
esac

while [ "$cycle" -le "$max_cycles" ]; do
  "$STATE_HOOK" cycle "$cycle"
  if run_sequence_from "$start_stage" "$plan_cmd" "$implement_cmd" "$verify_cmd" "$review_cmd" "$qa_cmd" "$GOAL" "$max_fix_attempts"; then
    "$STATE_HOOK" complete
    echo "run-autopilot: completed (cycle=$cycle)"
    exit 0
  fi

  cycle=$((cycle + 1))
  start_stage="$(jq -r '.last_stage // "implement"' "$STATE_FILE")"
  case "$start_stage" in
    plan|implement|validate|verify|review|quality|qa|delivery) ;;
    *) start_stage="implement" ;;
  esac
done

"$STATE_HOOK" fail "max_autopilot_cycles_exceeded"
echo "run-autopilot 실패: max_autopilot_cycles 초과" >&2
exit 2
