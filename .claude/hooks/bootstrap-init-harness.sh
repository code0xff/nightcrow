#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

AUTOMATION_FILE=".claude/project-automation.md"
APPROVALS_FILE=".claude/project-approvals.md"
CONTRACT_FILE=".claude/completion-contract.md"

if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "bootstrap 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi
if [ ! -f "$APPROVALS_FILE" ]; then
  echo "bootstrap 실패: $APPROVALS_FILE 파일이 없습니다." >&2
  exit 2
fi
get_session_value() {
  local key="$1"
  local session_file
  session_file="$(nightwalker_resolve_session_file)"
  [ -f "$session_file" ] || return 0
  grep -E "^${key}:" "$session_file" | head -n 1 | sed -E "s/^${key}:[[:space:]]*//" || true
}

if [ ! -f "$CONTRACT_FILE" ]; then
  project_archetype="$(get_session_value project_archetype)"
  if [ "$project_archetype" = "system-platform" ]; then
    cat > "$CONTRACT_FILE" <<'EOF'
# Completion Contract

프로젝트 개발 완료 판정을 위한 계약을 정의한다.
기본 정책은 non-blocking(report)이며, 미설정/실패 항목은 최종 보고서에 남긴다.

## Contract

- done_enforcement: report
- artifact_definition: interface contract validation completed
- artifact_check_cmd: unset
- run_smoke_cmd: unset
- acceptance_test_cmd: .claude/hooks/run-automation-gates.sh push
- release_readiness_cmd: .claude/hooks/run-quality-gates.sh push

## System Platform Checks

- interface_contract_check: public and internal interface contracts validated
- compatibility_check: backward compatibility risks assessed
- failure_mode_check: failure scenarios and recovery assumptions reviewed
- operability_check: logs, metrics, health checks baseline confirmed
EOF
  else
    # service-app (default)
    cat > "$CONTRACT_FILE" <<'EOF'
# Completion Contract

프로젝트 개발 완료 판정을 위한 계약을 정의한다.
기본 정책은 non-blocking(report)이며, 미설정/실패 항목은 최종 보고서에 남긴다.

## Contract

- done_enforcement: report
- artifact_definition: release artifact generated
- artifact_check_cmd: unset
- run_smoke_cmd: unset
- acceptance_test_cmd: .claude/hooks/run-automation-gates.sh push
- release_readiness_cmd: .claude/hooks/run-quality-gates.sh push
EOF
  fi
fi

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

get_automation_value() {
  local key="$1"
  grep -E "^- ${key}:" "$AUTOMATION_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

set_automation_if_unset() {
  local key="$1"
  local value="$2"
  local current
  current="$(get_automation_value "$key")"
  if [ -z "$current" ] || [ "$current" = "unset" ]; then
    set_automation_key "$key" "$value"
  fi
}

set_contract_key() {
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
  ' "$CONTRACT_FILE" > "${CONTRACT_FILE}.tmp"
  mv "${CONTRACT_FILE}.tmp" "$CONTRACT_FILE"
}

set_contract_if_unset() {
  local key="$1"
  local value="$2"
  local current
  current="$(grep -E "^- ${key}:" "$CONTRACT_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true)"
  if [ -z "$current" ] || [ "$current" = "unset" ]; then
    set_contract_key "$key" "$value"
  fi
}

ensure_allowlist_item() {
  local item="$1"
  [ -z "$item" ] && return 0
  if grep -Fq -- "- \`$item\`" "$APPROVALS_FILE"; then
    return 0
  fi
  awk -v item="$item" '
    BEGIN { in_section = 0; inserted = 0; seen_list = 0 }
    $0 == "## Command Prefix Allowlist" { in_section = 1; print; next }
    in_section == 1 && /^- `/ { seen_list = 1; print; next }
    in_section == 1 && /^## / {
      if (inserted == 0) {
        print "- `" item "`"
        inserted = 1
      }
      in_section = 0
      print
      next
    }
    in_section == 1 && seen_list == 1 && $0 !~ /^- `/ && $0 !~ /^[[:space:]]*$/ {
      if (inserted == 0) {
        print "- `" item "`"
        inserted = 1
      }
      in_section = 0
    }
    { print }
    END {
      if (in_section == 1 && inserted == 0) {
        print "- `" item "`"
      }
    }
  ' "$APPROVALS_FILE" > "${APPROVALS_FILE}.tmp"
  mv "${APPROVALS_FILE}.tmp" "$APPROVALS_FILE"
}

command_prefix() {
  local cmd="$1"
  [ -z "$cmd" ] || [ "$cmd" = "unset" ] && return 0
  case "$cmd" in
    if\ *|for\ *|while\ *|echo\ *|find\ *|rg\ *|grep\ *)
      return 0
      ;;
  esac

  read -r w1 w2 w3 _ <<< "$cmd"
  [ -z "${w1:-}" ] && return 0

  case "$w1" in
    .claude/hooks/*)
      echo "$w1"
      return 0
      ;;
    npm)
      if [ "${w2:-}" = "run" ] && [ -n "${w3:-}" ]; then
        echo "npm run $w3"
        return 0
      fi
      if [ "${w2:-}" = "test" ]; then
        echo "npm test"
        return 0
      fi
      ;;
    pnpm|yarn)
      if [ "${w2:-}" = "run" ] && [ -n "${w3:-}" ]; then
        echo "$w1 run $w3"
        return 0
      fi
      if [ "${w2:-}" = "-r" ] && [ -n "${w3:-}" ]; then
        echo "$w1 -r $w3"
        return 0
      fi
      if [ "${w2:-}" = "test" ]; then
        echo "$w1 test"
        return 0
      fi
      ;;
    python|python3)
      if [ "${w2:-}" = "-m" ] && [ -n "${w3:-}" ]; then
        echo "$w1 -m $w3"
        return 0
      fi
      ;;
    go|cargo|make|pytest|ruff|codex|claude|pnpm)
      if [ -n "${w2:-}" ]; then
        echo "$w1 $w2"
      else
        echo "$w1"
      fi
      return 0
      ;;
  esac
}

# 1) gate 기본값 자동 감지
.claude/hooks/suggest-automation-gates.sh >/dev/null

# 2) quality 세부 커맨드 자동 설정
quality_coverage_cmd="unset"
quality_perf_cmd="unset"
quality_architecture_cmd="unset"

if [ -f package.json ] && command -v jq >/dev/null 2>&1; then
  if jq -e '.scripts.coverage' package.json >/dev/null 2>&1; then
    quality_coverage_cmd="npm run coverage"
  elif jq -e '.scripts.test' package.json >/dev/null 2>&1; then
    quality_coverage_cmd="npm test -- --coverage"
  fi
  if jq -e '.scripts.perf' package.json >/dev/null 2>&1; then
    quality_perf_cmd="npm run perf"
  elif jq -e '.scripts.benchmark' package.json >/dev/null 2>&1; then
    quality_perf_cmd="npm run benchmark"
  fi
  if jq -e '.scripts.typecheck' package.json >/dev/null 2>&1; then
    quality_architecture_cmd="npm run typecheck"
  elif jq -e '.scripts.lint' package.json >/dev/null 2>&1; then
    quality_architecture_cmd="npm run lint"
  fi
elif [ -f pyproject.toml ]; then
  quality_coverage_cmd="pytest --cov -q"
  quality_perf_cmd="unset"
  quality_architecture_cmd="ruff check ."
elif [ -f go.mod ]; then
  quality_coverage_cmd="go test ./... -cover"
  quality_perf_cmd="go test ./... -run=^$ -bench ."
  quality_architecture_cmd="go vet ./..."
elif [ -f Cargo.toml ]; then
  quality_coverage_cmd="cargo test --all-targets"
  quality_perf_cmd="cargo bench"
  quality_architecture_cmd="cargo clippy --all-targets --all-features -- -D warnings"
fi

set_automation_key "quality_coverage_cmd" "$quality_coverage_cmd"
set_automation_key "quality_perf_cmd" "$quality_perf_cmd"
set_automation_key "quality_architecture_cmd" "$quality_architecture_cmd"
set_automation_if_unset "auto_start_autopilot_on_ready" "true"
set_automation_if_unset "auto_commit_on_success" "true"
set_automation_if_unset "auto_push_on_success" "false"
set_automation_if_unset "allow_auto_push" "false"
set_automation_if_unset "run_gates_on_commit" "true"
set_automation_if_unset "run_quality_on_commit" "true"
set_automation_if_unset "intent_retry_attempts" "2"
set_automation_if_unset "intent_timeout_seconds" "300"
set_automation_if_unset "qa_max_reopen_attempts" "3"
set_automation_if_unset "build_parallel_mode" "sequential"
set_automation_if_unset "build_parallel_max_jobs" "2"

# 3) engine adapter 커맨드 자동 설정
if command -v codex >/dev/null 2>&1; then
  set_automation_key "engine_cmd_codex" '.claude/hooks/run-codex-intent.sh {intent} {goal} {model}'
else
  set_automation_key "engine_cmd_codex" "unset"
fi

if command -v claude >/dev/null 2>&1; then
  set_automation_key "engine_cmd_claude" '.claude/hooks/run-claude-intent.sh {intent} {goal} {model}'
else
  set_automation_key "engine_cmd_claude" "unset"
fi

if command -v openai >/dev/null 2>&1; then
  set_automation_key "engine_cmd_openai" 'openai --help >/dev/null'
else
  set_automation_key "engine_cmd_openai" "unset"
fi

# 4) stage/fix 기본값 자동 연결
build_cmd="$(get_automation_value build_cmd)"
test_cmd="$(get_automation_value test_cmd)"
quality_cmd="$(get_automation_value quality_cmd)"
set_automation_if_unset "plan_cmd" '.claude/hooks/run-engine-intent.sh plan "${AUTOPILOT_GOAL:-autopilot-goal}"'
set_automation_if_unset "implement_cmd" '.claude/hooks/run-build-steps.sh "${AUTOPILOT_GOAL:-autopilot-goal}"'
set_automation_if_unset "verify_cmd" '.claude/hooks/run-verify-check.sh "${AUTOPILOT_GOAL:-autopilot-goal}"'
if [ "$quality_cmd" != "unset" ]; then
  set_automation_if_unset "review_cmd" "$quality_cmd"
elif [ "$test_cmd" != "unset" ]; then
  set_automation_if_unset "review_cmd" "$test_cmd"
fi
set_automation_if_unset "qa_cmd" '.claude/hooks/run-qa-check.sh "${AUTOPILOT_GOAL:-autopilot-goal}"'
if [ "$build_cmd" != "unset" ]; then
  set_automation_if_unset "build_fix_cmd" "$build_cmd"
fi
if [ "$test_cmd" != "unset" ]; then
  set_automation_if_unset "test_fix_cmd" "$test_cmd"
fi
lint_cmd="$(get_automation_value lint_cmd)"
security_cmd="$(get_automation_value security_cmd)"
if [ "$lint_cmd" != "unset" ]; then
  set_automation_if_unset "lint_fix_cmd" "$lint_cmd"
fi
if [ "$security_cmd" != "unset" ]; then
  set_automation_if_unset "security_fix_cmd" "$security_cmd"
fi

# legacy migration: plan 단계에서 onboarding 재실행을 제거한다.
legacy_plan_cmd='.claude/hooks/run-project-onboarding.sh && .claude/hooks/run-engine-intent.sh plan "${AUTOPILOT_GOAL:-autopilot-goal}"'
current_plan_cmd="$(get_automation_value plan_cmd)"
if [ "$current_plan_cmd" = "$legacy_plan_cmd" ]; then
  set_automation_key "plan_cmd" '.claude/hooks/run-engine-intent.sh plan "${AUTOPILOT_GOAL:-autopilot-goal}"'
fi

# 5) approvals allowlist 자동 보강
ensure_allowlist_item "git add"
ensure_allowlist_item "git commit"
ensure_allowlist_item "git push"

for key in lint_cmd build_cmd test_cmd plan_cmd implement_cmd verify_cmd review_cmd qa_cmd quality_coverage_cmd quality_perf_cmd quality_architecture_cmd; do
  cmd="$(get_automation_value "$key")"
  prefix="$(command_prefix "$cmd" || true)"
  if [ -n "${prefix:-}" ]; then
    ensure_allowlist_item "$prefix"
  fi
done

# 6) completion contract 자동 연결
build_cmd="$(get_automation_value build_cmd)"
test_cmd="$(get_automation_value test_cmd)"
quality_cmd="$(get_automation_value quality_cmd)"

set_contract_if_unset "done_enforcement" "report"
set_contract_if_unset "artifact_definition" "release artifact generated"
if [[ "$build_cmd" =~ ^echo[[:space:]]+ ]] || [ -z "$build_cmd" ] || [ "$build_cmd" = "unset" ]; then
  # build_cmd 미설정 시 기존 값을 보존한다 (의미 있는 값이 있을 수 있음).
  set_contract_if_unset "artifact_check_cmd" "unset"
else
  set_contract_key "artifact_check_cmd" "$build_cmd"
fi
if [[ "$test_cmd" =~ ^echo[[:space:]]+ ]] || [ -z "$test_cmd" ] || [ "$test_cmd" = "unset" ]; then
  # test_cmd 미설정 시 기존 값을 보존한다.
  set_contract_if_unset "run_smoke_cmd" "unset"
else
  set_contract_key "run_smoke_cmd" "$test_cmd"
fi
set_contract_if_unset "acceptance_test_cmd" ".claude/hooks/run-automation-gates.sh push"
if [[ "$quality_cmd" =~ ^echo[[:space:]]+ ]] || [ -z "$quality_cmd" ] || [ "$quality_cmd" = "unset" ]; then
  set_contract_if_unset "release_readiness_cmd" "unset"
else
  set_contract_key "release_readiness_cmd" "$quality_cmd"
fi

echo "init-harness bootstrap 완료:"
echo "- automation gates/quality/engine adapter 값 자동 설정"
echo "- stage/fix 기본 명령 자동 연결"
echo "- approvals allowlist 자동 보강"
echo "- completion contract 기본값 자동 설정"
exit 0
