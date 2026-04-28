## 자율 모드 트리거

- `/workstream` 실행 시 자율 모드 시작. 아래 "자율 허용" 항목을 확인 없이 수행한다.
- `/autopilot` 실행 시 full-auto 모드 시작. `project-automation.md` 정책을 함께 적용한다.
- workstream 종료, 에스컬레이션 발생, 또는 사용자 개입 시 자율 모드를 종료하고 사용자 확인 모드로 전환한다.
- 단순 작업(workstream 없이 진행)에서는 사용자 요청 단위로 자율 실행하고, 각 요청 완료 시 보고한다.

## 자율 허용

- workstream 실행 중 커밋 (commits.md Autonomy 참조)
- feature 브랜치 생성
- 빌드/테스트 실패 시 원인 분석 및 수정 (최대 3회 시도)
- 리뷰에서 즉시 반영 항목 수정

## 사용자 확인 필요

- push (기본 정책은 개발 후 별도 전략으로 처리한다. `allow_auto_push: true`일 때 `/autopilot`에서만 예외 허용)
- 브랜치 삭제
- 의존성 추가/제거
- rules, skills 수정
- `.claude/project-profile.md` 변경 (엔진/모델/게이트 고정값 변경)
- `.claude/project-approvals.md` 변경 (사전 승인 범위 변경)
- `.claude/project-automation.md` 변경 (자동화/게이트 정책 변경)
- 리뷰에서 사용자 판단 필요 항목
- 설계 문서(architecture, roadmap) 변경

`/autopilot`이 `automation_mode: full-auto`이고 `preapproval_enforcement/risk_enforcement/unresolved_config_enforcement=report`인 경우에는 위 항목도 개발 중에는 작업을 계속 진행할 수 있다.
이 경우 에이전트는 가장 합리적인 방향으로 임시 결정을 내리고, 최종 리포트에서 사용자 판단 필요 항목과 후속 보완 지점을 정리한다.
단, force push, 브랜치 삭제, secrets 처리, 치명적 실행 실패는 계속 즉시 중단 대상이다.

## 금지

- force push
- main 브랜치 직접 삭제
- 민감 파일(.env, credentials) 커밋

## 에스컬레이션

- 빌드/테스트 실패 수정이 3회 시도 후에도 해결되지 않으면 사용자에게 보고하고 판단을 요청한다.
- 규칙 간 충돌이 발생하면 Source of Truth Hierarchy(workflow.md)를 적용한다. 그래도 해결되지 않으면 사용자에게 판단을 요청한다.
- 작업 중 현재 scope 밖의 문제(버그, 설계 결함)를 발견하면 수정하지 않고 사용자에게 보고한다.
- 외부 도구(codex 등) 실행이 반복 실패하면 해당 단계를 건너뛰고 사용자에게 알린다.

## 서브 에이전트

- 독립적인 작업 단위가 2개 이상이고 병렬 실행이 가능하면 서브 에이전트를 생성할 수 있다.
- 코드를 변경하는 서브 에이전트는 worktree 격리 모드로 실행하여 충돌을 방지한다.
- 서브 에이전트도 이 harness의 rules를 동일하게 따른다.
- 서브 에이전트의 결과는 메인 에이전트가 검토한 후 통합한다.
- 서브 에이전트는 push, 브랜치 삭제, 의존성 변경을 하지 않는다.
