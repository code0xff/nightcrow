# Project Automation Profile

완전 자동화 실행 정책을 정의한다.
이 파일은 `/autopilot` 실행 기준이며, 변경 시 사용자 확인이 필요하다.

## Mode

- automation_mode: full-auto
- allow_midway_user_prompt: false
- final_report_only: true
- auto_start_autopilot_on_ready: true
- preapproval_enforcement: report
- risk_enforcement: report
- unresolved_config_enforcement: report
- auto_commit_on_success: true
- auto_push_on_success: false
- allow_auto_push: false
- engine_runtime_mode: strict
- allow_engine_stub: false
- execute_engine_commands: true
- intent_retry_attempts: 2
- intent_timeout_seconds: 300
- qa_max_reopen_attempts: 3
- build_parallel_mode: sequential
- build_parallel_max_jobs: 2

## Retry Policy

- max_fix_attempts_per_gate: 5
- max_autopilot_cycles: 10

## Stage Commands

- plan_cmd: .claude/hooks/run-engine-intent.sh plan "${AUTOPILOT_GOAL:-autopilot-goal}"
- implement_cmd: .claude/hooks/run-build-steps.sh "${AUTOPILOT_GOAL:-autopilot-goal}"
- verify_cmd: .claude/hooks/run-verify-check.sh "${AUTOPILOT_GOAL:-autopilot-goal}"
- review_cmd: .claude/hooks/run-engine-intent.sh review "${AUTOPILOT_GOAL:-autopilot-goal}"
- qa_cmd: .claude/hooks/run-qa-check.sh "${AUTOPILOT_GOAL:-autopilot-goal}"

## Engine Adapter Commands (optional)

- engine_cmd_codex: .claude/hooks/run-codex-intent.sh {intent} {goal} {model}
# codex adapter: plugin 가용 시 claude -p 세션 내 codex 도구 사용, 아니면 codex CLI fallback
- engine_cmd_claude: .claude/hooks/run-claude-intent.sh {intent} {goal} {model}
- engine_cmd_openai: unset
- engine_cmd_cursor: unset
- engine_cmd_gemini: unset
- engine_cmd_copilot: unset

## Gate Fix Commands

- lint_fix_cmd: .claude/hooks/run-engine-intent.sh build "${AUTOPILOT_GOAL:-autopilot-goal}"
- build_fix_cmd: .claude/hooks/run-engine-intent.sh build "${AUTOPILOT_GOAL:-autopilot-goal}"
- test_fix_cmd: .claude/hooks/run-engine-intent.sh build "${AUTOPILOT_GOAL:-autopilot-goal}"
- security_fix_cmd: .claude/hooks/run-engine-intent.sh build "${AUTOPILOT_GOAL:-autopilot-goal}"

## Gate Commands

gate 명령이 비어 있으면 `.claude/hooks/suggest-automation-gates.sh`를 먼저 실행해 후보를 채운 뒤 확정한다.

- lint_cmd: cargo clippy --all-targets --all-features -- -D warnings
- build_cmd: cargo build --all-targets
- test_cmd: cargo test --all-targets
- security_cmd: cargo audit

## Hook Enforcement

- run_gates_on_commit: false
- run_gates_on_push: false
- run_quality_on_commit: false
- run_quality_on_push: false

## Quality Policy

- enable_quality_gates: false
- quality_cmd: find .claude/hooks -type f -name "*.sh" -print0 | xargs -0 -I{} bash -n "{}" && .claude/hooks/validate-project-profile.sh && .claude/hooks/validate-project-approvals.sh && .claude/hooks/validate-project-automation.sh && .claude/hooks/validate-completion-contract.sh
- quality_coverage_cmd: unset
- quality_perf_cmd: unset
- quality_architecture_cmd: unset

## Risk Policy

- auto_apply_risk_tier: low,medium
- require_user_for_risk_tier: high,critical
