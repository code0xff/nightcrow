## Autopilot Goal

사용자가 목표만 주면 중간 승인 없이 가능한 범위에서 끝까지 구현 완료한다.

## Execution Loop

`plan -> implement -> validate -> verify -> review -> quality -> qa -> delivery -> repeat`

완료 직전에는 `delivery` 단계에서 done-check와 자동 커밋을 수행한다. push/배포는 기본적으로 개발 후 전략으로 분리한다.

- validate는 `project-automation.md`의 gate 명령을 따른다.
- verify는 `verify_cmd`를 사용해 acceptance criteria와 요구사항 충족 여부를 검증한다.
- plan/implement/review는 `project-automation.md`의 `plan_cmd`, `implement_cmd`, `review_cmd`를 따른다.
- plan은 `docs/acceptance-criteria.md`와 plan artifact의 `## Acceptance Criteria`를 함께 최신화해야 한다.
- implement 단계는 기본적으로 `run-build-steps.sh`가 plan의 Implementation Plan을 step 단위로 분할 실행한다. 각 step 완료 후 build/test gate를 검증하고, 실패 시 에러 컨텍스트를 포함하여 재시도한다. step 파싱이 불가능하면 단일 build로 fallback한다.
- `build_parallel_mode=parallel-safe`일 때만 `[parallel_safe]`로 표시된 독립 step을 제한적으로 병렬 실행할 수 있다.
- full-auto 모드에서는 `/plan`이 roadmap 전체 workstream 설계를 먼저 확정하고, implement 단계는 그 순서를 따라 연속 실행한다.
- stage 명령이 `unset`이면 `run-engine-intent.sh`로 profile 기반 엔진 어댑터를 실행한다.
- strict runtime에서는 `check-engine-readiness.sh`를 통과해야 시작할 수 있다.
- quality는 `quality_cmd`와 선택적인 `quality_coverage/perf/architecture` 명령을 따른다.
- qa는 verify/review/quality 이후의 최종 요구사항 충족 여부를 검수하고, 실패 시 remediation workstream을 등록한 뒤 다음 cycle의 `/plan`으로 되돌린다.
- gate 실패 시 원인 분석 후 자동 수정한다.
- 수정 후 같은 gate를 재실행한다.
- 검증이 끝난 변경은 `auto_commit_on_success=true`일 때 자동 커밋한다.
- `allow_auto_push=true`와 `auto_push_on_success=true`를 명시적으로 켠 경우에만 upstream이 있는 브랜치에 자동 push를 시도한다.
- 실행 상태는 `.claude/state/autopilot-state.json`에 기록한다.
- 중단/실패 후 재개 시 `last_stage` 다음 단계부터 시작한다.
- 자동 재개 실행은 `.claude/hooks/run-autopilot.sh resume`를 사용한다.

## Stop Conditions

- gate별 수정 시도가 `max_fix_attempts_per_gate`를 초과
- 전체 사이클이 `max_autopilot_cycles`를 초과
- high/critical 위험 작업이 감지됨

## Risk Handling

- `auto_apply_risk_tier` 범위의 변경은 자동 반영한다.
- `require_user_for_risk_tier` 범위는 자동 적용하지 않고 보고 후 대기한다.

## Reporting

- 기본은 `final_report_only: true`를 따른다.
- 최종 보고에는 변경 요약, 검증 결과, 남은 리스크만 포함한다.
