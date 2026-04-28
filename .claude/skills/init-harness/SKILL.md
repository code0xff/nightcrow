---
name: init-harness
description: 프로젝트 시작 시 엔진/모델/게이트를 고정하는 초기화 workflow
user-invocable: true
---

# Init Harness

프로젝트 시작 단계에서 실행할 초기화 절차다.
목표는 엔진/모델/게이트, 사전 승인 범위, 자동화 실행 정책을 한 번 확정하고 이후 일관되게 적용하는 것이다.

## 1. 프로파일 선택

다음 중 하나를 선택한다.

- `claude-default`: `Codex Plan → Claude Build → Codex Review`
- `generic-ai`: 범용 도구 조합으로 운영
- `lightweight-fast`: 속도 우선, 게이트를 recommended로 운영

필요하면 `.claude/profiles/`의 템플릿을 기준으로 시작한다.

## 2. 실행 엔진 고정

`.claude/project-profile.md`에 다음을 기록한다.

- `profile_name`
- `plan_engine`, `build_engine`, `review_engine`
- 필요하면 `plan_model`, `build_model`, `review_model`
- `plan_gate`, `review_gate` (`required`/`recommended`)

## 3. 검증

- rules의 required gate와 profile gate가 충돌하지 않는지 확인한다.
- 향후 `/plan`, `/workstream`, `/codex:review` 실행 시 이 고정값을 따를 수 있는지 확인한다.
- 프로젝트 README 또는 운영 문서에 활성 프로파일을 명시한다.
- `.claude/hooks/validate-project-profile.sh` 검증을 통과하는지 확인한다.

## 4. 사전 Approve 설정

`.claude/project-approvals.md`에 다음을 기록한다.

- Command Prefix Allowlist
- Always Require Explicit Approval 항목
- Sandbox / Escalation Policy

이 설정은 commit/push 전 `validate-project-approvals.sh` hook으로 검증된다.

## 5. 토큰 최적화 기준 확정

- 프로젝트 컨텍스트 운영 규칙은 `rules/token-optimization.md`를 기본값으로 사용한다.
- 대규모 저장소는 workstream 단위 요약, 부분 조회 우선 원칙을 문서에 명시한다.

## 6. 자동화 정책 확정

`.claude/project-automation.md`를 작성/확정한다.

- `automation_mode`, `allow_midway_user_prompt`, `final_report_only`
- `preapproval_enforcement`, `risk_enforcement`, `unresolved_config_enforcement` (`report`/`block`)
- `auto_commit_on_success`, `auto_push_on_success`
- `allow_auto_push` (기본값은 false. 개발 후 전략과 분리할지 여부)
- `engine_runtime_mode`, `allow_engine_stub` (엔진 실행 실패 시 stub 허용 정책)
- `execute_engine_commands` (실제 엔진 CLI 실행 여부)
- `max_fix_attempts_per_gate`, `max_autopilot_cycles`
- `plan_cmd`, `implement_cmd`, `review_cmd`, `qa_cmd` (자동 오케스트레이션 단계 명령)
- `engine_cmd_*` (엔진별 어댑터 명령 템플릿)
- `lint_fix_cmd/build_fix_cmd/test_fix_cmd/security_fix_cmd` (gate 실패 자동 복구 명령)
- lint/build/test/security gate 명령
- commit/push 시 gate/quality 강제 여부
- `quality_cmd`, `quality_coverage_cmd`, `quality_perf_cmd`, `quality_architecture_cmd`
- 위험도별 자동 반영 정책

가능하면 아래 명령으로 gate 후보를 자동 감지한 뒤, 결과를 검토해 확정한다.

```bash
.claude/hooks/suggest-automation-gates.sh
```

초기 입력을 최소화하려면 아래 bootstrap을 우선 실행한다.
이 스크립트는 gate, quality 세부 명령, engine adapter 템플릿, approvals allowlist, completion contract를 자동 채운다.
또한 `implement_cmd`, `review_cmd`, `*_fix_cmd`를 gate 기준으로 자동 연결한다.

```bash
.claude/hooks/bootstrap-init-harness.sh
```

## 7. Completion Contract 확정

`.claude/completion-contract.md`로 앱 완료 기준을 고정한다.

- `artifact_definition`
- `artifact_check_cmd`
- `run_smoke_cmd`
- `acceptance_test_cmd`
- `release_readiness_cmd`
- `done_enforcement` (`report`/`block`)

완주 자동화를 목표로 할 때는 기본값 `report`로 두고, 미설정/실패 항목을 최종 보고서에서 후처리한다.
검증은 아래 명령으로 수행한다.

```bash
.claude/hooks/validate-completion-contract.sh
```

## 8. 변경 규칙

- 프로파일 변경은 설계 변경으로 취급한다.
- 변경 전 사용자 확인을 받고, 변경 이유를 커밋 메시지나 문서에 남긴다.
